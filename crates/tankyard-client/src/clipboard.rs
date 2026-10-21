//! One long-lived native clipboard connection, serviced away from the render loop.
use std::{
    cell::RefCell,
    sync::mpsc,
    time::{Duration, Instant},
};

const MAX_TEXT: usize = 65_536;
pub struct Request<T> {
    result: mpsc::Receiver<Result<T, String>>,
    started: Instant,
}
impl<T> Request<T> {
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Clipboard is unavailable. Try again.".into()))
            }
            Err(mpsc::TryRecvError::Empty) if self.started.elapsed() > Duration::from_secs(3) => {
                Some(Err("Clipboard did not respond. Try again.".into()))
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}
enum Command {
    Copy(String, mpsc::Sender<Result<(), String>>),
    Paste(mpsc::Sender<Result<String, String>>),
}
struct Service {
    commands: mpsc::SyncSender<Command>,
}
impl Service {
    fn new() -> Self {
        let (commands, incoming) = mpsc::sync_channel(8);
        // Keep the connection alive: Linux selections need their owner to serve
        // external paste requests, including TARGETS and plain UTF-8 formats.
        let _ =
            std::thread::Builder::new()
                .name("clipboard".into())
                .spawn(move || {
                    let mut clipboard = None;
                    while let Ok(command) = incoming.recv() {
                        if clipboard.is_none() {
                            clipboard = arboard::Clipboard::new().ok();
                        }
                        match command {
                            Command::Copy(text, reply) => {
                                let result = clipboard.as_mut().ok_or_else(unavailable).and_then(
                                    |clipboard| clipboard.set_text(text).map_err(|_| unavailable()),
                                );
                                let _ = reply.send(result);
                            }
                            Command::Paste(reply) => {
                                let result = clipboard
                                    .as_mut()
                                    .ok_or_else(unavailable)
                                    .and_then(|clipboard| {
                                        clipboard.get_text().map_err(|_| {
                                            "No text is available on the clipboard.".into()
                                        })
                                    })
                                    .map(|mut text| {
                                        let mut end = text.len().min(MAX_TEXT);
                                        while !text.is_char_boundary(end) {
                                            end -= 1;
                                        }
                                        text.truncate(end);
                                        text
                                    });
                                let _ = reply.send(result);
                            }
                        }
                    }
                });
        Self { commands }
    }
}
fn unavailable() -> String {
    "Clipboard is unavailable. Try again.".into()
}
thread_local! { static SERVICE: RefCell<Option<Service>> = const { RefCell::new(None) }; }
fn enqueue(command: Command) {
    SERVICE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let service = slot.get_or_insert_with(Service::new);
        if let Err(error) = service.commands.try_send(command) {
            let (mpsc::TrySendError::Full(command) | mpsc::TrySendError::Disconnected(command)) =
                error;
            match command {
                Command::Copy(_, reply) => {
                    let _ = reply.send(Err(unavailable()));
                }
                Command::Paste(reply) => {
                    let _ = reply.send(Err(unavailable()));
                }
            }
        }
    });
}
pub fn copy(text: &str) -> Request<()> {
    let (reply, result) = mpsc::channel();
    if text.len() > MAX_TEXT {
        let _ = reply.send(Err("Text is too long to copy.".into()));
    } else {
        enqueue(Command::Copy(text.to_owned(), reply));
    }
    Request {
        result,
        started: Instant::now(),
    }
}
pub fn paste() -> Request<String> {
    let (reply, result) = mpsc::channel();
    enqueue(Command::Paste(reply));
    Request {
        result,
        started: Instant::now(),
    }
}
pub fn shutdown() {
    SERVICE.with(|service| {
        service.borrow_mut().take();
    });
}

use crate::{NetError, ServerSettings, UdpServer};
use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

enum HostCommand {
    Shutdown,
    Admin(String, SyncSender<String>),
}

/// Player-hosted matches run exactly the dedicated-server transport and
/// simulation on a small worker thread. A bounded command queue prevents a
/// stalled host from accumulating arbitrary UI work.
pub struct HostedServer {
    pub address: SocketAddr,
    sender: SyncSender<HostCommand>,
    worker: Option<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}
impl HostedServer {
    pub fn start(settings: ServerSettings) -> Result<Self, NetError> {
        let mut server = UdpServer::bind(settings)?;
        let address = server.address;
        let (sender, receiver) = mpsc::sync_channel(32);
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        let worker = thread::Builder::new()
            .name("tankyard-server".into())
            .spawn(move || {
                let mut previous = Instant::now();
                loop {
                    for command in receiver.try_iter().take(32) {
                        match command {
                            HostCommand::Shutdown => {
                                server.shutdown();
                                return;
                            }
                            HostCommand::Admin(text, reply) => {
                                let result = server.admin_command(&text);
                                let _ = reply.try_send(result.message);
                                if result.shutdown {
                                    return;
                                }
                            }
                        }
                    }
                    let now = Instant::now();
                    if let Err(e) = server.update(now.duration_since(previous)) {
                        if let Ok(mut error) = worker_error.lock() {
                            *error = Some(e.to_string());
                        }
                        log::error!("hosted server stopped: {e}");
                        break;
                    }
                    previous = now;
                    thread::sleep(Duration::from_millis(2));
                }
            })?;
        Ok(Self {
            address,
            sender,
            worker: Some(worker),
            error,
        })
    }
    pub fn command(&self, command: &str) -> Result<String, NetError> {
        self.command_async(command)?
            .recv_timeout(Duration::from_secs(2))
            .map_err(|e| NetError(format!("host did not respond: {e}")))
    }
    pub fn command_async(&self, command: &str) -> Result<mpsc::Receiver<String>, NetError> {
        if command.len() > 512 {
            return Err(NetError("host command is too long".into()));
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        self.sender
            .try_send(HostCommand::Admin(command.into(), sender))
            .map_err(|e| NetError(format!("host command could not be queued: {e}")))?;
        Ok(receiver)
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|e| e.clone())
    }
    pub fn shutdown(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = self.sender.send(HostCommand::Shutdown);
            let _ = worker.join();
        }
    }
}
impl Drop for HostedServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

//! Native clipboard interoperability fixture. Text never appears in command-line arguments.
use std::{
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant},
};
use tankyard_client::clipboard;
fn wait<T>(request: clipboard::Request<T>) -> Result<T, Box<dyn std::error::Error>> {
    loop {
        if let Some(result) = request.poll() {
            return result.map_err(Into::into);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn write(path: &Path, text: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 2 {
        return Err("Expected copy <text-file> <ready-file>, or paste <output-file>".into());
    }
    match args[0].to_str() {
        Some("copy") if args.len() == 3 => {
            let mut text = String::new();
            std::fs::File::open(&args[1])?
                .take(65_537)
                .read_to_string(&mut text)?;
            wait(clipboard::copy(&text))?;
            write(Path::new(&args[2]), b"ready")?;
            let started = Instant::now();
            while Path::new(&args[2]).exists() && started.elapsed() < Duration::from_secs(30) {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Some("paste") if args.len() == 2 => {
            write(Path::new(&args[1]), wait(clipboard::paste())?.as_bytes())?;
        }
        _ => return Err("Invalid clipboard fixture arguments".into()),
    }
    clipboard::shutdown();
    Ok(())
}

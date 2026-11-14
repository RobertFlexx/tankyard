use directories::ProjectDirs;
use macroquad::prelude::Conf;
use std::{backtrace::Backtrace, fs, path::PathBuf};

pub fn diagnostics_directory() -> PathBuf {
    ProjectDirs::from("org", "Tank Yard", "Tank Yard")
        .map(|dirs| dirs.cache_dir().to_owned())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn private_report(path: &std::path::Path, text: &str) {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut file) = options.open(path) {
        let _ = file.write_all(text.as_bytes());
    }
}

pub fn install(conf: &Conf) {
    let directory = diagnostics_directory();
    let path = directory.join("last-startup.log");
    let crash = directory.join("last-crash.log");
    let header = format!(
        "Tank Yard {}\nPlatform: {} / {}\nWindow: {}x{}, fullscreen={}, samples={}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        conf.window_width,
        conf.window_height,
        conf.fullscreen,
        conf.sample_count,
    );
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    private_report(&path, &header);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut report = format!("{header}\n{info}\n\n{}\n", Backtrace::force_capture());
        let mut end = report.len().min(256 * 1024);
        while !report.is_char_boundary(end) {
            end -= 1;
        }
        report.truncate(end);
        private_report(&path, &report);
        // Preserve the last failure when the next ordinary launch rewrites
        // its startup report. Reports stay local; nothing is uploaded.
        private_report(&crash, &report);
        eprintln!("Tank Yard stopped. Crash log: {}", crash.display());
        eprintln!("Try --safe-graphics when reporting a graphics startup failure.");
        previous(info);
    }));
}

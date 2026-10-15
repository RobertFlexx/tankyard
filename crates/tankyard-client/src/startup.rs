use directories::ProjectDirs;
use macroquad::prelude::Conf;
use std::{backtrace::Backtrace, fs, path::PathBuf};

pub fn install(conf: &Conf) {
    let path = ProjectDirs::from("org", "Tank Yard", "Tank Yard")
        .map(|dirs| dirs.cache_dir().join("last-startup.log"))
        .unwrap_or_else(|| PathBuf::from("tankyard-startup.log"));
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
    let _ = fs::write(&path, &header);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = format!("{header}\n{info}\n\n{}\n", Backtrace::force_capture());
        let _ = fs::write(&path, report);
        eprintln!("Tank Yard stopped. Startup log: {}", path.display());
        eprintln!("Try --safe-graphics when reporting a graphics startup failure.");
        previous(info);
    }));
}

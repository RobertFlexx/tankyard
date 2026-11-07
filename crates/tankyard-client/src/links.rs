//! Opening a mod's web links in the player's browser.

/// Open an http(s) link with the system's default browser. Anything else is
/// refused, so a pack can never launch a program or local file.
pub fn open(url: &str) -> Result<(), String> {
    if !tankyard_mods::project::web_link(url) {
        return Err("Only web links can be opened.".into());
    }
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        // Hands the link to the URL protocol handler without a shell.
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not open the browser: {e}"))
}

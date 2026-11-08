//! The system's own file chooser and file manager, used by the Mods page.
//! Each dialog runs in a helper process on a background thread, so the game
//! keeps drawing while it is open.
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{Receiver, TryRecvError, channel},
};

/// A file chooser that may still be open.
pub struct Pending(Receiver<Result<Option<PathBuf>, String>>);

impl Pending {
    /// `None` while the chooser is open; then the chosen path, or `Ok(None)`
    /// if the player cancelled.
    pub fn poll(&self) -> Option<Result<Option<PathBuf>, String>> {
        match self.0.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("The file chooser closed unexpectedly.".into()))
            }
        }
    }
}

/// Ask for a mod pack: a `.zip` archive or a pack folder's `mod.json`.
pub fn choose_mod_pack() -> Pending {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let _ = sender.send(run_chooser());
    });
    Pending(receiver)
}

fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes up behind the dialog.
        command.creation_flags(0x0800_0000);
    }
    command.stdin(Stdio::null()).stderr(Stdio::null())
}

/// Run one chooser command; a non-zero exit is a cancel.
fn chosen(command: &mut Command) -> std::io::Result<Option<PathBuf>> {
    let output = hidden(command).output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let path = text.trim();
    Ok((output.status.success() && !path.is_empty()).then(|| PathBuf::from(path)))
}

#[cfg(target_os = "macos")]
fn run_chooser() -> Result<Option<PathBuf>, String> {
    const SCRIPT: &str = r#"POSIX path of (choose file with prompt "Choose a Tank Yard mod pack: a .zip archive, or the mod.json inside a pack folder" of type {"zip", "public.zip-archive", "json", "public.json"})"#;
    chosen(Command::new("osascript").args(["-e", SCRIPT]))
        .map_err(|e| format!("Could not open the file chooser: {e}"))
}

#[cfg(target_os = "windows")]
fn run_chooser() -> Result<Option<PathBuf>, String> {
    const SCRIPT: &str = "Add-Type -AssemblyName System.Windows.Forms; \
        $d = New-Object System.Windows.Forms.OpenFileDialog; \
        $d.Title = 'Choose a Tank Yard mod pack'; \
        $d.Filter = 'Tank Yard mod packs (*.zip;mod.json)|*.zip;mod.json|All files (*.*)|*.*'; \
        if ($d.ShowDialog() -eq 'OK') { [Console]::Out.Write($d.FileName) }";
    chosen(Command::new("powershell").args(["-NoProfile", "-STA", "-Command", SCRIPT]))
        .map_err(|e| format!("Could not open the file chooser: {e}"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn run_chooser() -> Result<Option<PathBuf>, String> {
    let title = "Choose a Tank Yard mod pack";
    let attempts: [(&str, Vec<&str>); 3] = [
        (
            "zenity",
            vec![
                "--file-selection",
                "--title",
                title,
                "--file-filter=Tank Yard mod packs | *.zip mod.json",
                "--file-filter=All files | *",
            ],
        ),
        (
            "kdialog",
            vec![
                "--title",
                title,
                "--getopenfilename",
                ".",
                "*.zip mod.json|Tank Yard mod packs",
            ],
        ),
        ("qarma", vec!["--file-selection", "--title", title]),
    ];
    for (program, args) in attempts {
        match chosen(Command::new(program).args(&args)) {
            Ok(path) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Could not open the file chooser: {error}")),
        }
    }
    Err("No file chooser found. Install zenity or kdialog, or paste the pack's path.".into())
}

/// Show a folder in Finder, Explorer or the desktop's file manager,
/// creating it first if needed.
pub fn reveal(folder: &Path) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let program = "xdg-open";
    hidden(Command::new(program).arg(folder))
        .stdout(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not open {}: {e}", folder.display()))
}

//! A dedicated community server run from the Host page: the bundled
//! `tankyard-server` in its own process, configured from the menu, with its
//! console captured for display. It stops gracefully with the game.
use crate::settings::CommunityServer;
use directories::ProjectDirs;
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tankyard_core::WorldConfig;
use tankyard_server::ServerSettings;

const LOG_LINES: usize = 200;

#[derive(Default)]
pub struct Dedicated {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    log: Arc<Mutex<VecDeque<String>>>,
    /// Why the last server stopped, if it stopped on its own.
    pub stopped: Option<String>,
    pub port: u16,
}

/// Where the community server's configuration and data live.
pub fn server_dir() -> PathBuf {
    ProjectDirs::from("org", "Tank Yard", "Tank Yard")
        .map(|p| p.data_dir().join("server"))
        .unwrap_or_else(|| PathBuf::from("server"))
}

pub fn config_path() -> PathBuf {
    server_dir().join("server.toml")
}

/// The `tankyard-server` shipped beside the game, including outside a macOS
/// app bundle.
pub fn server_executable() -> Option<PathBuf> {
    let name = format!("tankyard-server{}", std::env::consts::EXE_SUFFIX);
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [
        dir.join(&name),
        // Test binaries live one level down, in `deps`.
        dir.join("..").join(&name),
        dir.join("../../..").join(&name),
        dir.join("../../../..").join(&name),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

/// The command that runs this server without the game, for an always-on host.
pub fn start_command() -> String {
    let server = server_executable()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "tankyard-server".into());
    let quote = |value: &str| {
        let escaped = if cfg!(windows) {
            value.replace('\'', "''")
        } else {
            value.replace('\'', "'\\''")
        };
        format!("'{escaped}'")
    };
    let directory = quote(&server_dir().to_string_lossy());
    let server = quote(&server);
    let config = quote(&config_path().to_string_lossy());
    if cfg!(windows) {
        format!(
            "Set-Location -LiteralPath {directory} -ErrorAction Stop; & {server} --config {config}"
        )
    } else {
        format!("cd {directory} && {server} --config {config}")
    }
}

/// The address other computers on this network use to reach `port`.
pub fn lan_address(port: u16) -> Option<String> {
    // Connecting a UDP socket picks the outgoing interface without sending.
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then(|| format!("{ip}:{port}"))
}

/// Server settings for a community server with the menu's arena choices.
pub fn settings(
    server: &CommunityServer,
    world: WorldConfig,
    mods: Vec<PathBuf>,
    difficulty: &str,
) -> Result<ServerSettings, String> {
    let mods = mods
        .into_iter()
        .map(std::path::absolute)
        .collect::<std::io::Result<_>>()
        .map_err(|e| format!("Could not resolve a mod path: {e}"))?;
    // Bans and logs use the server's relative defaults, inside `server_dir`.
    let settings = ServerSettings {
        name: server.name.clone(),
        bind: format!("0.0.0.0:{}", server.port),
        websocket: if server.websocket {
            "127.0.0.1:8080".into()
        } else {
            String::new()
        },
        max_players: usize::from(server.max_players),
        arena_capacity: usize::from(server.lobby_size),
        bot_count: usize::from(server.bots),
        password: server.password.clone(),
        difficulty: tankyard_server::difficulty_from_name(difficulty)?,
        world,
        mods,
        ..Default::default()
    };
    settings.validate().map_err(|e| e.to_string())?;
    Ok(settings)
}

impl Dedicated {
    pub fn running(&self) -> bool {
        self.child.is_some()
    }

    pub fn log(&self) -> Vec<String> {
        self.log
            .lock()
            .map(|log| log.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn start(&mut self, settings: &ServerSettings) -> Result<(), String> {
        self.start_in(settings, server_dir())
    }

    /// Start the server with its configuration and data in `dir`.
    fn start_in(&mut self, settings: &ServerSettings, dir: PathBuf) -> Result<(), String> {
        if self.running() {
            return Err("The community server is already running.".into());
        }
        let executable = server_executable().ok_or(
            "tankyard-server was not found beside the game. Reinstall Tank Yard, or run the server from the release archive.",
        )?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let config = dir.join("server.toml");
        settings.save(&config).map_err(|e| e.to_string())?;
        let mut command = Command::new(executable);
        command
            .arg("--config")
            .arg(&config)
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: the game shows the console itself.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Could not start the server: {e}"))?;
        if let Ok(mut log) = self.log.lock() {
            log.clear();
        }
        for stream in [
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let log = self.log.clone();
            let _ = std::thread::Builder::new()
                .name("server-log".into())
                .spawn(move || {
                    for line in BufReader::new(stream).lines() {
                        let Ok(line) = line else { break };
                        let line: String =
                            line.chars().filter(|c| !c.is_control()).take(240).collect();
                        if let Ok(mut log) = log.lock() {
                            if log.len() >= LOG_LINES {
                                log.pop_front();
                            }
                            log.push_back(line);
                        }
                    }
                });
        }
        self.stdin = child.stdin.take();
        self.child = Some(child);
        self.stopped = None;
        self.port = settings
            .bind
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5000);
        Ok(())
    }

    /// Notice a server that exited on its own.
    pub fn poll(&mut self) {
        let Some(child) = &mut self.child else { return };
        if let Ok(Some(status)) = child.try_wait() {
            let last = self.log().into_iter().rev().find(|l| !l.trim().is_empty());
            self.stopped = Some(match (status.success(), last) {
                (true, _) => "The server stopped.".into(),
                (false, Some(line)) => line,
                (false, None) => format!("The server exited ({status})."),
            });
            self.child = None;
            self.stdin = None;
        }
    }

    /// Ask the server to shut down, and end it if it does not within a moment.
    pub fn stop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.write_all(b"shutdown\n");
            let _ = stdin.flush();
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                self.stopped = Some("The server stopped.".into());
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        self.stopped = Some("The server did not stop in time and was ended.".into());
    }
}

impl Drop for Dedicated {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_server_starts_answers_and_stops() {
        if server_executable().is_none() {
            eprintln!("skipped: build tankyard-server first");
            return;
        }
        let port = std::net::UdpSocket::bind("127.0.0.1:0")
            .and_then(|s| s.local_addr())
            .unwrap()
            .port();
        let server = CommunityServer {
            name: "Hosting test".into(),
            port,
            lobby_size: 4,
            ..Default::default()
        };
        let mut config = settings(&server, WorldConfig::default(), vec![], "normal").unwrap();
        config.bind = format!("127.0.0.1:{port}");
        let mut dedicated = Dedicated::default();
        let dir = std::env::temp_dir().join(format!("tankyard-hosting-{port}"));
        dedicated.start_in(&config, dir.clone()).unwrap();
        let mut answer = None;
        for _ in 0..20 {
            if let Ok(probe) = tankyard_server::probe::probe(&format!("127.0.0.1:{port}")) {
                answer = Some(probe);
                break;
            }
            dedicated.poll();
            assert!(dedicated.running(), "{:?}", dedicated.log());
        }
        let probe = answer.expect("server answered a status probe");
        assert_eq!(probe.status.name, "Hosting test");
        dedicated.stop();
        assert_eq!(dedicated.stopped.as_deref(), Some("The server stopped."));
        let _ = std::fs::remove_dir_all(dir);
    }
}

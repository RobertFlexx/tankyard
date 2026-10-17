//! Embedded authoritative sessions, headless UDP hosting, and the reusable
//! graphical/headless network client. This crate has no graphics dependencies.
pub mod access;
pub mod accounts;
mod admin;
mod chat;
mod client;
mod community;
mod config;
mod host;
pub mod netem;
mod session;
mod storage;
mod transport;
pub mod websocket;

pub use client::{NetworkClient, NetworkDiagnostics};
pub use config::{ServerSettings, difficulty_from_name, mode_from_name};
pub use host::HostedServer;
pub use session::LocalSession;
pub use transport::{AdminReply, ServerMetrics, TickTiming, UdpServer};

use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone)]
pub struct NetError(pub String);
impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for NetError {}
impl From<std::io::Error> for NetError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}
impl From<tankyard_protocol::DecodeError> for NetError {
    fn from(error: tankyard_protocol::DecodeError) -> Self {
        Self(error.to_string())
    }
}

fn unix_time() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
}

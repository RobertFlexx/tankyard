//! Live status for servers shown in menus: lobbies, population and a
//! connection quality measured over the same path gameplay uses. Probes run on
//! background threads and refresh only while a server stays on screen.
use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender, channel},
};
use tankyard_server::probe::Probe;

/// Seconds between refreshes of a visible server.
const REFRESH: f64 = 5.;
/// A failed probe is retried a little later.
const RETRY: f64 = 8.;
/// Servers not drawn for this long stop being probed and are forgotten.
const FORGET: f64 = 60.;
const MAX_IN_FLIGHT: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Checking,
    Online(Probe),
    Offline(String),
}

struct Entry {
    state: State,
    /// Smoothed round trip across probes, in milliseconds.
    rtt: Option<f32>,
    in_flight: bool,
    checked_at: f64,
    seen_at: f64,
}

pub struct Monitor {
    entries: HashMap<String, Entry>,
    sender: Sender<(String, Result<Probe, String>)>,
    results: Receiver<(String, Result<Probe, String>)>,
}

impl Default for Monitor {
    fn default() -> Self {
        let (sender, results) = channel();
        Self {
            entries: HashMap::new(),
            sender,
            results,
        }
    }
}

/// Signal bars from round trip and loss, the way players feel them.
pub fn quality(rtt_ms: f32, loss: f32) -> u8 {
    let mut bars: u8 = match rtt_ms {
        r if r < 60. => 4,
        r if r < 110. => 3,
        r if r < 180. => 2,
        _ => 1,
    };
    if loss > 0.3 {
        bars = bars.min(1);
    } else if loss > 0.1 {
        bars = bars.saturating_sub(1).max(1);
    }
    bars
}

impl Monitor {
    /// Note that `address` is on screen; probes it when its status is older
    /// than the normal refresh.
    pub fn watch(&mut self, address: &str, now: f64) {
        self.watch_every(address, now, REFRESH);
    }

    /// Like [`Monitor::watch`], refreshing at most every `period` seconds.
    pub fn watch_every(&mut self, address: &str, now: f64, period: f64) {
        let address = address.trim();
        if address.is_empty() || address.starts_with("tankyard://") {
            return;
        }
        let in_flight = self.entries.values().filter(|e| e.in_flight).count();
        let entry = self
            .entries
            .entry(address.to_owned())
            .or_insert_with(|| Entry {
                state: State::Checking,
                rtt: None,
                in_flight: false,
                checked_at: f64::NEG_INFINITY,
                seen_at: now,
            });
        entry.seen_at = now;
        let wait = if matches!(entry.state, State::Offline(_)) {
            RETRY.max(period)
        } else {
            period
        };
        if entry.in_flight || now - entry.checked_at < wait || in_flight >= MAX_IN_FLIGHT {
            return;
        }
        entry.in_flight = true;
        entry.checked_at = now;
        let sender = self.sender.clone();
        let target = address.to_owned();
        let spawned = std::thread::Builder::new()
            .name("server-probe".into())
            .spawn(move || {
                let result = tankyard_server::probe::probe(&target).map_err(|e| e.to_string());
                let _ = sender.send((target, result));
            });
        if spawned.is_err() {
            entry.in_flight = false;
        }
    }

    /// Probe `address` again as soon as it is watched.
    pub fn refresh(&mut self, address: &str) {
        if let Some(entry) = self.entries.get_mut(address.trim()) {
            entry.checked_at = f64::NEG_INFINITY;
        }
    }

    /// Collect finished probes and forget servers no longer shown.
    pub fn poll(&mut self, now: f64) {
        while let Ok((address, result)) = self.results.try_recv() {
            let Some(entry) = self.entries.get_mut(&address) else {
                continue;
            };
            entry.in_flight = false;
            match result {
                Ok(probe) => {
                    entry.rtt = Some(match entry.rtt {
                        Some(previous) => previous * 0.4 + probe.rtt_ms * 0.6,
                        None => probe.rtt_ms,
                    });
                    entry.state = State::Online(probe);
                }
                Err(error) => {
                    entry.rtt = None;
                    entry.state = State::Offline(error);
                }
            }
        }
        self.entries
            .retain(|_, entry| entry.in_flight || now - entry.seen_at < FORGET);
    }

    pub fn state(&self, address: &str) -> Option<&State> {
        self.entries.get(address.trim()).map(|e| &e.state)
    }

    /// The smoothed round trip and the latest loss, for display.
    pub fn connection(&self, address: &str) -> Option<(f32, f32)> {
        let entry = self.entries.get(address.trim())?;
        match &entry.state {
            State::Online(probe) => Some((entry.rtt.unwrap_or(probe.rtt_ms), probe.loss)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_tracks_latency_and_loss() {
        assert_eq!(quality(20., 0.), 4);
        assert_eq!(quality(90., 0.), 3);
        assert_eq!(quality(150., 0.), 2);
        assert_eq!(quality(400., 0.), 1);
        assert_eq!(quality(20., 0.2), 3);
        assert_eq!(quality(20., 0.4), 1);
    }

    #[test]
    fn unreachable_servers_report_offline_and_are_forgotten_when_hidden() {
        let mut monitor = Monitor::default();
        monitor.watch("tankyard://party/abc", 0.);
        assert!(monitor.state("tankyard://party/abc").is_none());
        monitor.watch("127.0.0.1:9", 0.);
        assert_eq!(monitor.state("127.0.0.1:9"), Some(&State::Checking));
        let start = std::time::Instant::now();
        while matches!(monitor.state("127.0.0.1:9"), Some(State::Checking))
            && start.elapsed().as_secs() < 5
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
            monitor.poll(1.);
        }
        assert!(matches!(
            monitor.state("127.0.0.1:9"),
            Some(State::Offline(_))
        ));
        monitor.poll(100.);
        assert!(monitor.state("127.0.0.1:9").is_none());
    }
}

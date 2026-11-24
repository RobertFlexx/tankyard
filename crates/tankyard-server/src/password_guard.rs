//! Slows down anyone trying a dictionary against a private server.
use std::{collections::HashMap, net::IpAddr, time::Duration};

/// Wrong passwords one address may send in [`WINDOW`].
const TRIES: u32 = 8;
/// A shared relay address carries many people, so it gets more slack.
const SHARED_TRIES: u32 = 64;
const WINDOW: Duration = Duration::from_secs(5 * 60);
/// Enough for any honest crowd; a flood of addresses can't grow this forever.
const MOST_ADDRESSES: usize = 2048;

pub(crate) const LOCKED_OUT: &str = "Too many wrong passwords. Wait a few minutes and try again.";

#[derive(Default)]
pub(crate) struct PasswordGuard {
    misses: HashMap<IpAddr, (u32, Duration)>,
}

impl PasswordGuard {
    /// Whether this address has used up its guesses for now.
    pub(crate) fn locked(&self, ip: IpAddr, shared: bool, now: Duration) -> bool {
        self.misses.get(&ip).is_some_and(|&(count, since)| {
            now.saturating_sub(since) < WINDOW && count >= tries(shared)
        })
    }

    pub(crate) fn missed(&mut self, ip: IpAddr, now: Duration) {
        if self.misses.len() >= MOST_ADDRESSES && !self.misses.contains_key(&ip) {
            self.misses
                .retain(|_, (_, since)| now.saturating_sub(*since) < WINDOW);
            if self.misses.len() >= MOST_ADDRESSES {
                // Still full of live entries: forget the oldest one.
                if let Some(oldest) = self
                    .misses
                    .iter()
                    .min_by_key(|(_, (_, since))| *since)
                    .map(|(ip, _)| *ip)
                {
                    self.misses.remove(&oldest);
                }
            }
        }
        let entry = self.misses.entry(ip).or_insert((0, now));
        if now.saturating_sub(entry.1) >= WINDOW {
            *entry = (0, now);
        }
        entry.0 = entry.0.saturating_add(1);
    }

    /// The right password clears an address's record, unless the address is
    /// shared: one honest join mustn't refill a stranger's guesses.
    pub(crate) fn passed(&mut self, ip: IpAddr, shared: bool) {
        if !shared {
            self.misses.remove(&ip);
        }
    }
}

fn tries(shared: bool) -> u32 {
    if shared { SHARED_TRIES } else { TRIES }
}

/// Compares two passwords without stopping at the first difference.
pub(crate) fn same_password(given: &str, wanted: &str) -> bool {
    let (given, wanted) = (given.as_bytes(), wanted.as_bytes());
    let mut difference = given.len() ^ wanted.len();
    for index in 0..given.len().max(wanted.len()) {
        let a = given.get(index).copied().unwrap_or(0);
        let b = wanted.get(index).copied().unwrap_or(0);
        difference |= usize::from(a ^ b);
    }
    std::hint::black_box(difference) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(203, 0, 113, 7));
    const AWAY: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(203, 0, 113, 8));

    #[test]
    fn passwords_compare_exactly() {
        assert!(same_password("", ""));
        assert!(same_password("hunter2", "hunter2"));
        assert!(!same_password("hunter2", "hunter3"));
        assert!(!same_password("hunter", "hunter2"));
        assert!(!same_password("hunter2", ""));
        assert!(!same_password("a\0", "a"));
    }

    #[test]
    fn an_address_runs_out_of_guesses_and_gets_them_back_later() {
        let mut guard = PasswordGuard::default();
        let now = Duration::from_secs(100);
        for _ in 0..TRIES - 1 {
            guard.missed(HOME, now);
        }
        assert!(!guard.locked(HOME, false, now));
        guard.missed(HOME, now);
        assert!(guard.locked(HOME, false, now));
        assert!(!guard.locked(AWAY, false, now), "others are unaffected");
        assert!(!guard.locked(HOME, false, now + WINDOW));
        guard.missed(HOME, now + WINDOW);
        assert!(!guard.locked(HOME, false, now + WINDOW), "a new window");
    }

    #[test]
    fn a_shared_relay_gets_slack_and_one_join_never_refills_it() {
        let mut guard = PasswordGuard::default();
        let now = Duration::from_secs(5);
        for _ in 0..TRIES {
            guard.missed(HOME, now);
        }
        assert!(!guard.locked(HOME, true, now));
        for _ in TRIES..SHARED_TRIES {
            guard.missed(HOME, now);
        }
        assert!(guard.locked(HOME, true, now));
        guard.passed(HOME, true);
        assert!(guard.locked(HOME, true, now));
        guard.passed(HOME, false);
        assert!(!guard.locked(HOME, false, now));
    }

    #[test]
    fn a_flood_of_addresses_stays_bounded() {
        let mut guard = PasswordGuard::default();
        let now = Duration::from_secs(1);
        for n in 0..MOST_ADDRESSES as u32 + 500 {
            guard.missed(IpAddr::V4(n.into()), now);
        }
        assert!(guard.misses.len() <= MOST_ADDRESSES);
    }
}

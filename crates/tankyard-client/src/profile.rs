//! Where a frame's time goes, for smoke runs. Off by default: a phase then
//! costs one thread-local read.
use std::cell::RefCell;
use std::time::Instant;

struct Phase {
    name: &'static str,
    seconds: f64,
    calls: u32,
}

thread_local! {
    static PHASES: RefCell<Option<Vec<Phase>>> = const { RefCell::new(None) };
    /// Finishes queued GPU work, so a phase's time includes drawing it.
    static SYNC: std::cell::Cell<Option<fn()>> = const { std::cell::Cell::new(None) };
}

/// Starts collecting phase times on this thread, dropping any so far.
/// `sync`, run before and after each phase, finishes its GPU work.
pub fn enable(sync: Option<fn()>) {
    PHASES.with(|phases| *phases.borrow_mut() = Some(Vec::new()));
    SYNC.with(|cell| cell.set(sync));
}

/// Runs `work`, adding its time to `name` when collecting.
pub fn phase<T>(name: &'static str, work: impl FnOnce() -> T) -> T {
    if PHASES.with(|phases| phases.borrow().is_none()) {
        return work();
    }
    let sync = SYNC.with(std::cell::Cell::get);
    if let Some(sync) = sync {
        sync();
    }
    let start = Instant::now();
    let out = work();
    if let Some(sync) = sync {
        sync();
    }
    let seconds = start.elapsed().as_secs_f64();
    PHASES.with(|phases| {
        if let Some(list) = phases.borrow_mut().as_mut() {
            match list.iter_mut().find(|phase| phase.name == name) {
                Some(phase) => {
                    phase.seconds += seconds;
                    phase.calls += 1;
                }
                None => list.push(Phase {
                    name,
                    seconds,
                    calls: 1,
                }),
            }
        }
    });
    out
}

/// Each phase's milliseconds per frame over `frames`, slowest first.
pub fn report(frames: u64) -> Option<String> {
    PHASES.with(|phases| {
        let phases = phases.borrow();
        let list = phases.as_ref().filter(|list| !list.is_empty())?;
        let mut sorted: Vec<_> = list.iter().collect();
        sorted.sort_by(|a, b| b.seconds.total_cmp(&a.seconds));
        let frames = frames.max(1) as f64;
        Some(
            sorted
                .iter()
                .map(|phase| {
                    format!(
                        "{}={:.2}ms/{}",
                        phase.name,
                        phase.seconds * 1000. / frames,
                        phase.calls
                    )
                })
                .collect::<Vec<_>>()
                .join(" "),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_add_up_only_while_collecting() {
        assert_eq!(phase("idle", || 3), 3);
        assert!(report(1).is_none());
        enable(None);
        phase("work", || {
            std::thread::sleep(std::time::Duration::from_millis(2))
        });
        phase("work", || ());
        let line = report(2).expect("a phase ran");
        assert!(line.starts_with("work="), "{line}");
    }
}

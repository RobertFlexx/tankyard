//! Head-to-head reports: does a change make bots better? Run with
//! `cargo test --release -p tankyard-ai --test arena_report -- --ignored --nocapture`.
use tankyard_ai::arena::{Match, Side, play};
use tankyard_ai::{BotController, Difficulty, Knowledge};
use tankyard_core::Mode;

fn games(seeds: std::ops::Range<u64>) -> Vec<Match> {
    let mut games = Vec::new();
    for seed in seeds {
        for (mode, map) in [
            (Mode::Ffa, "open"),
            (Mode::Ffa, "fortress"),
            (Mode::TwoTeams, "open"),
        ] {
            games.push(Match {
                mode,
                map: map.into(),
                bots: 8,
                ticks: 3 * 3600,
                seed,
            });
        }
    }
    games
}

fn line(name: &str, side: &Side) -> String {
    format!(
        "{name:<10} fitness {:>8.0}  points/min {:>6.0}  kills {:>4}  deaths {:>4}",
        side.fitness(),
        side.points_per_minute(),
        side.kills,
        side.deaths
    )
}

#[test]
#[ignore = "head-to-head report; run with --ignored --nocapture"]
fn shared_memory_against_fresh_bots() {
    let memory = Knowledge::default().shared();
    // Warm up: bots play among themselves, learning into one memory.
    for game in games(100..120) {
        let mut a = BotController::new(Difficulty::Normal);
        let mut b = BotController::new(Difficulty::Normal);
        a.share_knowledge(memory.clone());
        b.share_knowledge(memory.clone());
        play(&game, &mut a, &mut b);
    }
    println!(
        "memory after warm-up: {} tries",
        tankyard_ai::lock_knowledge(&memory).lives()
    );
    let (mut learned, mut fresh) = (Side::default(), Side::default());
    for game in games(1..11) {
        let mut a = BotController::new(Difficulty::Normal);
        a.share_knowledge(memory.clone());
        let mut b = BotController::new(Difficulty::Normal);
        let (x, y) = play(&game, &mut a, &mut b);
        learned.add(&x);
        fresh.add(&y);
    }
    println!("{}", line("memory", &learned));
    println!("{}", line("fresh", &fresh));
}

#[test]
#[ignore = "head-to-head report; run with --ignored --nocapture"]
fn trained_driver_against_hand_written_steering() {
    // Modes the driver never trained in, as well as those it did.
    for (mode, map) in [
        (Mode::Ffa, "open"),
        (Mode::Ffa, "fortress"),
        (Mode::Maze, "open"),
        (Mode::TwoTeams, "open"),
        (Mode::FourTeams, "open"),
        (Mode::Domination, "open"),
        (Mode::Deathmatch, "open"),
        (Mode::Survival, "open"),
        (Mode::CoopPve, "open"),
    ] {
        let (mut driven, mut plain) = (Side::default(), Side::default());
        for seed in 1..7 {
            let game = Match {
                mode,
                map: map.into(),
                bots: 6,
                ticks: 3 * 3600,
                seed,
            };
            let mut a = BotController::new(Difficulty::Normal);
            let mut b = BotController::new(Difficulty::Normal);
            assert!(b.set_driver(None));
            let (x, y) = play(&game, &mut a, &mut b);
            driven.add(&x);
            plain.add(&y);
        }
        println!("{mode:?} {map}");
        println!("  {}", line("driver", &driven));
        println!("  {}", line("plain", &plain));
    }
}

#[test]
#[ignore = "head-to-head report; run with --ignored --nocapture"]
fn control_sides_are_fair() {
    // Identical controllers on both sides should come out even, and memory
    // should help whichever side has it.
    let (mut a_total, mut b_total) = (Side::default(), Side::default());
    for game in games(1..11) {
        let (x, y) = play(
            &game,
            &mut BotController::new(Difficulty::Normal),
            &mut BotController::new(Difficulty::Normal),
        );
        a_total.add(&x);
        b_total.add(&y);
    }
    println!("{}", line("fresh A", &a_total));
    println!("{}", line("fresh B", &b_total));
    let memory = Knowledge::default().shared();
    for game in games(100..120) {
        let mut a = BotController::new(Difficulty::Normal);
        let mut b = BotController::new(Difficulty::Normal);
        a.share_knowledge(memory.clone());
        b.share_knowledge(memory.clone());
        play(&game, &mut a, &mut b);
    }
    let (mut fresh, mut learned) = (Side::default(), Side::default());
    for game in games(1..11) {
        let mut b = BotController::new(Difficulty::Normal);
        b.share_knowledge(memory.clone());
        let (x, y) = play(&game, &mut BotController::new(Difficulty::Normal), &mut b);
        fresh.add(&x);
        learned.add(&y);
    }
    println!("{}", line("fresh A", &fresh));
    println!("{}", line("memory B", &learned));
}

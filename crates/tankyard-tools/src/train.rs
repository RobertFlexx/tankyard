//! Trains the bots' driving network by self-play.
//!
//! Each generation tries pairs of small random changes to the weights, one
//! in each direction. Every try plays the same few matches against bots
//! without the network, and the weights move toward the changes that won
//! (evolution strategies with ranked results and Adam steps). Every few
//! generations the current weights play held-out matches; the best so far is
//! written out as Rust source.
use std::path::PathBuf;
use tankyard_ai::arena::{Match, Side, play};
use tankyard_ai::driver::{DriverNet, initial_weights};
use tankyard_ai::{BotController, Difficulty};
use tankyard_core::Mode;

pub struct Options {
    pub generations: u32,
    pub pairs: usize,
    pub sigma: f32,
    pub rate: f32,
    pub seed: u64,
    pub bots: usize,
    pub ticks: u64,
    pub threads: usize,
    pub out: Option<PathBuf>,
    /// Weights to start from instead of the initial ones.
    pub start: Option<Vec<f32>>,
}

struct Random(u64);
impl Random {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    /// A standard normal sample.
    fn gauss(&mut self) -> f32 {
        let u = self.next().max(1e-12);
        let v = self.next();
        ((-2. * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()) as f32
    }
}

fn games(seed: u64, bots: usize, ticks: u64) -> Vec<Match> {
    // Free-for-all, cover, a maze, teams and objectives, so the driver
    // learns to win in all of them rather than in one.
    [
        (Mode::Ffa, "open"),
        (Mode::Ffa, "fortress"),
        (Mode::Maze, "open"),
        (Mode::TwoTeams, "open"),
        (Mode::FourTeams, "open"),
        (Mode::Domination, "open"),
    ]
    .into_iter()
    .enumerate()
    .map(|(n, (mode, map))| Match {
        mode,
        map: map.into(),
        bots,
        ticks,
        seed: seed.wrapping_mul(31).wrapping_add(n as u64),
    })
    .collect()
}

/// How much better bots driving with `weights` do than bots without the
/// network, over `games`; also the two sides' totals.
fn evaluate(weights: Option<&[f32]>, games: &[Match]) -> (f64, Side, Side) {
    let (mut mine, mut theirs) = (Side::default(), Side::default());
    for game in games {
        let mut a = BotController::new(Difficulty::Normal);
        assert!(a.set_driver(weights));
        let mut b = BotController::new(Difficulty::Normal);
        b.set_driver(None);
        let (x, y) = play(game, &mut a, &mut b);
        mine.add(&x);
        theirs.add(&y);
    }
    (mine.fitness() - theirs.fitness(), mine, theirs)
}

/// Evaluates every candidate on `threads` threads.
fn evaluate_all(candidates: &[Vec<f32>], games: &[Match], threads: usize) -> Vec<f64> {
    let mut results = vec![0.; candidates.len()];
    let chunk = candidates.len().div_ceil(threads.max(1));
    std::thread::scope(|scope| {
        for (slots, work) in results.chunks_mut(chunk).zip(candidates.chunks(chunk)) {
            scope.spawn(move || {
                for (slot, weights) in slots.iter_mut().zip(work) {
                    *slot = evaluate(Some(weights), games).0;
                }
            });
        }
    });
    results
}

/// Held-out matches, played on `threads` threads, for one set of weights.
fn validate(
    weights: Option<&[f32]>,
    seeds: std::ops::Range<u64>,
    options: &Options,
) -> (f64, Side, Side) {
    let all: Vec<Match> = seeds
        .flat_map(|seed| games(seed, options.bots, options.ticks))
        .collect();
    let chunk = all.len().div_ceil(options.threads.max(1));
    let parts: Vec<(f64, Side, Side)> = std::thread::scope(|scope| {
        let handles: Vec<_> = all
            .chunks(chunk)
            .map(|part| scope.spawn(move || evaluate(weights, part)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("validation thread"))
            .collect()
    });
    let (mut mine, mut theirs) = (Side::default(), Side::default());
    for (_, x, y) in &parts {
        mine.add(x);
        theirs.add(y);
    }
    (mine.fitness() - theirs.fitness(), mine, theirs)
}

pub fn rust_source(weights: &[f32]) -> String {
    let rows: Vec<String> = weights
        .chunks(6)
        .map(|row| {
            let values: Vec<String> = row.iter().map(|w| format!("{w:.6}")).collect();
            format!("    {},", values.join(", "))
        })
        .collect();
    format!(
        "//! The driving network's weights, written by `tankyard-tools train-driver`.\n\
         //! Hidden-layer weights come first, eight rows of twelve, then the output's\n\
         //! nine. Change them by training, not by hand.\n\
         pub const WEIGHTS: [f32; {}] = [\n{}\n];\n",
        weights.len(),
        rows.join("\n")
    )
}

pub fn train_driver(options: &Options) -> Result<(), String> {
    let size = DriverNet::WEIGHTS;
    let mut theta = options.start.clone().unwrap_or_else(initial_weights);
    if theta.len() != size || theta.iter().any(|w| !w.is_finite()) {
        return Err(format!("start weights need {size} finite values"));
    }
    let mut random = Random(options.seed.max(1));
    let (mut m, mut v) = (vec![0f32; size], vec![0f32; size]);
    let validation = 9000..9006;
    let (baseline, mine, theirs) = validate(Some(&theta), validation.clone(), options);
    println!(
        "start: lead {baseline:+.0} (driver {:.0} pts/min, {} kills, {} deaths; others {:.0} pts/min, {} kills, {} deaths)",
        mine.points_per_minute(),
        mine.kills,
        mine.deaths,
        theirs.points_per_minute(),
        theirs.kills,
        theirs.deaths
    );
    let mut best = (baseline, theta.clone());
    for generation in 1..=options.generations {
        let noise: Vec<Vec<f32>> = (0..options.pairs)
            .map(|_| (0..size).map(|_| random.gauss()).collect())
            .collect();
        let candidates: Vec<Vec<f32>> = noise
            .iter()
            .flat_map(|e| {
                [1f32, -1.].map(|sign| {
                    theta
                        .iter()
                        .zip(e)
                        .map(|(w, n)| (w + sign * options.sigma * n).clamp(-2., 2.))
                        .collect::<Vec<f32>>()
                })
            })
            .collect();
        let seed = options
            .seed
            .wrapping_mul(1000)
            .wrapping_add(u64::from(generation));
        let results = evaluate_all(
            &candidates,
            &games(seed, options.bots, options.ticks),
            options.threads,
        );
        // Ranks, centered on zero: one lucky match can't dominate a step.
        let mut order: Vec<usize> = (0..results.len()).collect();
        order.sort_by(|&a, &b| results[a].total_cmp(&results[b]));
        let mut rank = vec![0f32; results.len()];
        for (position, &index) in order.iter().enumerate() {
            rank[index] = position as f32 / (results.len() - 1).max(1) as f32 - 0.5;
        }
        let mut gradient = vec![0f32; size];
        for (pair, e) in noise.iter().enumerate() {
            let difference = rank[pair * 2] - rank[pair * 2 + 1];
            for (g, n) in gradient.iter_mut().zip(e) {
                *g += difference * n / (options.pairs as f32 * options.sigma);
            }
        }
        let t = generation as i32;
        for i in 0..size {
            // A little pull toward zero keeps unused weights from wandering.
            let g = gradient[i] - 0.002 * theta[i];
            m[i] = 0.9 * m[i] + 0.1 * g;
            v[i] = 0.999 * v[i] + 0.001 * g * g;
            let m_hat = m[i] / (1. - 0.9f32.powi(t));
            let v_hat = v[i] / (1. - 0.999f32.powi(t));
            theta[i] = (theta[i] + options.rate * m_hat / (v_hat.sqrt() + 1e-8)).clamp(-2., 2.);
        }
        let mean = results.iter().sum::<f64>() / results.len() as f64;
        println!("generation {generation}: mean lead {mean:+.0}");
        if generation % 5 == 0 || generation == options.generations {
            let (lead, mine, theirs) = validate(Some(&theta), validation.clone(), options);
            println!(
                "  held out: lead {lead:+.0} (driver {:.0} pts/min, {} kills, {} deaths, walls {:.1}%, idle {:.1}%; others {:.0} pts/min, walls {:.1}%, idle {:.1}%)",
                mine.points_per_minute(),
                mine.kills,
                mine.deaths,
                mine.scraping_share() * 100.,
                mine.idle_share() * 100.,
                theirs.points_per_minute(),
                theirs.scraping_share() * 100.,
                theirs.idle_share() * 100.
            );
            if lead > best.0 {
                best = (lead, theta.clone());
                if let Some(path) = &options.out {
                    std::fs::write(path, rust_source(&best.1)).map_err(|e| e.to_string())?;
                    println!("  best so far, written to {}", path.display());
                }
            }
        }
    }
    let (lead, mine, theirs) = validate(Some(&best.1), 9100..9112, options);
    let (none, _, _) = validate(None, 9100..9112, options);
    println!(
        "final check on fresh seeds: lead {lead:+.0} with the trained driver, {none:+.0} for identical bots (driver {:.0} pts/min, {} kills, {} deaths, walls {:.1}%, idle {:.1}%; others {:.0} pts/min, {} kills, {} deaths, walls {:.1}%, idle {:.1}%)",
        mine.points_per_minute(),
        mine.kills,
        mine.deaths,
        mine.scraping_share() * 100.,
        mine.idle_share() * 100.,
        theirs.points_per_minute(),
        theirs.kills,
        theirs.deaths,
        theirs.scraping_share() * 100.,
        theirs.idle_share() * 100.
    );
    Ok(())
}

//! What bots learn together and keep between matches: how many points each
//! tank earns, which tanks beat which, and the pooled weights of every bot's
//! build, tactics and motion networks.
//!
//! Only public information goes in: classes, the scoreboard, the kill feed
//! and each bot's own play. Players' stat choices and hidden positions stay
//! unread, as they do for a single bot.
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use tankyard_content::{ClassDef, Content};
use tankyard_core::{EntityId, EventKind, Mode, World};

use crate::neural::{MotionNet, Network};

/// A pair of class names, owned as a matchup's key or borrowed to look one
/// up. Bots ask about matchups constantly and shouldn't copy names to do it.
trait ClassPair {
    fn names(&self) -> (&str, &str);
}
impl ClassPair for (String, String) {
    fn names(&self) -> (&str, &str) {
        (&self.0, &self.1)
    }
}
impl ClassPair for (&str, &str) {
    fn names(&self) -> (&str, &str) {
        *self
    }
}
impl<'a> std::borrow::Borrow<dyn ClassPair + 'a> for (String, String) {
    fn borrow(&self) -> &(dyn ClassPair + 'a) {
        self
    }
}
impl std::hash::Hash for dyn ClassPair + '_ {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // As the tuple of strings hashes, which a borrowed key has to match.
        let (a, b) = self.names();
        a.hash(state);
        b.hash(state);
    }
}
impl PartialEq for dyn ClassPair + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.names() == other.names()
    }
}
impl Eq for dyn ClassPair + '_ {}

/// Bot memory shared by every arena of a server, or every match of a game.
pub type SharedKnowledge = Arc<Mutex<Knowledge>>;

/// Locks shared memory. A panic elsewhere cannot poison what bots know: the
/// data is plain numbers that are valid at every step.
pub fn lock(shared: &SharedKnowledge) -> MutexGuard<'_, Knowledge> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The file format's version. Older or newer files start a fresh memory.
pub const FORMAT: u32 = 1;
/// Saved memories larger than this are refused when loading.
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CLASSES: usize = 1024;
const MAX_MATCHUPS: usize = 8192;
const MAX_ID: usize = 64;
/// Time in one class that counts as a real try of it.
const MIN_SEGMENT_TICKS: u64 = 10 * 60;
/// A death costs this many minutes of play when rating a class.
const DEATH_MINUTES: f32 = 1.0;
/// Minutes of play before a class's own results outweigh its tier's.
const PRIOR_MINUTES: f32 = 3.0;
/// Kills between tanks this many levels apart say little about the tanks.
const MATCHUP_LEVELS: u8 = 6;
/// Older results fade so that balance changes and new strategies show.
const RECORD_LIMIT: f32 = 400.0;

/// Everything bots have seen one class do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClassRecord {
    /// Tries of at least ten seconds.
    pub lives: f32,
    /// Minutes played in it.
    pub minutes: f32,
    /// Points earned in it.
    pub score: f32,
    pub kills: f32,
    pub deaths: f32,
}

impl ClassRecord {
    fn valid(&self) -> bool {
        [
            self.lives,
            self.minutes,
            self.score,
            self.kills,
            self.deaths,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1e9)
    }
    fn fade(&mut self) {
        if self.lives > RECORD_LIMIT {
            let keep = RECORD_LIMIT / self.lives;
            for value in [
                &mut self.lives,
                &mut self.minutes,
                &mut self.score,
                &mut self.kills,
                &mut self.deaths,
            ] {
                *value *= keep;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Matchup {
    /// Times the first class destroyed the second.
    wins: f32,
    /// Times the second class destroyed the first.
    losses: f32,
}

#[derive(Serialize, Deserialize)]
struct SavedMatchup {
    a: String,
    b: String,
    wins: f32,
    losses: f32,
}

/// Blends what one bot's network learned since it took the pooled weights
/// into those weights. The more it trained, the more it moves them, but no
/// single life can drag the pool more than a quarter of the way.
pub(crate) fn pool<const I: usize, const O: usize>(
    shared: &mut Network<I, O>,
    mine: &Network<I, O>,
    adopted: u32,
) {
    let fresh = mine.samples.saturating_sub(adopted);
    if fresh == 0 {
        return;
    }
    let rate = (fresh as f32 / (fresh as f32 + 300.0)).clamp(0.02, 0.25);
    shared.blend_toward(mine, rate);
    shared.samples = shared.samples.saturating_add(fresh);
}

/// The networks every bot starts from, with how much experience each holds.
#[derive(Clone, Default)]
pub(crate) struct SharedModels {
    pub classes: Network<8, 1>,
    pub builds: Network<13, 1>,
    pub tactics: Network<14, 1>,
    pub motion: MotionNet,
    /// Results behind `classes`, `builds` and `tactics`.
    pub class_outcomes: u32,
    pub build_outcomes: u32,
    pub tactic_outcomes: u32,
}

#[derive(Serialize, Deserialize)]
struct SavedModels {
    classes: Vec<f32>,
    builds: Vec<f32>,
    tactics: Vec<f32>,
    motion: Vec<f32>,
    class_outcomes: u32,
    build_outcomes: u32,
    tactic_outcomes: u32,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    format: u32,
    lives: u64,
    classes: BTreeMap<String, ClassRecord>,
    matchups: Vec<SavedMatchup>,
    models: SavedModels,
}

/// One tank's time in its current class, from the public scoreboard.
struct Watch {
    class: String,
    level: u8,
    start: u64,
    score: u32,
    kills: u32,
    deaths: u32,
    last_score: u32,
    last_kills: u32,
}

#[derive(Default)]
pub struct Knowledge {
    classes: HashMap<String, ClassRecord>,
    matchups: HashMap<(String, String), Matchup>,
    pub(crate) models: SharedModels,
    /// Tries recorded since this memory began.
    lives: u64,
    watches: HashMap<EntityId, Watch>,
    observed: Option<u64>,
    /// How far each class leads toward the best tanks, refreshed as results
    /// come in.
    paths: HashMap<String, f32>,
    stale: bool,
    /// Changes since the memory was last saved.
    unsaved: u64,
    /// Learning pauses in Sandbox and modded games, where classes and levels
    /// don't play by the usual rules.
    pub paused: bool,
}

impl Knowledge {
    pub fn shared(self) -> SharedKnowledge {
        Arc::new(Mutex::new(self))
    }

    /// Tries of a class recorded so far, across every class.
    pub fn lives(&self) -> u64 {
        self.lives
    }

    /// Changes learned since the last [`Self::mark_saved`].
    pub fn unsaved(&self) -> u64 {
        self.unsaved
    }

    /// Records that the first `changes` changes are safely written; any
    /// learned while they were being written stay unsaved.
    pub fn mark_saved(&mut self, changes: u64) {
        self.unsaved = self.unsaved.saturating_sub(changes);
    }

    /// The record for one class, if bots have seen it played.
    pub fn record(&self, class: &str) -> Option<ClassRecord> {
        self.classes.get(class).copied()
    }

    /// Watches the scoreboard and kill feed once per tick. Bots call this
    /// themselves; several controllers sharing one memory count a tick once.
    pub fn observe(&mut self, world: &World) {
        if self.observed == Some(world.tick) {
            return;
        }
        self.observed = Some(world.tick);
        if self.paused || world.config.mode == Mode::Sandbox || world.config.modded {
            self.watches.clear();
            return;
        }
        for pair in world.events.windows(2) {
            if matches!(pair[0].kind, EventKind::Destroy)
                && matches!(pair[1].kind, EventKind::Kill)
                && pair[0].tick == pair[1].tick
            {
                self.duel(world, pair[1].subject, pair[0].subject);
            }
        }
        for tank in &world.tanks {
            let ended = self.watches.get(&tank.id).is_some_and(|watch| {
                !tank.alive || tank.deaths != watch.deaths || tank.class != watch.class
            });
            if ended {
                let watch = self.watches.remove(&tank.id).expect("checked above");
                let died = tank.deaths > watch.deaths || !tank.alive;
                self.close(&watch, world.tick, died);
            }
            if !tank.alive {
                continue;
            }
            let watch = self.watches.entry(tank.id).or_insert_with(|| Watch {
                class: tank.class.clone(),
                level: tank.level,
                start: world.tick,
                score: tank.score,
                kills: tank.kills,
                deaths: tank.deaths,
                last_score: tank.score,
                last_kills: tank.kills,
            });
            watch.last_score = tank.score;
            watch.last_kills = tank.kills;
            watch.level = tank.level;
        }
        if self.watches.len() > 512 {
            self.watches
                .retain(|id, _| world.tanks.iter().any(|tank| tank.id == *id));
        }
        if self.stale && world.tick.is_multiple_of(120) {
            self.refresh_paths(&world.content);
        }
    }

    fn duel(&mut self, world: &World, killer: EntityId, victim: EntityId) {
        let (Some(winner), Some(loser)) = (self.watches.get(&killer), self.watches.get(&victim))
        else {
            return;
        };
        if killer == victim
            || winner.level.abs_diff(loser.level) > MATCHUP_LEVELS
            || winner.class == loser.class
        {
            return;
        }
        let winner_team = world.tanks.iter().find(|t| t.id == killer).map(|t| t.team);
        let loser_team = world.tanks.iter().find(|t| t.id == victim).map(|t| t.team);
        if winner_team.is_some() && winner_team == loser_team && winner_team != Some(0) {
            return;
        }
        let (a, b, won) = if winner.class < loser.class {
            (winner.class.clone(), loser.class.clone(), true)
        } else {
            (loser.class.clone(), winner.class.clone(), false)
        };
        if !self.matchups.contains_key(&(a.clone(), b.clone()))
            && self.matchups.len() >= MAX_MATCHUPS
        {
            return;
        }
        let entry = self.matchups.entry((a, b)).or_default();
        if won {
            entry.wins += 1.0;
        } else {
            entry.losses += 1.0;
        }
        let total = entry.wins + entry.losses;
        if total > 200.0 {
            entry.wins *= 200.0 / total;
            entry.losses *= 200.0 / total;
        }
        self.unsaved += 1;
    }

    fn close(&mut self, watch: &Watch, tick: u64, died: bool) {
        let ticks = tick.saturating_sub(watch.start);
        if ticks < MIN_SEGMENT_TICKS && !died {
            return;
        }
        if !self.classes.contains_key(&watch.class)
            && (self.classes.len() >= MAX_CLASSES || watch.class.len() > MAX_ID)
        {
            return;
        }
        let record = self.classes.entry(watch.class.clone()).or_default();
        record.lives += 1.0;
        record.minutes += ticks as f32 / (60.0 * tankyard_core::TICK_RATE as f32);
        record.score += watch.last_score.saturating_sub(watch.score) as f32;
        record.kills += watch.last_kills.saturating_sub(watch.kills) as f32;
        record.deaths += f32::from(u8::from(died));
        record.fade();
        self.lives += 1;
        self.unsaved += 1;
        self.stale = true;
    }

    /// Points a minute that classes of one tier earn together.
    fn tier_rate(&self, content: &Content, tier: u8) -> Option<f32> {
        let (score, minutes) = self
            .classes
            .iter()
            .filter(|(id, _)| content.class(id).is_some_and(|c| c.tier == tier))
            .fold((0.0, 0.0), |(score, minutes), (_, r)| {
                (
                    score + r.score,
                    minutes + r.minutes + r.deaths * DEATH_MINUTES,
                )
            });
        (minutes >= 1.0).then(|| (score / minutes).max(1.0))
    }

    /// How much better than its tier a class earns: the natural log of the
    /// ratio, near 0 until there is evidence. None before its tier has been
    /// played at all.
    pub fn advantage(&self, content: &Content, class: &ClassDef) -> Option<f32> {
        let tier = self.tier_rate(content, class.tier)?;
        let record = self.classes.get(&class.id).copied().unwrap_or_default();
        let minutes = record.minutes + record.deaths * DEATH_MINUTES;
        let rate = (record.score + tier * PRIOR_MINUTES) / (minutes + PRIOR_MINUTES);
        Some((rate / tier).max(1e-3).ln().clamp(-1.5, 1.5))
    }

    /// How far choosing `class` leads toward the strongest tanks: its own
    /// advantage, then the best of what it upgrades to.
    pub fn path_value(&self, class: &ClassDef) -> f32 {
        self.paths.get(&class.id).copied().unwrap_or_default()
    }

    /// How sure the memory is about `class`: 0 with no tries, nearing 1.
    pub fn certainty(&self, class: &ClassDef) -> f32 {
        let lives = self.classes.get(&class.id).map_or(0.0, |r| r.lives);
        lives / (lives + 6.0)
    }

    fn refresh_paths(&mut self, content: &Content) {
        self.stale = false;
        self.paths.clear();
        fn value(
            knowledge: &Knowledge,
            content: &Content,
            class: &ClassDef,
            depth: u8,
            paths: &mut HashMap<String, f32>,
        ) -> f32 {
            if let Some(&known) = paths.get(&class.id) {
                return known;
            }
            let own = knowledge.advantage(content, class).unwrap_or_default();
            let best_next = (depth < 8)
                .then(|| {
                    class
                        .upgrades
                        .iter()
                        .filter_map(|id| content.class(id))
                        .map(|next| value(knowledge, content, next, depth + 1, paths))
                        .fold(None, |best: Option<f32>, v| {
                            Some(best.map_or(v, |b| b.max(v)))
                        })
                })
                .flatten();
            let total = best_next.map_or(own, |next| own * 0.4 + next * 0.6);
            paths.insert(class.id.clone(), total);
            total
        }
        let mut paths = HashMap::new();
        for class in &content.classes {
            value(self, content, class, 0, &mut paths);
        }
        self.paths = paths;
    }

    /// How `mine` fares against `theirs` in fair fights, from -1 (it loses
    /// them all) to 1 (it wins them all); 0 without evidence.
    pub fn matchup(&self, mine: &str, theirs: &str) -> f32 {
        let (key, flip) = if mine < theirs {
            ((mine, theirs), false)
        } else {
            ((theirs, mine), true)
        };
        let Some(entry) = self.matchups.get(&key as &dyn ClassPair) else {
            return 0.0;
        };
        let lead = (entry.wins - entry.losses) / (entry.wins + entry.losses + 4.0);
        if flip { -lead } else { lead }
    }

    /// The memory as JSON, for saving.
    pub fn to_json(&self) -> String {
        let models = &self.models;
        let saved = Saved {
            format: FORMAT,
            lives: self.lives,
            classes: self
                .classes
                .iter()
                .map(|(id, record)| (id.clone(), *record))
                .collect(),
            matchups: self
                .matchups
                .iter()
                .map(|((a, b), m)| SavedMatchup {
                    a: a.clone(),
                    b: b.clone(),
                    wins: m.wins,
                    losses: m.losses,
                })
                .collect(),
            models: SavedModels {
                classes: models.classes.weights(),
                builds: models.builds.weights(),
                tactics: models.tactics.weights(),
                motion: models.motion.weights(),
                class_outcomes: models.class_outcomes,
                build_outcomes: models.build_outcomes,
                tactic_outcomes: models.tactic_outcomes,
            },
        };
        serde_json::to_string(&saved).unwrap_or_default()
    }

    /// A saved memory. Anything malformed, oversized or from another format
    /// is refused whole, so a damaged file can only cost what bots learned.
    pub fn from_json(text: &str, content: &Content) -> Result<Self, String> {
        if text.len() > MAX_FILE_BYTES {
            return Err("bot memory is too large".into());
        }
        let saved: Saved =
            serde_json::from_str(text).map_err(|e| format!("bot memory is unreadable: {e}"))?;
        if saved.format != FORMAT {
            return Err(format!(
                "bot memory format {} is not {FORMAT}",
                saved.format
            ));
        }
        if saved.classes.len() > MAX_CLASSES || saved.matchups.len() > MAX_MATCHUPS {
            return Err("bot memory holds too many entries".into());
        }
        let mut knowledge = Self {
            lives: saved.lives,
            ..Default::default()
        };
        for (id, record) in saved.classes {
            if id.len() > MAX_ID || !record.valid() {
                return Err("bot memory holds an invalid class record".into());
            }
            knowledge.classes.insert(id, record);
        }
        for m in saved.matchups {
            let sane = |v: f32| v.is_finite() && (0.0..=1e6).contains(&v);
            if m.a.len() > MAX_ID
                || m.b.len() > MAX_ID
                || m.a >= m.b
                || !sane(m.wins)
                || !sane(m.losses)
            {
                return Err("bot memory holds an invalid matchup".into());
            }
            knowledge.matchups.insert(
                (m.a, m.b),
                Matchup {
                    wins: m.wins,
                    losses: m.losses,
                },
            );
        }
        let models = &mut knowledge.models;
        let loaded = models.classes.set_weights(&saved.models.classes)
            && models.builds.set_weights(&saved.models.builds)
            && models.tactics.set_weights(&saved.models.tactics)
            && models.motion.set_weights(&saved.models.motion);
        if !loaded {
            return Err("bot memory holds invalid network weights".into());
        }
        models.class_outcomes = saved.models.class_outcomes;
        models.build_outcomes = saved.models.build_outcomes;
        models.tactic_outcomes = saved.models.tactic_outcomes;
        knowledge.refresh_paths(content);
        Ok(knowledge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::{GameEvent, V2, WorldConfig};

    fn event(tick: u64, kind: EventKind, subject: EntityId) -> GameEvent {
        GameEvent {
            tick,
            kind,
            pos: V2::ZERO,
            subject,
            value: 0,
            team: 0,
            elimination: None,
        }
    }

    fn world() -> World {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Ffa,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("A", true);
        world.add_player("B", true);
        world
    }

    /// Plays `minutes` of one class earning `score`, then a death.
    fn life(knowledge: &mut Knowledge, world: &mut World, class: &str, score: u32, minutes: u64) {
        world.tanks[0].class = class.into();
        world.tanks[0].alive = true;
        world.tick += 1;
        knowledge.observe(world);
        world.tick += minutes * 3600;
        world.tanks[0].score += score;
        knowledge.observe(world);
        world.tanks[0].deaths += 1;
        world.tick += 1;
        knowledge.observe(world);
    }

    #[test]
    fn classes_that_earn_more_lead_their_tier_and_paths_lead_toward_them() {
        let mut world = world();
        let content = world.content.clone();
        let mut knowledge = Knowledge::default();
        // Two tier-two siblings: one earns far more for the same time.
        let base = content.class("vector").unwrap();
        let (strong, weak) = (&base.upgrades[0], &base.upgrades[1]);
        assert_eq!(
            content.class(strong).unwrap().tier,
            content.class(weak).unwrap().tier
        );
        for _ in 0..8 {
            life(&mut knowledge, &mut world, strong, 6000, 2);
            life(&mut knowledge, &mut world, weak, 1500, 2);
        }
        let strong = content.class(strong).unwrap();
        let weak = content.class(weak).unwrap();
        let a = knowledge.advantage(&content, strong).unwrap();
        let b = knowledge.advantage(&content, weak).unwrap();
        assert!(a > 0.2 && b < -0.2, "advantages {a} and {b}");
        knowledge.refresh_paths(&content);
        assert!(knowledge.path_value(strong) > knowledge.path_value(weak));
        assert!(knowledge.certainty(strong) > 0.5);
        assert_eq!(knowledge.lives(), 16);
        // Short tries that end without a death are not evidence.
        world.tanks[0].class = strong.id.clone();
        knowledge.observe(&world);
        world.tanks[0].class = weak.id.clone();
        world.tick += 60;
        knowledge.observe(&world);
        assert_eq!(knowledge.lives(), 16);
    }

    #[test]
    fn a_borrowed_pair_of_names_finds_the_owned_matchup() {
        let mut knowledge = Knowledge::default();
        knowledge.matchups.insert(
            ("alpha".to_owned(), "beta".to_owned()),
            Matchup {
                wins: 6.,
                losses: 1.,
            },
        );
        let lead = knowledge.matchup("alpha", "beta");
        assert!(lead > 0.);
        assert_eq!(
            knowledge.matchup("beta", "alpha"),
            -lead,
            "and the other way"
        );
        assert_eq!(knowledge.matchup("alpha", "gamma"), 0.);
        // Where one name ends is part of the key.
        assert_eq!(knowledge.matchup("alph", "abeta"), 0.);
    }

    #[test]
    fn kills_between_similar_levels_teach_matchups() {
        let mut world = world();
        let mut knowledge = Knowledge::default();
        world.tanks[0].class = "anchor".into();
        world.tanks[1].class = "hive".into();
        knowledge.observe(&world);
        let (a, b) = (world.tanks[0].id, world.tanks[1].id);
        for i in 0..10 {
            world.tick += 1;
            let (killer, victim) = if i < 8 { (a, b) } else { (b, a) };
            world.events = vec![
                event(world.tick, EventKind::Destroy, victim),
                event(world.tick, EventKind::Kill, killer),
            ];
            knowledge.observe(&world);
        }
        let lead = knowledge.matchup("anchor", "hive");
        assert!(lead > 0.3, "lead {lead}");
        assert_eq!(knowledge.matchup("hive", "anchor"), -lead);
        assert_eq!(knowledge.matchup("anchor", "vector"), 0.0);
        // A big level gap is not a fair fight.
        world.events.clear();
        world.tanks[0].level = 40;
        world.tick += 1;
        knowledge.observe(&world);
        world.tick += 1;
        world.events = vec![
            event(world.tick, EventKind::Destroy, b),
            event(world.tick, EventKind::Kill, a),
        ];
        knowledge.observe(&world);
        assert_eq!(knowledge.matchup("anchor", "hive"), lead);
    }

    #[test]
    fn memory_survives_a_save_and_refuses_damaged_files() {
        let mut world = world();
        let content = world.content.clone();
        let mut knowledge = Knowledge::default();
        life(&mut knowledge, &mut world, "vector", 900, 3);
        knowledge.models.class_outcomes = 7;
        knowledge.models.builds.train([0.5; 13], [0.8]);
        let text = knowledge.to_json();
        let loaded = Knowledge::from_json(&text, &content).unwrap();
        assert_eq!(loaded.record("vector"), knowledge.record("vector"));
        assert_eq!(loaded.models.class_outcomes, 7);
        assert_eq!(
            loaded.models.builds.predict([0.5; 13]),
            knowledge.models.builds.predict([0.5; 13])
        );
        assert!(Knowledge::from_json("{", &content).is_err());
        assert!(
            Knowledge::from_json(&text.replace("\"format\":1", "\"format\":9"), &content).is_err()
        );
        let poisoned = text.replacen("\"minutes\":", "\"minutes\":-", 1);
        assert!(Knowledge::from_json(&poisoned, &content).is_err());
        assert!(Knowledge::from_json(&" ".repeat(MAX_FILE_BYTES + 1), &content).is_err());
    }

    #[test]
    fn sandbox_and_modded_games_teach_nothing() {
        let mut world = world();
        world.config.mode = Mode::Sandbox;
        let mut knowledge = Knowledge::default();
        life(&mut knowledge, &mut world, "vector", 9000, 3);
        world.config.mode = Mode::Ffa;
        world.config.modded = true;
        life(&mut knowledge, &mut world, "vector", 9000, 3);
        assert_eq!(knowledge.lives(), 0);
        assert_eq!(knowledge.unsaved(), 0);
    }
}

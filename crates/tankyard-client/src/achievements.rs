//! Local cosmetic achievements based on authoritative player state.
use std::collections::VecDeque;
use tankyard_content::Content;
use tankyard_core::{EntityId, MatchState, Mode, Snapshot, Tank, World, WorldConfig};

#[derive(Clone, Copy)]
pub enum Goal {
    Games(u32),
    Kills(u32),
    Score(u32),
    Level(u8),
    Tier(u8),
    Points(u8),
    Survival(u64),
}
pub struct Achievement {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub goal: Goal,
}
pub static ACHIEVEMENTS: &[Achievement] = &[
    Achievement {
        id: "first_round",
        title: "First round",
        description: "Play a round outside Sandbox.",
        goal: Goal::Games(1),
    },
    Achievement {
        id: "score_100",
        title: "100 score",
        description: "Earn 100 score in a round.",
        goal: Goal::Score(100),
    },
    Achievement {
        id: "first_kill",
        title: "First kill",
        description: "Destroy another tank.",
        goal: Goal::Kills(1),
    },
    Achievement {
        id: "level_15",
        title: "Level 15",
        description: "Reach level 15.",
        goal: Goal::Level(15),
    },
    Achievement {
        id: "first_upgrade",
        title: "First upgrade",
        description: "Choose an upgraded tank.",
        goal: Goal::Tier(1),
    },
    Achievement {
        id: "level_30",
        title: "Level 30",
        description: "Reach level 30.",
        goal: Goal::Level(30),
    },
    Achievement {
        id: "score_10000",
        title: "10,000 score",
        description: "Earn 10,000 score in a round.",
        goal: Goal::Score(10_000),
    },
    Achievement {
        id: "level_45",
        title: "Level 45",
        description: "Reach the maximum level.",
        goal: Goal::Level(45),
    },
    Achievement {
        id: "full_build",
        title: "Full build",
        description: "Spend all 33 skill points.",
        goal: Goal::Points(33),
    },
    Achievement {
        id: "ten_kills",
        title: "Ten kills",
        description: "Destroy ten tanks in total.",
        goal: Goal::Kills(10),
    },
    Achievement {
        id: "five_minutes",
        title: "Five minutes",
        description: "Survive five minutes in one life.",
        goal: Goal::Survival(18_000),
    },
    Achievement {
        id: "ten_rounds",
        title: "Ten rounds",
        description: "Play ten rounds.",
        goal: Goal::Games(10),
    },
    Achievement {
        id: "score_50000",
        title: "50,000 score",
        description: "Earn 50,000 score in a round.",
        goal: Goal::Score(50_000),
    },
    Achievement {
        id: "hundred_kills",
        title: "100 kills",
        description: "Destroy 100 tanks in total.",
        goal: Goal::Kills(100),
    },
];
#[derive(Default, Clone, Copy)]
pub struct Progress {
    pub games: u32,
    pub kills: u32,
    pub score: u32,
    pub level: u8,
    pub tier: u8,
    pub points: u8,
    pub alive_ticks: u64,
}
impl Achievement {
    pub fn earned(&self, p: Progress) -> bool {
        match self.goal {
            Goal::Games(n) => p.games >= n,
            Goal::Kills(n) => p.kills >= n,
            Goal::Score(n) => p.score >= n,
            Goal::Level(n) => p.level >= n,
            Goal::Tier(n) => p.tier >= n,
            Goal::Points(n) => p.points >= n,
            Goal::Survival(n) => p.alive_ticks >= n,
        }
    }
}
#[derive(Default)]
pub struct Tracker {
    player: EntityId,
    round: u32,
    deaths: u32,
    born_at: u64,
    last_tick: u64,
    alive: bool,
    peak_score: u32,
    observed_rounds: u32,
    completed_kills: u32,
    current_kills: u32,
    highest_score: u32,
    progress: Option<Progress>,
}
impl Tracker {
    pub fn observe(
        &mut self,
        snapshot: &Snapshot,
        player: EntityId,
        content: &Content,
        games: u32,
        kills: u32,
    ) -> Option<Progress> {
        self.track(
            snapshot.tanks.iter().find(|tank| tank.id == player),
            &snapshot.config,
            &snapshot.match_state,
            snapshot.tick,
            content,
        )?;
        self.progress(games, kills)
    }
    /// Observe the authoritative local world without cloning a snapshot.
    pub fn observe_world(&mut self, world: &World, player: EntityId) -> Option<Progress> {
        self.track(
            world.tanks.iter().find(|tank| tank.id == player),
            &world.config,
            &world.match_state,
            world.tick,
            &world.content,
        )
    }
    pub fn suspend(&mut self) {
        self.alive = false;
        self.progress = None;
    }
    fn track(
        &mut self,
        tank: Option<&Tank>,
        config: &WorldConfig,
        state: &MatchState,
        tick: u64,
        content: &Content,
    ) -> Option<Progress> {
        let Some(tank) = tank.filter(|tank| tank.id != 0 && !tank.bot) else {
            self.suspend();
            return None;
        };
        if config.mode == Mode::Sandbox || config.modded {
            self.suspend();
            return None;
        }
        // A session owns one human tank per round. Delayed snapshots and an
        // ID belonging to another player cannot replace that ownership.
        if self.player != 0
            && (state.round < self.round
                || (state.round == self.round && (tick < self.last_tick || self.player != tank.id)))
        {
            return None;
        }
        let new_round = self.player == 0 || self.round != state.round;
        if new_round {
            self.completed_kills = self.completed_kills.saturating_add(self.current_kills);
            self.current_kills = 0;
            self.observed_rounds = self.observed_rounds.saturating_add(1);
            self.peak_score = 0;
        }
        self.current_kills = self.current_kills.max(tank.kills);
        if new_round || self.deaths != tank.deaths || (!self.alive && tank.alive) {
            self.born_at = tick;
        }
        self.player = tank.id;
        self.round = state.round;
        self.deaths = tank.deaths;
        self.last_tick = tick;
        self.alive = tank.alive;
        self.peak_score = self.peak_score.max(tank.score);
        self.highest_score = self.highest_score.max(tank.score);
        self.progress = Some(Progress {
            games: self.observed_rounds,
            kills: self.completed_kills.saturating_add(self.current_kills),
            score: self.peak_score,
            level: tank.level,
            tier: content.class(&tank.class).map_or(0, |c| c.tier),
            points: tank.stats.iter().copied().fold(0, u8::saturating_add),
            alive_ticks: if tank.alive {
                tick.saturating_sub(self.born_at)
            } else {
                0
            },
        });
        self.progress
    }
    pub fn progress(&self, games: u32, kills: u32) -> Option<Progress> {
        self.progress.map(|progress| Progress {
            games: games.saturating_add(progress.games),
            kills: kills.saturating_add(progress.kills),
            ..progress
        })
    }
    pub fn highest_score(&self) -> u32 {
        self.highest_score
    }
    pub fn session_totals(&self) -> (u32, u32) {
        (
            self.observed_rounds,
            self.completed_kills.saturating_add(self.current_kills),
        )
    }
}
pub struct Feed {
    queue: VecDeque<&'static Achievement>,
    age: f32,
}
impl Default for Feed {
    fn default() -> Self {
        Self {
            queue: VecDeque::with_capacity(ACHIEVEMENTS.len()),
            age: 0.0,
        }
    }
}
impl Feed {
    pub fn push(&mut self, a: &'static Achievement) {
        if self.queue.len() < ACHIEVEMENTS.len() {
            self.queue.push_back(a);
        }
    }
    pub fn update(&mut self, dt: f32) {
        if self.queue.is_empty() {
            self.age = 0.0;
            return;
        }
        self.age += dt.clamp(0.0, 0.1);
        if self.age >= 4.5 {
            self.queue.pop_front();
            self.age = 0.0;
        }
    }
    pub fn current(&self) -> Option<(&'static Achievement, f32)> {
        self.queue.front().map(|a| (*a, self.age))
    }
}
pub fn unlock(progress: Progress, unlocked: &mut Vec<String>, feed: &mut Feed) -> bool {
    let mut changed = false;
    for a in ACHIEVEMENTS {
        if a.earned(progress) && !unlocked.iter().any(|id| id == a.id) {
            unlocked.push(a.id.into());
            feed.push(a);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::{World, WorldConfig};

    #[test]
    fn bot_stats_cannot_unlock_achievements_or_enter_session_records() {
        let content = Content::builtin();
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            content.clone(),
        );
        let bot = world.add_player("Pilot", true);
        let player = world.add_player("Pilot", false);
        world.tick = 20_000;
        world.tanks[0].kills = 100;
        world.tanks[0].score = 50_000;
        world.tanks[0].level = 45;
        world.tanks[0].class = "fork".into();
        world.tanks[0].stats = [7, 7, 7, 7, 5, 0, 0, 0];
        let mut tracker = Tracker::default();
        assert!(
            tracker
                .observe(&world.snapshot(), bot, &content, 0, 0)
                .is_none()
        );
        assert_eq!(tracker.session_totals(), (0, 0));
        let progress = tracker
            .observe(&world.snapshot(), player, &content, 0, 0)
            .unwrap();
        let mut ids = vec![];
        unlock(progress, &mut ids, &mut Feed::default());
        assert_eq!(ids, ["first_round"]);
        assert_eq!(tracker.session_totals(), (1, 0));
    }

    #[test]
    fn other_players_and_stale_snapshots_cannot_replace_local_progress() {
        let content = Content::builtin();
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            content.clone(),
        );
        let player = world.add_player("Pilot", false);
        let other = world.add_player("Other", false);
        let mut tracker = Tracker::default();
        world.tick = 100;
        world.tanks[0].kills = 2;
        tracker.observe(&world.snapshot(), player, &content, 0, 0);
        world.tanks[1].kills = 100;
        assert!(
            tracker
                .observe(&world.snapshot(), other, &content, 0, 0)
                .is_none()
        );
        assert_eq!(tracker.session_totals(), (1, 2));

        let mut old = world.snapshot();
        old.tick = 99;
        old.tanks[0].kills = 200;
        assert!(tracker.observe(&old, player, &content, 0, 0).is_none());
        world.match_state.round += 1;
        world.tick = 200;
        world.tanks[0].kills = 1;
        let next_round = world.snapshot();
        assert_eq!(
            tracker
                .observe(&next_round, player, &content, 0, 0)
                .unwrap()
                .kills,
            3
        );
        old.tick = 201;
        assert!(tracker.observe(&old, player, &content, 0, 0).is_none());
        assert_eq!(tracker.session_totals(), (2, 3));
    }

    #[test]
    fn missing_or_ineligible_player_state_breaks_the_survival_streak() {
        let content = Content::builtin();
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            content.clone(),
        );
        let player = world.add_player("Pilot", false);
        for excluded in ["missing", "bot", "sandbox", "modded"] {
            let mut tracker = Tracker::default();
            let valid = world.snapshot();
            tracker.observe(&valid, player, &content, 0, 0);
            let mut invalid = valid.clone();
            invalid.tick = 18_000;
            match excluded {
                "missing" => invalid.tanks.clear(),
                "bot" => invalid.tanks[0].bot = true,
                "sandbox" => invalid.config.mode = Mode::Sandbox,
                "modded" => invalid.config.modded = true,
                _ => unreachable!(),
            }
            assert!(tracker.observe(&invalid, player, &content, 0, 0).is_none());
            let mut resumed = valid;
            resumed.tick = 18_001;
            let progress = tracker.observe(&resumed, player, &content, 0, 0).unwrap();
            assert_eq!(progress.alive_ticks, 0, "{excluded}");
            assert_eq!(tracker.session_totals(), (1, 0));
        }
    }
    #[test]
    fn unlocks_once_and_profile_roundtrips() {
        let mut ids = vec![];
        let mut feed = Feed::default();
        let progress = Progress {
            games: 1,
            kills: 1,
            score: 100,
            level: 15,
            ..Default::default()
        };
        assert!(unlock(progress, &mut ids, &mut feed));
        assert!(!unlock(progress, &mut ids, &mut feed));
        assert!(ids.iter().any(|id| id == "first_kill"));
        let saved = serde_json::to_string(&ids).unwrap();
        let mut loaded: Vec<String> = serde_json::from_str(&saved).unwrap();
        assert!(!unlock(progress, &mut loaded, &mut Feed::default()));
    }
    #[test]
    fn sandbox_does_not_unlock_and_death_resets_survival_time() {
        let content = Content::builtin();
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            content.clone(),
        );
        let player = world.add_player("Pilot", false);
        let mut tracker = Tracker::default();
        tracker.observe(&world.snapshot(), player, &content, 0, 0);
        world.tick = 18001;
        assert!(
            tracker
                .observe(&world.snapshot(), player, &content, 0, 0)
                .unwrap()
                .alive_ticks
                >= 18000
        );
        world.tanks[0].deaths = 1;
        world.tanks[0].alive = false;
        assert_eq!(
            tracker
                .observe(&world.snapshot(), player, &content, 0, 0)
                .unwrap()
                .alive_ticks,
            0
        );
        world.tick += 200;
        world.tanks[0].alive = true;
        assert_eq!(
            tracker
                .observe(&world.snapshot(), player, &content, 0, 0)
                .unwrap()
                .alive_ticks,
            0
        );
        world.config.mode = Mode::Sandbox;
        assert!(
            tracker
                .observe(&world.snapshot(), player, &content, 0, 0)
                .is_none()
        );
    }
    #[test]
    fn popups_are_queued_and_expire() {
        let mut feed = Feed::default();
        feed.push(&ACHIEVEMENTS[0]);
        feed.push(&ACHIEVEMENTS[1]);
        for _ in 0..46 {
            feed.update(0.1);
        }
        assert_eq!(feed.current().unwrap().0.id, "score_100");
        for _ in 0..46 {
            feed.update(0.1);
        }
        assert!(feed.current().is_none());
    }
    #[test]
    fn server_round_restarts_preserve_lifetime_kills_without_counting_deaths_twice() {
        let content = Content::builtin();
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            content.clone(),
        );
        let player = world.add_player("Pilot", false);
        let mut tracker = Tracker::default();
        world.tanks[0].kills = 8;
        tracker.observe(&world.snapshot(), player, &content, 3, 2);
        world.tanks[0].deaths = 1;
        tracker.observe(&world.snapshot(), player, &content, 3, 2);
        assert_eq!(tracker.session_totals(), (1, 8));
        world.match_state.round += 1;
        world.tanks[0].kills = 2;
        let progress = tracker
            .observe(&world.snapshot(), player, &content, 3, 2)
            .unwrap();
        assert_eq!(progress.games, 5);
        assert_eq!(progress.kills, 12);
        assert_eq!(tracker.session_totals(), (2, 10));
    }
}

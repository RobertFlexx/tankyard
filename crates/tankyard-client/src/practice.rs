use serde::{Deserialize, Serialize};
use tankyard_core::{EntityId, Mode, Snapshot, WorldConfig};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    Score(u32),
    Kills(u32),
    Survive,
    Boss,
}
#[derive(Clone, Copy)]
pub struct Challenge {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub tip: &'static str,
    pub class: &'static str,
    pub level: u8,
    pub bots: usize,
    pub seconds: u32,
    pub goal: Goal,
}
pub const CHALLENGES: [Challenge; 8] = [
    Challenge {
        id: "shape_sprint",
        title: "Shape sprint",
        description: "Earn 500 score in 90 seconds. One life, no enemy tanks.",
        tip: "Keep moving and choose a useful upgrade at level 15.",
        class: "vector",
        level: 1,
        bots: 0,
        seconds: 90,
        goal: Goal::Score(500),
    },
    Challenge {
        id: "drone_drill",
        title: "Drone drill",
        description: "Earn 1,500 score with an Overlord in two minutes.",
        tip: "Hold fire to guide drones. Right click recalls them.",
        class: "flock",
        level: 45,
        bots: 0,
        seconds: 120,
        goal: Goal::Score(1500),
    },
    Challenge {
        id: "skirmish",
        title: "Skirmish",
        description: "Destroy three tanks in three minutes. Everyone starts at level 30.",
        tip: "Use cover, watch your health, and spend your skill points.",
        class: "fork",
        level: 30,
        bots: 4,
        seconds: 180,
        goal: Goal::Kills(3),
    },
    Challenge {
        id: "last_stand",
        title: "Last stand",
        description: "Survive two minutes or be the last tank standing as the arena closes.",
        tip: "Farm early and keep an escape route toward the center.",
        class: "vector",
        level: 1,
        bots: 7,
        seconds: 120,
        goal: Goal::Survive,
    },
    Challenge {
        id: "parry_lab",
        title: "Counter school",
        description: "Counter three rushers in two minutes with the Counterguard.",
        tip: "Time the ability just before a Smasher touches your hull. A counter removes half its health.",
        class: "counterguard",
        level: 45,
        bots: 3,
        seconds: 120,
        goal: Goal::Kills(3),
    },
    Challenge {
        id: "stasis_lab",
        title: "Frozen front",
        description: "Win four duels in three minutes with the Chronarch.",
        tip: "Catch a volley, move the field toward your opponent, and let the outward wave do the work.",
        class: "chronarch",
        level: 55,
        bots: 5,
        seconds: 180,
        goal: Goal::Kills(4),
    },
    Challenge {
        id: "boss_hunt",
        title: "Sentinel hunt",
        description: "Defeat the Sentinel in four minutes. One life, a complete Reprisal build.",
        tip: "Watch the boss's white warning ring. Parry the heavy shots and dodge the second attack.",
        class: "reprisal",
        level: 55,
        bots: 0,
        seconds: 240,
        goal: Goal::Boss,
    },
    Challenge {
        id: "trap_fort",
        title: "Hold the line",
        description: "Stop three rushers in three minutes with the Triple Trapper.",
        tip: "Build a field before engaging. Your tougher traps protect several approaches at once.",
        class: "tri_bastion",
        level: 45,
        bots: 4,
        seconds: 180,
        goal: Goal::Kills(3),
    },
];
impl Challenge {
    pub fn config(self, seed: u64) -> WorldConfig {
        WorldConfig {
            mode: if self.goal == Goal::Survive {
                Mode::Survival
            } else {
                Mode::Sandbox
            },
            mode_name: Some(self.title.into()),
            map: match self.goal {
                Goal::Kills(_) => "competitive",
                Goal::Boss => "boss",
                Goal::Survive => "survival",
                _ => "open",
            }
            .into(),
            // Preset loadouts and practice results stay out of normal achievements.
            modded: true,
            seed,
            width: 2400.,
            height: 1800.,
            target_count: 130,
            spawn_protection: 15.,
            time_limit: self.seconds as f32,
            ..Default::default()
        }
    }
    pub fn target(self) -> u32 {
        match self.goal {
            Goal::Score(score) => score,
            Goal::Kills(kills) => kills,
            Goal::Survive => self.seconds * 60,
            Goal::Boss => 1,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Playing,
    Completed,
    Failed,
}
pub struct Run {
    pub index: usize,
    pub status: Status,
    pub ticks: u32,
    pub progress: u32,
    pub recorded: bool,
    player: EntityId,
    round: u32,
    start: u64,
    score: u32,
    kills: u32,
    boss: Option<EntityId>,
}
impl Run {
    pub fn new(index: usize, snapshot: &Snapshot, player: EntityId) -> Option<Self> {
        CHALLENGES.get(index)?;
        let tank = snapshot
            .tanks
            .iter()
            .find(|tank| tank.id == player && !tank.bot)?;
        Some(Self {
            index,
            status: Status::Playing,
            ticks: 0,
            progress: 0,
            recorded: false,
            player,
            round: snapshot.match_state.round,
            start: snapshot.tick,
            score: tank.score,
            kills: tank.kills,
            boss: snapshot.bosses.first().map(|boss| boss.id),
        })
    }
    pub fn challenge(&self) -> Challenge {
        CHALLENGES[self.index]
    }
    pub fn observe(&mut self, snapshot: &Snapshot) {
        if self.status != Status::Playing {
            return;
        }
        let Some(tank) = snapshot
            .tanks
            .iter()
            .find(|tank| tank.id == self.player && !tank.bot)
        else {
            self.status = Status::Failed;
            return;
        };
        if snapshot.match_state.round != self.round || snapshot.tick < self.start {
            self.status = Status::Failed;
            return;
        }
        let challenge = self.challenge();
        self.ticks = snapshot
            .tick
            .saturating_sub(self.start)
            .min(u64::from(challenge.seconds * 60)) as u32;
        let progress = match challenge.goal {
            Goal::Score(_) => tank.score.saturating_sub(self.score),
            Goal::Kills(_) => tank.kills.saturating_sub(self.kills),
            Goal::Survive => self.ticks,
            Goal::Boss => u32::from(
                self.boss
                    .is_some_and(|id| !snapshot.bosses.iter().any(|b| b.id == id && b.health > 0.)),
            ),
        };
        self.progress = self.progress.max(progress).min(challenge.target());
        if !tank.alive {
            self.status = Status::Failed;
        } else if self.progress >= challenge.target()
            || (challenge.goal == Goal::Survive && snapshot.match_state.finished)
        {
            self.status = Status::Completed;
        } else if self.ticks >= challenge.seconds * 60 {
            self.status = Status::Failed;
        }
    }
    pub fn done(&self) -> bool {
        self.status != Status::Playing
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub attempts: u32,
    pub completed: u32,
    pub best_ticks: u32,
    pub best_progress: u32,
}
pub fn record(records: &mut Vec<Record>, run: &mut Run) {
    if run.recorded {
        return;
    }
    run.recorded = true;
    if run.ticks == 0 {
        return;
    }
    let challenge = run.challenge();
    let index = records
        .iter()
        .position(|record| record.id == challenge.id)
        .unwrap_or_else(|| {
            records.push(Record {
                id: challenge.id.into(),
                attempts: 0,
                completed: 0,
                best_ticks: 0,
                best_progress: 0,
            });
            records.len() - 1
        });
    let record = &mut records[index];
    record.attempts = record.attempts.saturating_add(1);
    record.best_progress = record.best_progress.max(run.progress);
    if run.status == Status::Completed {
        record.completed = record.completed.saturating_add(1);
        if record.best_ticks == 0 || run.ticks < record.best_ticks {
            record.best_ticks = run.ticks;
        }
    }
}
pub fn clock(ticks: u32) -> String {
    format!("{}:{:02}", ticks / 3600, ticks / 60 % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::World;
    fn setup(index: usize) -> (World, Run) {
        let mut world = World::new(
            CHALLENGES[index].config(42),
            tankyard_content::Content::builtin(),
        );
        let player = world.add_player("Player", false);
        let run = Run::new(index, &world.snapshot(), player).unwrap();
        (world, run)
    }
    #[test]
    fn practice_only_counts_the_bound_player_and_records_a_result_once() {
        let (mut world, mut run) = setup(0);
        world.add_player("Bot", true);
        world.tanks[1].score = 10000;
        world.tick = 60;
        run.observe(&world.snapshot());
        assert!(run.status == Status::Playing);
        world.tanks[0].score += 500;
        world.tick = 120;
        run.observe(&world.snapshot());
        assert!(run.status == Status::Completed);
        let mut records = vec![];
        record(&mut records, &mut run);
        record(&mut records, &mut run);
        assert_eq!(records[0].completed, 1);
        assert_eq!(records[0].attempts, 1);
        assert_eq!(records[0].best_ticks, 120);
    }
    #[test]
    fn death_timeout_and_round_changes_cannot_complete_a_run() {
        for reason in 0..3 {
            let (mut world, mut run) = setup(0);
            world.tick = 6000;
            if reason == 0 {
                world.tanks[0].alive = false;
            }
            if reason == 2 {
                world.match_state.round += 1;
            }
            run.observe(&world.snapshot());
            assert!(run.status == Status::Failed);
        }
        let (mut world, mut run) = setup(3);
        world.tick = 7200;
        run.observe(&world.snapshot());
        assert!(run.status == Status::Completed);
    }
}

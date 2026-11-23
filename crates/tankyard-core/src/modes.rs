//! Mode objectives and seeded maps share the same combat simulation.
use crate::physics::circle_rect;
use crate::*;
/// Half-second tries a Boss Rush boss gets to find somewhere to land.
const RUSH_LANDING_TRIES: u8 = 20;

impl World {
    pub(crate) fn initialize_mode(&mut self) {
        match self.config.mode {
            Mode::Infection => self.initialize_infection(),
            Mode::KingOfTheHill => self.initialize_hill(),
            Mode::Domination => {
                let positions = if self.map.objective_positions.is_empty() {
                    vec![[0.25, 0.5], [0.5, 0.5], [0.75, 0.5]]
                } else {
                    self.map.objective_positions.clone()
                };
                for position in positions {
                    let pos = V2::new(
                        self.config.width * position[0],
                        self.config.height * position[1],
                    );
                    let id = self.id();
                    self.objectives.push(Objective {
                        id,
                        pos,
                        radius: self.map_object_scale() * 130.,
                        team: 0,
                        progress: 0.,
                        contested: false,
                        carrier: None,
                        home: pos,
                        return_timer: 0.,
                    });
                }
            }
            Mode::CaptureCore => {
                for team in 1..=2 {
                    let pos = self.team_home(team);
                    let id = self.id();
                    self.objectives.push(Objective {
                        id,
                        pos,
                        radius: 45.,
                        team,
                        progress: 0.,
                        contested: false,
                        carrier: None,
                        home: pos,
                        return_timer: 0.,
                    });
                }
            }
            Mode::Mothership => {
                for team in 1..=2 {
                    let kind = if team == 1 {
                        BossKind::Sentinel
                    } else {
                        BossKind::Foundry
                    };
                    let id = self.spawn_boss(kind);
                    let pos = self.team_home(team);
                    if let Some(b) = self.bosses.iter_mut().find(|b| b.id == id) {
                        b.team = team;
                        b.mothership = true;
                        b.pos = pos;
                        b.health = 9000.;
                        b.max_health = 9000.;
                        b.radius = 115.;
                    }
                }
            }
            Mode::BossRush => {
                self.next_rush_boss();
            }
            Mode::Survival => {
                self.match_state.safe_radius = self.config.width.min(self.config.height) * 0.55;
            }
            Mode::RandomEvents => {
                self.mode_timer = 10.0;
                self.match_state.arena.remaining = self.mode_timer;
                self.match_state.announcement = "Next event in 10s".into();
            }
            Mode::CoopPve => {
                self.mode_timer = 8.0;
                self.match_state.arena.intermission = true;
                self.match_state.arena.remaining = self.mode_timer;
                self.match_state.announcement = "First wave in 8s".into();
            }
            _ => {}
        }
    }
    /// Bring on the next boss of the Boss Rush; false when none are left.
    fn next_rush_boss(&mut self) -> bool {
        let lineup = self.config.rush_lineup();
        let total = lineup.len();
        let Some(&kind) = lineup.get(usize::from(self.match_state.rush_wave)) else {
            return false;
        };
        let wave = self.match_state.rush_wave + 1;
        // Its health depends on the wave, so count it before it lands.
        self.match_state.rush_wave = wave;
        if self.spawn_boss(kind) == 0 {
            // A boss won't land on a tank, so give the ground a moment to
            // clear. A map with no room for it at all ends the round plainly.
            self.match_state.rush_wave = wave - 1;
            self.match_state.rush_stalls += 1;
            if self.match_state.rush_stalls >= RUSH_LANDING_TRIES {
                self.finish(
                    None,
                    &format!("No room for the {} on this map", kind.name()),
                );
            } else {
                self.mode_timer = 0.5;
                self.match_state.announcement = format!("Clear the way for the {}", kind.name());
            }
            return true;
        }
        self.match_state.rush_stalls = 0;
        self.match_state.boss_wave = wave.min(SHOWN_RUSH_WAVES);
        self.match_state.announcement = format!("Boss wave {wave} / {total} · {}", kind.name());
        true
    }
    pub(crate) fn finish(&mut self, winner: Option<u8>, reason: &str) {
        if self.match_state.finished {
            return;
        }
        self.match_state.finished = true;
        self.match_state.winner = winner;
        self.match_state.announcement = match winner {
            Some(0) => reason.into(),
            Some(team) => format!("Team {team} wins - {reason}"),
            None => format!("Draw - {reason}"),
        };
        self.event(
            EventKind::Victory,
            V2::new(self.config.width * 0.5, self.config.height * 0.5),
            0,
            winner.unwrap_or(0) as u32,
            winner.unwrap_or(0),
        );
    }
    pub(crate) fn update_mode(&mut self) {
        match self.config.mode {
            Mode::Infection => self.update_infection(),
            Mode::KingOfTheHill => {
                self.update_hill();
                self.domination();
            }
            Mode::Domination => self.domination(),
            Mode::CaptureCore => self.capture_core(),
            Mode::RandomEvents => self.update_arena_events(),
            Mode::CoopPve => self.update_coop_pve(),
            Mode::Survival => {
                let elapsed = (self.match_state.elapsed - 10.).max(0.);
                self.match_state.safe_radius = (self.config.width.min(self.config.height)
                    * 0.55
                    * (1. - elapsed / self.config.time_limit))
                    .max(80.);
                let center = V2::new(self.config.width * 0.5, self.config.height * 0.5);
                for i in 0..self.tanks.len() {
                    if self.tanks[i].alive
                        && self.tanks[i].pos.distance(center) > self.match_state.safe_radius
                    {
                        self.damage_tank(i, 18. * DT, 0, V2::ZERO);
                    }
                }
                let living: Vec<_> = self
                    .tanks
                    .iter()
                    .filter(|t| t.alive || t.lives > 0)
                    .collect();
                if self.match_state.elapsed > 5. && self.tanks.len() > 1 && living.len() <= 1 {
                    let name = living
                        .first()
                        .map_or("No one", |t| t.name.as_str())
                        .to_owned();
                    self.finish(Some(0), &format!("{name} survives"));
                }
            }
            Mode::Tag => {
                if self.match_state.elapsed > 5. && self.tanks.len() > 1 {
                    let team = self.tanks[0].team;
                    if self.tanks.iter().all(|t| t.team == team) {
                        self.finish(Some(team), "Every tank converted");
                    }
                }
            }
            Mode::BossRush if self.bosses.iter().all(|b| b.health <= 0.) => {
                self.mode_timer -= DT;
                if self.mode_timer <= 0. && !self.next_rush_boss() {
                    let reason = match self.config.rush_lineup().len() {
                        1 => "Boss defeated".to_owned(),
                        6 => "All six bosses defeated".to_owned(),
                        total => format!("All {total} bosses defeated"),
                    };
                    self.finish(Some(1), &reason);
                }
            }
            _ => {}
        }
        if self.match_state.finished {
            return;
        }
        if matches!(
            self.config.mode,
            Mode::TwoTeams
                | Mode::FourTeams
                | Mode::Domination
                | Mode::Deathmatch
                | Mode::CaptureCore
                | Mode::KingOfTheHill
        ) {
            if let Some(i) = self
                .match_state
                .scores
                .iter()
                .position(|s| *s >= self.config.score_limit)
            {
                self.finish(Some(i as u8 + 1), "Score limit reached");
            } else if self.match_state.elapsed >= self.config.time_limit {
                let max = *self.match_state.scores.iter().max().unwrap_or(&0);
                let candidates: Vec<_> = self
                    .match_state
                    .scores
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| **s == max)
                    .map(|(i, _)| i)
                    .collect();
                self.finish(
                    if candidates.len() == 1 {
                        Some(candidates[0] as u8 + 1)
                    } else {
                        None
                    },
                    "Time limit",
                );
            }
        }
    }
    fn domination(&mut self) {
        let mut rewards = Vec::new();
        let mut captures = Vec::new();
        for o in &mut self.objectives {
            let mut counts = [0u16; 4];
            for t in &self.tanks {
                if t.tangible() && t.team > 0 && t.pos.distance(o.pos) < o.radius {
                    counts[t.team as usize - 1] += 1;
                }
            }
            let present: Vec<_> = counts.iter().enumerate().filter(|(_, n)| **n > 0).collect();
            o.contested = present.len() > 1;
            if present.len() == 1 {
                let (team, count) = present[0];
                let team = team as u8 + 1;
                let rate = DT * (*count as f32).min(3.) / 4.;
                if o.team == team {
                    o.progress = (o.progress + rate).min(1.);
                } else {
                    o.progress -= rate;
                    if o.progress <= -1. {
                        o.team = team;
                        o.progress = 1.;
                        captures.push((o.pos, o.id, team));
                        for t in &self.tanks {
                            if t.tangible() && t.team == team && t.pos.distance(o.pos) < o.radius {
                                rewards.push(t.id);
                            }
                        }
                    }
                }
            }
            if o.team > 0
                && !o.contested
                && (self.config.mode != Mode::KingOfTheHill || counts[o.team as usize - 1] > 0)
            {
                self.score_fraction[o.team as usize - 1] += DT;
            }
        }
        for i in 0..4 {
            while self.score_fraction[i] >= 1. {
                self.score_fraction[i] -= 1.;
                self.match_state.scores[i] += 1;
            }
        }
        for id in rewards {
            self.award(id, 800, 800);
        }
        for (pos, id, team) in captures {
            self.event(EventKind::Capture, pos, id, team as u32, team);
        }
    }
    fn capture_core(&mut self) {
        let homes: [V2; 4] = std::array::from_fn(|i| self.team_home(i as u8 + 1));
        let core_home: [bool; 4] = std::array::from_fn(|i| {
            self.objectives.iter().any(|o| {
                o.team == i as u8 + 1 && o.carrier.is_none() && o.pos.distance(o.home) < 1.
            })
        });
        for t in &mut self.tanks {
            t.carrying = false;
        }
        let mut rewards = Vec::new();
        let mut captures = Vec::new();
        for o in &mut self.objectives {
            if let Some(carrier) = o.carrier {
                if let Some(t) = self
                    .tanks
                    .iter_mut()
                    .find(|t| t.id == carrier && t.tangible())
                {
                    o.pos = t.pos;
                    t.carrying = true;
                    let home = homes[t.team as usize - 1];
                    if t.pos.distance(home) < 90. && core_home[t.team as usize - 1] {
                        rewards.push((t.id, t.team));
                        captures.push((home, o.id, t.team));
                        o.carrier = None;
                        o.pos = o.home;
                        t.carrying = false;
                    }
                } else {
                    o.carrier = None;
                    o.return_timer = 15.;
                }
            } else {
                if o.pos.distance(o.home) > 1. {
                    o.return_timer -= DT;
                    if o.return_timer <= 0. {
                        o.pos = o.home;
                    }
                }
                let mut pickup = None;
                for t in &self.tanks {
                    if !t.tangible() || t.pos.distance(o.pos) > o.radius + t.radius {
                        continue;
                    }
                    if t.team == o.team {
                        if o.pos.distance(o.home) > 1. {
                            o.pos = o.home;
                            o.return_timer = 0.;
                        }
                    } else {
                        pickup = Some(t.id);
                        break;
                    }
                }
                if let Some(id) = pickup {
                    o.carrier = Some(id);
                }
            }
        }
        for (id, team) in rewards {
            self.match_state.scores[team as usize - 1] += 1;
            self.award(id, 2200, 2200);
        }
        for (pos, id, team) in captures {
            self.event(EventKind::Capture, pos, id, team as u32, team);
        }
    }
    /// Check static navigation clearance without depending on renderer geometry.
    pub fn position_clear(&self, pos: V2, radius: f32) -> bool {
        self.config.bounds().contains_circle(pos, radius)
            && self
                .walls
                .iter()
                .all(|&w| circle_rect(pos, radius, w).is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;

    #[test]
    fn a_blocked_rush_retries_without_skipping_bosses_and_eventually_draws() {
        let blocked = || {
            let mut world = world(Mode::Sandbox);
            world.config.mode = Mode::BossRush;
            world.config.boss_rush = vec![BossKind::Trapper, BossKind::Prism];
            world.walls = vec![world.config.bounds()];
            world.initialize_mode();
            world
        };
        let mut retry = blocked();
        assert_eq!(retry.match_state.rush_wave, 0);
        assert_eq!(retry.match_state.rush_stalls, 1);
        assert!(retry.match_state.announcement.contains("Clear the way"));
        retry.walls.clear();
        for _ in 0..40 {
            retry.update_mode();
        }
        assert_eq!(retry.bosses[0].kind, BossKind::Trapper);
        assert_eq!(retry.match_state.rush_wave, 1);
        assert_eq!(retry.match_state.rush_stalls, 0);
        assert!(!retry.match_state.finished);

        let mut impossible = blocked();
        for _ in 0..700 {
            impossible.update_mode();
            if impossible.match_state.finished {
                break;
            }
        }
        assert!(impossible.match_state.finished);
        assert_eq!(impossible.match_state.winner, None);
        assert_eq!(impossible.match_state.rush_wave, 0);
        assert!(impossible.bosses.is_empty());
        assert!(impossible.match_state.announcement.contains("No room"));
        let stalls = impossible.match_state.rush_stalls;
        for _ in 0..1000 {
            impossible.step();
        }
        assert_eq!(impossible.match_state.rush_stalls, stalls);
    }

    fn world(mode: Mode) -> World {
        World::new(
            WorldConfig {
                mode,
                target_count: 0,
                ..WorldConfig::default()
            },
            Content::builtin(),
        )
    }
    #[test]
    fn all_modes_start_and_tick() {
        for mode in Mode::ALL {
            let mut w = world(mode);
            w.add_player("a", false);
            w.add_player("b", true);
            for _ in 0..60 {
                w.step();
            }
            assert_eq!(w.tick, 60, "{}", mode.name());
            assert!(w.tanks.iter().all(|t| t.pos.finite()));
        }
    }
    #[test]
    fn domination_captures_scores_and_finishes() {
        let mut w = world(Mode::Domination);
        let a = w.add_player("a", false);
        w.add_player("b", false);
        w.config.score_limit = 3;
        let pos = w.objectives[0].pos;
        w.tanks.iter_mut().find(|t| t.id == a).unwrap().pos = pos;
        for _ in 0..800 {
            w.step();
            if w.match_state.finished {
                break;
            }
        }
        assert_eq!(w.objectives[0].team, 1);
        assert!(w.match_state.finished);
        assert_eq!(w.match_state.winner, Some(1));
    }
    #[test]
    fn survival_finishes_with_one_remaining() {
        let mut w = world(Mode::Survival);
        w.add_player("a", false);
        w.add_player("b", false);
        w.tanks[1].alive = false;
        w.tanks[1].lives = 0;
        for _ in 0..310 {
            w.step();
        }
        assert!(w.match_state.finished);
    }
    #[test]
    fn tag_converts_and_wins() {
        let mut w = world(Mode::Tag);
        let id = w.add_player("a", false);
        w.add_player("b", false);
        w.tanks[1].health = 0.;
        w.tanks[1].last_attacker = id;
        w.step();
        assert_eq!(w.tanks[1].team, 1);
        for _ in 0..310 {
            w.step();
        }
        assert_eq!(w.match_state.winner, Some(1));
    }
    #[test]
    fn capture_core_transports_and_scores() {
        let mut w = world(Mode::CaptureCore);
        w.add_player("a", false);
        w.add_player("b", false);
        w.config.score_limit = 1;
        w.tanks[0].pos = w.objectives[1].home;
        w.step();
        assert_eq!(w.objectives[1].carrier, Some(w.tanks[0].id));
        w.tanks[0].pos = w.team_home(1);
        w.step();
        assert!(w.match_state.finished);
    }
    #[test]
    fn seeded_maze_connected() {
        let w = world(Mode::Maze);
        assert!(!w.walls.is_empty());
        let second = world(Mode::Maze);
        assert_eq!(w.walls.len(), second.walls.len());
        for (a, b) in w.walls.iter().zip(second.walls.iter()) {
            assert_eq!(a.x, b.x);
            assert_eq!(a.y, b.y);
        }
        let step = 50.;
        let cols = (w.config.width / step) as usize;
        let rows = (w.config.height / step) as usize;
        let mut seen = vec![false; cols * rows];
        let start = (0..seen.len())
            .find(|&i| {
                w.position_clear(
                    V2::new(
                        (i % cols) as f32 * step + step / 2.,
                        (i / cols) as f32 * step + step / 2.,
                    ),
                    24.,
                )
            })
            .unwrap();
        let mut queue = std::collections::VecDeque::from([start]);
        seen[start] = true;
        while let Some(i) = queue.pop_front() {
            let x = i % cols;
            let y = i / cols;
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx >= cols || ny >= rows {
                    continue;
                }
                let n = ny * cols + nx;
                if !seen[n]
                    && w.position_clear(
                        V2::new(nx as f32 * step + step / 2., ny as f32 * step + step / 2.),
                        24.,
                    )
                {
                    seen[n] = true;
                    queue.push_back(n);
                }
            }
        }
        let reached = seen.iter().filter(|&&v| v).count();
        let free = (0..seen.len())
            .filter(|&i| {
                w.position_clear(
                    V2::new(
                        (i % cols) as f32 * step + step / 2.,
                        (i / cols) as f32 * step + step / 2.,
                    ),
                    24.,
                )
            })
            .count();
        assert_eq!(
            reached, free,
            "all navigable cells should share a connected component"
        );
    }
}

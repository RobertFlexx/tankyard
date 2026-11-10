//! The parts of a custom arena that act during a round: pads under tanks,
//! groups that triggers show and hide, announcements and boss arrivals.
//! Pads only read positions, so clients predict their own speed with the
//! same functions the server uses.
use crate::*;
use tankyard_content::{MapDef, MapZone, TriggerAction, TriggerWhen, ZoneKind};

/// A normalized map rectangle in world units, following animated borders the
/// same way walls do.
pub fn map_rect(config: &WorldConfig, r: [f32; 4]) -> Rect {
    let bounds = config.bounds();
    let scale = bounds.w / config.width.max(1.);
    Rect {
        x: bounds.x + r[0] * config.width * scale,
        y: bounds.y + r[1] * config.height * scale,
        w: r[2] * config.width * scale,
        h: r[3] * config.height * scale,
    }
}

fn contains(rect: Rect, p: V2) -> bool {
    p.x >= rect.x && p.y >= rect.y && p.x <= rect.x + rect.w && p.y <= rect.y + rect.h
}

/// The shown pads whose area holds `pos`.
pub fn zones_at<'a>(
    map: &'a MapDef,
    config: &'a WorldConfig,
    hidden: u32,
    pos: V2,
) -> impl Iterator<Item = &'a MapZone> + 'a {
    map.zones.iter().filter(move |zone| {
        MapDef::group_bit(zone.group) & hidden == 0 && contains(map_rect(config, zone.rect), pos)
    })
}

/// How much faster (above 1) or slower a tank moves at `pos`.
pub fn terrain_factor(map: &MapDef, config: &WorldConfig, hidden: u32, pos: V2) -> f32 {
    if map.zones.is_empty() {
        return 1.;
    }
    zones_at(map, config, hidden, pos)
        .fold(1., |factor, zone| match zone.kind {
            ZoneKind::Speed => factor * (1. + 0.45 * zone.strength),
            ZoneKind::Mud => factor / (1. + 0.8 * zone.strength),
            _ => factor,
        })
        .clamp(0.3, 2.5)
}

/// Damage taken at `pos` as a fraction; Shelter pads reduce it.
pub fn shelter_factor(map: &MapDef, config: &WorldConfig, hidden: u32, pos: V2) -> f32 {
    zones_at(map, config, hidden, pos)
        .filter(|zone| zone.kind == ZoneKind::Shelter)
        .fold(1., |factor, zone| factor / (1. + 1.5 * zone.strength))
}

/// Per-round trigger bookkeeping; a new round starts it afresh.
#[derive(Clone, Debug, Default)]
pub(crate) struct TriggerState {
    /// When each trigger may next fire, in round seconds.
    next: Vec<f32>,
    spent: Vec<bool>,
    inside: Vec<std::collections::BTreeSet<EntityId>>,
    /// Groups to set back: when, which group, and whether it ends hidden.
    reverts: Vec<(f32, u8, bool)>,
}

impl World {
    pub(crate) fn map_hidden(&self) -> u32 {
        self.match_state.hidden_groups
    }

    /// Rebuild the walls from the static geometry and the blocks of every
    /// shown group, carving team bases as at the start of the round.
    pub(crate) fn compose_walls(&mut self) {
        let hidden = self.map_hidden();
        self.map_walls.clone_from(&self.static_walls);
        let (width, height) = (self.config.width, self.config.height);
        let blocks: Vec<Rect> = self
            .map
            .blocks
            .iter()
            .filter(|block| MapDef::group_bit(block.group) & hidden == 0)
            .map(|block| Rect {
                x: block.rect[0] * width,
                y: block.rect[1] * height,
                w: block.rect[2] * width,
                h: block.rect[3] * height,
            })
            .collect();
        let mut bases = Vec::new();
        let original_scale = self.config.arena_scale;
        self.config.arena_scale = 1.;
        for team in 1..=self.config.mode.teams() {
            if let Some(base) = self.config.team_base(team) {
                bases.push(base);
            }
        }
        self.config.arena_scale = original_scale;
        for block in blocks {
            let mut pieces = vec![block];
            for &base in &bases {
                pieces = pieces
                    .into_iter()
                    .flat_map(|piece| crate::map_layout::outside_base(piece, base))
                    .collect();
            }
            self.map_walls.extend(pieces);
        }
        self.walls.clone_from(&self.map_walls);
        self.scale_map_walls();
    }

    /// Pads: speed for movement, then repair and hazard over time.
    pub(crate) fn update_zones(&mut self) {
        if self.map.zones.is_empty() {
            for tank in &mut self.tanks {
                tank.terrain = 1.;
            }
            return;
        }
        let hidden = self.map_hidden();
        for index in 0..self.tanks.len() {
            let tank = &self.tanks[index];
            if !tank.alive {
                continue;
            }
            let pos = tank.pos;
            let terrain = terrain_factor(&self.map, &self.config, hidden, pos);
            let (mut repair, mut hazard) = (0., 0.);
            for zone in zones_at(&self.map, &self.config, hidden, pos) {
                match zone.kind {
                    ZoneKind::Repair => repair += 0.06 * zone.strength,
                    ZoneKind::Hazard => hazard += 0.08 * zone.strength,
                    _ => {}
                }
            }
            let tank = &mut self.tanks[index];
            tank.terrain = terrain;
            if repair > 0. && tank.health < tank.max_health {
                tank.health = (tank.health + tank.max_health * repair * DT).min(tank.max_health);
            }
            if hazard > 0. {
                let amount = tank.max_health * hazard * DT + 6. * DT;
                self.damage_tank(index, amount, 0, V2::ZERO);
            }
        }
    }

    /// Fire due triggers and apply reverts; rebuild walls if groups changed.
    pub(crate) fn run_triggers(&mut self) {
        let count = self.map.triggers.len();
        if count == 0 && self.trigger_state.reverts.is_empty() {
            return;
        }
        let now = self.match_state.elapsed;
        if self.trigger_state.next.len() != count {
            let started = (now - DT).max(0.);
            self.trigger_state = TriggerState {
                next: self
                    .map
                    .triggers
                    .iter()
                    .map(|trigger| match trigger.when {
                        TriggerWhen::Every => started + trigger.offset + trigger.seconds,
                        TriggerWhen::Enter => 0.,
                        TriggerWhen::Start => started + trigger.seconds,
                    })
                    .collect(),
                spent: vec![false; count],
                inside: vec![Default::default(); count],
                reverts: Vec::new(),
            };
        }
        let before = self.map_hidden();
        let mut hidden = before;
        self.trigger_state.reverts.retain(|&(at, group, hide)| {
            if now < at {
                return true;
            }
            let bit = MapDef::group_bit(group);
            hidden = if hide { hidden | bit } else { hidden & !bit };
            false
        });
        self.match_state.hidden_groups = hidden;
        for index in 0..count {
            let trigger = &self.map.triggers[index];
            // Track every crossing even during cooldown. Standing on a trigger
            // must not repeatedly activate it, and another tank may enter while
            // the first one stays inside.
            let entered = if trigger.when == TriggerWhen::Enter {
                let area = map_rect(&self.config, trigger.rect);
                let previous = &mut self.trigger_state.inside[index];
                let current: std::collections::BTreeSet<_> = self
                    .tanks
                    .iter()
                    .filter(|tank| tank.alive && contains(area, tank.pos))
                    .map(|tank| tank.id)
                    .collect();
                let entered = current.iter().any(|id| !previous.contains(id));
                *previous = current;
                entered
            } else {
                false
            };
            if self.trigger_state.spent[index] || now < self.trigger_state.next[index] {
                continue;
            }
            let fire = match trigger.when {
                TriggerWhen::Every => {
                    // Catch up without firing twice in one tick.
                    let period = trigger.seconds.max(0.5);
                    let missed = ((now - self.trigger_state.next[index]) / period).floor() + 1.;
                    self.trigger_state.next[index] += missed * period;
                    true
                }
                TriggerWhen::Start => true,
                TriggerWhen::Enter => {
                    if entered {
                        self.trigger_state.next[index] = now + trigger.seconds.max(0.25);
                    }
                    entered
                }
            };
            if !fire {
                continue;
            }
            if trigger.once || trigger.when == TriggerWhen::Start {
                self.trigger_state.spent[index] = true;
            }
            self.fire_trigger(index, now);
        }
        if self.map_hidden() != before {
            self.compose_walls();
        }
    }

    fn fire_trigger(&mut self, index: usize, now: f32) {
        let trigger = &self.map.triggers[index];
        let bit = MapDef::group_bit(trigger.group);
        let was_hidden = self.map_hidden() & bit != 0;
        let hide = match trigger.action {
            TriggerAction::Toggle => !was_hidden,
            TriggerAction::Show => false,
            TriggerAction::Hide => true,
            TriggerAction::Announce => {
                self.match_state.announcement = trigger.text.trim().to_owned();
                return;
            }
            TriggerAction::Boss => {
                if let Some(kind) = BossKind::from_id(&trigger.boss) {
                    let at = map_rect(&self.config, trigger.rect);
                    self.spawn_boss_near(kind, V2::new(at.x + at.w * 0.5, at.y + at.h * 0.5));
                }
                return;
            }
        };
        // Explicit actions supersede any pending restoration of this group.
        let group = trigger.group;
        self.trigger_state.reverts.retain(|&(_, g, _)| g != group);
        if trigger.duration > 0. && hide != was_hidden {
            self.trigger_state
                .reverts
                .push((now + trigger.duration, group, was_hidden));
        }
        let hidden = self.map_hidden();
        self.match_state.hidden_groups = if hide { hidden | bit } else { hidden & !bit };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::{MapBlock, MapTrigger};

    fn world_with(edit: impl FnOnce(&mut MapDef)) -> World {
        let mut maps = tankyard_content::builtin_maps();
        edit(&mut maps[0]);
        tankyard_content::validate_maps(&maps).unwrap();
        let config = WorldConfig {
            target_count: 0,
            ..WorldConfig::default()
        };
        World::with_maps_for_tests(config, maps)
    }

    #[test]
    fn timed_gates_open_and_close_and_rebuild_walls() {
        let mut world = world_with(|map| {
            map.blocks.push(MapBlock {
                rect: [0.1, 0.1, 0.05, 0.05],
                style: Default::default(),
                color: None,
                group: 3,
            });
            map.triggers.push(MapTrigger {
                when: TriggerWhen::Every,
                action: TriggerAction::Toggle,
                rect: [0.1, 0.1, 0.05, 0.05],
                group: 3,
                seconds: 1.,
                offset: 0.,
                duration: 0.,
                text: String::new(),
                boss: String::new(),
                once: false,
            });
        });
        let walls = world.walls.len();
        for _ in 0..61 {
            world.step();
        }
        assert_eq!(world.match_state.hidden_groups, 1 << 2);
        assert_eq!(world.walls.len(), walls - 1);
        for _ in 0..60 {
            world.step();
        }
        assert_eq!(world.match_state.hidden_groups, 0);
        assert_eq!(world.walls.len(), walls);
    }

    #[test]
    fn pads_change_speed_only_inside_shown_areas() {
        let mut map = tankyard_content::builtin_maps().remove(0);
        map.zones.push(MapZone {
            kind: ZoneKind::Speed,
            rect: [0.4, 0.4, 0.2, 0.2],
            strength: 1.,
            group: 2,
        });
        let config = WorldConfig::default();
        let inside = V2::new(config.width * 0.5, config.height * 0.5);
        assert!(terrain_factor(&map, &config, 0, inside) > 1.4);
        assert_eq!(terrain_factor(&map, &config, 0, V2::new(10., 10.)), 1.);
        assert_eq!(terrain_factor(&map, &config, 1 << 1, inside), 1.);
    }
    #[test]
    fn map_boss_ids_match_the_simulation_catalog() {
        assert_eq!(BossKind::ALL.map(BossKind::id), tankyard_content::BOSS_IDS);
    }
    #[test]
    fn entry_triggers_require_a_crossing_and_map_changes_reset_their_state() {
        let mut world = world_with(|map| {
            map.triggers.push(MapTrigger {
                when: TriggerWhen::Enter,
                action: TriggerAction::Toggle,
                rect: [0.4, 0.4, 0.2, 0.2],
                group: 2,
                seconds: 0.,
                offset: 0.,
                duration: 0.,
                text: String::new(),
                boss: String::new(),
                once: false,
            });
        });
        let id = world.add_player("Pilot", false);
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        world.tanks.iter_mut().find(|t| t.id == id).unwrap().pos = center;
        world.run_triggers();
        assert_eq!(world.match_state.hidden_groups, 2);
        for _ in 0..90 {
            world.match_state.elapsed += DT;
            world.run_triggers();
        }
        assert_eq!(
            world.match_state.hidden_groups, 2,
            "standing inside cannot toggle repeatedly"
        );
        world.tanks[0].pos = V2::new(200., 200.);
        world.run_triggers();
        world.tanks[0].pos = center;
        world.run_triggers();
        assert_eq!(world.match_state.hidden_groups, 0);
        world.change_map(world.map.clone()).unwrap();
        world.run_triggers();
        assert_eq!(
            world.match_state.hidden_groups, 2,
            "new map has a fresh entry state"
        );
    }
}

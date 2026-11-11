//! The parts of a custom arena that act during a round: pads under tanks and
//! the trigger logic that shows and hides groups, signals, counts, posts
//! announcements and brings shapes and bosses. [`MapLogic`] is the one rule
//! engine for servers, offline games and the editor's logic preview, and pads
//! are pure functions of position, so clients predict their own movement with
//! the server's arithmetic.
use crate::*;
use tankyard_content::{
    CONVEYOR_SPEED, COUNTER_LIMIT, MAP_COUNTERS, MapDef, MapZone, TRIGGER_SHAPES, TriggerAction,
    TriggerWhen, TriggerWho, ZoneKind,
};

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
/// A normalized map point in world units, as [`map_rect`] places rectangles.
pub fn map_point(config: &WorldConfig, p: [f32; 2]) -> V2 {
    let r = map_rect(config, [p[0], p[1], 0., 0.]);
    V2::new(r.x, r.y)
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

fn speed_of(factor: f32, zone: &MapZone) -> f32 {
    match zone.kind {
        ZoneKind::Speed => factor * (1. + 0.45 * zone.strength),
        ZoneKind::Mud => factor / (1. + 0.8 * zone.strength),
        _ => factor,
    }
}
fn push_of(zone: &MapZone) -> V2 {
    if zone.kind == ZoneKind::Conveyor {
        V2::from_angle(zone.angle.to_radians()) * CONVEYOR_SPEED * zone.strength
    } else {
        V2::ZERO
    }
}

/// How much faster (above 1) or slower a tank moves at `pos`.
pub fn terrain_factor(map: &MapDef, config: &WorldConfig, hidden: u32, pos: V2) -> f32 {
    if map.zones.is_empty() {
        return 1.;
    }
    zones_at(map, config, hidden, pos)
        .fold(1., speed_of)
        .clamp(0.3, 2.5)
}

/// The velocity conveyors at `pos` add, in world units per second.
pub fn terrain_push(map: &MapDef, config: &WorldConfig, hidden: u32, pos: V2) -> V2 {
    if map.zones.is_empty() {
        return V2::ZERO;
    }
    zones_at(map, config, hidden, pos)
        .fold(V2::ZERO, |sum, zone| sum + push_of(zone))
        .clamp_length(600.)
}

/// Damage taken at `pos` as a fraction; Shelter pads reduce it.
pub fn shelter_factor(map: &MapDef, config: &WorldConfig, hidden: u32, pos: V2) -> f32 {
    zones_at(map, config, hidden, pos)
        .filter(|zone| zone.kind == ZoneKind::Shelter)
        .fold(1., |factor, zone| factor / (1. + 1.5 * zone.strength))
}

/// A living tank, as map logic sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Occupant {
    pub id: EntityId,
    pub pos: V2,
    pub bot: bool,
    pub team: u8,
}

/// What map logic asks the arena to do. Group visibility and counters change
/// inside [`MapLogic`] itself.
#[derive(Clone, Debug, PartialEq)]
pub enum LogicEffect {
    /// Text with counter placeholders already filled in.
    Announce(String),
    Boss {
        kind: BossKind,
        at: V2,
    },
    Shapes {
        count: u32,
        area: Rect,
    },
}

/// Trigger actions per step, so signals that fire each other stop.
const CHAIN_BUDGET: u32 = 256;
/// Delayed actions waiting at once.
const PENDING_LIMIT: usize = 512;

/// Per-round trigger state. A new round, a map change or a different trigger
/// list starts it afresh.
#[derive(Clone, Debug, Default)]
pub struct MapLogic {
    ready: bool,
    /// When each trigger may next fire, in round seconds.
    next: Vec<f32>,
    spent: Vec<bool>,
    /// Sorted ids of the tanks inside each Enter or Exit area.
    inside: Vec<Vec<EntityId>>,
    /// Groups to set back: when, which group, and whether it ends hidden.
    reverts: Vec<(f32, u8, bool)>,
    /// Delayed actions: when, and which trigger.
    pending: Vec<(f32, usize)>,
    /// Triggers to fire from signals and counters, in order.
    queue: Vec<usize>,
    scratch: Vec<EntityId>,
    budget: u32,
    /// How many times each trigger has acted this round.
    pub acted: Vec<u32>,
    pub counters: [i32; MAP_COUNTERS as usize],
}

/// The tanks an area trigger notices.
fn notices(trigger: &tankyard_content::MapTrigger, tank: &Occupant) -> bool {
    (match trigger.who {
        TriggerWho::Anyone => true,
        TriggerWho::Players => !tank.bot,
        TriggerWho::Bots => tank.bot,
    }) && (trigger.team == 0 || trigger.team == tank.team)
}

/// Whether a counter moving from `old` to `new` reached `threshold`.
fn reached(old: i32, new: i32, threshold: i32) -> bool {
    (old < threshold && new >= threshold) || (old > threshold && new <= threshold)
}

impl MapLogic {
    fn prepare(&mut self, map: &MapDef, now: f32) {
        let count = map.triggers.len();
        if self.ready && self.next.len() == count {
            return;
        }
        // Schedules count from the round's first tick.
        let started = (now - DT).max(0.);
        self.next = map
            .triggers
            .iter()
            .map(|trigger| match trigger.when {
                TriggerWhen::Every => started + trigger.offset + trigger.seconds,
                TriggerWhen::Start => started + trigger.seconds,
                _ => 0.,
            })
            .collect();
        self.spent = vec![false; count];
        self.inside = vec![Vec::new(); count];
        self.acted = vec![0; count];
        self.reverts.clear();
        self.pending.clear();
        self.queue.clear();
        self.ready = true;
    }

    /// Advance to round time `now`: restore groups, act on due delays, run
    /// schedules and area checks, then follow signals and counters.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        map: &MapDef,
        config: &WorldConfig,
        now: f32,
        hidden: &mut u32,
        tanks: &[Occupant],
        random: &mut dyn FnMut() -> f32,
        effects: &mut Vec<LogicEffect>,
    ) {
        if map.triggers.is_empty() {
            *self = Self::default();
            return;
        }
        self.prepare(map, now);
        self.budget = CHAIN_BUDGET;
        self.reverts.retain(|&(at, group, hide)| {
            if now < at {
                return true;
            }
            let bit = MapDef::group_bit(group);
            *hidden = if hide { *hidden | bit } else { *hidden & !bit };
            false
        });
        if self.pending.iter().any(|&(at, _)| at <= now) {
            let mut due = Vec::new();
            self.pending.retain(|&(at, index)| {
                if at <= now {
                    due.push((at, index));
                    false
                } else {
                    true
                }
            });
            due.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (_, index) in due {
                self.act(index, map, config, now, hidden, effects);
            }
        }
        for index in 0..map.triggers.len() {
            let trigger = &map.triggers[index];
            // Track every crossing, even while spent or cooling down, so a tank
            // already standing inside never counts as entering later.
            let crossed = trigger.when.watches_area() && self.track(index, map, config, tanks);
            if self.spent[index] || now < self.next[index] {
                continue;
            }
            let fire = match trigger.when {
                TriggerWhen::Every => {
                    // Catch up without firing twice in one step.
                    let period = trigger.seconds.max(0.5);
                    let missed = ((now - self.next[index]) / period).floor() + 1.;
                    self.next[index] += missed * period;
                    true
                }
                TriggerWhen::Start => {
                    self.spent[index] = true;
                    true
                }
                TriggerWhen::Enter | TriggerWhen::Exit => crossed,
                TriggerWhen::Signal | TriggerWhen::Counter => false,
            };
            if fire
                && self.fire(index, map, config, now, hidden, random, effects)
                && trigger.when.watches_area()
            {
                self.next[index] = now + trigger.seconds.max(0.25);
            }
        }
        let mut cursor = 0;
        while cursor < self.queue.len() {
            let index = self.queue[cursor];
            cursor += 1;
            if !self.spent[index] {
                self.fire(index, map, config, now, hidden, random, effects);
            }
        }
        self.queue.clear();
    }

    /// Update who stands in an area trigger; true on the crossing it waits for.
    fn track(
        &mut self,
        index: usize,
        map: &MapDef,
        config: &WorldConfig,
        tanks: &[Occupant],
    ) -> bool {
        let trigger = &map.triggers[index];
        let area = map_rect(config, trigger.rect);
        self.scratch.clear();
        self.scratch.extend(
            tanks
                .iter()
                .filter(|tank| notices(trigger, tank) && contains(area, tank.pos))
                .map(|tank| tank.id),
        );
        self.scratch.sort_unstable();
        let previous = &self.inside[index];
        let crossed = match trigger.when {
            TriggerWhen::Enter => self
                .scratch
                .iter()
                .any(|id| previous.binary_search(id).is_err()),
            // Leaving means still alive and outside; dying inside is not leaving.
            _ => previous.iter().any(|id| {
                self.scratch.binary_search(id).is_err() && tanks.iter().any(|tank| tank.id == *id)
            }),
        };
        std::mem::swap(&mut self.inside[index], &mut self.scratch);
        crossed
    }

    /// A trigger fired: check it is switched on and wins its chance, then act
    /// now or after its delay. Returns whether it will act.
    #[allow(clippy::too_many_arguments)]
    fn fire(
        &mut self,
        index: usize,
        map: &MapDef,
        config: &WorldConfig,
        now: f32,
        hidden: &mut u32,
        random: &mut dyn FnMut() -> f32,
        effects: &mut Vec<LogicEffect>,
    ) -> bool {
        let trigger = &map.triggers[index];
        if MapDef::group_bit(trigger.member) & *hidden != 0 {
            return false;
        }
        if trigger.chance < 100 && random() * 100. >= f32::from(trigger.chance) {
            return false;
        }
        if trigger.once {
            self.spent[index] = true;
        }
        if trigger.delay > 0. {
            if self.pending.len() < PENDING_LIMIT {
                self.pending.push((now + trigger.delay, index));
            }
        } else {
            self.act(index, map, config, now, hidden, effects);
        }
        true
    }

    fn act(
        &mut self,
        index: usize,
        map: &MapDef,
        config: &WorldConfig,
        now: f32,
        hidden: &mut u32,
        effects: &mut Vec<LogicEffect>,
    ) {
        if self.budget == 0 {
            return;
        }
        self.budget -= 1;
        self.acted[index] = self.acted[index].saturating_add(1);
        let trigger = &map.triggers[index];
        match trigger.action {
            TriggerAction::Toggle | TriggerAction::Show | TriggerAction::Hide => {
                let bit = MapDef::group_bit(trigger.group);
                let was_hidden = *hidden & bit != 0;
                let hide = match trigger.action {
                    TriggerAction::Toggle => !was_hidden,
                    TriggerAction::Show => false,
                    _ => true,
                };
                // Explicit actions supersede any pending restoration of this group.
                self.reverts.retain(|&(_, group, _)| group != trigger.group);
                if trigger.duration > 0. && hide != was_hidden {
                    self.reverts
                        .push((now + trigger.duration, trigger.group, was_hidden));
                }
                *hidden = if hide { *hidden | bit } else { *hidden & !bit };
            }
            TriggerAction::Announce => effects.push(LogicEffect::Announce(
                tankyard_content::fill_counters(trigger.text.trim(), &self.counters).into_owned(),
            )),
            TriggerAction::Boss => {
                if let Some(kind) = BossKind::from_id(&trigger.boss) {
                    let area = map_rect(config, trigger.rect);
                    effects.push(LogicEffect::Boss {
                        kind,
                        at: V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5),
                    });
                }
            }
            TriggerAction::Shapes => effects.push(LogicEffect::Shapes {
                count: trigger.amount.clamp(1, TRIGGER_SHAPES) as u32,
                area: map_rect(config, trigger.rect),
            }),
            TriggerAction::Signal => {
                for (other, listener) in map.triggers.iter().enumerate() {
                    if listener.when == TriggerWhen::Signal && listener.source == trigger.group {
                        self.queue.push(other);
                    }
                }
            }
            TriggerAction::Count => {
                let Some(slot) = usize::from(trigger.group)
                    .checked_sub(1)
                    .filter(|&slot| slot < self.counters.len())
                else {
                    return;
                };
                let old = self.counters[slot];
                let new = old
                    .saturating_add(trigger.amount)
                    .clamp(-COUNTER_LIMIT, COUNTER_LIMIT);
                self.counters[slot] = new;
                for (other, listener) in map.triggers.iter().enumerate() {
                    if listener.when == TriggerWhen::Counter
                        && listener.source == trigger.group
                        && reached(old, new, listener.threshold)
                    {
                        self.queue.push(other);
                    }
                }
            }
        }
    }

    /// When trigger `index` acts next if nothing else changes, for the editor.
    pub fn next_at(&self, index: usize) -> Option<f32> {
        (self.ready && !self.spent.get(index).copied().unwrap_or(true))
            .then(|| self.next.get(index).copied())
            .flatten()
    }
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
        let mut bases = Vec::new();
        let original_scale = self.config.arena_scale;
        self.config.arena_scale = 1.;
        for team in 1..=self.config.mode.teams() {
            if let Some(base) = self.config.team_base(team) {
                bases.push(base);
            }
        }
        self.config.arena_scale = original_scale;
        for block in &self.map.blocks {
            if MapDef::group_bit(block.group) & hidden != 0 {
                continue;
            }
            let rect = Rect {
                x: block.rect[0] * width,
                y: block.rect[1] * height,
                w: block.rect[2] * width,
                h: block.rect[3] * height,
            };
            let overlaps = |base: Rect| {
                rect.x < base.x + base.w
                    && base.x < rect.x + rect.w
                    && rect.y < base.y + base.h
                    && base.y < rect.y + rect.h
            };
            if !bases.iter().any(|&base| overlaps(base)) {
                self.map_walls.push(rect);
                continue;
            }
            let mut pieces = vec![rect];
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

    /// Pads: speed and conveyors for movement, portals, then repair and
    /// hazard over time.
    pub(crate) fn update_zones(&mut self) {
        if self.map.zones.is_empty() {
            for tank in &mut self.tanks {
                tank.terrain = 1.;
                tank.push = V2::ZERO;
            }
            return;
        }
        let hidden = self.map_hidden();
        // Rectangles once per tick, not once per tank and pad.
        let mut areas = std::mem::take(&mut self.zone_areas);
        areas.clear();
        areas.extend(
            self.map
                .zones
                .iter()
                .enumerate()
                .filter(|(_, zone)| MapDef::group_bit(zone.group) & hidden == 0)
                .map(|(index, zone)| (index, map_rect(&self.config, zone.rect))),
        );
        self.portal_rest.retain(|_, rest| {
            *rest -= DT;
            *rest > 0.
        });
        for index in 0..self.tanks.len() {
            let tank = &self.tanks[index];
            if !tank.alive {
                continue;
            }
            let (id, pos, radius) = (tank.id, tank.pos, tank.radius);
            let (mut terrain, mut push, mut repair, mut hazard) = (1., V2::ZERO, 0., 0.);
            let mut portal = None;
            for &(zone_index, area) in &areas {
                if !contains(area, pos) {
                    continue;
                }
                let zone = &self.map.zones[zone_index];
                terrain = speed_of(terrain, zone);
                push += push_of(zone);
                match zone.kind {
                    ZoneKind::Repair => repair += 0.06 * zone.strength,
                    ZoneKind::Hazard => hazard += 0.08 * zone.strength,
                    ZoneKind::Portal if portal.is_none() => portal = Some(zone.exit),
                    _ => {}
                }
            }
            let tank = &mut self.tanks[index];
            tank.terrain = terrain.clamp(0.3, 2.5);
            tank.push = push.clamp_length(600.);
            if repair > 0. && tank.health < tank.max_health {
                tank.health = (tank.health + tank.max_health * repair * DT).min(tank.max_health);
            }
            if let Some(exit) = portal
                && !self.portal_rest.contains_key(&id)
            {
                let target = map_point(&self.config, exit);
                if let Some(clear) = crate::physics::clear_circle_position(
                    target,
                    radius,
                    self.config.bounds(),
                    &self.walls,
                ) {
                    self.tanks[index].pos = clear;
                    // Long enough to step off the exit pad.
                    self.portal_rest.insert(id, 1.2);
                    self.event(EventKind::Ability, pos, id, 0, 0);
                    self.event(EventKind::Ability, clear, id, 0, 0);
                }
            }
            if hazard > 0. {
                let tank = &self.tanks[index];
                let amount = tank.max_health * hazard * DT + 6. * DT;
                self.damage_tank(index, amount, 0, V2::ZERO);
            }
        }
        self.zone_areas = areas;
    }

    /// Run map logic for this tick and carry out what it asks.
    pub(crate) fn run_triggers(&mut self) {
        if self.map.triggers.is_empty() {
            return;
        }
        let now = self.match_state.elapsed;
        let mut tanks = std::mem::take(&mut self.logic_tanks);
        tanks.clear();
        tanks.extend(
            self.tanks
                .iter()
                .filter(|tank| tank.alive)
                .map(|tank| Occupant {
                    id: tank.id,
                    pos: tank.pos,
                    bot: tank.bot,
                    team: tank.team,
                }),
        );
        let mut hidden = self.map_hidden();
        let mut effects = Vec::new();
        let mut logic = std::mem::take(&mut self.logic);
        let mut rng = self.rng;
        logic.step(
            &self.map,
            &self.config,
            now,
            &mut hidden,
            &tanks,
            &mut || crate::world::next_random(&mut rng),
            &mut effects,
        );
        self.rng = rng;
        self.match_state.counters = logic.counters;
        self.logic = logic;
        self.logic_tanks = tanks;
        for effect in effects {
            match effect {
                LogicEffect::Announce(text) => self.match_state.announcement = text,
                LogicEffect::Boss { kind, at } => {
                    self.spawn_boss_near(kind, at);
                }
                LogicEffect::Shapes { count, area } => {
                    // Triggers may overfill the arena a little, never without bound.
                    let room = (self.target_capacity() + 256).saturating_sub(self.targets.len());
                    for _ in 0..(count as usize).min(room) {
                        self.spawn_target_in(Some(area));
                    }
                }
            }
        }
        if hidden != self.map_hidden() {
            self.match_state.hidden_groups = hidden;
            self.compose_walls();
        }
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
    fn trigger(when: TriggerWhen, action: TriggerAction) -> MapTrigger {
        MapTrigger {
            when,
            action,
            rect: [0.4, 0.4, 0.2, 0.2],
            ..Default::default()
        }
    }
    /// Map logic alone, stepped like the server: 60 steps a second.
    struct Bench {
        map: MapDef,
        config: WorldConfig,
        logic: MapLogic,
        hidden: u32,
        now: f32,
        seed: u64,
        effects: Vec<LogicEffect>,
    }
    impl Bench {
        fn new(triggers: Vec<MapTrigger>) -> Self {
            let mut map = tankyard_content::builtin_maps().remove(0);
            map.triggers = triggers;
            tankyard_content::validate_maps(std::slice::from_ref(&map)).unwrap();
            Self {
                hidden: map.starting_mask(),
                map,
                config: WorldConfig::default(),
                logic: MapLogic::default(),
                now: 0.,
                seed: 7,
                effects: vec![],
            }
        }
        fn step(&mut self, tanks: &[Occupant]) {
            self.now += DT;
            let seed = &mut self.seed;
            self.logic.step(
                &self.map,
                &self.config,
                self.now,
                &mut self.hidden,
                tanks,
                &mut || crate::world::next_random(seed),
                &mut self.effects,
            );
        }
        fn run(&mut self, seconds: f32, tanks: &[Occupant]) {
            for _ in 0..(seconds * 60.).round() as usize {
                self.step(tanks);
            }
        }
        fn center(&self) -> V2 {
            V2::new(self.config.width * 0.5, self.config.height * 0.5)
        }
    }
    fn tank(id: EntityId, pos: V2, bot: bool, team: u8) -> Occupant {
        Occupant { id, pos, bot, team }
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
                group: 3,
                seconds: 1.,
                ..trigger(TriggerWhen::Every, TriggerAction::Toggle)
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
    fn pads_change_speed_and_push_only_inside_shown_areas() {
        let mut map = tankyard_content::builtin_maps().remove(0);
        map.zones.push(MapZone {
            kind: ZoneKind::Speed,
            rect: [0.4, 0.4, 0.2, 0.2],
            strength: 1.,
            group: 2,
            angle: 0.,
            exit: [0., 0.],
        });
        map.zones.push(MapZone {
            kind: ZoneKind::Conveyor,
            rect: [0.4, 0.4, 0.2, 0.2],
            strength: 1.,
            group: 0,
            angle: 90.,
            exit: [0., 0.],
        });
        let config = WorldConfig::default();
        let inside = V2::new(config.width * 0.5, config.height * 0.5);
        assert!(terrain_factor(&map, &config, 0, inside) > 1.4);
        assert_eq!(terrain_factor(&map, &config, 0, V2::new(10., 10.)), 1.);
        assert_eq!(terrain_factor(&map, &config, 1 << 1, inside), 1.);
        let push = terrain_push(&map, &config, 0, inside);
        assert!(push.x.abs() < 0.01 && (push.y - CONVEYOR_SPEED).abs() < 0.01);
        assert_eq!(terrain_push(&map, &config, 0, V2::new(10., 10.)), V2::ZERO);
    }

    #[test]
    fn conveyors_carry_a_tank_that_does_not_steer() {
        let mut world = world_with(|map| {
            map.zones.push(MapZone {
                kind: ZoneKind::Conveyor,
                rect: [0.3, 0.3, 0.4, 0.4],
                strength: 1.,
                group: 0,
                angle: 0.,
                exit: [0., 0.],
            });
        });
        let id = world.add_player("Pilot", false);
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        let index = world.tanks.iter().position(|t| t.id == id).unwrap();
        world.tanks[index].pos = center;
        world.tanks[index].protection = 0.;
        for _ in 0..60 {
            world.step();
        }
        assert!(world.tanks[index].pos.x > center.x + 60., "carried right");
    }

    #[test]
    fn portals_send_tanks_to_their_exit_and_rest_before_reuse() {
        let mut world = world_with(|map| {
            map.zones.push(MapZone {
                kind: ZoneKind::Portal,
                rect: [0.45, 0.45, 0.1, 0.1],
                strength: 1.,
                group: 0,
                angle: 0.,
                exit: [0.2, 0.5],
            });
        });
        let id = world.add_player("Pilot", false);
        let index = world.tanks.iter().position(|t| t.id == id).unwrap();
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        world.tanks[index].pos = center;
        world.update_zones();
        let exit = map_point(&world.config, [0.2, 0.5]);
        assert!(world.tanks[index].pos.distance(exit) < 1.);
        world.tanks[index].pos = center;
        world.update_zones();
        assert!(world.tanks[index].pos.distance(center) < 1., "resting");
    }

    #[test]
    fn map_boss_ids_match_the_simulation_catalog() {
        assert_eq!(BossKind::ALL.map(BossKind::id), tankyard_content::BOSS_IDS);
    }

    #[test]
    fn entry_triggers_require_a_crossing_and_map_changes_reset_their_state() {
        let mut world = world_with(|map| {
            map.triggers.push(MapTrigger {
                group: 2,
                seconds: 0.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Toggle)
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

    #[test]
    fn exit_triggers_fire_on_leaving_but_not_on_dying_inside() {
        let mut bench = Bench::new(vec![MapTrigger {
            group: 4,
            seconds: 0.,
            ..trigger(TriggerWhen::Exit, TriggerAction::Toggle)
        }]);
        let inside = [tank(1, bench.center(), false, 0)];
        bench.step(&inside);
        bench.step(&[]);
        assert_eq!(bench.hidden, 0, "a tank that died inside did not leave");
        bench.step(&inside);
        bench.step(&[tank(1, V2::new(50., 50.), false, 0)]);
        assert_eq!(bench.hidden, 1 << 3);
    }

    #[test]
    fn area_filters_choose_players_bots_and_teams() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 1,
                who: TriggerWho::Players,
                team: 2,
                seconds: 0.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Toggle)
            },
            MapTrigger {
                group: 2,
                who: TriggerWho::Bots,
                seconds: 0.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Toggle)
            },
        ]);
        let c = bench.center();
        bench.step(&[tank(1, c, false, 1)]);
        assert_eq!(bench.hidden, 0, "wrong team");
        bench.step(&[tank(1, c, false, 1), tank(2, c, false, 2)]);
        assert_eq!(bench.hidden, 1);
        bench.step(&[
            tank(1, c, false, 1),
            tank(2, c, false, 2),
            tank(3, c, true, 2),
        ]);
        assert_eq!(bench.hidden, 0b11);
    }

    #[test]
    fn signals_chain_after_delays_and_respect_switched_off_listeners() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 5,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Signal)
            },
            MapTrigger {
                source: 5,
                group: 1,
                delay: 1.,
                ..trigger(TriggerWhen::Signal, TriggerAction::Hide)
            },
            MapTrigger {
                source: 5,
                group: 2,
                member: 3,
                ..trigger(TriggerWhen::Signal, TriggerAction::Hide)
            },
        ]);
        bench.map.hidden_groups = vec![3];
        bench.hidden = bench.map.starting_mask();
        bench.step(&[]);
        assert_eq!(bench.hidden, 1 << 2, "nothing yet; group 3 switched off");
        bench.run(1.1, &[]);
        assert_eq!(bench.hidden, (1 << 2) | 1, "delayed listener acted");
    }

    #[test]
    fn counters_fire_when_reaching_a_value_and_fill_labels() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 2,
                amount: 1,
                seconds: 1.,
                ..trigger(TriggerWhen::Every, TriggerAction::Count)
            },
            MapTrigger {
                source: 2,
                threshold: 3,
                text: "Counter two is {2}".into(),
                ..trigger(TriggerWhen::Counter, TriggerAction::Announce)
            },
        ]);
        bench.run(2.5, &[]);
        assert!(bench.effects.is_empty());
        bench.run(1., &[]);
        assert_eq!(bench.logic.counters[1], 3);
        assert_eq!(
            bench.effects,
            vec![LogicEffect::Announce("Counter two is 3".into())]
        );
        bench.run(1., &[]);
        assert_eq!(
            bench.effects.len(),
            1,
            "passing the value again is not reaching it"
        );
    }

    #[test]
    fn signal_loops_stop_at_the_step_budget() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 1,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Signal)
            },
            MapTrigger {
                source: 1,
                group: 1,
                ..trigger(TriggerWhen::Signal, TriggerAction::Signal)
            },
        ]);
        bench.step(&[]);
        assert_eq!(bench.logic.acted.iter().sum::<u32>(), CHAIN_BUDGET);
        bench.step(&[]);
        assert!(bench.logic.queue.is_empty());
    }

    #[test]
    fn chance_and_once_limit_how_often_a_trigger_acts() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 1,
                amount: 1,
                seconds: 0.5,
                chance: 50,
                ..trigger(TriggerWhen::Every, TriggerAction::Count)
            },
            MapTrigger {
                group: 2,
                amount: 1,
                seconds: 0.5,
                once: true,
                ..trigger(TriggerWhen::Every, TriggerAction::Count)
            },
        ]);
        bench.run(100., &[]);
        let half = bench.logic.counters[0];
        assert!((60..140).contains(&half), "about half of 200: {half}");
        assert_eq!(bench.logic.counters[1], 1);
    }

    #[test]
    fn shape_triggers_spawn_inside_their_area() {
        let mut world = world_with(|map| {
            map.triggers.push(MapTrigger {
                amount: 6,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Shapes)
            });
        });
        world.step();
        let area = map_rect(&world.config, [0.4, 0.4, 0.2, 0.2]);
        assert!(world.targets.len() >= 4, "{} shapes", world.targets.len());
        assert!(world.targets.iter().all(|t| contains(area, t.pos)));
    }
}

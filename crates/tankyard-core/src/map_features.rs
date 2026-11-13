//! The parts of a custom arena that act during a round: pads under tanks and
//! the trigger logic that shows and hides groups, signals, counts, posts
//! announcements and brings shapes and bosses. [`MapLogic`] is the one rule
//! engine for servers, offline games and the editor's logic preview, and pads
//! are pure functions of position, so clients predict their own movement with
//! the server's arithmetic.
use crate::*;
use tankyard_content::{
    CONVEYOR_SPEED, COUNTER_LIMIT, Easing, GateState, GroupPoses, MAP_COUNTERS, MAP_GROUPS, MapDef,
    MapTrigger, MapZone, MathOp, TRIGGER_AWARD, TRIGGER_SHAPES, TriggerAction, TriggerWhen,
    TriggerWho, Tween, TweenChannel, ZoneKind,
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

/// The shown pads whose area holds `pos`, where their groups have moved them.
pub fn zones_at<'a>(
    map: &'a MapDef,
    config: &'a WorldConfig,
    hidden: u32,
    poses: &'a GroupPoses,
    pos: V2,
) -> impl Iterator<Item = &'a MapZone> + 'a {
    map.zones.iter().filter(move |zone| {
        MapDef::group_bit(zone.group) & hidden == 0
            && contains(map_rect(config, poses.shift(zone.rect, zone.group)), pos)
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
pub fn terrain_factor(
    map: &MapDef,
    config: &WorldConfig,
    hidden: u32,
    poses: &GroupPoses,
    pos: V2,
) -> f32 {
    if map.zones.is_empty() {
        return 1.;
    }
    zones_at(map, config, hidden, poses, pos)
        .fold(1., speed_of)
        .clamp(0.3, 2.5)
}

/// The velocity conveyors at `pos` add, in world units per second.
pub fn terrain_push(
    map: &MapDef,
    config: &WorldConfig,
    hidden: u32,
    poses: &GroupPoses,
    pos: V2,
) -> V2 {
    if map.zones.is_empty() {
        return V2::ZERO;
    }
    zones_at(map, config, hidden, poses, pos)
        .fold(V2::ZERO, |sum, zone| sum + push_of(zone))
        .clamp_length(600.)
}

/// Damage taken at `pos` as a fraction; Shelter pads reduce it.
pub fn shelter_factor(
    map: &MapDef,
    config: &WorldConfig,
    hidden: u32,
    poses: &GroupPoses,
    pos: V2,
) -> f32 {
    zones_at(map, config, hidden, poses, pos)
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
    /// Restore `percent` of maximum health to `tanks`, the ones in `area`.
    Repair {
        tanks: Vec<EntityId>,
        percent: f32,
        area: Rect,
    },
    /// Deal `percent` of maximum health to `tanks`, the ones in `area`.
    Damage {
        tanks: Vec<EntityId>,
        percent: f32,
        area: Rect,
    },
    /// Give `points` to `tanks`, the ones in `area`.
    Award {
        tanks: Vec<EntityId>,
        points: u32,
        area: Rect,
    },
    /// Raise the `map_event` script event `name` for trigger `trigger`, with
    /// the noticed tanks in `area`.
    Script {
        name: String,
        trigger: usize,
        tanks: Vec<EntityId>,
        area: Rect,
    },
}

/// What scripts ask map logic to do; it happens at the start of the next
/// step, before schedules and areas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LogicInput {
    Signal(u8),
    SetCounter(u8, i32),
    AddCounter(u8, i32),
    Group(u8, bool),
    /// Start group `n`'s triggers afresh, every trigger for 0.
    Reset(u8),
    /// Play a motion on a group as a Move to (`Place`), Turn or Fade
    /// trigger would: to offset `value`, by `value[0]` degrees, or to
    /// `value[0]` percent, over `seconds`.
    Motion {
        group: u8,
        action: TriggerAction,
        value: [f32; 2],
        seconds: f32,
        easing: Easing,
    },
    /// A boss was defeated; the arena reports it for On boss down triggers.
    Defeated(BossKind),
}

/// A `map_event` raised this tick, for script packs.
#[derive(Clone, Debug, PartialEq)]
pub struct MapEvent {
    pub name: String,
    /// The trigger that raised it, from 0.
    pub trigger: usize,
    pub tanks: Vec<EntityId>,
    pub at: V2,
}

/// Trigger actions per step, so signals that fire each other stop.
const CHAIN_BUDGET: u32 = 256;
/// Delayed actions waiting at once.
const PENDING_LIMIT: usize = 512;
/// Script requests waiting for the next step.
const INBOX_LIMIT: usize = 256;
/// Script events raised in one tick.
pub const MAP_EVENT_LIMIT: usize = 64;

/// Which triggers each event wakes, built once per trigger list so a signal
/// or a counter change reaches its listeners without a search.
#[derive(Clone, Debug, Default)]
struct Listeners {
    /// By signal channel, 1 to 32.
    signal: Vec<Vec<usize>>,
    /// By counter, 1 to 8: triggers waiting for a value, then those
    /// waiting for any change.
    reach: Vec<Vec<usize>>,
    change: Vec<Vec<usize>>,
    /// Triggers watching groups be shown or hidden.
    groups: Vec<usize>,
    defeated: Vec<usize>,
    /// Triggers waiting for a group's motion to end.
    arrived: Vec<usize>,
    /// Triggers with a schedule or an area; event triggers never need the
    /// per-step scan.
    stepped: Vec<usize>,
}
impl Listeners {
    fn new(map: &MapDef) -> Self {
        let mut lists = Self {
            signal: vec![Vec::new(); usize::from(MAP_GROUPS) + 1],
            reach: vec![Vec::new(); usize::from(MAP_COUNTERS) + 1],
            change: vec![Vec::new(); usize::from(MAP_COUNTERS) + 1],
            ..Self::default()
        };
        for (index, trigger) in map.triggers.iter().enumerate() {
            let slot = usize::from(trigger.source);
            if let Some(list) = match trigger.when {
                TriggerWhen::Signal => lists.signal.get_mut(slot),
                TriggerWhen::Counter => lists.reach.get_mut(slot),
                TriggerWhen::Changed => lists.change.get_mut(slot),
                TriggerWhen::Shown | TriggerWhen::Hidden => Some(&mut lists.groups),
                TriggerWhen::Defeated => Some(&mut lists.defeated),
                TriggerWhen::Arrived => Some(&mut lists.arrived),
                _ => Some(&mut lists.stepped),
            } {
                list.push(index);
            }
        }
        lists
    }
}

/// Per-round trigger state. A new round, a map change or a different trigger
/// list starts it afresh.
#[derive(Clone, Debug, Default)]
pub struct MapLogic {
    ready: bool,
    /// When each trigger may next fire, in round seconds.
    next: Vec<f32>,
    spent: Vec<bool>,
    /// Sorted ids of the tanks inside each area trigger's rectangle.
    inside: Vec<Vec<EntityId>>,
    /// The next channel of each sequence, from 0.
    steps: Vec<u8>,
    /// The hidden groups watchers last saw.
    seen: u32,
    listeners: Listeners,
    /// Sorted ids of the living tanks this step.
    alive: Vec<EntityId>,
    /// Groups to set back: when, which group, and whether it ends hidden.
    reverts: Vec<(f32, u8, bool)>,
    /// Group motions, at most one per group and channel; each holds its end
    /// once finished, so it also says where a group has come to rest.
    pub tweens: Vec<Tween>,
    /// Motions to play back: when, and the motion that returns the group.
    returns: Vec<(f32, Tween)>,
    /// Motions under way: when each ends, and its group and channel. A
    /// motion that another takes over from never arrives.
    arrivals: Vec<(f32, u8, TweenChannel)>,
    /// Where every group stood when this step began.
    poses: GroupPoses,
    /// Delayed actions: when, and which trigger.
    pending: Vec<(f32, usize)>,
    /// Triggers to fire from signals and counters, in order.
    queue: Vec<usize>,
    scratch: Vec<EntityId>,
    /// Requests from scripts, oldest first.
    inbox: Vec<LogicInput>,
    budget: u32,
    /// How many times each trigger has acted this round.
    pub acted: Vec<u32>,
    pub counters: [i32; MAP_COUNTERS as usize],
}

/// The tanks an area trigger or tank action notices.
fn notices(trigger: &MapTrigger, tank: &Occupant) -> bool {
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
    /// Queue a script's request; past the limit, requests are dropped.
    pub fn input(&mut self, input: LogicInput) {
        if self.inbox.len() < INBOX_LIMIT {
            self.inbox.push(input);
        }
    }
    pub fn has_input(&self) -> bool {
        !self.inbox.is_empty()
    }

    /// Carry out script requests. Groups change directly and set aside any
    /// pending restoration of that group.
    fn take_inputs(&mut self, map: &MapDef, now: f32, hidden: &mut u32) {
        if self.inbox.is_empty() {
            return;
        }
        for input in std::mem::take(&mut self.inbox) {
            match input {
                LogicInput::Signal(channel) => self.signal(channel),
                LogicInput::SetCounter(counter, value) => self.set_counter(map, counter, value),
                LogicInput::AddCounter(counter, add) => {
                    let slot = usize::from(counter).wrapping_sub(1);
                    if let Some(&value) = self.counters.get(slot) {
                        self.set_counter(map, counter, value.saturating_add(add));
                    }
                }
                LogicInput::Group(group, shown) => {
                    let bit = MapDef::group_bit(group);
                    self.reverts.retain(|&(_, g, _)| g != group);
                    *hidden = if shown { *hidden & !bit } else { *hidden | bit };
                }
                // Without triggers there is nothing to reset.
                LogicInput::Reset(group) => {
                    if self.ready {
                        self.reset(map, group, now);
                    }
                }
                LogicInput::Motion {
                    group,
                    action,
                    value,
                    seconds,
                    easing,
                } if action.animates() => {
                    let trigger = MapTrigger {
                        action,
                        group,
                        travel: value,
                        amount: value[0] as i32,
                        tween: seconds,
                        easing,
                        duration: 0.,
                        ..Default::default()
                    };
                    self.animate(&trigger, now);
                }
                LogicInput::Motion { .. } => {}
                LogicInput::Defeated(kind) => {
                    let number = tankyard_content::BOSS_IDS
                        .iter()
                        .position(|&id| id == kind.id())
                        .map_or(0, |i| i + 1);
                    for &index in &self.listeners.defeated {
                        let wanted = usize::from(map.triggers[index].source);
                        if wanted == 0 || wanted == number {
                            self.queue.push(index);
                        }
                    }
                }
            }
        }
    }

    /// Queue the triggers waiting for a group whose motion has ended, once
    /// for each group however many of its motions end together.
    fn notice_arrivals(&mut self, map: &MapDef, now: f32) {
        if !self.arrivals.iter().any(|&(at, ..)| at <= now) {
            return;
        }
        let mut arrived = 0;
        self.arrivals.retain(|&(at, group, _)| {
            if at <= now {
                arrived |= MapDef::group_bit(group);
                false
            } else {
                true
            }
        });
        for &index in &self.listeners.arrived {
            if MapDef::group_bit(map.triggers[index].source) & arrived != 0 {
                self.queue.push(index);
            }
        }
    }

    /// Queue the triggers watching a group that was shown or hidden since
    /// they last looked.
    fn notice_groups(&mut self, map: &MapDef, hidden: u32) {
        let changed = self.seen ^ hidden;
        self.seen = hidden;
        if changed == 0 {
            return;
        }
        for &index in &self.listeners.groups {
            let trigger = &map.triggers[index];
            let bit = MapDef::group_bit(trigger.source);
            if changed & bit != 0 && (hidden & bit != 0) == (trigger.when == TriggerWhen::Hidden) {
                self.queue.push(index);
            }
        }
    }

    /// Start the round's state for `map`, unless it already has. The groups
    /// hidden now are what watchers compare against.
    fn prepare(&mut self, map: &MapDef, now: f32, hidden: u32) {
        let count = map.triggers.len();
        if self.ready && self.next.len() == count {
            return;
        }
        // Schedules count from the round's first tick.
        let started = (now - DT).max(0.);
        self.next = map
            .triggers
            .iter()
            .map(|trigger| schedule(trigger, started))
            .collect();
        self.spent = vec![false; count];
        self.inside = vec![Vec::new(); count];
        self.steps = vec![0; count];
        self.acted = vec![0; count];
        self.listeners = Listeners::new(map);
        self.seen = hidden;
        self.reverts.clear();
        self.tweens.clear();
        self.returns.clear();
        self.arrivals.clear();
        self.poses = GroupPoses::default();
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
            // Scripts may still use counters, groups and motions on a map
            // without triggers; they last until the round or map changes.
            let mut kept = Self {
                counters: self.counters,
                inbox: std::mem::take(&mut self.inbox),
                tweens: std::mem::take(&mut self.tweens),
                returns: std::mem::take(&mut self.returns),
                ..Self::default()
            };
            kept.take_inputs(map, now, hidden);
            kept.poses = GroupPoses::at(&kept.tweens, now);
            *self = kept;
            return;
        }
        self.prepare(map, now, *hidden);
        self.budget = CHAIN_BUDGET;
        self.alive.clear();
        self.alive.extend(tanks.iter().map(|tank| tank.id));
        self.alive.sort_unstable();
        self.take_inputs(map, now, hidden);
        self.poses = GroupPoses::at(&self.tweens, now);
        self.reverts.retain(|&(at, group, hide)| {
            if now < at {
                return true;
            }
            let bit = MapDef::group_bit(group);
            *hidden = if hide { *hidden | bit } else { *hidden & !bit };
            false
        });
        self.play_returns(now);
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
                self.act(index, map, config, now, hidden, tanks, random, effects);
            }
        }
        for cursor in 0..self.listeners.stepped.len() {
            let index = self.listeners.stepped[cursor];
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
                TriggerWhen::Occupied => {
                    let crowded = self.inside[index].len() >= trigger.threshold.max(1) as usize;
                    // The period runs even when chance says no, so chance
                    // thins out the ticks instead of rolling every step.
                    if crowded {
                        self.next[index] = now + trigger.seconds.max(0.25);
                    }
                    crowded
                }
                TriggerWhen::Enter
                | TriggerWhen::Exit
                | TriggerWhen::Cleared
                | TriggerWhen::Destroyed => crossed,
                TriggerWhen::Signal
                | TriggerWhen::Counter
                | TriggerWhen::Changed
                | TriggerWhen::Shown
                | TriggerWhen::Hidden
                | TriggerWhen::Defeated
                | TriggerWhen::Arrived => false,
            };
            if fire
                && self.fire(index, map, config, now, hidden, tanks, random, effects)
                && trigger.when.watches_area()
                && trigger.when != TriggerWhen::Occupied
            {
                self.next[index] = now + trigger.seconds.max(0.25);
            }
        }
        // Motions that end now wake their watchers once per step, so a
        // chain of instant motions moves on a step at a time.
        self.notice_arrivals(map, now);
        self.follow(map, config, now, hidden, tanks, random, effects);
    }

    /// Fire the triggers that signals and counters woke, in order.
    #[allow(clippy::too_many_arguments)]
    fn follow(
        &mut self,
        map: &MapDef,
        config: &WorldConfig,
        now: f32,
        hidden: &mut u32,
        tanks: &[Occupant],
        random: &mut dyn FnMut() -> f32,
        effects: &mut Vec<LogicEffect>,
    ) {
        let mut cursor = 0;
        loop {
            while cursor < self.queue.len() {
                let index = self.queue[cursor];
                cursor += 1;
                if !self.spent[index] {
                    self.fire(index, map, config, now, hidden, tanks, random, effects);
                }
            }
            // Group watchers wake after what changed the group, and may
            // change more groups themselves.
            self.notice_groups(map, *hidden);
            if cursor == self.queue.len() {
                break;
            }
        }
        self.queue.clear();
    }

    /// Carry out trigger `index` right away, as if it had fired and won its
    /// chance, then follow what it sets off. The editor's Fire now button.
    #[allow(clippy::too_many_arguments)]
    pub fn act_now(
        &mut self,
        index: usize,
        map: &MapDef,
        config: &WorldConfig,
        now: f32,
        hidden: &mut u32,
        tanks: &[Occupant],
        random: &mut dyn FnMut() -> f32,
        effects: &mut Vec<LogicEffect>,
    ) {
        if index >= map.triggers.len() {
            return;
        }
        self.prepare(map, now, *hidden);
        self.poses = GroupPoses::at(&self.tweens, now);
        self.budget = CHAIN_BUDGET;
        self.alive.clear();
        self.alive.extend(tanks.iter().map(|tank| tank.id));
        self.alive.sort_unstable();
        self.act(index, map, config, now, hidden, tanks, random, effects);
        self.follow(map, config, now, hidden, tanks, random, effects);
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
        let area = self.area(config, trigger);
        self.scratch.clear();
        self.scratch.extend(
            tanks
                .iter()
                .filter(|tank| notices(trigger, tank) && contains(area, tank.pos))
                .map(|tank| tank.id),
        );
        self.scratch.sort_unstable();
        let previous = &self.inside[index];
        let alive = |id: &EntityId| self.alive.binary_search(id).is_ok();
        let crossed = match trigger.when {
            TriggerWhen::Enter => self
                .scratch
                .iter()
                .any(|id| previous.binary_search(id).is_err()),
            // Leaving means still alive and outside; dying inside is not leaving.
            TriggerWhen::Exit => previous
                .iter()
                .any(|id| self.scratch.binary_search(id).is_err() && alive(id)),
            TriggerWhen::Cleared => !previous.is_empty() && self.scratch.is_empty(),
            // Only living tanks are listed, so one that vanished from inside died there.
            TriggerWhen::Destroyed => previous.iter().any(|id| !alive(id)),
            _ => false,
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
        tanks: &[Occupant],
        random: &mut dyn FnMut() -> f32,
        effects: &mut Vec<LogicEffect>,
    ) -> bool {
        let trigger = &map.triggers[index];
        if MapDef::group_bit(trigger.member) & *hidden != 0 {
            return false;
        }
        if trigger.only_if.is_some() && !self.gate_open(trigger, config, now, *hidden, tanks) {
            // The otherwise signal counts against the step's budget, so a
            // shut gate signalling itself still stops.
            if trigger.otherwise > 0 && self.budget > 0 {
                self.budget -= 1;
                self.signal(trigger.otherwise);
            }
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
            self.act(index, map, config, now, hidden, tanks, random, effects);
        }
        true
    }

    /// Whether `trigger`'s gate is open now.
    fn gate_open(
        &self,
        trigger: &MapTrigger,
        config: &WorldConfig,
        now: f32,
        hidden: u32,
        tanks: &[Occupant],
    ) -> bool {
        trigger.gate_open(&gate_state(
            trigger,
            config,
            &self.counters,
            now,
            hidden,
            &self.poses,
            tanks,
        ))
    }

    /// Where `trigger`'s area is now: it moves with the trigger's own group.
    fn area(&self, config: &WorldConfig, trigger: &MapTrigger) -> Rect {
        map_rect(config, self.poses.shift(trigger.rect, trigger.member))
    }

    /// The value `group` has on `channel` at `now`.
    pub fn tween_value(&self, group: u8, channel: TweenChannel, now: f32) -> [f32; 2] {
        self.tweens
            .iter()
            .find(|tween| tween.group == group && tween.channel == channel)
            .map_or(channel.rest(), |tween| tween.value(now))
    }

    /// Send groups back whose Go back after time has come.
    fn play_returns(&mut self, now: f32) {
        if !self.returns.iter().any(|&(at, _)| at <= now) {
            return;
        }
        let mut due = Vec::new();
        self.returns.retain(|&(at, tween)| {
            if at <= now {
                due.push(tween);
                false
            } else {
                true
            }
        });
        for back in due {
            let from = self.tween_value(back.group, back.channel, now);
            self.start_tween(Tween {
                from,
                start: now,
                ..back
            });
        }
    }

    /// Whether any group has moved, turned or faded, or will go back.
    pub fn in_motion(&self) -> bool {
        !self.tweens.is_empty() || !self.returns.is_empty()
    }

    /// Play `tween`, replacing whatever that group was doing on its channel.
    fn start_tween(&mut self, tween: Tween) {
        if let Some(slot) = self
            .tweens
            .iter_mut()
            .find(|slot| slot.group == tween.group && slot.channel == tween.channel)
        {
            *slot = tween;
        } else if self.tweens.len() < tankyard_content::TWEEN_SLOTS {
            self.tweens.push(tween);
        } else {
            return;
        }
        self.arrivals
            .retain(|&(_, group, channel)| group != tween.group || channel != tween.channel);
        self.arrivals
            .push((tween.start + tween.length, tween.group, tween.channel));
    }

    /// Move, Move to, Turn or Fade: glide the group from where it is now.
    fn animate(&mut self, trigger: &MapTrigger, now: f32) {
        let group = trigger.group;
        if !(1..=MAP_GROUPS).contains(&group) {
            return;
        }
        let channel = match trigger.action {
            TriggerAction::Turn => TweenChannel::Turn,
            TriggerAction::Fade => TweenChannel::Fade,
            _ => TweenChannel::Move,
        };
        let from = self.tween_value(group, channel, now);
        // Offsets stay within a few arenas however often a Move repeats, and
        // turns wrap so they never lose precision.
        let reach = tankyard_content::TRAVEL_LIMIT * 2.;
        let to = match trigger.action {
            TriggerAction::Move => [
                (from[0] + trigger.travel[0]).clamp(-reach, reach),
                (from[1] + trigger.travel[1]).clamp(-reach, reach),
            ],
            TriggerAction::Place => trigger.travel,
            TriggerAction::Turn => [from[0] % 360. + trigger.amount as f32, 0.],
            _ => [trigger.amount.clamp(0, 100) as f32 / 100., 0.],
        };
        let from = if channel == TweenChannel::Turn {
            [from[0] % 360., 0.]
        } else {
            from
        };
        let tween = Tween {
            group,
            channel,
            from,
            to,
            start: now,
            length: trigger.tween.clamp(0., tankyard_content::TWEEN_SECONDS),
            easing: trigger.easing,
        };
        // A new motion replaces any return the group was waiting for.
        self.returns
            .retain(|(_, back)| back.group != group || back.channel != channel);
        if trigger.duration > 0. && from != to {
            self.returns.push((
                now + tween.length + trigger.duration,
                Tween {
                    from: to,
                    to: from,
                    ..tween
                },
            ));
        }
        self.start_tween(tween);
    }

    /// Queue every trigger listening on signal channel `channel`.
    fn signal(&mut self, channel: u8) {
        if let Some(listeners) = self.listeners.signal.get(usize::from(channel)) {
            self.queue.extend_from_slice(listeners);
        }
    }

    /// Start the triggers of `group` afresh (every trigger for 0).
    fn reset(&mut self, map: &MapDef, group: u8, now: f32) {
        let chosen = |index: usize| group == 0 || map.triggers[index].member == group;
        self.pending.retain(|&(_, index)| !chosen(index));
        for (index, trigger) in map.triggers.iter().enumerate() {
            if chosen(index) {
                self.spent[index] = false;
                self.next[index] = schedule(trigger, now);
                self.steps[index] = 0;
            }
        }
    }

    /// Store `value` in counter `counter` (1 to 8) and queue the triggers
    /// waiting for a value it passed.
    fn set_counter(&mut self, map: &MapDef, counter: u8, value: i32) {
        let Some(slot) = usize::from(counter)
            .checked_sub(1)
            .filter(|&slot| slot < self.counters.len())
        else {
            return;
        };
        let old = self.counters[slot];
        let new = value.clamp(-COUNTER_LIMIT, COUNTER_LIMIT);
        self.counters[slot] = new;
        // A map without triggers has no listener lists at all.
        let counter = usize::from(counter);
        for &other in self.listeners.reach.get(counter).into_iter().flatten() {
            if reached(old, new, map.triggers[other].threshold) {
                self.queue.push(other);
            }
        }
        if old != new
            && let Some(listeners) = self.listeners.change.get(counter)
        {
            self.queue.extend_from_slice(listeners);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn act(
        &mut self,
        index: usize,
        map: &MapDef,
        config: &WorldConfig,
        now: f32,
        hidden: &mut u32,
        tanks: &[Occupant],
        random: &mut dyn FnMut() -> f32,
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
            TriggerAction::Move
            | TriggerAction::Place
            | TriggerAction::Turn
            | TriggerAction::Fade => self.animate(trigger, now),
            TriggerAction::Announce => effects.push(LogicEffect::Announce(
                tankyard_content::fill_counters(trigger.text.trim(), &self.counters).into_owned(),
            )),
            TriggerAction::Boss => {
                if let Some(kind) = BossKind::from_id(&trigger.boss) {
                    let area = self.area(config, trigger);
                    effects.push(LogicEffect::Boss {
                        kind,
                        at: V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5),
                    });
                }
            }
            TriggerAction::Shapes => effects.push(LogicEffect::Shapes {
                count: trigger.amount.clamp(1, TRIGGER_SHAPES) as u32,
                area: self.area(config, trigger),
            }),
            TriggerAction::Signal => self.signal(trigger.group),
            TriggerAction::Random => {
                let choices = trigger.amount.clamp(1, i32::from(MAP_GROUPS)) as f32;
                let pick = (random() * choices).floor().clamp(0., choices - 1.) as u8;
                self.signal(trigger.group.saturating_add(pick));
            }
            TriggerAction::Sequence => {
                let choices = trigger.amount.clamp(1, i32::from(MAP_GROUPS)) as u8;
                let step = self.steps[index] % choices;
                self.steps[index] = (step + 1) % choices;
                self.signal(trigger.group.saturating_add(step));
            }
            TriggerAction::Reset => self.reset(map, trigger.group, now),
            TriggerAction::Count => {
                let slot = usize::from(trigger.group).wrapping_sub(1);
                if let Some(&value) = self.counters.get(slot) {
                    self.set_counter(map, trigger.group, value.saturating_add(trigger.amount));
                }
            }
            TriggerAction::Set => self.set_counter(map, trigger.group, trigger.amount),
            TriggerAction::Calculate => {
                let counter = |n: u8| {
                    usize::from(n)
                        .checked_sub(1)
                        .and_then(|slot| self.counters.get(slot).copied())
                };
                if let Some(value) = counter(trigger.group) {
                    let other = if trigger.operand > 0 {
                        counter(trigger.operand).unwrap_or(0)
                    } else {
                        trigger.amount
                    };
                    // Only Random draws, so other triggers' dice stay the same.
                    let roll = if trigger.math == MathOp::Random {
                        random()
                    } else {
                        0.
                    };
                    if let Some(result) = trigger.math.apply(value, other, roll) {
                        self.set_counter(map, trigger.group, result);
                    }
                }
            }
            TriggerAction::Script => {
                let area = self.area(config, trigger);
                effects.push(LogicEffect::Script {
                    name: trigger.text.trim().to_owned(),
                    trigger: index,
                    tanks: tanks
                        .iter()
                        .filter(|tank| notices(trigger, tank) && contains(area, tank.pos))
                        .map(|tank| tank.id)
                        .collect(),
                    area,
                });
            }
            TriggerAction::Heal | TriggerAction::Hurt | TriggerAction::Award => {
                let area = self.area(config, trigger);
                let ids = tanks
                    .iter()
                    .filter(|tank| notices(trigger, tank) && contains(area, tank.pos))
                    .map(|tank| tank.id)
                    .collect();
                let percent = trigger.amount.clamp(1, 100) as f32;
                effects.push(match trigger.action {
                    TriggerAction::Heal => LogicEffect::Repair {
                        tanks: ids,
                        percent,
                        area,
                    },
                    TriggerAction::Hurt => LogicEffect::Damage {
                        tanks: ids,
                        percent,
                        area,
                    },
                    _ => LogicEffect::Award {
                        tanks: ids,
                        points: trigger.amount.clamp(1, TRIGGER_AWARD) as u32,
                        area,
                    },
                });
            }
        }
    }

    /// When trigger `index` acts next if nothing else changes, for the editor.
    pub fn next_at(&self, index: usize) -> Option<f32> {
        (self.ready && !self.spent.get(index).copied().unwrap_or(true))
            .then(|| self.next.get(index).copied())
            .flatten()
    }
    /// Where the motions have put each group as of the last step.
    pub fn poses(&self) -> &GroupPoses {
        &self.poses
    }
    /// Whether trigger `index` has used up its one action this round.
    pub fn spent(&self, index: usize) -> bool {
        self.spent.get(index).copied().unwrap_or(false)
    }
    /// The channel a sequence sends on next, from 0.
    pub fn sequence_step(&self, index: usize) -> u8 {
        self.steps.get(index).copied().unwrap_or(0)
    }
    /// How many delayed actions are waiting.
    pub fn waiting(&self) -> usize {
        self.pending.len()
    }
}

/// When a trigger first fires on its own, for rounds starting at `from`.
fn schedule(trigger: &MapTrigger, from: f32) -> f32 {
    match trigger.when {
        TriggerWhen::Every => from + trigger.offset + trigger.seconds,
        TriggerWhen::Start => from + trigger.seconds,
        _ => 0.,
    }
}

/// What `trigger`'s gate sees: the counters, the groups, the round time and
/// the noticed tanks in its area, counted only when a condition asks.
#[allow(clippy::too_many_arguments)]
pub fn gate_state<'a>(
    trigger: &MapTrigger,
    config: &WorldConfig,
    counters: &'a [i32; MAP_COUNTERS as usize],
    now: f32,
    hidden: u32,
    poses: &GroupPoses,
    tanks: &[Occupant],
) -> GateState<'a> {
    let inside = if trigger.counts_inside() {
        let area = map_rect(config, poses.shift(trigger.rect, trigger.member));
        tanks
            .iter()
            .filter(|tank| notices(trigger, tank) && contains(area, tank.pos))
            .count()
    } else {
        0
    };
    GateState {
        counters,
        hidden,
        time: now,
        inside,
    }
}

impl World {
    pub(crate) fn map_hidden(&self) -> u32 {
        self.match_state.hidden_groups
    }

    /// Whether map logic is gliding any group's cover right now.
    pub fn cover_gliding(&self) -> bool {
        let now = self.match_state.elapsed;
        self.match_state
            .tweens
            .iter()
            .any(|t| t.channel == TweenChannel::Move && t.moving(now))
    }

    /// Where map logic has put each group.
    pub fn group_poses(&self) -> &GroupPoses {
        &self.poses
    }

    /// A script's request to map logic, carried out next tick.
    pub fn map_input(&mut self, input: LogicInput) {
        self.logic.input(input);
    }

    /// Rebuild the walls from the static geometry and the blocks of every
    /// shown group, carving team bases as at the start of the round.
    pub(crate) fn compose_walls(&mut self) {
        let hidden = self.map_hidden();
        self.map_walls.clone_from(&self.static_walls);
        self.wall_groups.clear();
        self.wall_groups.resize(self.static_walls.len(), 0);
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
                self.wall_groups.push(block.group);
                continue;
            }
            let mut pieces = vec![rect];
            for &base in &bases {
                pieces = pieces
                    .into_iter()
                    .flat_map(|piece| crate::map_layout::outside_base(piece, base))
                    .collect();
            }
            self.wall_groups
                .extend(std::iter::repeat_n(block.group, pieces.len()));
            self.map_walls.extend(pieces);
        }
        self.scale_map_walls();
    }

    /// Follow map logic's group motions: move the cover of moving groups,
    /// carrying tanks, shapes and bosses that stand in its way, and publish
    /// the motions so clients draw the same glide.
    pub(crate) fn follow_group_motion(&mut self) {
        let now = self.match_state.elapsed;
        // A motion that ended where the map placed its group tells clients
        // nothing, so snapshots leave it out; a second's grace lets clients
        // that draw a little behind the server finish drawing it.
        let shown = |t: &&Tween| t.to != t.channel.rest() || now < t.start + t.length + 1.;
        if !self
            .match_state
            .tweens
            .iter()
            .eq(self.logic.tweens.iter().filter(shown))
        {
            self.match_state.tweens.clear();
            self.match_state
                .tweens
                .extend(self.logic.tweens.iter().filter(shown));
        }
        let poses = GroupPoses::at(&self.match_state.tweens, now);
        if poses == self.poses {
            return;
        }
        // Groups whose cover glided at some point since the last step. The
        // step that ends a glide still moves it, often fastest of all.
        let gliding = self
            .match_state
            .tweens
            .iter()
            .filter(|t| t.channel == TweenChannel::Move && t.moving(now - DT))
            .fold(0, |mask, t| mask | MapDef::group_bit(t.group));
        let before = std::mem::replace(&mut self.poses, poses);
        if before.offset == self.poses.offset {
            return;
        }
        let mut old = std::mem::take(&mut self.motion_scratch);
        old.clone_from(&self.walls);
        self.scale_map_walls();
        if gliding != 0 {
            self.push_from_moving_walls(&old, gliding);
        }
        self.motion_scratch = old;
    }

    /// Cover that glided this step pushes what its leading face swept into,
    /// sideways first and then up or down, so a diagonal glide pushes like
    /// the two straight ones it is made of. Whatever is beside the cover or
    /// behind it stays put. A wall that jumps (an instant move) is handled
    /// like one that appears.
    fn push_from_moving_walls(&mut self, old: &[Rect], gliding: u32) {
        let bounds = self.config.bounds();
        for (index, (&before, &after)) in old.iter().zip(&self.walls).enumerate() {
            let group = self.wall_groups.get(index).copied().unwrap_or(0);
            if before == after || MapDef::group_bit(group) & gliding == 0 {
                continue;
            }
            let across = Rect {
                x: after.x,
                w: after.w,
                ..before
            };
            for (from, to, vertical) in [(before, across, false), (across, after, true)] {
                let push = |pos: V2, radius: f32| -> Option<V2> {
                    let pushed = swept_by(pos, radius, from, to, vertical)?;
                    let (lo, hi) = if vertical {
                        (bounds.y + radius, bounds.y + bounds.h - radius)
                    } else {
                        (bounds.x + radius, bounds.x + bounds.w - radius)
                    };
                    let pushed = pushed.clamp(lo, hi.max(lo));
                    Some(if vertical {
                        V2::new(pos.x, pushed)
                    } else {
                        V2::new(pushed, pos.y)
                    })
                };
                for tank in self.tanks.iter_mut().filter(|t| t.alive) {
                    if let Some(pos) = push(tank.pos, tank.radius) {
                        tank.pos = pos;
                    }
                }
                for target in &mut self.targets {
                    if let Some(pos) = push(target.pos, target.radius) {
                        target.pos = pos;
                    }
                }
                for boss in &mut self.bosses {
                    if let Some(pos) = push(boss.pos, boss.radius) {
                        boss.pos = pos;
                    }
                }
            }
        }
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
                .map(|(index, zone)| {
                    (
                        index,
                        map_rect(&self.config, self.poses.shift(zone.rect, zone.group)),
                    )
                }),
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
        if self.map.triggers.is_empty() && !self.logic.has_input() && !self.logic.in_motion() {
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
                LogicEffect::Repair { tanks, percent, .. } => {
                    for id in tanks {
                        let Some(tank) = self.tanks.iter_mut().find(|t| t.id == id && t.alive)
                        else {
                            continue;
                        };
                        if tank.health < tank.max_health {
                            tank.health = (tank.health + tank.max_health * percent / 100.)
                                .min(tank.max_health);
                            let (pos, team) = (tank.pos, tank.team);
                            self.event(EventKind::Ability, pos, id, 0, team);
                        }
                    }
                }
                LogicEffect::Damage { tanks, percent, .. } => {
                    for id in tanks {
                        if let Some(index) = self.tanks.iter().position(|t| t.id == id && t.alive) {
                            let amount = self.tanks[index].max_health * percent / 100.;
                            self.damage_tank(index, amount, 0, V2::ZERO);
                        }
                    }
                }
                LogicEffect::Award { tanks, points, .. } => {
                    for id in tanks {
                        self.award(id, points, points);
                    }
                }
                LogicEffect::Script {
                    name,
                    trigger,
                    tanks,
                    area,
                } => {
                    if self.map_events.len() < MAP_EVENT_LIMIT {
                        self.map_events.push(MapEvent {
                            name,
                            trigger,
                            tanks,
                            at: V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5),
                        });
                    }
                }
            }
        }
        if hidden != self.map_hidden() {
            self.match_state.hidden_groups = hidden;
            self.compose_walls();
        }
        self.follow_group_motion();
    }
}

/// Where map logic has put each group at round time `now`, as a client
/// replays it from a snapshot.
pub fn snapshot_poses(snapshot: &Snapshot, now: f32) -> GroupPoses {
    GroupPoses::at(&snapshot.match_state.tweens, now)
}

/// `snapshot`'s walls where map logic has moved them at round time `now`.
/// Snapshots carry walls where the map places them, so a gliding door costs
/// no traffic and every client draws the same easing the server plays.
pub fn place_walls(snapshot: &Snapshot, poses: &GroupPoses, out: &mut Vec<Rect>) {
    out.clone_from(&snapshot.walls);
    if !poses.moved || snapshot.wall_groups.len() != snapshot.walls.len() {
        return;
    }
    let scale = snapshot.config.arena_scale;
    let (width, height) = (
        snapshot.config.width * scale,
        snapshot.config.height * scale,
    );
    for (wall, &group) in out.iter_mut().zip(&snapshot.wall_groups) {
        let [dx, dy] = poses.offset(group);
        wall.x += dx * width;
        wall.y += dy * height;
    }
}

/// Where a circle goes along one axis when a wall glides from `from` to `to`
/// along it: just clear of the leading face, if that face reached the circle.
/// Only a circle whose center started at or past the leading face is pushed,
/// which leaves alone one the wall slides past or moves away from, however
/// it touches, and still catches one a fast wall passed right over.
fn swept_by(pos: V2, radius: f32, from: Rect, to: Rect, vertical: bool) -> Option<f32> {
    // (start, length) of a rectangle along the motion, then across it.
    let along = |r: Rect| if vertical { (r.y, r.h) } else { (r.x, r.w) };
    let side = |r: Rect| if vertical { (r.x, r.w) } else { (r.y, r.h) };
    let (at, beside) = if vertical {
        (pos.y, pos.x)
    } else {
        (pos.x, pos.y)
    };
    let ((start, length), (end, size)) = (along(from), along(to));
    let (low, wide) = side(to);
    let gap = beside - beside.clamp(low, low + wide);
    if gap.abs() >= radius {
        return None;
    }
    // How far from the face the center sits when the circle just touches it.
    let clear = (radius * radius - gap * gap).sqrt();
    let forward = (end + size * 0.5) - (start + length * 0.5);
    if forward > 0. {
        let (face, reached) = (start + length, end + size);
        (at >= face && at - clear < reached).then_some(reached + clear + 0.5)
    } else if forward < 0. {
        let (face, reached) = (start, end);
        (at <= face && at + clear > reached).then_some(reached - clear - 0.5)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::{Compare, MapBlock, MapTrigger, TestKind, TriggerTest};

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
    fn moving_groups_glide_cover_push_tanks_and_return() {
        let mut world = world_with(|map| {
            map.blocks.push(MapBlock {
                rect: [0.3, 0.3, 0.04, 0.1],
                style: Default::default(),
                color: None,
                group: 5,
            });
            map.zones.push(MapZone {
                kind: ZoneKind::Speed,
                rect: [0.3, 0.6, 0.05, 0.05],
                strength: 1.,
                group: 5,
                angle: 0.,
                exit: [0., 0.],
            });
            map.triggers.push(MapTrigger {
                group: 5,
                travel: [0.1, 0.],
                tween: 1.,
                easing: tankyard_content::Easing::LINEAR,
                duration: 2.,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Move)
            });
        });
        world.add_player("in the way", false);
        let block = *world.walls.last().unwrap();
        let start = block.x;
        let width = world.config.width * world.config.arena_scale;
        world.tanks[0].pos = V2::new(block.x + block.w + 0.05 * width, block.y + block.h * 0.5);
        world.tanks[0].vel = V2::ZERO;
        for _ in 0..30 {
            world.step();
        }
        // The arena resizes as players join, so measure from the block's home now.
        let shift =
            |world: &World| world.walls.last().unwrap().x - world.rest_walls.last().unwrap().x;
        assert!(shift(&world) > 0.03 * width, "the block glides");
        assert!(
            (world.rest_walls.last().unwrap().x - start).abs() < 10.,
            "snapshots keep its home"
        );
        assert_eq!(*world.wall_groups.last().unwrap(), 5);
        assert!(world.cover_gliding());
        assert_eq!(world.match_state.tweens.len(), 1);
        for _ in 0..40 {
            world.step();
        }
        let wall = *world.walls.last().unwrap();
        let width = world.config.width * world.config.arena_scale;
        assert!(
            (shift(&world) - 0.1 * width).abs() < 1.,
            "it stops at the end"
        );
        let tank = &world.tanks[0];
        assert!(
            tank.pos.x - tank.radius >= wall.x + wall.w - 1.,
            "the tank it swept along stays clear of it"
        );
        assert!(!world.cover_gliding());
        let pad = map_point(&world.config, [0.32 + 0.1, 0.62]);
        assert!(
            terrain_factor(&world.map, &world.config, 0, &world.poses, pad) > 1.,
            "pads move with their group"
        );
        // Two seconds after arriving it glides home over the same second.
        for _ in 0..200 {
            world.step();
        }
        assert!(shift(&world).abs() < 1e-3, "and comes home");
        let snapshot = world.snapshot();
        assert_eq!(snapshot.walls.len(), snapshot.wall_groups.len());
        assert!(snapshot.match_state.tweens.iter().all(|t| t.valid()));
        // Once home, the motion soon drops out of snapshots.
        for _ in 0..90 {
            world.step();
        }
        assert!(world.snapshot().match_state.tweens.is_empty());
        assert!(shift(&world).abs() < 1e-3, "and it stays home");
    }

    fn gliding_block(travel: [f32; 2], easing: tankyard_content::Easing) -> World {
        world_with(|map| {
            map.blocks.push(MapBlock {
                rect: [0.3, 0.3, 0.04, 0.1],
                style: Default::default(),
                color: None,
                group: 5,
            });
            map.triggers.push(MapTrigger {
                group: 5,
                travel,
                tween: 1.,
                easing,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Move)
            });
        })
    }

    #[test]
    fn gliding_cover_pushes_only_what_lies_ahead_of_it() {
        let mut world = gliding_block([0.1, 0.], tankyard_content::Easing::LINEAR);
        for name in ["above", "below", "behind", "corner", "ahead"] {
            world.add_player(name, false);
        }
        // Joining players grow the arena, so place tanks once it has settled.
        for _ in 0..240 {
            world.update_arena_size();
        }
        world.scale_map_walls();
        let block = *world.walls.last().unwrap();
        let r = world.tanks[0].radius;
        let (right, bottom) = (block.x + block.w, block.y + block.h);
        // Each rests against the block, a hair inside it as collisions leave
        // tanks, except the one just past its top right corner.
        let spots = [
            V2::new(block.x + block.w * 0.5, block.y - r + 0.01),
            V2::new(block.x + block.w * 0.75, bottom + r - 0.01),
            V2::new(block.x - r + 0.01, block.y + block.h * 0.5),
            V2::new(right + r * 0.8, block.y - r * 0.7),
            V2::new(right + r - 0.01, block.y + block.h * 0.5),
        ];
        for (tank, spot) in world.tanks.iter_mut().zip(spots) {
            tank.pos = spot;
            tank.vel = V2::ZERO;
        }
        for _ in 0..3 {
            world.step();
        }
        let wall = *world.walls.last().unwrap();
        assert!(wall.x > block.x + 1., "the block glides");
        let [above, below, behind, corner, ahead] = [0, 1, 2, 3, 4].map(|i| world.tanks[i].pos);
        assert!(
            (above.x - spots[0].x).abs() < 0.5 && (below.x - spots[1].x).abs() < 0.5,
            "cover sliding past tanks leaves them be: {above:?} {below:?}"
        );
        assert!(
            (behind.x - spots[2].x).abs() < 0.5,
            "cover moving away leaves a tank behind: {behind:?}"
        );
        assert!(
            corner.x > spots[3].x && corner.x - spots[3].x < wall.x - block.x,
            "a corner nudges a tank it meets, less than it moved: {corner:?}"
        );
        assert!(
            ahead.x - r >= wall.x + wall.w,
            "and carries one ahead of it"
        );
        for _ in 0..80 {
            world.step();
        }
        let wall = *world.walls.last().unwrap();
        for tank in &world.tanks {
            assert!(
                crate::physics::circle_rect(tank.pos, tank.radius - 0.5, wall).is_none(),
                "no tank ends inside the block: {:?} {wall:?}",
                tank.pos
            );
        }
    }

    #[test]
    fn fast_diagonal_glides_push_through_their_last_step() {
        // Ease-in moves fastest at the very end, as an instant move would.
        let easing = tankyard_content::Easing {
            curve: tankyard_content::Curve::Expo,
            mode: tankyard_content::EaseMode::In,
        };
        let mut world = gliding_block([0.12, 0.08], easing);
        world.add_player("ahead", false);
        world.config.dynamic_arena = false;
        world.step();
        let block = *world.walls.last().unwrap();
        let width = world.config.width * world.config.arena_scale;
        let height = world.config.height * world.config.arena_scale;
        let r = world.tanks[0].radius;
        // Where the block ends, just past its far corner: only the last few
        // steps reach it.
        let goal = V2::new(
            block.x + block.w + 0.12 * width,
            block.y + block.h + 0.08 * height,
        );
        world.tanks[0].pos = goal + V2::new(r * 0.5, r * 0.5);
        world.tanks[0].vel = V2::ZERO;
        for _ in 0..90 {
            world.step();
        }
        let wall = *world.walls.last().unwrap();
        assert!((wall.x + wall.w - goal.x).abs() < 1., "the block arrives");
        let tank = &world.tanks[0];
        assert!(
            crate::physics::circle_rect(tank.pos, tank.radius - 0.5, wall).is_none(),
            "the tank was pushed clear: {:?} {wall:?}",
            tank.pos
        );
        assert!(
            tank.pos.x > goal.x && tank.pos.y > goal.y,
            "ahead of the block"
        );
    }

    #[test]
    fn turn_and_fade_change_only_how_decor_looks() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 2,
                amount: 90,
                tween: 1.,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Turn)
            },
            MapTrigger {
                group: 2,
                amount: 25,
                tween: 0.,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Fade)
            },
            MapTrigger {
                group: 3,
                travel: [0.2, 0.1],
                tween: 0.5,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Place)
            },
        ]);
        bench.run(2., &[]);
        let poses = GroupPoses::at(&bench.logic.tweens, bench.now);
        assert!((poses.turn(2) - 90.).abs() < 1e-3);
        assert!((poses.opacity(2) - 0.25).abs() < 1e-3);
        assert_eq!(poses.offset(3), [0.2, 0.1]);
        assert_eq!(poses.offset(2), [0., 0.]);
    }

    #[test]
    fn arrivals_wake_watchers_once_each_motion_ends() {
        let count = |counter: u8, source: u8| MapTrigger {
            source,
            group: counter,
            amount: 1,
            seconds: 0.,
            ..trigger(TriggerWhen::Arrived, TriggerAction::Count)
        };
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 3,
                travel: [0.1, 0.],
                tween: 1.,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Move)
            },
            count(1, 3),
            // Group 3 arriving fades group 4 at once, which arrives in turn.
            MapTrigger {
                source: 3,
                group: 4,
                amount: 40,
                seconds: 0.,
                ..trigger(TriggerWhen::Arrived, TriggerAction::Fade)
            },
            count(2, 4),
        ]);
        bench.run(0.9, &[]);
        assert_eq!(bench.logic.counters[..2], [0, 0], "still gliding");
        bench.run(0.2, &[]);
        assert_eq!(
            bench.logic.counters[..2],
            [1, 1],
            "it arrived, then the fade"
        );
        bench.run(2., &[]);
        assert_eq!(bench.logic.counters[..2], [1, 1], "nothing arrives twice");

        // A motion that another takes over from never arrives; the new one does.
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 3,
                travel: [0.1, 0.],
                tween: 1.,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Move)
            },
            MapTrigger {
                group: 3,
                tween: 1.,
                seconds: 0.5,
                ..trigger(TriggerWhen::Start, TriggerAction::Place)
            },
            count(1, 3),
        ]);
        bench.run(1.2, &[]);
        assert_eq!(bench.logic.counters[0], 0, "the first glide was cut short");
        bench.run(0.5, &[]);
        assert_eq!(bench.logic.counters[0], 1);
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
        assert!(terrain_factor(&map, &config, 0, &GroupPoses::default(), inside) > 1.4);
        assert_eq!(
            terrain_factor(&map, &config, 0, &GroupPoses::default(), V2::new(10., 10.)),
            1.
        );
        assert_eq!(
            terrain_factor(&map, &config, 1 << 1, &GroupPoses::default(), inside),
            1.
        );
        let push = terrain_push(&map, &config, 0, &GroupPoses::default(), inside);
        assert!(push.x.abs() < 0.01 && (push.y - CONVEYOR_SPEED).abs() < 0.01);
        assert_eq!(
            terrain_push(&map, &config, 0, &GroupPoses::default(), V2::new(10., 10.)),
            V2::ZERO
        );
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

    #[test]
    fn occupied_areas_repeat_while_crowded_and_clear_or_destroy_once() {
        let counter = |when, threshold, amount| MapTrigger {
            group: 1,
            seconds: 1.,
            threshold,
            amount,
            ..trigger(when, TriggerAction::Count)
        };
        let mut bench = Bench::new(vec![
            counter(TriggerWhen::Occupied, 2, 1),
            MapTrigger {
                group: 2,
                ..counter(TriggerWhen::Cleared, 1, 1)
            },
            MapTrigger {
                group: 3,
                ..counter(TriggerWhen::Destroyed, 1, 1)
            },
        ]);
        let center = bench.center();
        let one = [tank(1, center, false, 0)];
        let two = [tank(1, center, false, 0), tank(2, center, true, 0)];
        bench.run(2., &one);
        assert_eq!(bench.logic.counters[0], 0, "one tank is not a crowd");
        bench.run(2.5, &two);
        assert_eq!(bench.logic.counters[0], 3, "at once, then each second");
        // Tank 2 dies inside: destroyed, but the area is not yet empty.
        bench.run(0.5, &one);
        assert_eq!(bench.logic.counters[1..3], [0, 1]);
        bench.step(&[tank(1, V2::new(10., 10.), false, 0)]);
        assert_eq!(bench.logic.counters[1..3], [1, 1], "left alive, so emptied");
        bench.run(1., &[]);
        assert_eq!(bench.logic.counters[0], 3);
    }

    #[test]
    fn set_counters_and_random_signals_reach_their_listeners() {
        let mut triggers = vec![
            MapTrigger {
                seconds: 1.,
                group: 2,
                amount: 3,
                ..trigger(TriggerWhen::Every, TriggerAction::Random)
            },
            MapTrigger {
                seconds: 0.5,
                once: true,
                group: 1,
                amount: 40,
                ..trigger(TriggerWhen::Start, TriggerAction::Set)
            },
            MapTrigger {
                source: 1,
                threshold: 40,
                text: "Forty".into(),
                ..trigger(TriggerWhen::Counter, TriggerAction::Announce)
            },
        ];
        for channel in 2..=4 {
            triggers.push(MapTrigger {
                source: channel,
                group: channel,
                amount: 1,
                ..trigger(TriggerWhen::Signal, TriggerAction::Count)
            });
        }
        let mut bench = Bench::new(triggers);
        bench.run(60.5, &[]);
        assert_eq!(bench.logic.counters[0], 40);
        assert_eq!(
            bench.effects,
            vec![LogicEffect::Announce("Forty".into())],
            "a set that lands on the threshold fires it once"
        );
        let picks = &bench.logic.counters[1..4];
        assert_eq!(picks.iter().sum::<i32>(), 60, "one channel each second");
        assert!(picks.iter().all(|&n| n >= 8), "{picks:?} spread out");
    }

    #[test]
    fn tank_actions_reach_only_the_noticed_tanks_inside() {
        let mut world = world_with(|map| {
            for (action, amount) in [
                (TriggerAction::Hurt, 50),
                (TriggerAction::Heal, 20),
                (TriggerAction::Award, 300),
            ] {
                map.triggers.push(MapTrigger {
                    seconds: 1.,
                    amount,
                    who: TriggerWho::Players,
                    ..trigger(TriggerWhen::Start, action)
                });
            }
        });
        let inside = world.add_player("Inside", false);
        let outside = world.add_player("Outside", false);
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        for tank in &mut world.tanks {
            tank.protection = 0.;
            tank.pos = if tank.id == inside {
                center
            } else {
                V2::new(40., 40.)
            };
        }
        let before = |world: &World, id| {
            let t = world.tanks.iter().find(|t| t.id == id).unwrap();
            (t.health / t.max_health, t.score)
        };
        let (_, score) = before(&world, inside);
        for _ in 0..62 {
            world.match_state.elapsed += DT;
            world.run_triggers();
        }
        let (health, after) = before(&world, inside);
        assert!(
            (health - 0.7).abs() < 0.02,
            "half off, a fifth back: {health}"
        );
        assert!(after >= score + 300);
        assert_eq!(before(&world, outside).0, 1.);
    }

    #[test]
    fn act_now_runs_one_trigger_and_what_it_sets_off() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                seconds: 100.,
                group: 5,
                ..trigger(TriggerWhen::Every, TriggerAction::Signal)
            },
            MapTrigger {
                source: 5,
                group: 4,
                ..trigger(TriggerWhen::Signal, TriggerAction::Hide)
            },
        ]);
        bench.step(&[]);
        let seed = &mut bench.seed;
        bench.logic.act_now(
            0,
            &bench.map,
            &bench.config,
            bench.now,
            &mut bench.hidden,
            &[],
            &mut || crate::world::next_random(seed),
            &mut bench.effects,
        );
        assert_eq!(bench.hidden & MapDef::group_bit(4), MapDef::group_bit(4));
        assert_eq!(bench.logic.acted, vec![1, 1]);
    }

    fn counter_test(index: u8, compare: Compare, value: i32, against: u8) -> Option<TriggerTest> {
        Some(TriggerTest {
            kind: TestKind::Counter,
            index,
            compare,
            value,
            against,
        })
    }

    #[test]
    fn gates_let_triggers_act_only_while_their_condition_holds() {
        let every = |group: u8, only_if: Option<TriggerTest>| MapTrigger {
            seconds: 1.,
            group,
            only_if,
            ..trigger(TriggerWhen::Every, TriggerAction::Count)
        };
        let mut bench = Bench::new(vec![
            every(1, None),
            every(2, counter_test(1, Compare::AtLeast, 3, 0)),
            every(
                3,
                Some(TriggerTest {
                    kind: TestKind::Hidden,
                    index: 4,
                    ..Default::default()
                }),
            ),
            every(4, counter_test(2, Compare::AtLeast, 0, 1)),
            every(5, counter_test(1, Compare::NotEqual, 2, 0)),
        ]);
        bench.run(5.5, &[]);
        assert_eq!(bench.logic.counters[..5], [5, 3, 0, 0, 4]);
        assert_eq!(
            bench.logic.acted,
            vec![5, 3, 0, 0, 4],
            "failing gates do not act"
        );
        bench.hidden |= MapDef::group_bit(4);
        bench.run(1., &[]);
        assert_eq!(bench.logic.counters[2], 1, "the group gate opened");
    }

    #[test]
    fn calculate_works_on_counters_and_reaches_their_listeners() {
        let calc = |group: u8, math: MathOp, amount: i32, operand: u8| MapTrigger {
            seconds: 1000.,
            group,
            math,
            amount,
            operand,
            ..trigger(TriggerWhen::Every, TriggerAction::Calculate)
        };
        let mut bench = Bench::new(vec![
            MapTrigger {
                seconds: 1000.,
                group: 1,
                amount: 7,
                ..trigger(TriggerWhen::Every, TriggerAction::Set)
            },
            calc(2, MathOp::Copy, 0, 1),
            calc(2, MathOp::Multiply, 6, 0),
            calc(2, MathOp::Divide, -4, 0),
            calc(3, MathOp::Flip, 0, 0),
            calc(4, MathOp::Random, 6, 0),
            calc(5, MathOp::Divide, 0, 0),
            calc(2, MathOp::Remainder, -4, 0),
            calc(6, MathOp::Subtract, -99_999, 0),
            calc(6, MathOp::Multiply, 99_999, 0),
            MapTrigger {
                source: 2,
                threshold: 42,
                text: "Answer".into(),
                ..trigger(TriggerWhen::Counter, TriggerAction::Announce)
            },
        ]);
        bench.step(&[]);
        let run = |bench: &mut Bench, index: usize| {
            let seed = &mut bench.seed;
            bench.logic.act_now(
                index,
                &bench.map,
                &bench.config,
                bench.now,
                &mut bench.hidden,
                &[],
                &mut || crate::world::next_random(seed),
                &mut bench.effects,
            );
        };
        for index in [0, 1, 2] {
            run(&mut bench, index);
        }
        assert_eq!(bench.logic.counters[1], 42);
        assert_eq!(bench.effects, vec![LogicEffect::Announce("Answer".into())]);
        run(&mut bench, 3);
        assert_eq!(bench.logic.counters[1], -11, "division rounds down");
        run(&mut bench, 7);
        assert_eq!(bench.logic.counters[1], 1, "remainders are never negative");
        run(&mut bench, 4);
        assert_eq!(bench.logic.counters[2], 1);
        run(&mut bench, 4);
        assert_eq!(bench.logic.counters[2], 0, "flip goes back");
        for _ in 0..40 {
            run(&mut bench, 5);
            assert!((0..6).contains(&bench.logic.counters[3]));
        }
        run(&mut bench, 6);
        assert_eq!(bench.logic.counters[4], 0, "dividing by zero leaves it");
        run(&mut bench, 8);
        run(&mut bench, 9);
        assert_eq!(
            bench.logic.counters[5], COUNTER_LIMIT,
            "results stay in range"
        );
        assert_eq!(MathOp::Divide.apply(7, 0, 0.5), None);
        assert_eq!(MathOp::Remainder.apply(-7, 3, 0.), Some(2));
        assert_eq!(MathOp::Random.apply(0, 3, 0.999_999), Some(2));
        assert_eq!(MathOp::Random.apply(0, 0, 0.5), None);
        assert_eq!(MathOp::Lowest.apply(9, 4, 0.), Some(4));
        assert_eq!(MathOp::Highest.apply(9, 4, 0.), Some(9));
    }

    #[test]
    fn script_events_name_the_noticed_tanks_inside() {
        let mut bench = Bench::new(vec![MapTrigger {
            text: " door ".into(),
            who: TriggerWho::Players,
            ..trigger(TriggerWhen::Enter, TriggerAction::Script)
        }]);
        bench.step(&[]);
        let (inside, away) = (bench.center(), V2::new(10., 10.));
        bench.step(&[
            tank(1, inside, false, 1),
            tank(2, inside, true, 1),
            tank(3, away, false, 1),
        ]);
        assert!(matches!(
            bench.effects.as_slice(),
            [LogicEffect::Script { name, trigger: 0, tanks, .. }] if name == "door" && tanks == &[1]
        ));
    }

    #[test]
    fn script_requests_signal_count_and_switch_groups_next_step() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                source: 3,
                group: 1,
                ..trigger(TriggerWhen::Signal, TriggerAction::Count)
            },
            MapTrigger {
                source: 2,
                threshold: 9,
                group: 6,
                ..trigger(TriggerWhen::Counter, TriggerAction::Hide)
            },
        ]);
        bench.step(&[]);
        for input in [
            LogicInput::Signal(3),
            LogicInput::AddCounter(1, 5),
            LogicInput::SetCounter(2, 9),
            LogicInput::Group(4, false),
            LogicInput::SetCounter(9, 1),
        ] {
            bench.logic.input(input);
        }
        bench.step(&[]);
        assert!(!bench.logic.has_input());
        assert_eq!(bench.logic.counters[..2], [6, 9]);
        let hidden = MapDef::group_bit(4) | MapDef::group_bit(6);
        assert_eq!(
            bench.hidden & hidden,
            hidden,
            "a script hid 4; the goal hid 6"
        );
        for _ in 0..1000 {
            bench.logic.input(LogicInput::AddCounter(1, 1));
        }
        bench.step(&[]);
        assert_eq!(
            bench.logic.counters[0],
            6 + INBOX_LIMIT as i32,
            "the inbox is bounded"
        );
    }

    #[test]
    fn worlds_raise_script_events_and_take_requests_without_triggers() {
        let mut world = world_with(|map| {
            map.triggers.push(MapTrigger {
                seconds: 0.,
                text: "hello".into(),
                ..trigger(TriggerWhen::Start, TriggerAction::Script)
            });
        });
        world.step();
        assert_eq!(world.map_events.len(), 1);
        assert_eq!(world.map_events[0].name, "hello");
        world.step();
        assert!(world.map_events.is_empty(), "events last one tick");

        let mut world = world_with(|map| {
            map.blocks.push(MapBlock {
                rect: [0.1, 0.1, 0.05, 0.05],
                style: Default::default(),
                color: None,
                group: 4,
            });
        });
        world.step();
        let walls = world.walls.len();
        world.map_input(LogicInput::Group(4, false));
        world.map_input(LogicInput::SetCounter(3, 12));
        world.step();
        assert_ne!(world.match_state.hidden_groups & MapDef::group_bit(4), 0);
        assert_eq!(
            world.walls.len(),
            walls - 1,
            "the hidden block stops colliding"
        );
        world.step();
        assert_eq!(
            world.match_state.counters[2], 12,
            "counters last without triggers"
        );
    }

    #[test]
    fn a_lever_opens_and_shuts_a_gate_as_the_guide_describes() {
        let goal = |threshold: i32, action: TriggerAction| MapTrigger {
            source: 1,
            threshold,
            group: 4,
            ..trigger(TriggerWhen::Counter, action)
        };
        let mut bench = Bench::new(vec![
            MapTrigger {
                math: MathOp::Flip,
                seconds: 1.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Calculate)
            },
            goal(1, TriggerAction::Hide),
            goal(0, TriggerAction::Show),
        ]);
        let (lever, away) = (bench.center(), V2::new(10., 10.));
        let gate = MapDef::group_bit(4);
        let visit = |bench: &mut Bench| {
            bench.run(1.5, &[tank(1, lever, false, 0)]);
            bench.run(1.5, &[tank(1, away, false, 0)]);
            (bench.logic.counters[0], bench.hidden & gate)
        };
        assert_eq!(visit(&mut bench), (1, gate), "on: the gate opens");
        assert_eq!(visit(&mut bench), (0, 0), "off: it shuts");
        assert_eq!(visit(&mut bench), (1, gate), "and opens again");
    }

    fn test(kind: TestKind, index: u8, compare: Compare, value: i32) -> TriggerTest {
        TriggerTest {
            kind,
            index,
            compare,
            value,
            against: 0,
        }
    }

    #[test]
    fn gate_modes_combine_their_conditions_like_logic_gates() {
        use tankyard_content::GateMode;
        // Counter 1 is 1 and counter 2 is 0 throughout: one of two holds.
        let gated = |group: u8, gate: GateMode| {
            let mut t = MapTrigger {
                seconds: 1.,
                group,
                gate,
                ..trigger(TriggerWhen::Every, TriggerAction::Count)
            };
            t.set_conditions(vec![
                test(TestKind::Counter, 8, Compare::AtLeast, 1),
                test(TestKind::Counter, 7, Compare::AtLeast, 1),
            ]);
            t
        };
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 8,
                amount: 1,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Set)
            },
            gated(1, GateMode::All),
            gated(2, GateMode::Any),
            gated(3, GateMode::NotAll),
            gated(4, GateMode::NoneOf),
            gated(5, GateMode::One),
        ]);
        bench.run(3.5, &[]);
        assert_eq!(bench.logic.counters[..5], [0, 3, 3, 0, 3]);
    }

    #[test]
    fn a_shut_gate_sends_its_otherwise_signal_and_loops_still_stop() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                seconds: 1.,
                group: 1,
                otherwise: 2,
                only_if: counter_test(1, Compare::AtLeast, 3, 0),
                ..trigger(TriggerWhen::Every, TriggerAction::Count)
            },
            MapTrigger {
                source: 2,
                group: 2,
                ..trigger(TriggerWhen::Signal, TriggerAction::Count)
            },
            // A shut gate that answers its own signal: only the budget ends it.
            MapTrigger {
                source: 5,
                otherwise: 5,
                only_if: counter_test(3, Compare::Above, 0, 0),
                ..trigger(TriggerWhen::Signal, TriggerAction::Count)
            },
        ]);
        bench.run(2.5, &[]);
        assert_eq!(
            bench.logic.counters[..2],
            [0, 2],
            "two shut gates, two signals"
        );
        bench.logic.input(LogicInput::Signal(5));
        bench.step(&[]);
        assert_eq!(bench.logic.counters[2], 0);
    }

    #[test]
    fn conditions_read_tanks_inside_and_round_time() {
        let mut inside = MapTrigger {
            seconds: 1.,
            group: 1,
            ..trigger(TriggerWhen::Every, TriggerAction::Count)
        };
        inside.set_conditions(vec![test(TestKind::Inside, 0, Compare::AtLeast, 2)]);
        let mut late = MapTrigger {
            seconds: 1.,
            group: 2,
            ..trigger(TriggerWhen::Every, TriggerAction::Count)
        };
        late.set_conditions(vec![test(TestKind::Time, 0, Compare::AtLeast, 3)]);
        let mut bench = Bench::new(vec![inside, late]);
        let center = bench.center();
        bench.run(2.5, &[tank(1, center, false, 0)]);
        assert_eq!(bench.logic.counters[..2], [0, 0]);
        bench.run(2., &[tank(1, center, false, 0), tank(2, center, true, 0)]);
        assert_eq!(
            bench.logic.counters[..2],
            [2, 2],
            "two inside, and past 3 s"
        );
    }

    #[test]
    fn watchers_hear_counter_changes_groups_and_boss_defeats() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                seconds: 1.,
                group: 1,
                ..trigger(TriggerWhen::Every, TriggerAction::Count)
            },
            MapTrigger {
                source: 1,
                group: 2,
                ..trigger(TriggerWhen::Changed, TriggerAction::Count)
            },
            MapTrigger {
                seconds: 2.,
                group: 6,
                ..trigger(TriggerWhen::Every, TriggerAction::Toggle)
            },
            MapTrigger {
                source: 6,
                group: 3,
                ..trigger(TriggerWhen::Hidden, TriggerAction::Count)
            },
            MapTrigger {
                source: 6,
                group: 4,
                ..trigger(TriggerWhen::Shown, TriggerAction::Count)
            },
            MapTrigger {
                source: 2,
                group: 5,
                ..trigger(TriggerWhen::Defeated, TriggerAction::Count)
            },
            MapTrigger {
                source: 0,
                group: 7,
                ..trigger(TriggerWhen::Defeated, TriggerAction::Count)
            },
        ]);
        bench.run(4.5, &[]);
        // Counter 1 changed four times; group 6 hid at 2 s and came back at 4 s.
        assert_eq!(bench.logic.counters[..4], [4, 4, 1, 1]);
        bench.logic.input(LogicInput::Defeated(BossKind::Sentinel));
        bench.step(&[]);
        bench.logic.input(LogicInput::Defeated(BossKind::Prism));
        bench.step(&[]);
        assert_eq!(bench.logic.counters[4], 1, "only the prism counts for 5");
        assert_eq!(bench.logic.counters[6], 2, "any boss counts for 7");
    }

    #[test]
    fn group_watchers_chain_and_ignore_the_starting_layout() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 2,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Show)
            },
            // Seeing group 2 appear hides group 3, which counts.
            MapTrigger {
                source: 2,
                group: 3,
                ..trigger(TriggerWhen::Shown, TriggerAction::Hide)
            },
            MapTrigger {
                source: 3,
                group: 1,
                ..trigger(TriggerWhen::Hidden, TriggerAction::Count)
            },
        ]);
        bench.map.hidden_groups = vec![2];
        bench.hidden = bench.map.starting_mask();
        bench.run(1., &[]);
        assert_eq!(bench.logic.counters[0], 1);
        assert_eq!(bench.hidden, MapDef::group_bit(3));
    }

    #[test]
    fn sequences_step_through_their_channels_and_reset_starts_over() {
        let listener = |channel: u8| MapTrigger {
            source: channel,
            group: channel,
            ..trigger(TriggerWhen::Signal, TriggerAction::Count)
        };
        let mut bench = Bench::new(vec![
            MapTrigger {
                seconds: 1.,
                group: 1,
                amount: 3,
                member: 9,
                ..trigger(TriggerWhen::Every, TriggerAction::Sequence)
            },
            listener(1),
            listener(2),
            listener(3),
            MapTrigger {
                seconds: 0.5,
                once: true,
                member: 9,
                group: 4,
                ..trigger(TriggerWhen::Start, TriggerAction::Count)
            },
            MapTrigger {
                source: 7,
                group: 9,
                ..trigger(TriggerWhen::Signal, TriggerAction::Reset)
            },
        ]);
        bench.run(4.5, &[]);
        assert_eq!(
            bench.logic.counters[..4],
            [2, 1, 1, 1],
            "1, 2, 3, then 1 again"
        );
        assert_eq!(bench.logic.sequence_step(0), 1);
        bench.logic.input(LogicInput::Signal(7));
        bench.step(&[]);
        assert_eq!(bench.logic.sequence_step(0), 0, "the sequence starts over");
        assert!(!bench.logic.spent(4), "the start trigger can act again");
        bench.run(1.1, &[]);
        assert_eq!(bench.logic.counters[..4], [3, 1, 1, 2]);
    }

    #[test]
    fn reset_drops_waiting_delays() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                seconds: 0.1,
                delay: 2.,
                member: 3,
                group: 1,
                ..trigger(TriggerWhen::Start, TriggerAction::Count)
            },
            MapTrigger {
                seconds: 1.,
                group: 3,
                ..trigger(TriggerWhen::Start, TriggerAction::Reset)
            },
        ]);
        bench.run(0.5, &[]);
        assert_eq!(bench.logic.waiting(), 1);
        bench.run(0.55, &[]);
        assert_eq!(bench.logic.waiting(), 0, "the countdown was called off");
        bench.run(4., &[]);
        // Restarted at 1 s, it fires again at 1.1 s and acts 2 s later.
        assert_eq!(bench.logic.counters[0], 1);
    }
}

//! The parts of a custom arena that act during a round: pads under tanks and
//! the trigger logic that shows and hides groups, signals, counts, posts
//! announcements and brings shapes and bosses. [`MapLogic`] is the one rule
//! engine for servers, offline games and the editor's logic preview, and pads
//! are pure functions of position, so clients predict their own movement with
//! the server's arithmetic.
use crate::*;
use tankyard_content::{
    AnimateStyle, CONVEYOR_SPEED, COUNTER_LIMIT, Curve, EVERY_LEAST, EaseMode, Easing, GateState,
    GroupPoses, ICE_GRIP, LAUNCH_SPEED, MAP_CHANNELS, MAP_COUNTERS, MAP_GROUPS, MapCounters,
    MapDef, MapSound, MapTrigger, MapZone, MathOp, PULL_SPEED, RandomPick, SCALE_LIMIT,
    SHAKE_SECONDS, SOUND_GAP, SOUND_PITCH, SOUND_REPEATS, SOUND_VARY, SoundRange, TRIGGER_AWARD,
    TRIGGER_SHAPES, TriggerAction, TriggerWhen, TriggerWho, Tween, TweenChannel, TweenValue,
    UiAudience, ZOOM_EASE, ZOOM_RANGE, ZOOM_SECONDS, ZoneKind,
};

mod bodies;
mod paths;
mod ui;
pub use bodies::{BodySpace, BodyState, BodyStep, snapshot_body_states, snapshot_poses};
use paths::PathPlay;
pub use ui::{HudPick, MAP_HUD, ShownUi, fill_map_widget, menu_number, ui_slot_id};

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

/// Whether two normalized rectangles overlap.
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
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
/// What a pad adds to the motion of a tank at `pos`: conveyors carry it
/// along, gravity wells draw it toward their center, harder near the middle.
fn push_of(zone: &MapZone, area: Rect, pos: V2) -> V2 {
    match zone.kind {
        ZoneKind::Conveyor => {
            V2::from_angle(zone.angle.to_radians()) * CONVEYOR_SPEED * zone.strength
        }
        ZoneKind::Pull => {
            let center = V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5);
            let toward = center - pos;
            let reach = (area.w.min(area.h) * 0.5).max(1.);
            let distance = toward.length();
            if distance < 4. {
                return V2::ZERO;
            }
            let closeness = (1. - distance / reach).clamp(0., 1.);
            toward / distance * PULL_SPEED * zone.strength * (1. + closeness)
        }
        _ => V2::ZERO,
    }
}

/// How firmly a tank on `zone` grips, 1 for full grip.
fn grip_of(grip: f32, zone: &MapZone) -> f32 {
    if zone.kind == ZoneKind::Ice {
        grip * ICE_GRIP.powf(zone.strength.sqrt())
    } else {
        grip
    }
}

fn fling_of(zone: &MapZone) -> V2 {
    if zone.kind == ZoneKind::Launch {
        V2::from_angle(zone.angle.to_radians()) * LAUNCH_SPEED * zone.strength
    } else {
        V2::ZERO
    }
}

/// The fling launch pads at `pos` give, in world units per second.
pub fn terrain_launch(
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
        .fold(V2::ZERO, |sum, zone| sum + fling_of(zone))
        .clamp_length(LAUNCH_SPEED * 2.)
}

/// How long a launched tank coasts before steering takes over again.
pub const LAUNCH_COAST: f32 = 0.45;

/// Fling `tank` along `fling` unless it already travels that way at most of
/// that speed, so a pad launches once rather than every tick a tank is on it.
/// Its sideways motion is kept. Returns whether it launched.
pub fn launch_tank(tank: &mut Tank, fling: V2) -> bool {
    let speed = fling.length();
    if !speed.is_finite() || speed < 1. {
        return false;
    }
    let direction = fling / speed;
    let along = tank.vel.dot(direction);
    if along >= speed * 0.8 {
        return false;
    }
    tank.vel += direction * (speed - along);
    tank.knockback = tank.knockback.max(LAUNCH_COAST);
    true
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

/// The velocity conveyors and gravity wells at `pos` add, in world units
/// per second.
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
        .fold(V2::ZERO, |sum, zone| {
            let area = map_rect(config, poses.shift(zone.rect, zone.group));
            sum + push_of(zone, area, pos)
        })
        .clamp_length(600.)
}

/// How firmly a tank at `pos` grips the floor: 1, or less on ice.
pub fn terrain_grip(
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
        .fold(1., grip_of)
        .clamp(0.05, 1.)
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
    /// Text with counter placeholders filled in, and the tank that set it off.
    Announce {
        text: String,
        cause: EntityId,
    },
    /// Show trigger `trigger`'s widget in `slot` to `viewers`, or to every
    /// player; `cause` is the tank that set it off, or 0.
    Ui {
        trigger: usize,
        slot: u8,
        viewers: Option<Vec<EntityId>>,
        cause: EntityId,
    },
    /// Take `slot`'s widget (every slot for 0) off `viewers`' screens, or
    /// everyone's.
    HideUi {
        slot: u8,
        viewers: Option<Vec<EntityId>>,
    },
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
    /// Shake `tanks`' views, `strength` from 0 to 1, for `seconds`.
    Shake {
        tanks: Vec<EntityId>,
        strength: f32,
        seconds: f32,
    },
    /// Play `cue` at `at`.
    Sound {
        cue: SoundCue,
        at: V2,
    },
    /// Push body `group` at `velocity`, in world units a second.
    Shove {
        group: u8,
        velocity: V2,
    },
    /// Fling `tanks` at `velocity`, in world units a second.
    Fling {
        tanks: Vec<EntityId>,
        velocity: V2,
    },
    /// Throw `tanks` away from `center` at `speed`, or pull them toward it
    /// when `speed` is negative.
    Blast {
        tanks: Vec<EntityId>,
        center: V2,
        speed: f32,
    },
    /// Change `tanks`' views to `view`, turning `focus` percent of the way
    /// toward `at`.
    Zoom {
        tanks: Vec<EntityId>,
        view: ZoomView,
        at: V2,
        focus: u8,
    },
}

/// A Zoom view on a player's screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomView {
    /// 1 as usual; below 1 shows more of the arena, above it closes in.
    pub scale: f32,
    /// How long it lasts, its return included; `None` until the next one.
    pub seconds: Option<f32>,
    /// Seconds the view takes to change, on the way in and back.
    pub ease: f32,
    pub easing: Easing,
}
impl ZoomView {
    /// The change a trigger's 0 ease stands for.
    pub const EASE: f32 = 0.5;
    /// What `trigger`, a Zoom view, does to the view.
    pub fn of(trigger: &MapTrigger) -> Self {
        Self {
            scale: trigger.amount.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1) as f32 / 100.,
            seconds: (!trigger.hold).then(|| trigger.tween.clamp(0.1, ZOOM_SECONDS)),
            ease: if trigger.ease > 0. {
                trigger.ease.clamp(0.1, ZOOM_EASE)
            } else {
                Self::EASE
            },
            easing: trigger.easing,
        }
    }
}

/// A Zoom view as an event value: the zoom in percent above the lowest in
/// the low byte, then its length in tenths of a second (0 until the next),
/// its change in tenths, the curve and the ease mode.
pub fn zoom_event(view: &ZoomView) -> u32 {
    let percent =
        ((view.scale * 100.).round() as i32).clamp(ZOOM_RANGE.0, ZOOM_RANGE.1) - ZOOM_RANGE.0;
    let tenths = view.seconds.map_or(0, |s| {
        (s * 10.).round().clamp(1., ZOOM_SECONDS * 10.) as u32
    });
    let ease = (view.ease * 10.).round().clamp(0., ZOOM_EASE * 10.) as u32;
    let curve = Curve::ALL
        .iter()
        .position(|&c| c == view.easing.curve)
        .unwrap_or(0) as u32;
    let mode = EaseMode::ALL
        .iter()
        .position(|&m| m == view.easing.mode)
        .unwrap_or(0) as u32;
    percent as u32 | tenths << 8 | ease << 18 | curve << 25 | mode << 29
}
/// The view a [`zoom_event`] value describes.
pub fn zoom_of(value: u32) -> ZoomView {
    let tenths = (value >> 8) & 0x3ff;
    ZoomView {
        scale: ((value & 0xff) as i32 + ZOOM_RANGE.0).clamp(ZOOM_RANGE.0, ZOOM_RANGE.1) as f32
            / 100.,
        seconds: (tenths > 0).then(|| (tenths as f32 / 10.).min(ZOOM_SECONDS)),
        ease: (((value >> 18) & 0x7f) as f32 / 10.).min(ZOOM_EASE),
        easing: Easing {
            curve: Curve::ALL
                .get(((value >> 25) & 0xf) as usize)
                .copied()
                .unwrap_or_default(),
            mode: EaseMode::ALL
                .get(((value >> 29) & 0x3) as usize)
                .copied()
                .unwrap_or_default(),
        },
    }
}
/// Where a Zoom view turns as an event value: the spot as fractions of the
/// arena's width and height, 12 bits each, then how far toward it in percent.
pub fn focus_event(at: [f32; 2], percent: u8) -> u32 {
    let fraction = |v: f32| (v.clamp(0., 1.) * 4095.).round() as u32;
    fraction(at[0]) | fraction(at[1]) << 12 | u32::from(percent.min(100)) << 24
}
/// The spot, as fractions of the arena's size, and how far toward it (0 to
/// 1) from a [`focus_event`] value.
pub fn focus_of(value: u32) -> ([f32; 2], f32) {
    (
        [
            (value & 0xfff) as f32 / 4095.,
            ((value >> 12) & 0xfff) as f32 / 4095.,
        ],
        ((value >> 24) & 0x7f).min(100) as f32 / 100.,
    )
}

/// Seconds a view stays in reach past its end, for players whose screens
/// run behind the server.
const VIEW_GRACE: f32 = 0.5;
/// Views kept for one tank; a burst of zooms keeps the newest.
const VIEW_LIMIT: usize = 8;

/// A Zoom view that widens or moves what a player sees, kept so servers
/// send everything on their screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapView {
    pub tank: EntityId,
    pub scale: f32,
    /// Round time it stops counting; infinite until the next view.
    pub until: f32,
    /// The spot it turns toward, and how far, from 0 to 1.
    pub spot: V2,
    pub focus: f32,
}

/// The smallest circle around two circles, each a middle and a radius.
fn enclose(a: (V2, f32), b: (V2, f32)) -> (V2, f32) {
    let apart = a.0.distance(b.0);
    if apart + b.1 <= a.1 {
        return a;
    }
    if apart + a.1 <= b.1 {
        return b;
    }
    let radius = (apart + a.1 + b.1) * 0.5;
    // From a's far side toward b's.
    let middle = a.0.lerp(b.0, (radius - a.1) / apart.max(f32::EPSILON));
    (middle, radius)
}

impl World {
    /// The middle and radius of the arena tank `id`'s player sees: `radius`
    /// around `pos`, widened and moved by the map's Zoom views.
    pub fn view_reach(&self, id: EntityId, pos: V2, radius: f32) -> (V2, f32) {
        self.map_views
            .iter()
            .filter(|view| view.tank == id)
            .fold((pos, radius), |reach, view| {
                let camera = pos.lerp(view.spot, view.focus.clamp(0., 1.));
                enclose(reach, (camera, radius / view.scale.clamp(0.25, 1.)))
            })
    }
}

/// A Shake screen as an event value: strength in percent in the low byte and
/// length in tenths of a second above it.
pub fn shake_event(strength: f32, seconds: f32) -> u32 {
    let strength = (strength * 100.).round().clamp(1., 100.) as u32;
    let tenths = (seconds * 10.).round().clamp(1., SHAKE_SECONDS * 10.) as u32;
    strength | tenths << 8
}
/// Strength (0 to 1) and seconds from a [`shake_event`] value.
pub fn shake_of(value: u32) -> (f32, f32) {
    (
        (value & 0xff).min(100) as f32 / 100.,
        ((value >> 8) as f32 / 10.).min(SHAKE_SECONDS),
    )
}
/// A Play sound as an event value: the sound in the low byte, volume in
/// percent above it. This is what every version reads; [`SoundCue`] adds
/// the rest.
pub fn sound_event(sound: MapSound, volume: f32) -> u32 {
    u32::from(sound.index()) | ((volume * 100.).round().clamp(1., 100.) as u32) << 8
}
/// The sound and volume (0 to 1) from a [`sound_event`] value.
pub fn sound_of(value: u32) -> Option<(MapSound, f32)> {
    let sound = MapSound::from_index((value & 0xff) as u8)?;
    Some((sound, ((value >> 8).min(100)) as f32 / 100.))
}

/// Marks a sound event's subject as carrying a [`SoundCue`].
const CUE_MARK: u64 = 1 << 63;

/// One Play sound as players receive it. The event's value keeps the
/// classic layout, holding the nearest of the first eight sounds, so older
/// versions still play something close; the rest rides in the event's
/// subject, which they don't read for sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundCue {
    pub sound: MapSound,
    /// 0 to 1.
    pub volume: f32,
    /// Semitones, -12 to 12.
    pub pitch: i8,
    /// How far each play wanders either way, in tenths of a semitone.
    pub vary: u8,
    pub range: SoundRange,
    /// How many times it plays, 1 to 8.
    pub plays: u8,
    /// Seconds between plays.
    pub gap: f32,
    /// For [`SoundRange::Cause`], the one tank that hears it; 0 is nobody.
    pub listener: EntityId,
}
impl SoundCue {
    pub fn new(sound: MapSound, volume: f32) -> Self {
        Self {
            sound,
            volume: volume.clamp(0.01, 1.),
            pitch: 0,
            vary: 0,
            range: SoundRange::Near,
            plays: 1,
            gap: 0.,
            listener: 0,
        }
    }
    /// A Play sound trigger's cue, set off by `cause`.
    pub fn of(trigger: &MapTrigger, cause: EntityId) -> Self {
        Self {
            pitch: trigger.pitch.clamp(-SOUND_PITCH, SOUND_PITCH),
            vary: trigger.vary.min(SOUND_VARY),
            range: trigger.range,
            plays: trigger.repeat.clamp(1, SOUND_REPEATS),
            gap: if trigger.gap.is_finite() {
                trigger.gap.clamp(0., SOUND_GAP)
            } else {
                0.
            },
            listener: cause,
            ..Self::new(trigger.sound, trigger.amount.clamp(1, 100) as f32 / 100.)
        }
    }
    /// The event value and subject that carry it.
    pub fn encode(self) -> (u32, u64) {
        let value = sound_event(self.sound.fallback(), self.volume);
        // A listener too large to pack can't be told apart, so nobody hears
        // it rather than everyone.
        let listener = match self.range {
            SoundRange::Cause => u32::try_from(self.listener).unwrap_or(0),
            _ => 0,
        };
        let pitch = (self.pitch.clamp(-SOUND_PITCH, SOUND_PITCH) + SOUND_PITCH) as u64;
        let gap = (self.gap.clamp(0., SOUND_GAP) * 20.).round() as u64;
        let subject = CUE_MARK
            | u64::from(listener)
            | u64::from(self.sound.index()) << 32
            | pitch << 40
            | u64::from(self.vary.min(SOUND_VARY)) << 45
            | (self.range as u64) << 51
            | u64::from(self.plays.clamp(1, SOUND_REPEATS) - 1) << 53
            | gap << 56;
        (value, subject)
    }
    /// The cue a sound event carries. Events from scripts and older servers
    /// have no subject, and play once, nearby, as written.
    pub fn decode(value: u32, subject: u64) -> Option<Self> {
        let (classic, volume) = sound_of(value)?;
        if subject & CUE_MARK == 0 {
            return Some(Self::new(classic, volume));
        }
        let bits = |shift: u32, width: u32| (subject >> shift) & ((1 << width) - 1);
        Some(Self {
            // A sound from a newer version than this one plays as the
            // classic sound it falls back to.
            sound: MapSound::from_index(bits(32, 8) as u8).unwrap_or(classic),
            volume,
            pitch: (bits(40, 5) as i8 - SOUND_PITCH).clamp(-SOUND_PITCH, SOUND_PITCH),
            vary: (bits(45, 6) as u8).min(SOUND_VARY),
            range: SoundRange::ALL[bits(51, 2) as usize],
            plays: bits(53, 3) as u8 + 1,
            gap: (bits(56, 6) as f32 / 20.).min(SOUND_GAP),
            listener: bits(0, 32),
        })
    }
}

/// What scripts ask map logic to do; it happens at the start of the next
/// step, before schedules and areas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LogicInput {
    Signal(u8),
    /// Player `tank` picked a choice that signals `channel`; they are the
    /// tank that set off what listens.
    Pick {
        channel: u8,
        tank: EntityId,
    },
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
    /// Mix `color` into a group at `percent`, as a Tint trigger would.
    Tint {
        group: u8,
        color: [u8; 3],
        percent: f32,
        seconds: f32,
        easing: Easing,
    },
    /// Play or settle an animation, as an Animate trigger would; a `speed`
    /// of 0 keeps the style's own pace.
    Animate {
        group: u8,
        style: AnimateStyle,
        size: f32,
        speed: f32,
        seconds: f32,
        easing: Easing,
    },
    /// Halt a group's motions where they are, as a Stop trigger would.
    Stop(u8),
    /// Send `group` along path `path`, counting from 1, as a Play path
    /// trigger would.
    Path {
        group: u8,
        path: u8,
    },
    /// Change the lighting, as a Lighting trigger would: `darkness` 0 to
    /// 0.95, tinted `color`.
    Light {
        color: [u8; 3],
        darkness: f32,
        seconds: f32,
        easing: Easing,
    },
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

/// Trigger actions per step, so signals that fire each other stop. An
/// action takes tens of nanoseconds, so a full step stays under a
/// millisecond.
const CHAIN_BUDGET: u32 = 32_768;
/// Triggers woken per step, acting or not: a signal heard by many shut
/// gates costs no actions but still takes checking.
const CHECK_BUDGET: u32 = 262_144;
/// Delayed actions waiting at once.
const PENDING_LIMIT: usize = 16_384;
/// Effects one step may raise: sounds, words, widgets and the like reach
/// players, so however much a map computes it can't flood them.
const EFFECT_LIMIT: usize = 4096;
/// Script requests waiting for the next step.
const INBOX_LIMIT: usize = 256;
/// Script events raised in one tick.
pub const MAP_EVENT_LIMIT: usize = 64;

/// Which triggers each event wakes, built once per trigger list so a signal
/// or a counter change reaches its listeners without a search.
#[derive(Clone, Debug, Default)]
struct Listeners {
    /// By signal channel, from 1.
    signal: Vec<Vec<usize>>,
    /// By counter, from 1: triggers waiting for a value, then those
    /// waiting for any change.
    reach: Vec<Vec<usize>>,
    change: Vec<Vec<usize>>,
    /// Triggers watching groups be shown or hidden.
    groups: Vec<usize>,
    /// Triggers waking on any change to their conditions' inputs, which
    /// also wake as the round starts; their counters are in `change`.
    inputs: Vec<usize>,
    /// Those of them whose conditions look at groups, with the groups as bits.
    input_groups: Vec<(usize, u32)>,
    defeated: Vec<usize>,
    /// Triggers waiting for a group's motion to end.
    arrived: Vec<usize>,
    /// Triggers with a schedule or an area; event triggers never need the
    /// per-step scan.
    stepped: Vec<usize>,
    /// The counters the map's labels show, which players need.
    labels: Vec<u8>,
}
impl Listeners {
    fn new(map: &MapDef) -> Self {
        let mut lists = Self {
            signal: vec![Vec::new(); usize::from(MAP_CHANNELS) + 1],
            reach: vec![Vec::new(); usize::from(MAP_COUNTERS) + 1],
            change: vec![Vec::new(); usize::from(MAP_COUNTERS) + 1],
            ..Self::default()
        };
        let mut counters = Vec::new();
        for (index, trigger) in map.triggers.iter().enumerate() {
            if trigger.when == TriggerWhen::Inputs {
                let groups = trigger.inputs(&mut counters);
                for &counter in &counters {
                    lists.change[usize::from(counter)].push(index);
                }
                if groups != 0 {
                    lists.input_groups.push((index, groups));
                }
                lists.inputs.push(index);
                continue;
            }
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
        for decor in &map.decor {
            tankyard_content::counters_named(&decor.text, &mut lists.labels);
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
    /// What's left in each shuffled Random signal's bag, by trigger.
    bags: Vec<(usize, Draws)>,
    /// The trigger acting now and the dice its gate rolled.
    rolled: Option<(usize, u32)>,
    /// The channels each Random signal sent on last, by trigger.
    sent_last: Vec<(usize, Draws)>,
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
    /// The groups and channels whose motion is a return, under way or done.
    homeward: Vec<(u8, TweenChannel)>,
    /// Motions under way: when each ends, and its group and channel. A
    /// motion that another takes over from never arrives.
    arrivals: Vec<(f32, u8, TweenChannel)>,
    /// Where every group stood when this step began.
    poses: GroupPoses,
    /// Delayed actions: when, which trigger, and the tank that set it off.
    pending: Vec<(f32, usize, EntityId)>,
    /// Triggers to fire from signals and counters, in order, each with the
    /// tank that set it off.
    queue: Vec<(usize, EntityId)>,
    /// The tank that set off what is acting now; 0 for none.
    cause: EntityId,
    /// The tank whose crossing the last area check saw.
    crosser: EntityId,
    scratch: Vec<EntityId>,
    /// Requests from scripts, oldest first.
    inbox: Vec<LogicInput>,
    budget: u32,
    /// How long the effects list may grow this step.
    effect_cap: usize,
    /// Woken triggers left to check this step.
    checks: u32,
    /// How many times each trigger has acted this round.
    pub acted: Vec<u32>,
    pub counters: MapCounters,
    /// Whether each On touch or On release trigger's group overlapped its
    /// area last step; None until it first looks.
    touching: Vec<Option<bool>>,
    /// Groups travelling along paths.
    paths: Vec<PathPlay>,
    /// Where pushed bodies sit, by group, in arena fractions; the world
    /// fills this in before each step so areas ride along with them.
    pub body_offsets: Vec<(u8, [f32; 2])>,
}

/// The dice `trigger`'s gate rolls this time: a fresh draw when a Chance
/// condition reads it, and none otherwise, so gates without one leave the
/// round's random numbers as they were.
fn dice_for(trigger: &MapTrigger, random: &mut dyn FnMut() -> f32) -> u32 {
    if trigger.rolls() {
        (random() * 4_294_967_296.) as u32
    } else {
        0
    }
}

/// A set of a Random signal's channels, as offsets from its first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Draws([u64; 4]);
impl Draws {
    const EMPTY: Self = Self([0; 4]);
    /// Offsets 0 to `count - 1`.
    fn first(count: usize) -> Self {
        let mut set = Self::EMPTY;
        for (word, bits) in set.0.iter_mut().enumerate() {
            let filled = count.saturating_sub(word * 64).min(64);
            *bits = if filled == 64 {
                u64::MAX
            } else {
                (1 << filled) - 1
            };
        }
        set
    }
    fn one(offset: usize) -> Self {
        Self::EMPTY.with(offset)
    }
    fn with(mut self, offset: usize) -> Self {
        if let Some(bits) = self.0.get_mut(offset / 64) {
            *bits |= 1 << (offset % 64);
        }
        self
    }
    fn without(self, other: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] & !other.0[i]))
    }
    fn within(self, other: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] & other.0[i]))
    }
    fn count(self) -> usize {
        self.0.iter().map(|bits| bits.count_ones() as usize).sum()
    }
    fn iter(self) -> impl Iterator<Item = usize> {
        (0..4).flat_map(move |word| {
            let mut bits = self.0[word];
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                Some(word * 64 + bit)
            })
        })
    }
}

/// The tanks an area trigger or tank action notices.
fn notices(trigger: &MapTrigger, tank: &Occupant) -> bool {
    (match trigger.who {
        TriggerWho::Anyone | TriggerWho::Cause => true,
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
                LogicInput::Signal(channel) => {
                    self.cause = 0;
                    self.signal(channel);
                }
                LogicInput::Pick { channel, tank } => {
                    self.cause = tank;
                    self.signal(channel);
                    self.cause = 0;
                }
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
                LogicInput::Tint {
                    group,
                    color,
                    percent,
                    seconds,
                    easing,
                } => self.animate(
                    &MapTrigger {
                        action: TriggerAction::Tint,
                        group,
                        color,
                        amount: percent.round() as i32,
                        tween: seconds,
                        easing,
                        duration: 0.,
                        ..Default::default()
                    },
                    now,
                ),
                LogicInput::Animate {
                    group,
                    style,
                    size,
                    speed,
                    seconds,
                    easing,
                } => self.animate(
                    &MapTrigger {
                        action: TriggerAction::Animate,
                        group,
                        style,
                        speed,
                        amount: size.round() as i32,
                        tween: seconds,
                        easing,
                        duration: 0.,
                        ..Default::default()
                    },
                    now,
                ),
                LogicInput::Stop(group) => self.stop(group, now),
                LogicInput::Path { group, path } => {
                    self.play_path(map, group, i32::from(path), now);
                }
                LogicInput::Light {
                    color,
                    darkness,
                    seconds,
                    easing,
                } => self.light(map, color, darkness, (seconds, 0.), easing, now),
                LogicInput::Defeated(kind) => {
                    let number = tankyard_content::BOSS_IDS
                        .iter()
                        .position(|&id| id == kind.id())
                        .map_or(0, |i| i + 1);
                    for &index in &self.listeners.defeated {
                        let wanted = usize::from(map.triggers[index].source);
                        if wanted == 0 || wanted == number {
                            self.queue.push((index, 0));
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
        // A group on its way along a path arrives at the end, not at each
        // point.
        for play in &self.paths {
            arrived &= !MapDef::group_bit(play.group);
        }
        for &index in &self.listeners.arrived {
            if MapDef::group_bit(map.triggers[index].source) & arrived != 0 {
                self.queue.push((index, 0));
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
                self.queue.push((index, self.cause));
            }
        }
        for &(index, groups) in &self.listeners.input_groups {
            if changed & groups != 0 {
                self.queue.push((index, self.cause));
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
        self.bags.clear();
        self.sent_last.clear();
        self.acted = vec![0; count];
        self.touching = vec![None; count];
        self.paths.clear();
        self.listeners = Listeners::new(map);
        self.seen = hidden;
        self.reverts.clear();
        self.tweens.clear();
        self.returns.clear();
        self.homeward.clear();
        self.arrivals.clear();
        self.poses = GroupPoses::default();
        self.pending.clear();
        self.queue.clear();
        // Input watchers settle once as the round starts, so a gate's
        // counter is right before any of its inputs changes.
        let inputs = &self.listeners.inputs;
        self.queue.extend(inputs.iter().map(|&index| (index, 0)));
        self.cause = 0;
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
                homeward: std::mem::take(&mut self.homeward),
                paths: std::mem::take(&mut self.paths),
                body_offsets: std::mem::take(&mut self.body_offsets),
                ..Self::default()
            };
            kept.take_inputs(map, now, hidden);
            kept.advance_paths(map, now);
            kept.poses = kept.current_poses(now);
            *self = kept;
            return;
        }
        self.prepare(map, now, *hidden);
        self.budget = CHAIN_BUDGET;
        self.checks = CHECK_BUDGET;
        self.effect_cap = effects.len() + EFFECT_LIMIT;
        self.alive.clear();
        self.alive.extend(tanks.iter().map(|tank| tank.id));
        self.alive.sort_unstable();
        self.take_inputs(map, now, hidden);
        self.poses = self.current_poses(now);
        self.reverts.retain(|&(at, group, hide)| {
            if now < at {
                return true;
            }
            let bit = MapDef::group_bit(group);
            *hidden = if hide { *hidden | bit } else { *hidden & !bit };
            false
        });
        self.play_returns(now);
        if self.pending.iter().any(|&(at, ..)| at <= now) {
            let mut due = Vec::new();
            self.pending.retain(|&(at, index, cause)| {
                if at <= now {
                    due.push((at, index, cause));
                    false
                } else {
                    true
                }
            });
            due.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (_, index, cause) in due {
                self.cause = cause;
                self.act(index, map, config, now, hidden, tanks, random, effects);
            }
        }
        for cursor in 0..self.listeners.stepped.len() {
            let index = self.listeners.stepped[cursor];
            let trigger = &map.triggers[index];
            // Track every crossing, even while spent or cooling down, so a tank
            // already standing inside never counts as entering later.
            let crossed = trigger.when.watches_area() && self.track(index, map, config, tanks);
            let touched = trigger.when.watches_blocks() && self.touch(index, map, *hidden);
            if self.spent[index] || now < self.next[index] {
                continue;
            }
            self.cause = match trigger.when {
                TriggerWhen::Occupied => self.inside[index].first().copied().unwrap_or(0),
                when if when.watches_area() => self.crosser,
                _ => 0,
            };
            let fire = match trigger.when {
                TriggerWhen::Every => {
                    // Catch up without firing twice in one step.
                    let period = trigger.seconds.max(EVERY_LEAST);
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
                TriggerWhen::Touch | TriggerWhen::Release => {
                    if touched {
                        self.next[index] = now + trigger.seconds.max(0.25);
                    }
                    touched
                }
                TriggerWhen::Signal
                | TriggerWhen::Counter
                | TriggerWhen::Changed
                | TriggerWhen::Inputs
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
        self.advance_paths(map, now);
        self.follow(map, config, now, hidden, tanks, random, effects);
    }

    /// Where every group stands at `now`: its motions, and its pushes if it
    /// is a body.
    fn current_poses(&self, now: f32) -> GroupPoses {
        let mut poses = GroupPoses::at(&self.tweens, now);
        for &(group, offset) in &self.body_offsets {
            poses.nudge(group, offset);
        }
        poses
    }

    /// Update whether trigger `index`'s group overlaps its area; true on the
    /// change it waits for. The first look only notes where things stand.
    fn touch(&mut self, index: usize, map: &MapDef, hidden: u32) -> bool {
        let trigger = &map.triggers[index];
        let area = self.poses.shift(trigger.rect, trigger.member);
        let group = trigger.source;
        let now_touching = MapDef::group_bit(group) & hidden == 0
            && map.blocks.iter().any(|block| {
                block.group == group && overlaps(self.poses.shift(block.rect, group), area)
            });
        let before = self.touching[index].replace(now_touching);
        match (before, trigger.when) {
            (Some(false), TriggerWhen::Touch) => now_touching,
            (Some(true), TriggerWhen::Release) => !now_touching,
            _ => false,
        }
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
                // Past the check budget the rest of this step's wake-ups go.
                if self.checks == 0 {
                    cursor = self.queue.len();
                    break;
                }
                self.checks -= 1;
                let (index, cause) = self.queue[cursor];
                cursor += 1;
                self.cause = cause;
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
        self.cause = 0;
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
        self.poses = self.current_poses(now);
        self.budget = CHAIN_BUDGET;
        self.checks = CHECK_BUDGET;
        self.effect_cap = effects.len() + EFFECT_LIMIT;
        self.alive.clear();
        self.alive.extend(tanks.iter().map(|tank| tank.id));
        self.alive.sort_unstable();
        self.cause = 0;
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
        let crosser = match trigger.when {
            TriggerWhen::Enter => self
                .scratch
                .iter()
                .find(|id| previous.binary_search(id).is_err()),
            // Leaving means still alive and outside; dying inside is not leaving.
            TriggerWhen::Exit => previous
                .iter()
                .find(|id| self.scratch.binary_search(id).is_err() && alive(id)),
            TriggerWhen::Cleared => previous.first().filter(|_| self.scratch.is_empty()),
            // Only living tanks are listed, so one that vanished from inside died there.
            TriggerWhen::Destroyed => previous.iter().find(|id| !alive(id)),
            _ => None,
        }
        .copied();
        self.crosser = crosser.unwrap_or(0);
        std::mem::swap(&mut self.inside[index], &mut self.scratch);
        crosser.is_some()
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
        // A gate with Chance conditions rolls once as the trigger fires, and
        // a gate to counter acting now stores that same roll's answer.
        let dice = dice_for(trigger, random);
        if trigger.only_if.is_some() && !self.gate_open(trigger, config, now, *hidden, tanks, dice)
        {
            // The otherwise signal counts against the step's budget, so a
            // shut gate signalling itself still stops.
            if trigger.otherwise > 0 && self.budget > 0 {
                self.budget -= 1;
                self.signal(trigger.otherwise);
            }
            // A gate's result goes to its counter open or shut.
            if trigger.action != TriggerAction::Output {
                return false;
            }
        }
        if trigger.chance < 100 && random() * 100. >= f32::from(trigger.chance) {
            return false;
        }
        if trigger.once {
            self.spent[index] = true;
        }
        if trigger.delay > 0. {
            if self.pending.len() < PENDING_LIMIT {
                self.pending.push((now + trigger.delay, index, self.cause));
            }
        } else {
            self.rolled = Some((index, dice));
            self.act(index, map, config, now, hidden, tanks, random, effects);
            self.rolled = None;
        }
        true
    }

    /// Whether `trigger`'s gate is open now, with its Chance conditions
    /// reading `dice`.
    fn gate_open(
        &self,
        trigger: &MapTrigger,
        config: &WorldConfig,
        now: f32,
        hidden: u32,
        tanks: &[Occupant],
        dice: u32,
    ) -> bool {
        trigger.gate_open(&GateState {
            dice,
            ..gate_state(
                trigger,
                config,
                &self.counters,
                now,
                hidden,
                &self.poses,
                tanks,
            )
        })
    }

    /// The tanks `trigger`'s action reaches: the noticed tanks in `area`,
    /// or only the tank that set it off.
    fn affected(&self, trigger: &MapTrigger, area: Rect, tanks: &[Occupant]) -> Vec<EntityId> {
        tanks
            .iter()
            .filter(|tank| match trigger.who {
                TriggerWho::Cause => {
                    self.cause != 0
                        && tank.id == self.cause
                        && (trigger.team == 0 || trigger.team == tank.team)
                }
                _ => notices(trigger, tank) && contains(area, tank.pos),
            })
            .map(|tank| tank.id)
            .collect()
    }

    /// The tank that set off what is acting now; 0 for none.
    pub fn cause(&self) -> EntityId {
        self.cause
    }

    /// Where `trigger`'s area is now: it moves with the trigger's own group.
    fn area(&self, config: &WorldConfig, trigger: &MapTrigger) -> Rect {
        map_rect(config, self.poses.shift(trigger.rect, trigger.member))
    }

    /// The value `group` has on `channel` at `now`.
    pub fn tween_value(&self, group: u8, channel: TweenChannel, now: f32) -> TweenValue {
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
        for mut back in due {
            let mut from = self.tween_value(back.group, back.channel, now);
            if back.channel.style().is_some() {
                // Settle at the pace it goes back to, from where the cycle is.
                back.to[2] = keep_phase(now, from, back.to[1]);
                from[1] = back.to[1];
                from[2] = back.to[2];
            }
            self.start_tween(Tween {
                from,
                start: now,
                ..back
            });
            if !self.homeward.contains(&(back.group, back.channel)) {
                self.homeward.push((back.group, back.channel));
            }
        }
    }

    /// Whether any group has moved, turned or faded, or will go back.
    pub fn in_motion(&self) -> bool {
        !self.tweens.is_empty() || !self.returns.is_empty() || !self.paths.is_empty()
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
        // A return or replacement may begin on the same tick the old motion
        // finished. Preserve that completed arrival for the watchers below,
        // while cancelling future superseded arrivals. One completed entry
        // per channel keeps chains bounded even with many instant motions.
        let mut completed = false;
        self.arrivals.retain(|&(at, group, channel)| {
            if group != tween.group || channel != tween.channel {
                return true;
            }
            if at <= tween.start && !completed {
                completed = true;
                true
            } else {
                false
            }
        });
        // A motion with nowhere to go, like a door told to open while it is
        // open, arrives nowhere.
        if tween.from != tween.to {
            self.arrivals
                .push((tween.start + tween.length, tween.group, tween.channel));
        }
    }

    /// Move, Move to, Turn, Fade, Tint, Scale or Animate: glide the group
    /// from where it is now.
    fn animate(&mut self, trigger: &MapTrigger, now: f32) {
        let group = trigger.group;
        if !(1..=MAP_GROUPS).contains(&group) {
            return;
        }
        let channel = match trigger.action {
            TriggerAction::Turn => TweenChannel::Turn,
            TriggerAction::Fade => TweenChannel::Fade,
            TriggerAction::Tint => TweenChannel::Tint,
            TriggerAction::Scale => TweenChannel::Scale,
            TriggerAction::Animate => trigger.style.channel(),
            _ => TweenChannel::Move,
        };
        if channel == TweenChannel::Move {
            self.paths.retain(|play| play.group != group);
        }
        let here = self.tween_value(group, channel, now);
        // Offsets stay within a few arenas however often a Move repeats, and
        // turns wrap so they never lose precision.
        let reach = tankyard_content::TRAVEL_LIMIT * 2.;
        let (mut from, wrapped) = if channel == TweenChannel::Turn {
            let turn = here[0] % 360.;
            ([turn, 0., 0., 0.], here[0] - turn)
        } else {
            (here, 0.)
        };
        let percent = |high: i32| trigger.amount.clamp(0, high) as f32 / 100.;
        let to = match trigger.action {
            TriggerAction::Move => [
                (from[0] + trigger.travel[0]).clamp(-reach, reach),
                (from[1] + trigger.travel[1]).clamp(-reach, reach),
                0.,
                0.,
            ],
            TriggerAction::Place => [trigger.travel[0], trigger.travel[1], 0., 0.],
            TriggerAction::Turn => [from[0] + trigger.amount as f32, 0., 0., 0.],
            TriggerAction::Tint => {
                let [r, g, b] = trigger.color.map(|c| f32::from(c) / 255.);
                // A tint starting from none takes its colour at once, and one
                // that ends keeps it, so neither passes through another hue.
                if from[3] <= 0. {
                    from = [r, g, b, 0.];
                }
                match percent(100) {
                    0. => [from[0], from[1], from[2], 0.],
                    strength => [r, g, b, strength],
                }
            }
            TriggerAction::Scale => [percent((SCALE_LIMIT * 100.) as i32), 0., 0., 0.],
            TriggerAction::Animate => {
                let speed = trigger.animate_speed();
                let phase = keep_phase(now, here, speed);
                from = [here[0], speed, phase, 0.];
                let size = trigger.amount.clamp(0, trigger.style.limit()) as f32;
                [size, speed, phase, 0.]
            }
            _ => [percent(100), 0., 0., 0.],
        };
        // Going back takes the group to where it rested before it set off,
        // even when it sets off again on the way out or back: a door that
        // tanks keep opening still shuts all the way.
        let slot = |g: u8, c: TweenChannel| g == group && c == channel;
        let rest = self
            .returns
            .iter()
            .find(|(_, back)| slot(back.group, back.channel))
            .map(|(_, back)| back.to)
            .or_else(|| {
                self.homeward
                    .contains(&(group, channel))
                    .then(|| self.tweens.iter().find(|t| slot(t.group, t.channel)))
                    .flatten()
                    .map(|t| t.to)
            });
        let home = rest.map_or(from, |mut rest| {
            rest[0] -= wrapped;
            rest
        });
        let tween = Tween {
            group,
            channel,
            from,
            to,
            start: now,
            length: trigger.tween.clamp(0., tankyard_content::TWEEN_SECONDS),
            easing: trigger.easing,
        };
        // A new motion replaces any return the group was waiting for, and
        // one that doesn't go back leaves the group where it ends.
        self.returns
            .retain(|(_, back)| !slot(back.group, back.channel));
        self.homeward.retain(|&(g, c)| !slot(g, c));
        if trigger.duration > 0. && home != to {
            self.returns.push((
                now + tween.length + trigger.duration,
                Tween {
                    from: to,
                    to: home,
                    ..tween
                },
            ));
        }
        self.start_tween(tween);
    }

    /// Stop: hold every motion of `group` where it is now, drop the trip back
    /// it was waiting for and freeze its animations mid-cycle. Nothing it
    /// was doing arrives.
    fn stop(&mut self, group: u8, now: f32) {
        if !(1..=MAP_GROUPS).contains(&group) {
            return;
        }
        for tween in self.tweens.iter_mut().filter(|t| t.group == group) {
            let mut here = tween.value(now);
            if tween.channel.style().is_some() {
                here[2] = (now * here[1] + here[2]).rem_euclid(1.);
                here[1] = 0.;
            }
            *tween = Tween {
                from: here,
                to: here,
                start: now,
                length: 0.,
                ..*tween
            };
        }
        self.returns.retain(|(_, back)| back.group != group);
        self.homeward.retain(|&(g, _)| g != group);
        self.arrivals.retain(|&(_, g, _)| g != group);
        self.paths.retain(|play| play.group != group);
    }

    /// Queue every trigger listening on signal channel `channel`.
    fn signal(&mut self, channel: u8) {
        let cause = self.cause;
        // Wake-ups past what this step can still check would only be dropped.
        let room = (self.checks as usize).saturating_sub(self.queue.len());
        if let Some(listeners) = self.listeners.signal.get(usize::from(channel)) {
            self.queue
                .extend(listeners.iter().take(room).map(|&index| (index, cause)));
        }
    }

    /// Start the triggers of `group` afresh (every trigger for 0).
    fn reset(&mut self, map: &MapDef, group: u8, now: f32) {
        let chosen = |index: usize| group == 0 || map.triggers[index].member == group;
        self.pending.retain(|&(_, index, _)| !chosen(index));
        self.bags.retain(|&(index, _)| !chosen(index));
        self.sent_last.retain(|&(index, _)| !chosen(index));
        for (index, trigger) in map.triggers.iter().enumerate() {
            if chosen(index) {
                self.spent[index] = false;
                self.next[index] = schedule(trigger, now);
                self.steps[index] = 0;
            }
        }
    }

    /// The channels, as offsets from the first, that Random signal
    /// `index` sends on this time.
    fn pick_random(
        &mut self,
        index: usize,
        trigger: &MapTrigger,
        channels: usize,
        random: &mut dyn FnMut() -> f32,
    ) -> Vec<u8> {
        let picks = usize::from(trigger.picks).clamp(1, channels);
        // One pick from any channel draws exactly as it always has, so maps
        // made before the other ways keep their dice.
        if trigger.pick == RandomPick::Any && picks == 1 {
            let pick = (random() * channels as f32).floor() as usize;
            return vec![pick.min(channels - 1) as u8];
        }
        let all = Draws::first(channels);
        let last = self.last_sent(index);
        let shuffle = trigger.pick == RandomPick::Shuffle;
        // A shuffle deals from what's left of its bag; the rest draw from
        // every channel each time.
        let mut bag = if shuffle {
            self.bag(index).within(all)
        } else {
            all
        };
        let weight = |offset: usize| match trigger.pick {
            RandomPick::Weighted => trigger.weights.get(offset).map_or(0., |&w| f32::from(w)),
            _ => 1.,
        };
        let mut sent = Draws::EMPTY;
        let mut order = Vec::with_capacity(picks);
        while order.len() < picks {
            if shuffle && bag.without(sent).count() == 0 {
                // Every channel has had its turn: a new round.
                bag = all;
            }
            let open = bag.without(sent);
            // No repeats, and a shuffle as its bag turns over, leave out
            // what went last time while anything else is left.
            let open = match trigger.pick {
                RandomPick::Fresh | RandomPick::Shuffle => match open.without(last) {
                    rest if rest.count() > 0 => rest,
                    _ => open,
                },
                _ => open,
            };
            let total: f32 = open.iter().map(weight).sum();
            if total <= 0. {
                break;
            }
            let mut left = random() * total;
            let mut chosen = None;
            for offset in open.iter() {
                let w = weight(offset);
                if w <= 0. {
                    continue;
                }
                chosen = Some(offset);
                if left < w {
                    break;
                }
                left -= w;
            }
            let Some(offset) = chosen else { break };
            bag = bag.without(Draws::one(offset));
            sent = sent.with(offset);
            order.push(offset as u8);
        }
        if shuffle {
            self.bags.retain(|&(i, _)| i != index);
            self.bags.push((index, bag));
        }
        self.sent_last.retain(|&(i, _)| i != index);
        self.sent_last.push((index, sent));
        order
    }

    /// The channels Random signal `index` sent on last, as offsets.
    fn last_sent(&self, index: usize) -> Draws {
        self.sent_last
            .iter()
            .find(|&&(i, _)| i == index)
            .map_or(Draws::EMPTY, |&(_, set)| set)
    }
    /// What's left in shuffled Random signal `index`'s bag.
    fn bag(&self, index: usize) -> Draws {
        self.bags
            .iter()
            .find(|&&(i, _)| i == index)
            .map_or(Draws::EMPTY, |&(_, set)| set)
    }

    /// Store `value` in counter `counter` (1 to 255) and queue the triggers
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
                self.queue.push((other, self.cause));
            }
        }
        let cause = self.cause;
        if old != new
            && let Some(listeners) = self.listeners.change.get(counter)
        {
            self.queue
                .extend(listeners.iter().map(|&index| (index, cause)));
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
        if effects.len() >= self.effect_cap && raises_effect(trigger.action) {
            return;
        }
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
            | TriggerAction::Fade
            | TriggerAction::Tint
            | TriggerAction::Scale
            | TriggerAction::Animate => self.animate(trigger, now),
            TriggerAction::Stop => self.stop(trigger.group, now),
            TriggerAction::Path => self.play_path(map, trigger.group, trigger.amount, now),
            TriggerAction::Light => self.light(
                map,
                trigger.color,
                trigger.amount as f32 / 100.,
                (trigger.tween, trigger.duration),
                trigger.easing,
                now,
            ),
            TriggerAction::Shove => effects.push(LogicEffect::Shove {
                group: trigger.group,
                velocity: V2::from_angle(trigger.angle.to_radians())
                    * trigger.amount.clamp(1, tankyard_content::SHOVE_LIMIT) as f32,
            }),
            TriggerAction::Sound => {
                let area = self.area(config, trigger);
                effects.push(LogicEffect::Sound {
                    cue: SoundCue::of(trigger, self.cause),
                    at: V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5),
                });
            }
            TriggerAction::Announce => effects.push(LogicEffect::Announce {
                text: tankyard_content::fill_counters(trigger.text.trim(), &self.counters)
                    .into_owned(),
                cause: self.cause,
            }),
            TriggerAction::Ui | TriggerAction::HideUi => {
                let viewers = trigger.ui.as_deref().and_then(|ui| {
                    let players = |tank: &&Occupant| {
                        !tank.bot && (trigger.team == 0 || trigger.team == tank.team)
                    };
                    match ui.to {
                        UiAudience::Everyone if trigger.team == 0 => None,
                        UiAudience::Everyone => {
                            Some(tanks.iter().filter(players).map(|tank| tank.id).collect())
                        }
                        UiAudience::Inside => {
                            let area = self.area(config, trigger);
                            Some(
                                tanks
                                    .iter()
                                    .filter(players)
                                    .filter(|tank| contains(area, tank.pos))
                                    .map(|tank| tank.id)
                                    .collect(),
                            )
                        }
                        UiAudience::Cause => Some(
                            tanks
                                .iter()
                                .filter(players)
                                .filter(|tank| self.cause != 0 && tank.id == self.cause)
                                .map(|tank| tank.id)
                                .collect(),
                        ),
                    }
                });
                // Showing to nobody in particular is nothing to do.
                if viewers
                    .as_ref()
                    .is_none_or(|list: &Vec<EntityId>| !list.is_empty())
                {
                    effects.push(if trigger.action == TriggerAction::Ui {
                        LogicEffect::Ui {
                            trigger: index,
                            slot: trigger.group,
                            viewers,
                            cause: self.cause,
                        }
                    } else {
                        LogicEffect::HideUi {
                            slot: trigger.group,
                            viewers,
                        }
                    });
                }
            }
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
                let channels = trigger.amount.clamp(1, i32::from(MAP_CHANNELS)) as usize;
                for offset in self.pick_random(index, trigger, channels, random) {
                    self.signal(trigger.group.saturating_add(offset));
                }
            }
            TriggerAction::Sequence => {
                let choices = trigger.amount.clamp(1, i32::from(MAP_CHANNELS)) as u8;
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
            TriggerAction::Output => {
                let dice = match self.rolled.take() {
                    Some((rolled, dice)) if rolled == index => dice,
                    _ => dice_for(trigger, random),
                };
                let open = trigger.only_if.is_none()
                    || self.gate_open(trigger, config, now, *hidden, tanks, dice);
                let value = if open { trigger.amount } else { 0 };
                self.set_counter(map, trigger.group, value);
            }
            TriggerAction::Calculate => {
                let counter = |n: usize| {
                    n.checked_sub(1)
                        .and_then(|slot| self.counters.get(slot).copied())
                };
                // A counter's number, moved along by another counter's value
                // when `by` is set: lists of counters, read and written by
                // position.
                let number = |base: u8, by: u8| -> Option<usize> {
                    if by == 0 {
                        return Some(usize::from(base));
                    }
                    usize::try_from(i64::from(base) + i64::from(counter(usize::from(by))?)).ok()
                };
                let target = number(trigger.group, trigger.target_offset)
                    .filter(|n| (1..=usize::from(MAP_COUNTERS)).contains(n));
                if let Some(target) = target
                    && let Some(value) = counter(target)
                {
                    let other = if trigger.operand > 0 || trigger.operand_offset > 0 {
                        number(trigger.operand, trigger.operand_offset)
                            .and_then(counter)
                            .unwrap_or(0)
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
                        self.set_counter(map, target as u8, result);
                    }
                }
            }
            TriggerAction::Script => {
                let area = self.area(config, trigger);
                effects.push(LogicEffect::Script {
                    name: trigger.text.trim().to_owned(),
                    trigger: index,
                    tanks: self.affected(trigger, area, tanks),
                    area,
                });
            }
            TriggerAction::Heal
            | TriggerAction::Hurt
            | TriggerAction::Award
            | TriggerAction::Shake
            | TriggerAction::Fling
            | TriggerAction::Blast
            | TriggerAction::Zoom => {
                let area = self.area(config, trigger);
                let ids = self.affected(trigger, area, tanks);
                let percent = trigger.amount.clamp(1, 100) as f32;
                let speed = trigger.amount.clamp(
                    -tankyard_content::FLING_LIMIT,
                    tankyard_content::FLING_LIMIT,
                ) as f32;
                effects.push(match trigger.action {
                    TriggerAction::Fling => LogicEffect::Fling {
                        tanks: ids,
                        velocity: V2::from_angle(trigger.angle.to_radians()) * speed.abs(),
                    },
                    TriggerAction::Blast => LogicEffect::Blast {
                        tanks: ids,
                        center: V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5),
                        speed,
                    },
                    TriggerAction::Zoom => LogicEffect::Zoom {
                        tanks: ids,
                        view: ZoomView::of(trigger),
                        at: V2::new(area.x + area.w * 0.5, area.y + area.h * 0.5),
                        focus: trigger.focus.min(100),
                    },
                    TriggerAction::Shake => LogicEffect::Shake {
                        tanks: ids,
                        strength: percent / 100.,
                        seconds: trigger.tween.clamp(0.1, SHAKE_SECONDS),
                    },
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
    /// The counters the map's labels show, with their values.
    pub fn shown_counters(&self) -> impl Iterator<Item = (u8, i32)> + '_ {
        self.listeners.labels.iter().filter_map(|&counter| {
            let slot = usize::from(counter).checked_sub(1)?;
            Some((counter, *self.counters.get(slot)?))
        })
    }
}

/// The phase that keeps an animation at the same point of its cycle at
/// `now` when its pace changes to `speed`, so a new pace never jumps.
fn keep_phase(now: f32, current: TweenValue, speed: f32) -> f32 {
    let cycle = now * current[1] + current[2];
    (cycle - now * speed).rem_euclid(1.)
}

/// When a trigger first fires on its own, for rounds starting at `from`.
fn schedule(trigger: &MapTrigger, from: f32) -> f32 {
    match trigger.when {
        TriggerWhen::Every => from + trigger.offset + trigger.seconds.max(EVERY_LEAST),
        TriggerWhen::Start => from + trigger.seconds,
        _ => 0.,
    }
}

/// Whether `action` raises an effect for the world to carry out, rather
/// than changing map logic's own state.
fn raises_effect(action: TriggerAction) -> bool {
    action.affects_tanks()
        || matches!(
            action,
            TriggerAction::Shove
                | TriggerAction::Sound
                | TriggerAction::Announce
                | TriggerAction::Ui
                | TriggerAction::HideUi
                | TriggerAction::Boss
                | TriggerAction::Shapes
                | TriggerAction::Script
        )
}

/// What `trigger`'s gate sees: the counters, the groups, the round time and
/// the noticed tanks in its area, counted only when a condition asks.
#[allow(clippy::too_many_arguments)]
pub fn gate_state<'a>(
    trigger: &MapTrigger,
    config: &WorldConfig,
    counters: &'a [i32],
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
        dice: 0,
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
        // Lighting always travels: its rest is the map's own, not the
        // channel's.
        let shown = |t: &&Tween| {
            t.channel == TweenChannel::Ambient
                || t.to != t.channel.rest()
                || now < t.start + t.length + 1.
        };
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
        let poses = self.composed_poses(now);
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
                for tank in self.tanks.iter_mut().filter(|t| t.tangible()) {
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
        // Rifts rest tanks too, on maps without a single pad.
        self.portal_rest.retain(|_, rest| {
            *rest -= DT;
            *rest > 0.
        });
        if self.map.zones.is_empty() {
            for tank in &mut self.tanks {
                tank.terrain = 1.;
                tank.push = V2::ZERO;
                tank.grip = 1.;
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
        for index in 0..self.tanks.len() {
            let tank = &self.tanks[index];
            if !tank.tangible() {
                self.tanks[index].terrain = 1.;
                self.tanks[index].push = V2::ZERO;
                self.tanks[index].grip = 1.;
                continue;
            }
            let (id, pos, radius) = (tank.id, tank.pos, tank.radius);
            let (mut terrain, mut push, mut repair, mut hazard) = (1., V2::ZERO, 0., 0.);
            let mut grip = 1.;
            let mut fling = V2::ZERO;
            let mut portal = None;
            for &(zone_index, area) in &areas {
                if !contains(area, pos) {
                    continue;
                }
                let zone = &self.map.zones[zone_index];
                terrain = speed_of(terrain, zone);
                push += push_of(zone, area, pos);
                grip = grip_of(grip, zone);
                fling += fling_of(zone);
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
            tank.grip = grip.clamp(0.05, 1.);
            let launched = launch_tank(tank, fling.clamp_length(LAUNCH_SPEED * 2.));
            if repair > 0. && tank.health < tank.max_health {
                tank.health = (tank.health + tank.max_health * repair * DT).min(tank.max_health);
            }
            if launched {
                let team = self.tanks[index].team;
                self.event(EventKind::Launch, pos, id, 0, team);
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

    /// Fling tank `id` at `velocity`, as launch pads do.
    pub(crate) fn fling_tank(&mut self, id: EntityId, velocity: V2) {
        let Some(tank) = self.tanks.iter_mut().find(|t| t.id == id && t.tangible()) else {
            return;
        };
        if launch_tank(tank, velocity) {
            let (pos, team) = (tank.pos, tank.team);
            self.event(EventKind::Launch, pos, id, 0, team);
        }
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
                .filter(|tank| tank.tangible())
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
        self.body_offsets(&mut logic.body_offsets);
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
        self.match_state.shown_counters.clear();
        self.match_state
            .shown_counters
            .extend(logic.shown_counters());
        let alive = &self.tanks;
        self.map_views.retain(|view| {
            view.until > now && alive.iter().any(|tank| tank.id == view.tank && tank.alive)
        });
        self.logic = logic;
        self.logic_tanks = tanks;
        for effect in effects {
            match effect {
                LogicEffect::Announce { text, cause } => {
                    let name = self
                        .tanks
                        .iter()
                        .find(|tank| tank.id == cause)
                        .map_or("", |tank| tank.name.as_str());
                    self.match_state.announcement =
                        tankyard_content::clip_text(&text.replace("{cause}", name), 256);
                }
                LogicEffect::Ui {
                    trigger,
                    slot,
                    viewers,
                    cause,
                } => self.show_ui(trigger, slot, viewers, cause),
                LogicEffect::HideUi { slot, viewers } => self.hide_ui(slot, viewers),
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
                LogicEffect::Shake {
                    tanks,
                    strength,
                    seconds,
                } => {
                    let value = shake_event(strength, seconds);
                    for id in tanks {
                        if let Some(tank) = self.tanks.iter().find(|t| t.id == id && t.alive) {
                            let (pos, team) = (tank.pos, tank.team);
                            self.event(EventKind::MapShake, pos, id, value, team);
                        }
                    }
                }
                LogicEffect::Sound { cue, at } => {
                    let (value, subject) = cue.encode();
                    self.event(EventKind::MapSound, at, subject, value, 0);
                }
                LogicEffect::Shove { group, velocity } => {
                    self.shove_body(group, velocity);
                }
                LogicEffect::Fling { tanks, velocity } => {
                    for id in tanks {
                        self.fling_tank(id, velocity);
                    }
                }
                LogicEffect::Blast {
                    tanks,
                    center,
                    speed,
                } => {
                    for id in tanks {
                        let Some(tank) = self.tanks.iter().find(|t| t.id == id && t.alive) else {
                            continue;
                        };
                        let away = tank.pos - center;
                        // A tank right at the center goes the way it faces.
                        let direction = if away.length_squared() > 1. {
                            away.normalized()
                        } else {
                            V2::from_angle(tank.aim)
                        };
                        self.fling_tank(id, direction * speed);
                    }
                }
                LogicEffect::Zoom {
                    tanks,
                    view,
                    at,
                    focus,
                } => {
                    let value = zoom_event(&view);
                    let spot = [
                        at.x / self.config.width.max(1.),
                        at.y / self.config.height.max(1.),
                    ];
                    let aim = focus_event(spot, focus);
                    for id in tanks {
                        let Some(tank) = self.tanks.iter().find(|t| t.id == id && t.alive) else {
                            continue;
                        };
                        let (pos, team) = (tank.pos, tank.team);
                        // Both ride at the tank, so its player always gets
                        // them, the zoom first.
                        self.event(EventKind::MapZoom, pos, id, value, team);
                        if focus > 0 {
                            self.event(EventKind::MapFocus, pos, id, aim, team);
                        }
                        // Earlier views stay in reach while this one eases in.
                        let handover = now + view.ease + VIEW_GRACE;
                        for other in self.map_views.iter_mut().filter(|v| v.tank == id) {
                            other.until = other.until.min(handover);
                        }
                        if view.scale < 1. || focus > 0 {
                            let mut extra = (self.map_views.iter().filter(|v| v.tank == id))
                                .count()
                                .saturating_sub(VIEW_LIMIT - 1);
                            self.map_views.retain(|v| {
                                let old = extra > 0 && v.tank == id;
                                extra -= usize::from(old);
                                !old
                            });
                            self.map_views.push(MapView {
                                tank: id,
                                scale: view.scale,
                                until: view.seconds.map_or(f32::INFINITY, |s| now + s + VIEW_GRACE),
                                spot: at,
                                focus: f32::from(focus) / 100.,
                            });
                        }
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
    use tankyard_content::{Compare, MapBlock, MapDecor, MapTrigger, TestKind, TriggerTest};
    // RandomPick and SoundRange come in with the module's own imports.

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
    fn short_return_delays_do_not_swallow_the_outward_arrival() {
        for action in [TriggerAction::Scale, TriggerAction::Animate] {
            let mut bench = Bench::new(vec![
                MapTrigger {
                    group: 2,
                    amount: if action == TriggerAction::Scale {
                        140
                    } else {
                        14
                    },
                    tween: 0.02,
                    duration: 0.001,
                    seconds: 0.,
                    ..trigger(TriggerWhen::Start, action)
                },
                MapTrigger {
                    source: 2,
                    group: 1,
                    amount: 1,
                    seconds: 0.,
                    ..trigger(TriggerWhen::Arrived, TriggerAction::Count)
                },
            ]);
            bench.run(0.3, &[]);
            assert_eq!(
                bench.logic.counters[0], 2,
                "{action:?}: outward and return each arrive"
            );
        }
    }

    #[test]
    fn stop_holds_motions_where_they_are_and_cancels_the_trip_back() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 3,
                travel: [0.2, 0.],
                tween: 2.,
                duration: 4.,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Place)
            },
            MapTrigger {
                group: 3,
                delay: 1.,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Stop)
            },
            // Would count when the glide or the trip back arrives.
            MapTrigger {
                source: 3,
                group: 1,
                amount: 1,
                seconds: 0.,
                ..trigger(TriggerWhen::Arrived, TriggerAction::Count)
            },
        ]);
        bench.run(1.5, &[]);
        let held = GroupPoses::at(&bench.logic.tweens, bench.now).offset(3)[0];
        assert!(held > 0.01 && held < 0.19, "{held}");
        bench.run(10., &[]);
        let poses = GroupPoses::at(&bench.logic.tweens, bench.now);
        assert_eq!(poses.offset(3)[0], held, "neither finishes nor goes back");
        assert_eq!(bench.logic.counters[0], 0, "nothing arrived");
    }

    #[test]
    fn animations_keep_their_place_in_the_cycle_when_changed_or_stopped() {
        let animate = |speed: f32, tween: f32, seconds: f32| MapTrigger {
            group: 2,
            amount: 30,
            speed,
            style: AnimateStyle::Orbit,
            tween,
            seconds,
            ..trigger(TriggerWhen::Start, TriggerAction::Animate)
        };
        let mut bench = Bench::new(vec![
            // Eases in over a second, then speeds up at once.
            animate(0.4, 1., 0.),
            animate(2.5, 0., 1.3),
            MapTrigger {
                group: 2,
                seconds: 2.1,
                ..trigger(TriggerWhen::Start, TriggerAction::Stop)
            },
        ]);
        let mut checked = 0;
        for _ in 0..180 {
            let before = bench.logic.tweens.clone();
            bench.step(&[]);
            // The pose at this instant is the same whether the change has
            // happened or not: no jump.
            let old = GroupPoses::at(&before, bench.now).wobble(2);
            let new = GroupPoses::at(&bench.logic.tweens, bench.now).wobble(2);
            if before != bench.logic.tweens {
                checked += 1;
                assert!(
                    (old[0] - new[0]).abs() < 0.05 && (old[1] - new[1]).abs() < 0.05,
                    "jumped from {old:?} to {new:?} at {}",
                    bench.now
                );
            }
        }
        assert!(checked >= 3, "start, speed change and stop all checked");
        // Stopped: it stays put however long passes.
        let held = GroupPoses::at(&bench.logic.tweens, bench.now).wobble(2);
        bench.run(3., &[]);
        assert_eq!(
            GroupPoses::at(&bench.logic.tweens, bench.now).wobble(2),
            held
        );
        assert!((held[0].hypot(held[1]) - 30.).abs() < 0.1);
    }

    #[test]
    fn shakes_reach_the_noticed_tanks_inside_and_sounds_play_at_the_area() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                amount: 40,
                tween: 0.8,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Shake)
            },
            MapTrigger {
                amount: 70,
                sound: MapSound::Bell,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Sound)
            },
        ]);
        let center = bench.center();
        bench.step(&[
            tank(1, center, false, 0),
            tank(2, V2::new(5., 5.), false, 0),
        ]);
        let mut shaken = None;
        let mut heard = None;
        for effect in &bench.effects {
            match effect {
                LogicEffect::Shake {
                    tanks,
                    strength,
                    seconds,
                } => shaken = Some((tanks.clone(), *strength, *seconds)),
                LogicEffect::Sound { cue, at } => heard = Some((cue.sound, cue.volume, *at)),
                _ => {}
            }
        }
        assert_eq!(shaken, Some((vec![1], 0.4, 0.8)));
        let (sound, volume, at) = heard.expect("the bell plays");
        assert_eq!((sound, volume), (MapSound::Bell, 0.7));
        assert!(at.distance(center) < 1.);
        // Event values carry them to clients intact.
        assert_eq!(shake_of(shake_event(0.4, 0.8)), (0.4, 0.8));
        assert_eq!(
            sound_of(sound_event(MapSound::Bell, 0.7)),
            Some((MapSound::Bell, 0.7))
        );
        assert_eq!(sound_of(200), None);
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

        // A motion with nowhere to go, like a door told to open while it is
        // open, arrives nowhere.
        let place = |seconds: f32| MapTrigger {
            group: 3,
            travel: [0.1, 0.],
            tween: 0.5,
            seconds,
            ..trigger(TriggerWhen::Start, TriggerAction::Place)
        };
        let mut bench = Bench::new(vec![place(0.), place(2.), count(1, 3)]);
        bench.run(1., &[]);
        assert_eq!(bench.logic.counters[0], 1);
        bench.run(2., &[]);
        assert_eq!(bench.logic.counters[0], 1, "the second had nowhere to go");
    }

    #[test]
    fn motions_retriggered_on_the_way_back_still_go_home() {
        // The Door and Fader pieces: a tank arriving opens or fades a group,
        // which goes back a while after. Arriving again before it is home must
        // put the return off, not forget where the group rests.
        let triggers = vec![
            MapTrigger {
                group: 3,
                travel: [0., -0.06],
                tween: 0.5,
                duration: 4.,
                seconds: 1.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Move)
            },
            MapTrigger {
                group: 4,
                amount: 15,
                tween: 0.6,
                duration: 5.,
                seconds: 1.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Fade)
            },
            MapTrigger {
                group: 5,
                travel: [0.05, 0.],
                tween: 0.5,
                duration: 4.,
                seconds: 1.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Place)
            },
            MapTrigger {
                group: 6,
                amount: 400,
                tween: 1.,
                duration: 3.5,
                seconds: 1.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Turn)
            },
        ];
        let at = |bench: &Bench, group, channel| bench.logic.tween_value(group, channel, bench.now);
        let home = |bench: &Bench| {
            at(bench, 3, TweenChannel::Move) == TweenChannel::Move.rest()
                && at(bench, 4, TweenChannel::Fade) == TweenChannel::Fade.rest()
                && at(bench, 5, TweenChannel::Move) == TweenChannel::Move.rest()
                && at(bench, 6, TweenChannel::Turn)[0] % 360. == 0.
        };
        let mut bench = Bench::new(triggers.clone());
        let inside = [tank(1, bench.center(), false, 0)];
        let away = [tank(1, V2::new(10., 10.), false, 0)];

        // In again once everything has arrived, before anything goes back.
        bench.run(1.5, &inside);
        bench.run(1., &away);
        bench.run(1.5, &inside);
        bench.run(1., &away);
        assert_eq!(
            at(&bench, 3, TweenChannel::Move),
            [0., -0.12, 0., 0.],
            "a Move goes a step further"
        );
        assert_eq!(
            at(&bench, 4, TweenChannel::Fade),
            [0.15, 0., 0., 0.],
            "a fade holds"
        );
        assert_eq!(
            at(&bench, 5, TweenChannel::Move),
            [0.05, 0., 0., 0.],
            "a Move to holds"
        );
        assert_eq!(
            at(&bench, 6, TweenChannel::Turn),
            [440., 0., 0., 0.],
            "a turn goes on, wrapped"
        );
        bench.run(8., &away);
        assert!(
            home(&bench),
            "after the last wait, every group goes all the way home"
        );

        // In again while they glide home.
        let mut bench = Bench::new(triggers);
        bench.run(1.5, &inside);
        bench.run(3.05, &away);
        let door = at(&bench, 3, TweenChannel::Move)[1];
        assert!(-0.06 < door && door < 0., "the door is on its way home");
        assert!(
            at(&bench, 6, TweenChannel::Turn)[0] > 360.,
            "and the turn past a full circle"
        );
        bench.run(0.5, &inside);
        bench.run(10., &away);
        assert!(
            home(&bench),
            "they turn round, and still go all the way home"
        );
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
    fn launch_pads_fling_once_and_prediction_agrees() {
        let mut world = world_with(|map| {
            map.zones.push(MapZone {
                kind: ZoneKind::Launch,
                rect: [0.45, 0.45, 0.1, 0.1],
                strength: 1.,
                group: 0,
                angle: 90.,
                exit: [0., 0.],
            });
        });
        let id = world.add_player("Pilot", false);
        let index = world.tanks.iter().position(|t| t.id == id).unwrap();
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        world.tanks[index].pos = center;
        world.tanks[index].vel = V2::new(50., 0.);
        // What a client predicts for the same tank.
        let mut predicted = world.tanks[index].clone();
        let fling = terrain_launch(
            &world.map,
            &world.config,
            world.match_state.hidden_groups,
            world.group_poses(),
            center,
        );
        assert!(launch_tank(&mut predicted, fling));
        world.update_zones();
        let tank = &world.tanks[index];
        assert_eq!(tank.vel, predicted.vel);
        assert!((tank.vel.y - LAUNCH_SPEED).abs() < 1e-3, "flung down");
        assert!((tank.vel.x - 50.).abs() < 1e-3, "sideways motion kept");
        assert_eq!(tank.knockback, LAUNCH_COAST);
        let launches = |world: &World| {
            world
                .events
                .iter()
                .filter(|e| matches!(e.kind, EventKind::Launch))
                .count()
        };
        assert_eq!(launches(&world), 1);
        // Still on the pad and already flying: no second launch.
        world.events.clear();
        world.update_zones();
        assert_eq!(launches(&world), 0);
        // A launch pad that is hidden flings nothing.
        world.tanks[index].vel = V2::ZERO;
        let mut hidden = world.map.clone();
        hidden.zones[0].group = 4;
        assert_eq!(
            terrain_launch(&hidden, &world.config, 1 << 3, world.group_poses(), center),
            V2::ZERO
        );
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
            vec![LogicEffect::Announce {
                text: "Counter two is 3".into(),
                cause: 0
            }]
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
            vec![LogicEffect::Announce {
                text: "Forty".into(),
                cause: 0
            }],
            "a set that lands on the threshold fires it once"
        );
        let picks = &bench.logic.counters[1..4];
        assert_eq!(picks.iter().sum::<i32>(), 60, "one channel each second");
        assert!(picks.iter().all(|&n| n >= 8), "{picks:?} spread out");
    }

    /// What a Random signal on channels 2 to 5 sends each second for
    /// `draws` seconds, as the channels each draw reached.
    fn draws(pick: RandomPick, picks: u8, weights: Vec<u8>, draws: usize) -> Vec<Vec<u8>> {
        let mut triggers = vec![MapTrigger {
            seconds: 1.,
            group: 2,
            amount: 4,
            pick,
            picks,
            weights,
            ..trigger(TriggerWhen::Every, TriggerAction::Random)
        }];
        for channel in 2..=5 {
            triggers.push(MapTrigger {
                source: channel,
                group: channel,
                amount: 1,
                ..trigger(TriggerWhen::Signal, TriggerAction::Count)
            });
        }
        let mut bench = Bench::new(triggers);
        bench.run(0.5, &[]);
        let mut seen = [0; 4];
        (0..draws)
            .map(|_| {
                bench.run(1., &[]);
                let now: [i32; 4] = bench.logic.counters[1..5].try_into().unwrap();
                let reached = (0..4)
                    .filter(|&i| now[i] > seen[i])
                    .map(|i| i as u8 + 2)
                    .collect();
                seen = now;
                reached
            })
            .collect()
    }

    #[test]
    fn random_signals_skip_repeats_deal_shuffled_rounds_and_follow_weights() {
        let fresh = draws(RandomPick::Fresh, 0, vec![], 80);
        assert!(fresh.iter().all(|draw| draw.len() == 1));
        assert!(fresh.windows(2).all(|pair| pair[0] != pair[1]), "{fresh:?}");
        for channel in 2..=5 {
            assert!(fresh.iter().any(|draw| draw[0] == channel));
        }
        let shuffled = draws(RandomPick::Shuffle, 0, vec![], 80);
        for (round, deal) in shuffled.chunks(4).enumerate() {
            let mut deal: Vec<u8> = deal.iter().map(|draw| draw[0]).collect();
            deal.sort_unstable();
            assert_eq!(deal, [2, 3, 4, 5], "round {round} of {shuffled:?}");
        }
        assert!(
            shuffled.windows(2).all(|pair| pair[0] != pair[1]),
            "a new round never starts with the last one: {shuffled:?}"
        );
        // Three of four at a time: every two deals reach every channel.
        let dealt = draws(RandomPick::Shuffle, 3, vec![], 40);
        for pair in dealt.chunks(2) {
            let mut reached: Vec<u8> = pair.concat();
            reached.sort_unstable();
            reached.dedup();
            assert_eq!(reached, [2, 3, 4, 5], "{dealt:?}");
        }
        let weighed = draws(RandomPick::Weighted, 0, vec![0, 30, 0, 70], 200);
        let fives = weighed.iter().filter(|draw| draw[..] == [5]).count();
        assert!(
            weighed
                .iter()
                .all(|draw| draw[..] == [3] || draw[..] == [5])
        );
        assert!((110..=170).contains(&fives), "{fives} of 200 at 70%");
        let pairs = draws(RandomPick::Any, 2, vec![], 40);
        assert!(
            pairs.iter().all(|draw| draw.len() == 2),
            "two different channels"
        );
    }

    #[test]
    fn chance_conditions_open_their_share_and_a_gate_to_counter_keeps_the_roll() {
        let chance = |value| TriggerTest {
            kind: TestKind::Chance,
            value,
            ..Default::default()
        };
        let mut triggers = Vec::new();
        for (counter, percent) in [(1, 25), (2, 0), (3, 100)] {
            let mut count = MapTrigger {
                seconds: 0.25,
                group: counter,
                amount: 1,
                ..trigger(TriggerWhen::Every, TriggerAction::Count)
            };
            count.set_conditions(vec![chance(percent)]);
            triggers.push(count);
        }
        // Sets counter 4 to 1 when its roll comes up and signals 9 when it
        // doesn't, which counts counter 5.
        let mut output = MapTrigger {
            seconds: 1.,
            group: 4,
            amount: 1,
            otherwise: 9,
            ..trigger(TriggerWhen::Every, TriggerAction::Output)
        };
        output.set_conditions(vec![chance(50)]);
        triggers.push(output);
        triggers.push(MapTrigger {
            source: 9,
            group: 5,
            amount: 1,
            ..trigger(TriggerWhen::Signal, TriggerAction::Count)
        });
        let mut bench = Bench::new(triggers);
        bench.run(0.5, &[]);
        let (mut shut, mut open) = (0, 0);
        for _ in 0..100 {
            let before = bench.logic.counters[4];
            bench.run(1., &[]);
            let otherwise = bench.logic.counters[4] - before;
            match bench.logic.counters[3] {
                1 => {
                    open += 1;
                    assert_eq!(otherwise, 0, "open, so no otherwise signal");
                }
                _ => {
                    shut += 1;
                    assert_eq!(otherwise, 1, "shut by the same roll it stored");
                }
            }
        }
        assert!(open > 20 && shut > 20, "{open} open, {shut} shut");
        let counters = &bench.logic.counters;
        // Four tries a second for 100.5 seconds.
        assert_eq!(counters[2], 402, "100% always");
        assert_eq!(counters[1], 0, "0% never");
        assert!(
            (60..=140).contains(&counters[0]),
            "{} of 402 at 25%",
            counters[0]
        );
    }

    #[test]
    fn sound_cues_ride_in_events_and_older_readers_hear_a_classic_sound() {
        let cue = SoundCue {
            sound: MapSound::Coin,
            volume: 0.6,
            pitch: -5,
            vary: 25,
            range: SoundRange::Cause,
            plays: 3,
            gap: 0.5,
            listener: 77,
        };
        let (value, subject) = cue.encode();
        // 2.0.4 reads only the value: the classic sound and the volume.
        assert_eq!(sound_of(value), Some((MapSound::Chime, 0.6)));
        assert_eq!(value >> 16, 0, "nothing above the volume");
        assert_eq!(SoundCue::decode(value, subject), Some(cue));
        // Scripts and older servers send no subject: once, nearby, as is.
        assert_eq!(
            SoundCue::decode(sound_event(MapSound::Bell, 0.7), 0),
            Some(SoundCue::new(MapSound::Bell, 0.7))
        );
        // Only a sound for one tank says which.
        let near = SoundCue {
            range: SoundRange::Near,
            ..cue
        };
        let (value, subject) = near.encode();
        assert_eq!(SoundCue::decode(value, subject).unwrap().listener, 0);
        // A sound from a newer version plays as the classic one it names.
        let unknown = (subject & !(0xff << 32)) | (250 << 32);
        assert_eq!(
            SoundCue::decode(value, unknown).unwrap().sound,
            MapSound::Chime
        );
        // Every setting at its limits survives the trip.
        for (pitch, vary, plays, gap) in [(-12, 60, 8, 3.), (12, 0, 1, 0.), (0, 1, 2, 0.05)] {
            let cue = SoundCue {
                pitch,
                vary,
                plays,
                gap,
                ..cue
            };
            let (value, subject) = cue.encode();
            assert_eq!(SoundCue::decode(value, subject), Some(cue));
        }
    }

    #[test]
    fn the_tank_that_set_it_off_carries_through_signals_and_delays() {
        let mut bench = Bench::new(vec![
            MapTrigger {
                group: 6,
                seconds: 0.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Signal)
            },
            MapTrigger {
                source: 6,
                amount: 10,
                delay: 0.5,
                who: TriggerWho::Cause,
                ..trigger(TriggerWhen::Signal, TriggerAction::Award)
            },
        ]);
        let center = bench.center();
        let first = [tank(1, center, false, 0)];
        bench.run(1., &first);
        let awarded = |bench: &Bench| -> Vec<Vec<EntityId>> {
            bench
                .effects
                .iter()
                .filter_map(|effect| match effect {
                    LogicEffect::Award { tanks, .. } => Some(tanks.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(awarded(&bench), vec![vec![1]]);
        bench.effects.clear();
        // A second tank drives in while the first stays: only it is rewarded.
        let both = [tank(1, center, false, 0), tank(2, center, false, 0)];
        bench.run(1., &both);
        assert_eq!(awarded(&bench), vec![vec![2]]);
        bench.effects.clear();
        // A script's signal has no tank behind it, so nobody is rewarded.
        bench.logic.input(LogicInput::Signal(6));
        bench.run(1., &both);
        assert_eq!(awarded(&bench), vec![Vec::<EntityId>::new()]);
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
        assert_eq!(
            bench.effects,
            vec![LogicEffect::Announce {
                text: "Answer".into(),
                cause: 0
            }]
        );
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

    /// A live gate: counter `out` holds whether `gate` is open over `tests`.
    fn live(gate: tankyard_content::GateMode, tests: Vec<TriggerTest>, out: u8) -> MapTrigger {
        let mut t = MapTrigger {
            group: out,
            amount: 1,
            gate,
            ..trigger(TriggerWhen::Inputs, TriggerAction::Output)
        };
        t.set_conditions(tests);
        t
    }
    fn on(index: u8) -> TriggerTest {
        test(TestKind::Counter, index, Compare::AtLeast, 1)
    }

    #[test]
    fn live_gates_keep_their_counters_up_to_date() {
        use tankyard_content::GateMode;
        let mut bench = Bench::new(vec![
            // NOT is right from the first step, before anything changes.
            live(GateMode::NoneOf, vec![on(1)], 3),
            live(GateMode::All, vec![on(1), on(2)], 4),
            // Fed by the NOT: a chain settles within the step.
            live(GateMode::Any, vec![on(3), on(4)], 5),
            live(GateMode::Odd, vec![on(1), on(2), on(6)], 7),
            live(GateMode::Most, vec![on(1), on(2), on(6)], 8),
        ]);
        bench.step(&[]);
        assert_eq!(bench.logic.counters[..8], [0, 0, 1, 0, 1, 0, 0, 0]);
        let mut check = |a: i32, b: i32, c: i32| {
            bench.logic.input(LogicInput::SetCounter(1, a));
            bench.logic.input(LogicInput::SetCounter(2, b));
            bench.logic.input(LogicInput::SetCounter(6, c));
            bench.step(&[]);
            let counters = &bench.logic.counters;
            let not = i32::from(a == 0);
            let and = i32::from(a == 1 && b == 1);
            assert_eq!(counters[2], not, "NOT {a}");
            assert_eq!(counters[3], and, "AND {a} {b}");
            assert_eq!(counters[4], not | and, "OR after them");
            assert_eq!(counters[6], (a + b + c) % 2, "sum of {a} {b} {c}");
            assert_eq!(
                counters[7],
                i32::from(a + b + c >= 2),
                "carry of {a} {b} {c}"
            );
        };
        for bits in 0..8 {
            check(bits & 1, bits >> 1 & 1, bits >> 2 & 1);
        }
    }

    #[test]
    fn a_live_majority_gate_on_its_own_counter_is_a_latch() {
        use tankyard_content::GateMode;
        let off = test(TestKind::Counter, 2, Compare::AtMost, 0);
        let mut bench = Bench::new(vec![live(GateMode::Most, vec![on(1), on(3), off], 3)]);
        let mut press = |set: i32, reset: i32| {
            bench.logic.input(LogicInput::SetCounter(1, set));
            bench.logic.input(LogicInput::SetCounter(2, reset));
            bench.step(&[]);
            bench.logic.counters[2]
        };
        assert_eq!(press(0, 0), 0);
        assert_eq!(press(1, 0), 1, "set");
        assert_eq!(press(0, 0), 1, "holds");
        assert_eq!(press(0, 1), 0, "reset");
        assert_eq!(press(0, 0), 0, "holds off");
        assert_eq!(press(1, 1), 0, "both hold it as it was");
        assert_eq!(press(1, 0), 1);
        assert_eq!(press(1, 1), 1);
    }

    #[test]
    fn live_gates_watch_groups_and_store_a_value() {
        use tankyard_content::GateMode;
        let mut bench = Bench::new(vec![
            MapTrigger {
                amount: 40,
                ..live(
                    GateMode::All,
                    vec![test(TestKind::Hidden, 2, Compare::AtLeast, 0)],
                    1,
                )
            },
            MapTrigger {
                source: 1,
                group: 2,
                ..trigger(TriggerWhen::Signal, TriggerAction::Toggle)
            },
        ]);
        bench.step(&[]);
        assert_eq!(bench.logic.counters[0], 0, "group 2 starts shown");
        bench.logic.input(LogicInput::Signal(1));
        bench.step(&[]);
        assert_eq!(bench.logic.counters[0], 40);
        bench.logic.input(LogicInput::Signal(1));
        bench.step(&[]);
        assert_eq!(bench.logic.counters[0], 0);
    }

    #[test]
    fn loops_that_compute_cannot_flood_players_with_effects() {
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
            MapTrigger {
                source: 1,
                text: "again".into(),
                ..trigger(TriggerWhen::Signal, TriggerAction::Announce)
            },
            MapTrigger {
                source: 1,
                group: 1,
                ..trigger(TriggerWhen::Signal, TriggerAction::Count)
            },
        ]);
        bench.step(&[]);
        assert_eq!(bench.effects.len(), EFFECT_LIMIT);
        let counted = bench.logic.counters[0];
        assert!(
            counted > EFFECT_LIMIT as i32,
            "the counting goes on past the effects: {counted}"
        );
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

    #[test]
    fn ice_slows_turns_and_gravity_wells_draw_tanks_in() {
        let pad = |kind| MapZone {
            kind,
            rect: [0.4, 0.4, 0.2, 0.2],
            strength: 1.,
            group: 0,
            angle: 0.,
            exit: [0., 0.],
        };
        let speed_after = |kind: Option<ZoneKind>| {
            let mut world = world_with(|map| map.zones.extend(kind.map(pad)));
            world.add_player("Driver", false);
            let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
            world.tanks[0].pos = center;
            world.tanks[0].vel = V2::ZERO;
            world.tanks[0].input.movement = V2::new(1., 0.);
            for _ in 0..6 {
                world.step();
            }
            (world.tanks[0].vel.length(), world.tanks[0].grip)
        };
        let (dry, dry_grip) = speed_after(None);
        let (icy, icy_grip) = speed_after(Some(ZoneKind::Ice));
        assert_eq!(dry_grip, 1.);
        assert!((icy_grip - ICE_GRIP).abs() < 1e-4, "{icy_grip}");
        assert!(icy < dry * 0.6, "slow to get going on ice: {icy} vs {dry}");

        let mut world = world_with(|map| map.zones.push(pad(ZoneKind::Pull)));
        world.add_player("Drifter", false);
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        let start = center + V2::new(250., 0.);
        world.tanks[0].pos = start;
        world.tanks[0].vel = V2::ZERO;
        for _ in 0..30 {
            world.step();
        }
        let pos = world.tanks[0].pos;
        assert!(pos.x < start.x - 20., "drawn toward the middle: {pos:?}");
        assert!((pos.y - start.y).abs() < 2., "straight in");
        // Prediction sees the same pull.
        let push = terrain_push(
            &world.map,
            &world.config,
            world.match_state.hidden_groups,
            world.group_poses(),
            start,
        );
        assert!(push.x < 0. && push.y.abs() < 1e-3);
    }

    #[test]
    fn maps_reject_physics_and_lighting_out_of_range() {
        let check = |edit: &dyn Fn(&mut MapDef)| {
            let mut maps = tankyard_content::builtin_maps();
            edit(&mut maps[0]);
            tankyard_content::validate_maps(&maps)
        };
        let shove = |map: &mut MapDef| {
            map.triggers.push(MapTrigger {
                group: 4,
                amount: 500,
                ..trigger(TriggerWhen::Start, TriggerAction::Shove)
            })
        };
        // Unfinished but harmless: a shove with no body to move.
        assert!(check(&shove).is_ok());
        assert!(
            check(&|map| {
                shove(map);
                map.bodies.push(tankyard_content::MapBody {
                    group: 4,
                    ..Default::default()
                });
            })
            .is_ok()
        );
        assert!(
            check(&|map| {
                map.bodies.push(tankyard_content::MapBody::default());
                map.bodies.push(tankyard_content::MapBody::default());
            })
            .is_err(),
            "one body per group"
        );
        // So is a path trigger whose path isn't drawn yet, but not one
        // past the most paths a map can hold.
        assert!(
            check(&|map| map.triggers.push(MapTrigger {
                group: 1,
                amount: 1,
                ..trigger(TriggerWhen::Start, TriggerAction::Path)
            }))
            .is_ok()
        );
        assert!(
            check(&|map| map.triggers.push(MapTrigger {
                group: 1,
                amount: tankyard_content::MAP_PATHS as i32 + 1,
                ..trigger(TriggerWhen::Start, TriggerAction::Path)
            }))
            .is_err()
        );
        assert!(check(&|map| map.ambient.darkness = 0.99).is_err());
        assert!(check(&|map| map.ambient.darkness = 0.7).is_ok());
        assert!(
            check(&|map| map.triggers.push(MapTrigger {
                amount: 500,
                angle: f32::NAN,
                ..trigger(TriggerWhen::Enter, TriggerAction::Fling)
            }))
            .is_err()
        );
        assert!(
            check(&|map| map.triggers.push(MapTrigger {
                amount: 10,
                tween: 2.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Zoom)
            }))
            .is_err(),
            "zoom stays within its range"
        );
    }

    #[test]
    fn calculate_finds_counters_by_offset_and_skips_numbers_out_of_range() {
        let calc = |group: u8, by: u8, math: MathOp, operand: u8, operand_by: u8| MapTrigger {
            seconds: 1000.,
            group,
            target_offset: by,
            math,
            operand,
            operand_offset: operand_by,
            amount: 5,
            ..trigger(TriggerWhen::Every, TriggerAction::Calculate)
        };
        let set = |group: u8, amount: i32| MapTrigger {
            seconds: 1000.,
            group,
            amount,
            ..trigger(TriggerWhen::Every, TriggerAction::Set)
        };
        let mut bench = Bench::new(vec![
            set(10, 2),
            set(32, 40),
            // Counter 20 + 2: counter 22, from counter 30 + 2.
            calc(20, 10, MathOp::Copy, 30, 10),
            // Counter 22 plus 5, the amount, with no operand.
            calc(20, 10, MathOp::Add, 0, 0),
            set(10, 300),
            set(10, -30),
            set(10, 245),
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
        assert_eq!(bench.logic.counters[21], 40);
        run(&mut bench, 3);
        assert_eq!(bench.logic.counters[21], 45);
        let before = bench.logic.counters;
        for setting in [4, 5] {
            run(&mut bench, setting);
            run(&mut bench, 2);
            run(&mut bench, 3);
        }
        // Counter 10 aside, nothing moved.
        let others = |counters: &[i32]| [&counters[..9], &counters[10..]].concat();
        assert_eq!(others(&bench.logic.counters), others(&before));
        // 20 + 245 is past the last counter.
        run(&mut bench, 6);
        run(&mut bench, 2);
        run(&mut bench, 3);
        assert_eq!(others(&bench.logic.counters), others(&before));
    }

    #[test]
    fn many_shut_listeners_cost_checks_not_a_frozen_step() {
        let mut triggers = vec![
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
        ];
        // Counter 1 stays 0, so these never act.
        triggers.extend((0..4000).map(|_| MapTrigger {
            source: 1,
            group: 2,
            amount: 1,
            only_if: Some(TriggerTest {
                kind: TestKind::Counter,
                index: 1,
                compare: Compare::AtLeast,
                value: 1,
                against: 0,
            }),
            ..trigger(TriggerWhen::Signal, TriggerAction::Count)
        }));
        let mut bench = Bench::new(triggers);
        let started = std::time::Instant::now();
        bench.step(&[]);
        bench.step(&[]);
        assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
        let loops = bench.logic.acted[1];
        assert!((1..=34).contains(&loops), "{loops}");
        assert_eq!(bench.logic.counters[1], 0);
        assert!(bench.logic.queue.is_empty());
    }

    #[test]
    fn labels_send_the_counters_they_show() {
        let mut world = world_with(|map| {
            map.decor.push(MapDecor {
                kind: tankyard_content::DecorKind::Label,
                rect: [0.1, 0.1, 0.2, 0.05],
                color: [255; 3],
                alpha: 255,
                rotation: 0.,
                pulse: 0.,
                spin: 0.,
                flicker: 0.,
                above: false,
                text: "Score {200:,} of {3|4:4}".into(),
                effect: None,
                group: 0,
                react: Default::default(),
                reach: 0.,
            });
            map.triggers.push(MapTrigger {
                group: 200,
                amount: 12345,
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Set)
            });
        });
        world.step();
        let mut shown = world.match_state.shown_counters.clone();
        shown.sort_unstable();
        assert_eq!(shown, [(3, 0), (4, 0), (200, 12345)]);
        let counters = tankyard_content::MapCounters::from_shown(&shown);
        assert_eq!(counters[199], 12345);
        assert_eq!(
            tankyard_content::fill_counters("{200:,}", &counters),
            "12,345"
        );
        let snapshot = world.snapshot_for(V2::new(0., 0.), 100.);
        assert_eq!(snapshot.match_state.shown_counters.len(), 3);
    }

    #[test]
    fn zoom_and_focus_values_survive_the_trip() {
        // Each part fits its bits.
        const {
            assert!(ZOOM_RANGE.1 - ZOOM_RANGE.0 <= 0xff);
            assert!(ZOOM_SECONDS * 10. <= 1023.);
            assert!(ZOOM_EASE * 10. <= 127.);
            assert!(Curve::ALL.len() <= 16 && EaseMode::ALL.len() <= 4);
        }
        for (curve, mode) in Curve::ALL
            .into_iter()
            .zip(EaseMode::ALL.into_iter().cycle())
        {
            for (percent, seconds, ease) in [
                (ZOOM_RANGE.0, Some(0.1), 0.1),
                (ZOOM_RANGE.1, Some(ZOOM_SECONDS), ZOOM_EASE),
                (100, None, 2.5),
            ] {
                let view = ZoomView {
                    scale: percent as f32 / 100.,
                    seconds,
                    ease,
                    easing: Easing { curve, mode },
                };
                assert_eq!(zoom_of(zoom_event(&view)), view);
            }
        }
        let (spot, focus) = focus_of(focus_event([0.25, 1.], 100));
        assert!((spot[0] - 0.25).abs() < 1e-3 && spot[1] == 1.);
        assert_eq!(focus, 1.);
        assert_eq!(focus_of(focus_event([-1., 2.], 255)), ([0., 1.], 1.));
    }

    #[test]
    fn zoomed_out_views_widen_what_the_server_sends_until_they_end() {
        let mut world = world_with(|map| {
            map.triggers.push(MapTrigger {
                amount: 50,
                tween: 2.,
                ease: 0.5,
                focus: 100,
                rect: [0., 0., 1., 1.],
                seconds: 0.,
                ..trigger(TriggerWhen::Start, TriggerAction::Zoom)
            });
        });
        let id = world.add_player("Viewer", false);
        let pos = world.tanks.iter().find(|t| t.id == id).unwrap().pos;
        world.step();
        let (middle, reach) = world.view_reach(id, pos, 1000.);
        assert!(reach >= 2000., "half the zoom sees twice as far: {reach}");
        let spot = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        assert!(middle.distance(spot) + 2000. <= reach + 1.);
        assert!(
            middle.distance(pos) + 1000. <= reach + 1.,
            "and still the tank"
        );
        assert_eq!(world.view_reach(7777, pos, 1000.), (pos, 1000.));
        for _ in 0..((2. + VIEW_GRACE) * 60.) as usize + 2 {
            world.step();
        }
        assert_eq!(world.view_reach(id, pos, 1000.), (pos, 1000.));
        let circle = |x: f32, r: f32| (V2::new(x, 0.), r);
        assert_eq!(enclose(circle(0., 10.), circle(3., 2.)), circle(0., 10.));
        assert_eq!(enclose(circle(0., 2.), circle(3., 10.)), circle(3., 10.));
        let (middle, radius) = enclose(circle(0., 1.), circle(10., 1.));
        assert!((middle.x - 5.).abs() < 1e-4 && (radius - 6.).abs() < 1e-4);
    }

    #[test]
    fn a_burst_of_zooms_keeps_a_few_views_per_tank() {
        // A signal that sets itself off zooms as often as a step allows.
        let mut world = world_with(|map| {
            map.triggers.extend([
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
                MapTrigger {
                    source: 1,
                    amount: 50,
                    hold: true,
                    rect: [0., 0., 1., 1.],
                    ..trigger(TriggerWhen::Signal, TriggerAction::Zoom)
                },
            ]);
        });
        let id = world.add_player("Viewer", false);
        world.step();
        assert!(world.logic.acted.iter().sum::<u32>() > 1000);
        let views = world.map_views.iter().filter(|v| v.tank == id).count();
        assert!((1..=VIEW_LIMIT).contains(&views), "{views}");
    }
}

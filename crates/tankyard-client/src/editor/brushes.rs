//! The build palette: every placeable object with sensible defaults, and the
//! small pictures the dock shows for them.
use crate::{
    scenery,
    settings::Settings,
    ui::{AMBER, CYAN, *},
};
use macroquad::prelude::*;
use tankyard_client::editor_document::Object;
use tankyard_content::*;

pub struct Brush {
    pub name: &'static str,
    pub object: Object,
    /// Size in world units when placed with a click; zero means one grid cell.
    pub size: [f32; 2],
    /// What it does, shown when the pointer rests on it; empty for none.
    pub hint: &'static str,
}
impl Brush {
    fn hint(mut self, hint: &'static str) -> Self {
        self.hint = hint;
        self
    }
}

pub const CATEGORIES: [&str; 7] = [
    "Blocks", "Pads", "Decor", "Logic", "Gates", "Motion", "Markers",
];

fn block(name: &'static str, style: BlockStyle) -> Brush {
    Brush {
        name,
        object: Object::Block(MapBlock {
            rect: [0., 0., 0.05, 0.05],
            style,
            color: None,
            group: 0,
        }),
        size: [0., 0.],
        hint: "",
    }
}
fn pad(kind: ZoneKind) -> Brush {
    Brush {
        name: kind.name(),
        object: Object::Zone(MapZone {
            kind,
            rect: [0., 0., 0.05, 0.05],
            strength: 1.,
            group: 0,
            angle: 0.,
            exit: [0.5, 0.5],
        }),
        size: [160., 160.],
        hint: match kind {
            ZoneKind::Speed => "Faster driving",
            ZoneKind::Mud => "Slower driving",
            ZoneKind::Repair => "Heals tanks standing on it",
            ZoneKind::Hazard => "Hurts tanks standing on it",
            ZoneKind::Shelter => "Less damage taken",
            ZoneKind::Conveyor => "Carries tanks in its direction",
            ZoneKind::Portal => "Sends tanks to its exit marker",
        },
    }
}
fn decor(name: &'static str, kind: DecorKind, size: [f32; 2]) -> Brush {
    Brush {
        name,
        object: Object::Decor(MapDecor {
            kind,
            rect: [0., 0., 0.05, 0.05],
            color: match kind {
                DecorKind::Light => [236, 196, 112],
                DecorKind::Label => [222, 228, 232],
                DecorKind::Arrow => [232, 196, 92],
                _ => [113, 158, 181],
            },
            alpha: if kind == DecorKind::Tile { 90 } else { 190 },
            rotation: 0.,
            pulse: 0.,
            spin: 0.,
            above: false,
            text: if kind == DecorKind::Label {
                "Arena".into()
            } else {
                String::new()
            },
            effect: (kind == DecorKind::Emitter).then(ParticleEffect::default),
            group: 0,
            flicker: 0.,
        }),
        size,
        hint: "",
    }
}
/// A light of `color` that wavers by `flicker`.
fn lamp(name: &'static str, color: [u8; 3], flicker: f32, size: f32) -> Brush {
    let mut brush = decor(name, DecorKind::Light, [size, size]);
    if let Object::Decor(item) = &mut brush.object {
        item.color = color;
        item.flicker = flicker;
        item.alpha = 220;
    }
    brush
}
/// A motion trigger on group 1 that plays over `tween` seconds.
fn motion(
    when: TriggerWhen,
    action: TriggerAction,
    tween: f32,
    curve: Curve,
    mode: EaseMode,
) -> MapTrigger {
    MapTrigger {
        when,
        action,
        group: 1,
        tween,
        easing: Easing { curve, mode },
        ..Default::default()
    }
}
fn effect(name: &'static str, preset: usize) -> Brush {
    let mut brush = decor(name, DecorKind::Emitter, [80., 80.]);
    if let Object::Decor(item) = &mut brush.object {
        item.effect = Some(effect_preset(preset));
    }
    brush
}
fn logic(name: &'static str, size: [f32; 2], trigger: MapTrigger) -> Brush {
    Brush {
        name,
        object: Object::Trigger(trigger),
        size,
        hint: "",
    }
}
fn marker(name: &'static str, object: Object, size: [f32; 2]) -> Brush {
    Brush {
        name,
        object,
        size,
        hint: "",
    }
}
/// A signal relay whose conditions combine with `gate`, from channel 1 to 3.
fn relay(gate: GateMode, tests: Vec<TriggerTest>) -> MapTrigger {
    let mut trigger = MapTrigger {
        when: TriggerWhen::Signal,
        action: TriggerAction::Signal,
        group: 3,
        gate,
        ..Default::default()
    };
    trigger.set_conditions(tests);
    trigger
}
/// Counter `index` is at least 1: a switch that is on.
fn on(index: u8) -> TriggerTest {
    TriggerTest {
        index,
        ..Default::default()
    }
}

/// Particle presets shared by the palette and the inspector.
pub const EFFECT_PRESETS: [&str; 10] = [
    "Embers",
    "Rain",
    "Dust",
    "Sparks",
    "Fireflies",
    "Smoke",
    "Snow",
    "Glitter",
    "Fountain",
    "Pulse",
];
pub fn effect_preset(index: usize) -> ParticleEffect {
    match index {
        0 => ParticleEffect {
            shape: ParticleShape::Soft,
            glow: true,
            rate: 10.,
            speed: 50.,
            spread: 40.,
            gravity: -18.,
            size: 8.,
            end_size: 2.,
            wobble: 24.,
            twinkle: 0.3,
            ..Default::default()
        },
        1 => ParticleEffect {
            shape: ParticleShape::Spark,
            direction: 90.,
            spread: 10.,
            speed: 260.,
            rate: 28.,
            color: [126, 174, 211],
            end_color: [103, 148, 190],
            size: 9.,
            end_size: 5.,
            ..Default::default()
        },
        2 => ParticleEffect {
            shape: ParticleShape::Soft,
            speed: 18.,
            spread: 360.,
            rate: 5.,
            size: 14.,
            end_size: 22.,
            color: [164, 156, 140],
            end_color: [164, 156, 140],
            wobble: 12.,
            ..Default::default()
        },
        3 => ParticleEffect {
            shape: ParticleShape::Spark,
            glow: true,
            speed: 240.,
            spread: 75.,
            gravity: 170.,
            rate: 14.,
            lifetime: 1.2,
            ..Default::default()
        },
        4 => ParticleEffect {
            shape: ParticleShape::Soft,
            glow: true,
            rate: 3.,
            lifetime: 4.,
            speed: 12.,
            spread: 360.,
            drag: 0.2,
            size: 7.,
            end_size: 6.,
            color: [210, 255, 120],
            end_color: [170, 230, 90],
            wobble: 40.,
            twinkle: 0.85,
            ..Default::default()
        },
        5 => ParticleEffect {
            shape: ParticleShape::Soft,
            rate: 6.,
            lifetime: 3.5,
            speed: 30.,
            spread: 25.,
            gravity: -10.,
            drag: 0.4,
            size: 16.,
            end_size: 46.,
            color: [120, 120, 124],
            end_color: [168, 168, 172],
            wobble: 18.,
            area: EmitArea::Point,
            ..Default::default()
        },
        6 => ParticleEffect {
            shape: ParticleShape::Soft,
            rate: 14.,
            lifetime: 6.,
            speed: 40.,
            direction: 90.,
            spread: 20.,
            drag: 0.,
            size: 5.,
            end_size: 4.,
            color: [240, 245, 255],
            end_color: [220, 230, 245],
            wobble: 30.,
            ..Default::default()
        },
        7 => ParticleEffect {
            shape: ParticleShape::Star,
            glow: true,
            rate: 10.,
            lifetime: 1.4,
            speed: 20.,
            spread: 360.,
            size: 7.,
            end_size: 1.,
            color: [255, 240, 170],
            end_color: [255, 180, 220],
            spin: 120.,
            twinkle: 0.8,
            ..Default::default()
        },
        8 => ParticleEffect {
            rate: 30.,
            lifetime: 1.3,
            speed: 300.,
            spread: 18.,
            gravity: 420.,
            drag: 0.1,
            size: 5.,
            end_size: 3.,
            color: [130, 200, 255],
            end_color: [90, 150, 230],
            area: EmitArea::Point,
            ..Default::default()
        },
        _ => ParticleEffect {
            shape: ParticleShape::Ring,
            glow: true,
            rate: 1.5,
            lifetime: 1.6,
            speed: 0.,
            spread: 0.,
            size: 6.,
            end_size: 60.,
            color: [120, 220, 255],
            end_color: [90, 140, 255],
            area: EmitArea::Point,
            ..Default::default()
        },
    }
}

pub fn catalog() -> Vec<Vec<Brush>> {
    let small = [80., 80.];
    let area = [240., 240.];
    vec![
        BlockStyle::ALL
            .into_iter()
            .map(|style| block(style.name(), style))
            .collect(),
        ZoneKind::ALL.into_iter().map(pad).collect(),
        vec![
            decor("Tile", DecorKind::Tile, [160., 160.]),
            decor("Stripes", DecorKind::Stripes, [240., 80.]),
            decor("Light", DecorKind::Light, [160., 160.]),
            lamp("Torch", [255, 158, 72], 0.55, 150.).hint("A warm light that wavers like a flame"),
            lamp("Neon", [96, 224, 255], 0., 200.).hint("A cool, steady glow"),
            lamp("Lantern", [255, 214, 150], 0.15, 260.).hint("A wide, soft light"),
            decor("Ring", DecorKind::Ring, [120., 120.]),
            decor("Arrow", DecorKind::Arrow, [120., 60.]),
            decor("Label", DecorKind::Label, [280., 60.]),
            effect("Embers", 0),
            effect("Rain", 1),
            effect("Dust", 2),
            effect("Sparks", 3),
            effect("Fireflies", 4),
            effect("Smoke", 5),
            effect("Snow", 6),
            effect("Glitter", 7),
            effect("Fountain", 8),
            effect("Pulse", 9),
        ],
        vec![
            logic(
                "Timer",
                small,
                MapTrigger {
                    when: TriggerWhen::Every,
                    seconds: 10.,
                    ..Default::default()
                },
            )
            .hint("Toggles a group on a repeating timer"),
            logic(
                "Plate",
                [160., 160.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Show,
                    seconds: 0.,
                    duration: 5.,
                    ..Default::default()
                },
            )
            .hint("Shows a group while tanks cross it"),
            logic(
                "Exit pad",
                [160., 160.],
                MapTrigger {
                    when: TriggerWhen::Exit,
                    action: TriggerAction::Hide,
                    seconds: 0.,
                    ..Default::default()
                },
            )
            .hint("Hides a group when a tank leaves"),
            logic(
                "Start",
                small,
                MapTrigger {
                    when: TriggerWhen::Start,
                    action: TriggerAction::Announce,
                    seconds: 0.,
                    text: "The arena is open.".into(),
                    ..Default::default()
                },
            )
            .hint("Announces text when the round starts"),
            logic(
                "Sender",
                [160., 160.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Signal,
                    seconds: 1.,
                    ..Default::default()
                },
            )
            .hint("Sends a signal when entered"),
            logic(
                "Receiver",
                small,
                MapTrigger {
                    when: TriggerWhen::Signal,
                    action: TriggerAction::Toggle,
                    ..Default::default()
                },
            )
            .hint("Acts when a signal arrives"),
            logic(
                "Counter",
                [160., 160.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Count,
                    seconds: 1.,
                    ..Default::default()
                },
            )
            .hint("Adds one to a counter when entered"),
            logic(
                "Goal",
                small,
                MapTrigger {
                    when: TriggerWhen::Counter,
                    action: TriggerAction::Announce,
                    threshold: 5,
                    text: "Counter one reached {1}.".into(),
                    ..Default::default()
                },
            )
            .hint("Acts when a counter reaches a value"),
            logic(
                "Shapes",
                area,
                MapTrigger {
                    when: TriggerWhen::Every,
                    action: TriggerAction::Shapes,
                    seconds: 20.,
                    amount: 6,
                    ..Default::default()
                },
            )
            .hint("Drops shapes in its area on a timer"),
            logic(
                "Boss",
                area,
                MapTrigger {
                    when: TriggerWhen::Start,
                    action: TriggerAction::Boss,
                    seconds: 60.,
                    boss: "sentinel".into(),
                    ..Default::default()
                },
            )
            .hint("Brings a boss partway through the round"),
            logic(
                "Capture",
                area,
                MapTrigger {
                    when: TriggerWhen::Occupied,
                    action: TriggerAction::Count,
                    seconds: 1.,
                    ..Default::default()
                },
            )
            .hint("Counts each second a tank holds it"),
            logic(
                "Bonus",
                [120., 120.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Award,
                    once: true,
                    amount: 500,
                    ..Default::default()
                },
            )
            .hint("Gives points to the first tank here"),
            logic(
                "Medic",
                [200., 200.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Heal,
                    seconds: 15.,
                    amount: 50,
                    ..Default::default()
                },
            )
            .hint("Repairs tanks, then rests 15 seconds"),
            logic(
                "Trap",
                [160., 160.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Hurt,
                    seconds: 3.,
                    amount: 25,
                    delay: 0.5,
                    ..Default::default()
                },
            )
            .hint("Hurts tanks a moment after they step on"),
            logic(
                "Script",
                [160., 160.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Script,
                    seconds: 1.,
                    text: "door".into(),
                    ..Default::default()
                },
            )
            .hint("Raises a script event for script packs"),
        ],
        vec![
            logic(
                "Gate",
                small,
                MapTrigger {
                    when: TriggerWhen::Signal,
                    action: TriggerAction::Signal,
                    group: 2,
                    only_if: Some(TriggerTest::default()),
                    ..Default::default()
                },
            )
            .hint("Passes signal 1 on as 2 while counter 1 is 1 or more"),
            logic("AND", small, relay(GateMode::All, vec![on(1), on(2)]))
                .hint("Passes signal 1 on as 3 while counters 1 and 2 are both on"),
            logic("OR", small, relay(GateMode::Any, vec![on(1), on(2)]))
                .hint("Passes signal 1 on as 3 while counter 1 or 2 is on"),
            logic("XOR", small, relay(GateMode::One, vec![on(1), on(2)]))
                .hint("Passes signal 1 on as 3 while just one of counters 1 and 2 is on"),
            logic(
                "If/else",
                small,
                MapTrigger {
                    group: 2,
                    otherwise: 3,
                    ..relay(GateMode::All, vec![on(1)])
                },
            )
            .hint("On signal 1: signal 2 if counter 1 is on, otherwise signal 3"),
            logic(
                "Lever",
                [120., 120.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Calculate,
                    math: MathOp::Flip,
                    seconds: 1.,
                    ..Default::default()
                },
            )
            .hint("Flips counter 1 between 0 and 1 when entered"),
            logic(
                "Coin flip",
                small,
                MapTrigger {
                    when: TriggerWhen::Every,
                    seconds: 5.,
                    chance: 50,
                    ..Default::default()
                },
            )
            .hint("Toggles a group half of the time"),
            logic(
                "Dice",
                small,
                MapTrigger {
                    when: TriggerWhen::Every,
                    action: TriggerAction::Random,
                    seconds: 15.,
                    amount: 3,
                    ..Default::default()
                },
            )
            .hint("Sends one of three signals at random"),
            logic(
                "Sequence",
                small,
                MapTrigger {
                    when: TriggerWhen::Every,
                    action: TriggerAction::Sequence,
                    seconds: 3.,
                    group: 1,
                    amount: 3,
                    ..Default::default()
                },
            )
            .hint("Sends signals 1, 2 and 3 in turn, one every 3 seconds"),
            logic(
                "Display",
                small,
                MapTrigger {
                    when: TriggerWhen::Changed,
                    action: TriggerAction::Announce,
                    text: "Counter one is now {1}.".into(),
                    ..Default::default()
                },
            )
            .hint("Announces counter 1 every time it changes"),
            logic(
                "Alarm",
                small,
                MapTrigger {
                    when: TriggerWhen::Hidden,
                    action: TriggerAction::Announce,
                    text: "The door is open!".into(),
                    ..Default::default()
                },
            )
            .hint("Announces when group 1 is hidden, however it happened"),
            logic(
                "Reward",
                small,
                MapTrigger {
                    when: TriggerWhen::Defeated,
                    action: TriggerAction::Hide,
                    source: 0,
                    ..Default::default()
                },
            )
            .hint("Hides group 1 when any boss is defeated"),
            logic(
                "Reset",
                small,
                MapTrigger {
                    when: TriggerWhen::Cleared,
                    action: TriggerAction::Set,
                    seconds: 0.,
                    amount: 0,
                    ..Default::default()
                },
            )
            .hint("Sets a counter back to 0 when emptied"),
            logic(
                "Rearm",
                small,
                MapTrigger {
                    when: TriggerWhen::Signal,
                    action: TriggerAction::Reset,
                    ..Default::default()
                },
            )
            .hint("On signal 1, group 1's triggers start afresh"),
            logic("Crowd", [200., 200.], {
                let mut plate = MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Hide,
                    seconds: 1.,
                    duration: 8.,
                    ..Default::default()
                };
                plate.set_conditions(vec![TriggerTest {
                    kind: TestKind::Inside,
                    value: 2,
                    ..Default::default()
                }]);
                plate
            })
            .hint("Hides group 1 for 8 s once two tanks stand on it together"),
            logic("Clock", small, {
                let mut overtime = MapTrigger {
                    when: TriggerWhen::Every,
                    action: TriggerAction::Announce,
                    seconds: 30.,
                    text: "Overtime! Finish it.".into(),
                    ..Default::default()
                };
                overtime.set_conditions(vec![TriggerTest {
                    kind: TestKind::Time,
                    value: 300,
                    ..Default::default()
                }]);
                overtime
            })
            .hint("Announces every 30 s once the round passes 5 minutes"),
        ],
        vec![
            // Move to rather than Move: a busy doorway holds the door open
            // instead of sliding it a step further for every tank.
            logic(
                "Door",
                [160., 160.],
                MapTrigger {
                    travel: [0., -0.06],
                    duration: 4.,
                    seconds: 1.,
                    ..motion(
                        TriggerWhen::Enter,
                        TriggerAction::Place,
                        0.5,
                        Curve::Cubic,
                        EaseMode::Out,
                    )
                },
            )
            .hint("Slides group 1 up when a tank arrives, and back 4 s later"),
            logic(
                "Shuttle",
                small,
                MapTrigger {
                    travel: [0.15, 0.],
                    duration: 1.,
                    seconds: 6.,
                    ..motion(
                        TriggerWhen::Every,
                        TriggerAction::Move,
                        2.,
                        Curve::Sine,
                        EaseMode::InOut,
                    )
                },
            )
            .hint("Glides group 1 across and back every 6 seconds"),
            logic(
                "Piston",
                small,
                MapTrigger {
                    travel: [0.04, 0.],
                    duration: 0.4,
                    seconds: 2.,
                    ..motion(
                        TriggerWhen::Every,
                        TriggerAction::Move,
                        0.2,
                        Curve::Expo,
                        EaseMode::Out,
                    )
                },
            )
            .hint("Punches group 1 out and pulls it back every 2 seconds"),
            logic(
                "Lift",
                small,
                MapTrigger {
                    travel: [0., 0.12],
                    ..motion(
                        TriggerWhen::Signal,
                        TriggerAction::Place,
                        1.5,
                        Curve::Quad,
                        EaseMode::InOut,
                    )
                },
            )
            .hint("On signal 1, carries group 1 to 12% below its place"),
            logic(
                "Home",
                small,
                MapTrigger {
                    source: 2,
                    ..motion(
                        TriggerWhen::Signal,
                        TriggerAction::Place,
                        1.5,
                        Curve::Quad,
                        EaseMode::InOut,
                    )
                },
            )
            .hint("On signal 2, brings group 1 back to where the map puts it"),
            logic(
                "Drop",
                small,
                MapTrigger {
                    travel: [0., 0.08],
                    seconds: 0.,
                    ..motion(
                        TriggerWhen::Start,
                        TriggerAction::Move,
                        1.2,
                        Curve::Bounce,
                        EaseMode::Out,
                    )
                },
            )
            .hint("Drops group 1 down with a bounce as the round starts"),
            logic(
                "Spinner",
                small,
                MapTrigger {
                    amount: 90,
                    seconds: 3.,
                    ..motion(
                        TriggerWhen::Every,
                        TriggerAction::Turn,
                        0.8,
                        Curve::Back,
                        EaseMode::Out,
                    )
                },
            )
            .hint("Turns group 1's decor a quarter turn every 3 seconds"),
            logic(
                "Fader",
                [160., 160.],
                MapTrigger {
                    amount: 15,
                    duration: 5.,
                    seconds: 1.,
                    ..motion(
                        TriggerWhen::Enter,
                        TriggerAction::Fade,
                        0.6,
                        Curve::Sine,
                        EaseMode::InOut,
                    )
                },
            )
            .hint("Fades group 1's decor while tanks pass, back after 5 s"),
            logic(
                "Blink",
                small,
                MapTrigger {
                    amount: 0,
                    duration: 0.7,
                    seconds: 2.,
                    ..motion(
                        TriggerWhen::Every,
                        TriggerAction::Fade,
                        0.3,
                        Curve::Sine,
                        EaseMode::InOut,
                    )
                },
            )
            .hint("Dims group 1's decor and brings it back every 2 seconds"),
            logic(
                "Then",
                small,
                MapTrigger {
                    when: TriggerWhen::Arrived,
                    action: TriggerAction::Signal,
                    source: 1,
                    group: 1,
                    ..Default::default()
                },
            )
            .hint("Sends signal 1 each time group 1 finishes a motion"),
        ],
        vec![
            marker(
                "Spawn",
                Object::Spawn(SpawnRegion {
                    team: 0,
                    rect: [0., 0., 0.1, 0.1],
                }),
                [320., 320.],
            )
            .hint("Where tanks enter the arena"),
            marker(
                "Team 1 spawn",
                Object::Spawn(SpawnRegion {
                    team: 1,
                    rect: [0., 0., 0.1, 0.1],
                }),
                [320., 320.],
            )
            .hint("Where tanks enter the arena"),
            marker(
                "Team 2 spawn",
                Object::Spawn(SpawnRegion {
                    team: 2,
                    rect: [0., 0., 0.1, 0.1],
                }),
                [320., 320.],
            )
            .hint("Where tanks enter the arena"),
            marker(
                "Base 1",
                Object::Base(TeamBase {
                    team: 1,
                    position: [0.5, 0.5],
                }),
                [0., 0.],
            )
            .hint("A team's home base"),
            marker(
                "Base 2",
                Object::Base(TeamBase {
                    team: 2,
                    position: [0.5, 0.5],
                }),
                [0., 0.],
            )
            .hint("A team's home base"),
            marker("Objective", Object::Objective([0.5, 0.5]), [0., 0.])
                .hint("A point for objective modes"),
        ],
    ]
}

/// Whether a trigger's rectangle is an area that matters in play, rather
/// than just where its badge sits.
pub fn trigger_area(trigger: &MapTrigger) -> bool {
    trigger.when.watches_area()
        || trigger.action.affects_tanks()
        || matches!(
            trigger.action,
            TriggerAction::Boss | TriggerAction::Shapes | TriggerAction::Script
        )
}

pub const LOGIC: Color = Color::new(0.93, 0.70, 0.27, 1.0);

/// A trigger's badge: a dark tile with a mark for what it does.
pub fn trigger_badge(action: TriggerAction, r: Rect, lit: f32) {
    let ink = mix(LOGIC, WHITE, lit * 0.6);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.13, 0.12, 0.10, 0.88));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, (r.w * 0.06).max(1.5), ink);
    let c = r.center();
    let u = r.w.min(r.h) * 0.3;
    let line = (u * 0.22).max(1.5);
    match action {
        TriggerAction::Toggle => {
            draw_rectangle_lines(c.x - u, c.y - u * 0.6, u * 2., u * 1.2, line, ink);
            draw_rectangle(c.x, c.y - u * 0.6, u, u * 1.2, ink);
        }
        TriggerAction::Show => {
            draw_rectangle(c.x - u, c.y - u * 0.6, u * 2., u * 1.2, ink);
        }
        TriggerAction::Hide => {
            draw_rectangle_lines(c.x - u, c.y - u * 0.6, u * 2., u * 1.2, line, ink);
            draw_line(c.x - u, c.y + u * 0.6, c.x + u, c.y - u * 0.6, line, ink);
        }
        TriggerAction::Announce => {
            for (i, width) in [1., 0.7, 0.85].into_iter().enumerate() {
                let y = c.y - u * 0.6 + i as f32 * u * 0.6;
                draw_line(c.x - u, y, c.x - u + 2. * u * width, y, line, ink);
            }
        }
        TriggerAction::Boss => {
            draw_poly(c.x, c.y, 5, u, -90., ink);
            draw_circle(c.x, c.y, u * 0.35, Color::new(0.13, 0.12, 0.10, 1.));
        }
        TriggerAction::Signal => {
            draw_circle(c.x - u * 0.7, c.y, u * 0.28, ink);
            for radius in [0.75, 1.3] {
                draw_arc(c.x - u * 0.7, c.y, 24, u * radius, -45., line, 90., ink);
            }
        }
        TriggerAction::Count => {
            draw_line(c.x - u, c.y, c.x + u, c.y, line * 1.3, ink);
            draw_line(c.x, c.y - u, c.x, c.y + u, line * 1.3, ink);
            draw_rectangle_lines(
                c.x - u,
                c.y - u,
                u * 2.,
                u * 2.,
                line * 0.7,
                alpha(ink, 0.5),
            );
        }
        TriggerAction::Shapes => {
            draw_rectangle(c.x - u, c.y - u * 0.1, u * 0.9, u * 0.9, ink);
            draw_triangle(
                vec2(c.x + u * 0.5, c.y - u),
                vec2(c.x + u * 1.1, c.y + u * 0.8),
                vec2(c.x - u * 0.1, c.y + u * 0.8),
                ink,
            );
        }
        TriggerAction::Set => {
            for y in [-0.3, 0.3] {
                draw_line(
                    c.x - u * 0.6,
                    c.y + u * y,
                    c.x + u * 0.6,
                    c.y + u * y,
                    line * 1.3,
                    ink,
                );
            }
            draw_rectangle_lines(
                c.x - u,
                c.y - u,
                u * 2.,
                u * 2.,
                line * 0.7,
                alpha(ink, 0.5),
            );
        }
        TriggerAction::Random => {
            draw_rectangle_lines(c.x - u, c.y - u, u * 2., u * 2., line, ink);
            for (x, y) in [(-0.45, -0.45), (0., 0.), (0.45, 0.45)] {
                draw_circle(c.x + u * x, c.y + u * y, u * 0.18, ink);
            }
        }
        TriggerAction::Heal => {
            let arm = u * 0.32;
            draw_rectangle(c.x - arm, c.y - u, arm * 2., u * 2., ink);
            draw_rectangle(c.x - u, c.y - arm, u * 2., arm * 2., ink);
        }
        TriggerAction::Hurt => {
            for (dx, dy) in [(-0.95, -0.1), (-0.05, -0.1), (0.85, -0.1)] {
                draw_triangle(
                    vec2(c.x + u * dx, c.y + u * 0.8),
                    vec2(c.x + u * (dx + 0.25), c.y + u * (dy - 0.9)),
                    vec2(c.x + u * (dx + 0.5), c.y + u * 0.8),
                    ink,
                );
            }
        }
        TriggerAction::Award => {
            draw_circle(c.x, c.y, u, ink);
            draw_circle_lines(
                c.x,
                c.y,
                u * 0.62,
                line * 0.8,
                Color::new(0.13, 0.12, 0.10, 1.),
            );
        }
        TriggerAction::Calculate => {
            // A division sign in a box: arithmetic on a counter.
            draw_line(c.x - u * 0.6, c.y, c.x + u * 0.6, c.y, line * 1.3, ink);
            for y in [-0.45, 0.45] {
                draw_circle(c.x, c.y + u * y, u * 0.16, ink);
            }
            draw_rectangle_lines(
                c.x - u,
                c.y - u,
                u * 2.,
                u * 2.,
                line * 0.7,
                alpha(ink, 0.5),
            );
        }
        TriggerAction::Script => {
            // Angle brackets: something a script answers.
            for side in [-1., 1.] {
                let (tip, back) = (c.x + side * u, c.x + side * u * 0.35);
                draw_line(back, c.y - u * 0.7, tip, c.y, line, ink);
                draw_line(tip, c.y, back, c.y + u * 0.7, line, ink);
            }
            draw_line(
                c.x + u * 0.15,
                c.y - u * 0.75,
                c.x - u * 0.15,
                c.y + u * 0.75,
                line * 0.8,
                ink,
            );
        }
        TriggerAction::Sequence => {
            // Three steps, the first lit: one channel after another.
            for (i, x) in [-0.7, 0., 0.7].into_iter().enumerate() {
                let (cx, size) = (c.x + u * x, u * 0.26);
                if i == 0 {
                    draw_circle(cx, c.y, size, ink);
                } else {
                    draw_circle_lines(cx, c.y, size, line * 0.7, ink);
                }
            }
            draw_line(
                c.x - u,
                c.y + u * 0.7,
                c.x + u,
                c.y + u * 0.7,
                line * 0.7,
                ink,
            );
            draw_triangle(
                vec2(c.x + u, c.y + u * 0.45),
                vec2(c.x + u * 1.25, c.y + u * 0.7),
                vec2(c.x + u, c.y + u * 0.95),
                ink,
            );
        }
        TriggerAction::Move => {
            // A block with speed lines behind it.
            draw_rectangle_lines(c.x - u * 0.1, c.y - u * 0.55, u * 1.1, u * 1.1, line, ink);
            for (y, from) in [(-0.35, 1.), (0., 0.8), (0.35, 1.)] {
                draw_line(
                    c.x - u * from,
                    c.y + u * y,
                    c.x - u * 0.35,
                    c.y + u * y,
                    line * 0.8,
                    alpha(ink, 0.75),
                );
            }
        }
        TriggerAction::Place => {
            // A target: where the group goes.
            draw_circle_lines(c.x, c.y, u * 0.75, line, ink);
            draw_circle(c.x, c.y, u * 0.2, ink);
            for (dx, dy) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
                draw_line(
                    c.x + dx * u * 0.75,
                    c.y + dy * u * 0.75,
                    c.x + dx * u * 1.1,
                    c.y + dy * u * 1.1,
                    line,
                    ink,
                );
            }
        }
        TriggerAction::Turn => {
            draw_poly_lines(c.x, c.y, 4, u * 0.75, 25., line, ink);
            draw_arc(c.x, c.y, 24, u * 1.05, -80., line * 0.8, 70., ink);
        }
        TriggerAction::Fade => {
            for i in 0..3 {
                draw_rectangle(
                    c.x - u + i as f32 * u * 0.72,
                    c.y - u * 0.6,
                    u * 0.56,
                    u * 1.2,
                    alpha(ink, 1. - i as f32 * 0.35),
                );
            }
        }
        TriggerAction::Reset => {
            // A circular arrow: start again.
            draw_arc(c.x, c.y, 24, u * 0.8, 40., line, 280., ink);
            let tip = vec2(
                c.x + u * 0.8 * 40f32.to_radians().cos(),
                c.y + u * 0.8 * 40f32.to_radians().sin(),
            );
            draw_triangle(
                tip + vec2(-u * 0.45, -u * 0.05),
                tip + vec2(u * 0.3, -u * 0.25),
                tip + vec2(u * 0.05, u * 0.45),
                ink,
            );
        }
    }
}

/// The corner mark of a trigger that only acts when its gate is open.
/// The mark of a trigger with conditions, on the corner of its badge, clear
/// of the action's mark: the gate's name when it combines several and the
/// badge has room, otherwise a diamond.
pub fn gate_mark(trigger: &MapTrigger, r: Rect, lit: f32) {
    let ink = mix(LOGIC, WHITE, lit * 0.6);
    let dark = Color::new(0.13, 0.12, 0.10, 1.);
    let size = r.w.min(r.h) * 0.24;
    let c = vec2(r.right(), r.y);
    if trigger.conditions().nth(1).is_some() && r.w >= 30. {
        let name = trigger.gate.short();
        let text = (r.w * 0.24).clamp(8., 13.);
        let w = label_width(name, text) + text * 0.6;
        let h = text * 1.3;
        let x = (c.x - w * 0.6).min(r.right() - w * 0.4);
        draw_rectangle(x, c.y - h * 0.5, w, h, dark);
        draw_rectangle_lines(x, c.y - h * 0.5, w, h, (size * 0.2).max(1.), ink);
        center_label(name, x + w * 0.5, c.y + text * 0.36, text, ink);
        return;
    }
    draw_poly(c.x, c.y, 4, size, 0., dark);
    draw_poly_lines(c.x, c.y, 4, size, 0., (size * 0.25).max(1.), ink);
}

/// A spawn region on the canvas and in the palette.
pub fn spawn_mark(team: u8, r: Rect, s: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, alpha(CYAN, 0.07));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2., alpha(CYAN, 0.7));
    let title = if team == 0 {
        "Spawn".to_owned()
    } else {
        format!("Team {team}")
    };
    let size = (12. * s).min(r.h * 0.4).max(6.);
    center_label(
        &fitted_label(&title, size, r.w),
        r.center().x,
        r.center().y + size * 0.35,
        size,
        CYAN,
    );
}
pub fn base_mark(team: u8, r: Rect) {
    let c = r.center();
    let radius = r.w.min(r.h) * 0.5;
    draw_circle(c.x, c.y, radius, alpha(CYAN, 0.18));
    draw_circle_lines(c.x, c.y, radius, 2., CYAN);
    let size = (radius * 0.9).max(6.);
    center_label(&team.to_string(), c.x, c.y + size * 0.35, size, CYAN);
}
pub fn objective_mark(r: Rect) {
    let c = r.center();
    draw_poly(c.x, c.y, 4, r.w * 0.5, 0., alpha(AMBER, 0.25));
    draw_poly_lines(c.x, c.y, 4, r.w * 0.5, 0., 2., AMBER);
}

/// The palette picture of a brush, drawn inside `r`.
pub fn thumbnail(brush: &Brush, r: Rect, map: &MapDef, settings: &Settings, time: f32) {
    let inner = Rect::new(r.x + r.w * 0.16, r.y + r.h * 0.16, r.w * 0.68, r.h * 0.68);
    let point = |r: Rect| {
        let c = r.center();
        Rect::new(c.x - r.w * 0.3, c.y - r.w * 0.3, r.w * 0.6, r.w * 0.6)
    };
    match &brush.object {
        Object::Wall(_) => {}
        Object::Block(b) => scenery::block(
            inner,
            scenery::wall_style(
                if b.style == BlockStyle::Map {
                    map.style
                } else {
                    b.style
                },
                &map.id,
            ),
            None,
            settings.dark_arena,
            settings.shape_quality,
        ),
        Object::Zone(z) => {
            let c = crate::accessibility::palette(scenery::zone_color(z.kind));
            draw_rectangle(inner.x, inner.y, inner.w, inner.h, alpha(c, 0.22));
            draw_rectangle_lines(inner.x, inner.y, inner.w, inner.h, 2., alpha(c, 0.8));
            scenery::zone_mark(z.kind, z.angle, inner.center(), inner.w * 0.28, c);
        }
        Object::Decor(d) if d.kind == DecorKind::Emitter => {
            let mut remaining = 60;
            if let Some(effect) = &d.effect {
                let shown = ParticleEffect {
                    speed: effect.speed.min(60.),
                    lifetime: effect.lifetime.min(1.2),
                    gravity: effect.gravity.clamp(-60., 60.),
                    ..*effect
                };
                let spot = Rect::new(inner.center().x - 2., inner.center().y - 2., 4., 4.);
                let t = if settings.reduced_motion { 2. } else { time };
                scenery::particles(&shown, spot, 0.35, t + 2., 7, &mut remaining, 1.);
            }
            draw_circle_lines(inner.center().x, inner.center().y, 3., 1., muted_color());
        }
        Object::Decor(d) => {
            let mut item = d.clone();
            item.alpha = item.alpha.max(150);
            if item.kind == DecorKind::Label {
                item.text = "Aa".into();
            }
            scenery::decoration(&item, inner, 0., 0.4, &[0; MAP_COUNTERS as usize], 0., 1.);
        }
        Object::Trigger(t) => {
            let size = inner.w.min(inner.h) * 0.8;
            let c = inner.center();
            let badge = Rect::new(c.x - size * 0.5, c.y - size * 0.5, size, size);
            trigger_badge(t.action, badge, 0.);
            if t.only_if.is_some() {
                gate_mark(t, badge, 0.);
            }
        }
        Object::Spawn(region) => spawn_mark(region.team, inner, inner.w / 44.),
        Object::Base(base) => base_mark(base.team, point(inner)),
        Object::Objective(_) => objective_mark(point(inner)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_client::editor_document::Document;
    use tankyard_core::{DT, LogicEffect, MapLogic, Occupant, V2, WorldConfig};

    /// A palette trigger as placed, at `rect`.
    fn piece(tab: &str, name: &str, rect: [f32; 4]) -> MapTrigger {
        let tab = CATEGORIES.iter().position(|&t| t == tab).unwrap();
        let brush = catalog()
            .swap_remove(tab)
            .into_iter()
            .find(|b| b.name == name);
        match brush.map(|b| b.object) {
            Some(Object::Trigger(trigger)) => MapTrigger { rect, ..trigger },
            _ => panic!("no {name} trigger in the palette"),
        }
    }

    /// The vault from docs/logic-gates-tutorial.md, built step by step from
    /// the palette with the settings the tutorial gives.
    fn tutorial_vault() -> MapDef {
        let mut map = tankyard_content::builtin_maps().remove(0);
        map.triggers.clear();
        map.hidden_groups.clear();
        map.blocks.retain(|b| b.group == 0);
        map.zones.retain(|z| z.group == 0);
        map.decor.retain(|d| d.group == 0);
        let spot = |x: f32, y: f32| [x, y, 0.04, 0.04];
        // 2. The wall, in group 1.
        for i in 0..3 {
            map.blocks.push(MapBlock {
                rect: [0.44 + i as f32 * 0.04, 0.3, 0.04, 0.04],
                style: BlockStyle::default(),
                color: None,
                group: 1,
            });
        }
        // 3. Two levers; the second flips counter 2.
        map.triggers.push(piece("Gates", "Lever", spot(0.25, 0.55)));
        map.triggers.push(MapTrigger {
            group: 2,
            ..piece("Gates", "Lever", spot(0.7, 0.55))
        });
        // 4. A watcher on each counter sends signal 1.
        for counter in [1, 2] {
            map.triggers.push(MapTrigger {
                when: TriggerWhen::Changed,
                source: counter,
                action: TriggerAction::Signal,
                group: 1,
                ..piece(
                    "Logic",
                    "Receiver",
                    spot(0.2 + f32::from(counter) * 0.1, 0.65),
                )
            });
        }
        // 5 and 6. The AND hides the wall, otherwise signals 2, which shows it.
        map.triggers.push(MapTrigger {
            action: TriggerAction::Hide,
            group: 1,
            otherwise: 2,
            ..piece("Gates", "AND", spot(0.48, 0.4))
        });
        map.triggers.push(MapTrigger {
            source: 2,
            action: TriggerAction::Show,
            group: 1,
            ..piece("Logic", "Receiver", spot(0.55, 0.4))
        });
        // 7. The alarm already watches group 1.
        map.triggers.push(MapTrigger {
            text: "The vault is open!".into(),
            ..piece("Gates", "Alarm", spot(0.4, 0.4))
        });
        // 8. Three lights in groups 2-4, toggled in turn while the vault is open.
        for group in 2..=4 {
            map.decor.push(MapDecor {
                group,
                ..match &catalog()[2]
                    .iter()
                    .find(|b| b.name == "Light")
                    .unwrap()
                    .object
                {
                    Object::Decor(light) => MapDecor {
                        rect: spot(0.35 + f32::from(group) * 0.05, 0.2),
                        ..light.clone()
                    },
                    _ => unreachable!(),
                }
            });
        }
        let mut sequence = MapTrigger {
            group: 10,
            seconds: 0.5,
            ..piece("Gates", "Sequence", spot(0.6, 0.2))
        };
        sequence.set_conditions(vec![TriggerTest {
            kind: TestKind::Hidden,
            ..Default::default()
        }]);
        map.triggers.push(sequence);
        for (channel, group) in [(10, 2), (11, 3), (12, 4)] {
            map.triggers.push(MapTrigger {
                source: channel,
                group,
                ..piece(
                    "Logic",
                    "Receiver",
                    spot(0.6, 0.1 + f32::from(group) * 0.05),
                )
            });
        }
        // 9. A reset plate clears both counters.
        map.triggers.push(MapTrigger {
            action: TriggerAction::Signal,
            group: 20,
            seconds: 3.,
            ..piece("Logic", "Plate", spot(0.48, 0.8))
        });
        for counter in [1, 2] {
            map.triggers.push(MapTrigger {
                source: 20,
                action: TriggerAction::Set,
                group: counter,
                amount: 0,
                ..piece(
                    "Logic",
                    "Receiver",
                    spot(0.4 + f32::from(counter) * 0.05, 0.9),
                )
            });
        }
        map
    }

    /// The gatehouse from docs/motions-tutorial.md, built step by step from
    /// the palette with the settings the tutorial gives.
    fn tutorial_gatehouse() -> MapDef {
        // 1. A new map: 3600 by 2600 with a spawn region in two corners.
        let mut map = Document::default().map;
        // World units to the map's fractions; the workshop's grid is 40.
        let at = |x: f32, y: f32, w: f32, h: f32| [x / 3600., y / 2600., w / 3600., h / 2600.];
        let spot = |x: f32, y: f32| [x, y, 0.04, 0.04];
        let block = |rect: [f32; 4], group: u8| MapBlock {
            rect,
            style: BlockStyle::default(),
            color: None,
            group,
        };
        let decor = |name: &str, rect: [f32; 4], group: u8| match catalog()[2]
            .iter()
            .find(|b| b.name == name)
            .map(|b| &b.object)
        {
            Some(Object::Decor(item)) => MapDecor {
                rect,
                group,
                ..item.clone()
            },
            _ => panic!("no {name} decor in the palette"),
        };
        // 2. A wall down the middle with a gap three cells high, the door in
        // group 1, and the Door over the gap. The door slides up into the wall.
        // Tanks can drive around the wall's ends: the map check wants every
        // part of the arena reachable while the door is shut.
        map.blocks.push(block(at(1780., 400., 40., 840.), 0));
        map.blocks.push(block(at(1780., 1360., 40., 840.), 0));
        map.blocks.push(block(at(1780., 1240., 40., 120.), 1));
        map.triggers
            .push(piece("Motion", "Door", at(1720., 1220., 160., 160.)));
        // 5. Cover in group 2, and the Shuttle built while group 2 is chosen.
        map.blocks.push(block(at(600., 1800., 40., 200.), 2));
        map.triggers.push(MapTrigger {
            group: 2,
            ..piece("Motion", "Shuttle", spot(0.1, 0.1))
        });
        // 6. Barricades in groups 3 to 5 that land in turn, then an announcement.
        for group in 3..=5u8 {
            let x = 2200. + f32::from(group - 3) * 200.;
            map.blocks.push(block(at(x, 600., 160., 40.), group));
        }
        map.triggers.push(MapTrigger {
            group: 3,
            ..piece("Motion", "Drop", spot(0.1, 0.2))
        });
        for group in [4u8, 5] {
            map.triggers.push(MapTrigger {
                group,
                when: TriggerWhen::Arrived,
                source: group - 1,
                ..piece("Motion", "Drop", spot(0.1, 0.1 + f32::from(group) * 0.05))
            });
        }
        map.triggers.push(MapTrigger {
            source: 5,
            action: TriggerAction::Announce,
            text: "The gates are set!".into(),
            ..piece("Motion", "Then", spot(0.1, 0.4))
        });
        // 7. Torches on each side of the door and a lantern in the yard.
        for x in [1615., 1835.] {
            map.decor.push(decor("Torch", at(x, 1095., 150., 150.), 0));
        }
        map.decor
            .push(decor("Lantern", at(2470., 1170., 260., 260.), 0));
        // 8. A turning arrow in group 6 and a light in group 7 beside the
        // door; the light blinks each time the door comes to rest.
        map.decor
            .push(decor("Arrow", at(1860., 1420., 120., 60.), 6));
        map.triggers.push(MapTrigger {
            group: 6,
            ..piece("Motion", "Spinner", spot(0.2, 0.1))
        });
        map.decor
            .push(decor("Neon", at(1620., 1400., 120., 120.), 7));
        map.triggers.push(MapTrigger {
            group: 7,
            when: TriggerWhen::Arrived,
            source: 1,
            ..piece("Motion", "Blink", spot(0.25, 0.1))
        });
        // 9. Embers above the torches, a fountain and fireflies.
        for x in [1650., 1870.] {
            map.decor.push(decor("Embers", at(x, 1010., 80., 80.), 0));
        }
        map.decor
            .push(decor("Fountain", at(2600., 1700., 200., 200.), 0));
        map.decor
            .push(decor("Fireflies", at(3060., 160., 360., 260.), 0));
        map
    }

    #[test]
    fn every_palette_piece_makes_a_valid_map() {
        let rect = [0.4, 0.4, 0.05, 0.05];
        for (tab, brushes) in CATEGORIES.iter().zip(catalog()) {
            for brush in brushes {
                let mut map = tankyard_content::builtin_maps().remove(0);
                match brush.object {
                    Object::Block(b) => map.blocks.push(MapBlock { rect, ..b }),
                    Object::Zone(z) => map.zones.push(MapZone { rect, ..z }),
                    Object::Decor(d) => map.decor.push(MapDecor { rect, ..d }),
                    Object::Trigger(t) => map.triggers.push(MapTrigger { rect, ..t }),
                    _ => continue,
                }
                if let Err(error) = tankyard_content::validate_map_features(&map) {
                    panic!("{tab} / {}: {error}", brush.name);
                }
            }
        }
    }

    #[test]
    fn motion_pieces_glide_turn_and_fade_their_group() {
        let mut map = tankyard_content::builtin_maps().remove(0);
        map.triggers.clear();
        for name in ["Shuttle", "Spinner"] {
            map.triggers
                .push(piece("Motion", name, [0.1, 0.1, 0.04, 0.04]));
        }
        map.triggers.push(MapTrigger {
            group: 2,
            ..piece("Motion", "Blink", [0.2, 0.1, 0.04, 0.04])
        });
        tankyard_content::validate_map_features(&map).unwrap();
        let config = WorldConfig {
            width: map.width,
            height: map.height,
            ..WorldConfig::default()
        };
        let mut logic = MapLogic::default();
        let mut hidden = map.starting_mask();
        let mut effects = Vec::new();
        let mut now = 0.;
        let mut run_to = |until: f32, logic: &mut MapLogic| {
            while now < until {
                now += DT;
                logic.step(
                    &map,
                    &config,
                    now,
                    &mut hidden,
                    &[],
                    &mut || 0.5,
                    &mut effects,
                );
            }
        };
        // The shuttle leaves at 6 s, arrives 2 s later, waits 1 s and returns.
        run_to(5.9, &mut logic);
        assert_eq!(logic.poses().offset(1), [0., 0.]);
        run_to(7., &mut logic);
        let halfway = logic.poses().offset(1)[0];
        assert!((0.05..0.1).contains(&halfway), "{halfway}");
        run_to(8.5, &mut logic);
        assert!((logic.poses().offset(1)[0] - 0.15).abs() < 1e-4);
        run_to(11.5, &mut logic);
        assert_eq!(logic.poses().offset(1), [0., 0.]);
        // By then the spinner has turned a quarter three times.
        assert!((logic.poses().turn(1) - 270.).abs() < 1e-3);
        // Blink dims group 2 to nothing for a moment every 2 s.
        run_to(12.4, &mut logic);
        assert!(logic.poses().opacity(2) < 0.05);
        run_to(13.5, &mut logic);
        assert!((logic.poses().opacity(2) - 1.).abs() < 1e-4);
    }

    #[test]
    fn the_door_holds_open_for_a_busy_doorway_then_shuts() {
        let mut map = tankyard_content::builtin_maps().remove(0);
        map.triggers = vec![piece("Motion", "Door", [0.4, 0.4, 0.2, 0.2])];
        tankyard_content::validate_map_features(&map).unwrap();
        let config = WorldConfig {
            width: map.width,
            height: map.height,
            ..WorldConfig::default()
        };
        let inside = Occupant {
            id: 1,
            pos: V2::new(config.width * 0.5, config.height * 0.5),
            bot: false,
            team: 0,
        };
        let away = Occupant {
            pos: V2::new(10., 10.),
            ..inside
        };
        let mut logic = MapLogic::default();
        let mut hidden = map.starting_mask();
        let mut effects = Vec::new();
        let mut now = 0.;
        let mut run = |seconds: f32, tank: Occupant, logic: &mut MapLogic| {
            let until = now + seconds;
            while now < until {
                now += DT;
                logic.step(
                    &map,
                    &config,
                    now,
                    &mut hidden,
                    &[tank],
                    &mut || 0.5,
                    &mut effects,
                );
            }
        };
        // A tank arrives every two seconds: the door slides up once and stays.
        for _ in 0..4 {
            run(1., inside, &mut logic);
            run(1., away, &mut logic);
            assert_eq!(logic.poses().offset(1), [0., -0.06]);
        }
        // The last tank arrived at 6 s; the door waits four seconds after its
        // slide, then slides home.
        run(2.4, away, &mut logic);
        assert_eq!(logic.poses().offset(1), [0., -0.06]);
        run(0.8, away, &mut logic);
        assert_eq!(logic.poses().offset(1), [0., 0.]);
    }

    #[test]
    fn the_logic_gates_tutorial_vault_works_as_written() {
        let map = tutorial_vault();
        tankyard_content::validate_map_features(&map).unwrap();
        let doc = Document::from_map(map.clone());
        assert!(doc.problems().is_empty(), "{:?}", doc.problems());

        let config = WorldConfig {
            width: map.width,
            height: map.height,
            ..WorldConfig::default()
        };
        let centre = |index: usize| {
            let r = map.triggers[index].rect;
            V2::new(
                (r[0] + r[2] * 0.5) * map.width,
                (r[1] + r[3] * 0.5) * map.height,
            )
        };
        let plate = map
            .triggers
            .iter()
            .position(|t| t.when == TriggerWhen::Enter && t.action == TriggerAction::Signal)
            .unwrap();
        let (left, right, plate) = (centre(0), centre(1), centre(plate));
        let away = V2::new(map.width * 0.05, map.height * 0.05);
        let mut logic = MapLogic::default();
        let mut hidden = map.starting_mask();
        let mut effects = Vec::new();
        let mut now = 0.;
        // Drive the tank to each stop for a second, then away for a second.
        let mut visit = |stops: &[V2], logic: &mut MapLogic, hidden: &mut u32| {
            for &stop in stops {
                for at in [stop, away] {
                    for _ in 0..60 {
                        now += DT;
                        let tank = Occupant {
                            id: 1,
                            pos: at,
                            bot: false,
                            team: 1,
                        };
                        logic.step(
                            &map,
                            &config,
                            now,
                            hidden,
                            &[tank],
                            &mut || 0.5,
                            &mut effects,
                        );
                    }
                }
            }
        };
        let open = |hidden: u32| hidden & MapDef::group_bit(1) != 0;

        visit(&[left], &mut logic, &mut hidden);
        assert_eq!(logic.counters[..2], [1, 0]);
        assert!(!open(hidden), "one lever keeps the vault shut");
        visit(&[right], &mut logic, &mut hidden);
        assert!(open(hidden), "both levers open it");
        let lights = (2..=4).map(MapDef::group_bit).sum::<u32>();
        assert_ne!(
            hidden & lights,
            0,
            "the warning lights cycle while it's open"
        );
        visit(&[left], &mut logic, &mut hidden);
        assert!(!open(hidden), "turning a lever off closes it again");
        visit(&[left], &mut logic, &mut hidden);
        assert!(open(hidden));
        visit(&[plate], &mut logic, &mut hidden);
        assert_eq!(logic.counters[..2], [0, 0], "the plate resets both levers");
        assert!(!open(hidden), "and the vault closes by itself");
        // Shut, the sequence stops: the lights stay as they were left.
        let still = hidden & lights;
        visit(&[away], &mut logic, &mut hidden);
        assert_eq!(hidden & lights, still);
        let opened = effects
            .iter()
            .filter(|e| **e == LogicEffect::Announce("The vault is open!".into()))
            .count();
        assert_eq!(opened, 2, "the alarm announces each opening");
    }

    #[test]
    fn the_motions_tutorial_gatehouse_works_as_written() {
        let map = tutorial_gatehouse();
        tankyard_content::validate_map_features(&map).unwrap();
        let doc = Document::from_map(map.clone());
        assert!(doc.problems().is_empty(), "{:?}", doc.problems());

        let config = WorldConfig {
            width: map.width,
            height: map.height,
            ..WorldConfig::default()
        };
        // In the Door's area beside the gap, and far off in a corner.
        let door = V2::new(1750., 1300.);
        let away = V2::new(300., 300.);
        let mut logic = MapLogic::default();
        let mut hidden = map.starting_mask();
        let mut effects = Vec::new();
        let mut now = 0.;
        let mut blinks = 0;
        let mut dark = false;
        // Step to round time `until` with the tank at `at`, counting each
        // time the warning light goes out.
        let mut run = |until: f32, at: V2, logic: &mut MapLogic| {
            while now < until {
                now += DT;
                let tank = Occupant {
                    id: 1,
                    pos: at,
                    bot: false,
                    team: 1,
                };
                logic.step(
                    &map,
                    &config,
                    now,
                    &mut hidden,
                    &[tank],
                    &mut || 0.5,
                    &mut effects,
                );
                let out = logic.poses().opacity(7) < 0.05;
                blinks += usize::from(out && !dark);
                dark = out;
            }
            (blinks, effects.clone())
        };
        let landed = |logic: &MapLogic, group: u8| logic.poses().offset(group) == [0., 0.08];

        // The barricades land one after another as the round starts.
        run(1., away, &mut logic);
        assert!(!landed(&logic, 3) && logic.poses().offset(4) == [0., 0.]);
        run(2., away, &mut logic);
        assert!(landed(&logic, 3) && !landed(&logic, 4));
        assert_eq!(logic.poses().offset(5), [0., 0.]);
        let (_, said) = run(4., away, &mut logic);
        assert!((3..=5).all(|group| landed(&logic, group)));
        let set = LogicEffect::Announce("The gates are set!".into());
        assert_eq!(said.iter().filter(|e| **e == set).count(), 1);

        // A tank drives up: the door slides open and the light blinks.
        run(5., door, &mut logic);
        assert_eq!(logic.poses().offset(1), [0., -0.06]);
        let (blinked, _) = run(7., away, &mut logic);
        assert_eq!(blinked, 1);
        // Another arrives while it's open: it stays put, with no blink.
        run(8., door, &mut logic);
        let (blinked, _) = run(11.4, away, &mut logic);
        assert_eq!(logic.poses().offset(1), [0., -0.06]);
        assert_eq!(
            blinked, 1,
            "a door with nowhere to go doesn't blink the light"
        );
        // Four seconds after the last arrival's half-second motion, it shuts
        // and the light blinks again.
        let (blinked, _) = run(14., away, &mut logic);
        assert_eq!(logic.poses().offset(1), [0., 0.]);
        assert_eq!(blinked, 2);

        // Meanwhile the shuttle has set off every six seconds, and rests across
        // the yard for a second each time; the arrow has turned four times.
        let (_, said) = run(14.5, away, &mut logic);
        assert_eq!(logic.poses().offset(2), [0.15, 0.]);
        assert!((logic.poses().turn(6) - 360.).abs() < 1e-3);
        assert_eq!(said.iter().filter(|e| **e == set).count(), 1);
    }
}

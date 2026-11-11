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
}

pub const CATEGORIES: [&str; 5] = ["Blocks", "Pads", "Decor", "Logic", "Markers"];

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
        }),
        size,
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
    }
}
fn marker(name: &'static str, object: Object, size: [f32; 2]) -> Brush {
    Brush { name, object, size }
}

/// Particle presets shared by the palette and the inspector.
pub const EFFECT_PRESETS: [&str; 4] = ["Embers", "Rain", "Dust", "Sparks"];
pub fn effect_preset(index: usize) -> ParticleEffect {
    match index {
        0 => ParticleEffect {
            shape: ParticleShape::Square,
            gravity: -18.,
            spin: 80.,
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
            speed: 18.,
            spread: 360.,
            rate: 5.,
            size: 12.,
            end_size: 18.,
            color: [164, 156, 140],
            end_color: [164, 156, 140],
            ..Default::default()
        },
        _ => ParticleEffect {
            shape: ParticleShape::Spark,
            speed: 240.,
            spread: 75.,
            gravity: 170.,
            rate: 14.,
            lifetime: 1.2,
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
            decor("Ring", DecorKind::Ring, [120., 120.]),
            decor("Arrow", DecorKind::Arrow, [120., 60.]),
            decor("Label", DecorKind::Label, [280., 60.]),
            effect("Embers", 0),
            effect("Rain", 1),
            effect("Dust", 2),
            effect("Sparks", 3),
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
            ),
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
            ),
            logic(
                "Exit pad",
                [160., 160.],
                MapTrigger {
                    when: TriggerWhen::Exit,
                    action: TriggerAction::Hide,
                    seconds: 0.,
                    ..Default::default()
                },
            ),
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
            ),
            logic(
                "Sender",
                [160., 160.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Signal,
                    seconds: 1.,
                    ..Default::default()
                },
            ),
            logic(
                "Receiver",
                small,
                MapTrigger {
                    when: TriggerWhen::Signal,
                    action: TriggerAction::Toggle,
                    ..Default::default()
                },
            ),
            logic(
                "Counter",
                [160., 160.],
                MapTrigger {
                    when: TriggerWhen::Enter,
                    action: TriggerAction::Count,
                    seconds: 1.,
                    ..Default::default()
                },
            ),
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
            ),
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
            ),
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
            ),
            logic(
                "Coin flip",
                small,
                MapTrigger {
                    when: TriggerWhen::Every,
                    seconds: 5.,
                    chance: 50,
                    ..Default::default()
                },
            ),
            logic(
                "Capture",
                area,
                MapTrigger {
                    when: TriggerWhen::Occupied,
                    action: TriggerAction::Count,
                    seconds: 1.,
                    ..Default::default()
                },
            ),
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
            ),
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
            ),
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
            ),
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
            ),
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
            ),
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
            ),
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
            ),
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
            ),
        ],
        vec![
            marker(
                "Spawn",
                Object::Spawn(SpawnRegion {
                    team: 0,
                    rect: [0., 0., 0.1, 0.1],
                }),
                [320., 320.],
            ),
            marker(
                "Team 1 spawn",
                Object::Spawn(SpawnRegion {
                    team: 1,
                    rect: [0., 0., 0.1, 0.1],
                }),
                [320., 320.],
            ),
            marker(
                "Team 2 spawn",
                Object::Spawn(SpawnRegion {
                    team: 2,
                    rect: [0., 0., 0.1, 0.1],
                }),
                [320., 320.],
            ),
            marker(
                "Base 1",
                Object::Base(TeamBase {
                    team: 1,
                    position: [0.5, 0.5],
                }),
                [0., 0.],
            ),
            marker(
                "Base 2",
                Object::Base(TeamBase {
                    team: 2,
                    position: [0.5, 0.5],
                }),
                [0., 0.],
            ),
            marker("Objective", Object::Objective([0.5, 0.5]), [0., 0.]),
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
    }
}

/// The corner mark of a trigger that only acts when its gate is open.
pub fn gate_mark(r: Rect, lit: f32) {
    let ink = mix(LOGIC, WHITE, lit * 0.6);
    // On the corner, clear of the action's mark.
    let size = r.w.min(r.h) * 0.24;
    let c = vec2(r.right(), r.y);
    draw_poly(c.x, c.y, 4, size, 0., Color::new(0.13, 0.12, 0.10, 1.));
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
            scenery::decoration(&item, inner, 0., 0.4, &[0; MAP_COUNTERS as usize]);
        }
        Object::Trigger(t) => {
            let size = inner.w.min(inner.h) * 0.8;
            let c = inner.center();
            trigger_badge(
                t.action,
                Rect::new(c.x - size * 0.5, c.y - size * 0.5, size, size),
                0.,
            );
        }
        Object::Spawn(region) => spawn_mark(region.team, inner, inner.w / 44.),
        Object::Base(base) => base_mark(base.team, point(inner)),
        Object::Objective(_) => objective_mark(point(inner)),
    }
}

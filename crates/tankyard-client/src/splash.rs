//! The startup splash, played out in the game's own look: a tank pops in,
//! shoots three shapes, and the name springs out of the bursts. The menu's
//! live arena fades in behind it, and the splash hands that arena to the
//! menu, so the two never cut.
//!
//! It waits for the window to settle first (a Mac moving into full screen
//! takes most of a second), and keeps still for anyone who asked the game or
//! the system to reduce motion.
//!
//! The credits NVIDIA's DLSS license and Apple's trademark guidelines ask
//! for are a line of plain text with the owners' trademark notices; no one's
//! logo is drawn.
use crate::{
    ambient::Ambient,
    dlss, graphics, hardware, render,
    settings::Settings,
    tank_model::OUTLINE,
    ui::{AMBER, RED, *},
};
use macroquad::{miniquad, prelude::*};
use std::f32::consts::TAU;
use tankyard_content::ClassDef;

const TITLE: &str = "Tank Yard";
/// When the tank fires at each shape.
const SHOTS: [f32; 3] = [0.30, 0.46, 0.62];
/// When the splash leaves on its own, and how long leaving takes.
const LEAVE: f32 = 1.8;
const CALM_LEAVE: f32 = 1.3;
const LEAVING: f32 = 0.28;
/// Input before this is ignored, so the click or key that launched the game
/// can't skip the splash.
const SKIP_AFTER: f32 = 0.15;
/// The longest the still frame waits for the window to settle.
const SETTLE_LIMIT: f64 = 1.5;
/// The longest the arena waits for the skipping key or click to end.
const RELEASE_LIMIT: f64 = 0.6;
const MOUSE_BUTTONS: [MouseButton; 3] =
    [MouseButton::Left, MouseButton::Right, MouseButton::Middle];
/// In pixels per second at the reference scale.
const BULLET_SPEED: f32 = 1100.;
/// Where the tank rests once the name is up, like the menu's showcase tank.
const REST_AIM: f32 = -0.38;
const LINE_SIZE: f32 = 15.;
const NOTICE_SIZE: f32 = 11.;
const PENTAGON: Color = Color::new(0.463, 0.553, 0.988, 1.0);
const NVIDIA: &str = "NVIDIA, GeForce, RTX and DLSS are trademarks and/or registered trademarks of NVIDIA Corporation in the U.S. and other countries.";
const APPLE: &str = "Apple, Metal and MetalFX are trademarks of Apple Inc., registered in the U.S. and other countries and regions.";

/// Show the splash. Returns the arena it revealed, for the menu to carry
/// on with, or None when the player quit during it.
pub async fn show(settings: &Settings) -> Option<Ambient> {
    set_theme(settings.dark_arena);
    set_reduced_motion(settings.reduced_motion);
    crate::accessibility::configure(
        settings.colorblind,
        settings.high_contrast,
        settings.reduced_motion,
    );
    graphics::set_quality(settings.shape_quality);
    // It animates like a game: at the chosen frame cap, or battery saver's,
    // and otherwise as smoothly as the display allows.
    let cap = crate::frame_cap(settings.frame_cap, true, settings.battery_saver);
    // A still frame in the game's color, so the window has something to
    // show while the slow start-up work runs, before anything moves.
    clear_background(bg_color());
    next_frame().await;
    // A quit request lasts only the frame it arrives in.
    if is_quit_requested() {
        return None;
    }
    hardware::probe();
    if settings.upscaler == crate::upscale::DLSS {
        // DLSS takes a few hundred milliseconds to start. Doing it on the
        // still frame keeps the animation and the menu's first frames smooth.
        let _ = dlss::status();
    }
    let tank = tankyard_content::Content::builtin()
        .class("vector")
        .cloned();
    let mut arena = Ambient::new();
    // Stay still until the window settles: a Mac animates a window into full
    // screen for most of a second, and window managers can take a frame or
    // two to apply a size. Playing through either would lose the opening.
    let settle = get_time();
    let mut size = (screen_width(), screen_height());
    loop {
        let frame = get_time();
        clear_background(bg_color());
        crate::hold_frame_rate(frame, cap, settings.vsync);
        next_frame().await;
        if is_quit_requested() {
            return None;
        }
        let now = (screen_width(), screen_height());
        let settled = now == size && !miniquad::window::fullscreen_transition();
        size = now;
        if settled || get_time() - settle > SETTLE_LIMIT {
            break;
        }
    }
    let credits = credits(settings);
    // The system's Reduce motion counts as well: nobody has had a chance to
    // set the game's own yet.
    let motion = !settings.reduced_motion && !crate::accessibility::system_reduces_motion();
    let mut leave_at = if motion { LEAVE } else { CALM_LEAVE };
    let start = get_time();
    loop {
        let frame_start = get_time();
        let t = (frame_start - start) as f32;
        if t < leave_at && t > SKIP_AFTER && skip_pressed() {
            leave_at = t;
        }
        let end = leave_at + LEAVING;
        let frame = Frame {
            t: t.min(end),
            leave_at,
            motion,
        };
        frame.draw(&mut arena, settings, &credits, tank.as_ref());
        crate::hold_frame_rate(frame_start, cap, settings.vsync);
        // The finished frame is presented once before moving on.
        next_frame().await;
        if is_quit_requested() {
            return None;
        }
        if t >= end {
            break;
        }
    }
    // Wait briefly for the key or click that skipped to end, so the next
    // screen doesn't receive it.
    let release = get_time();
    while held() && get_time() - release < RELEASE_LIMIT {
        let frame_start = get_time();
        arena.draw(settings);
        crate::hold_frame_rate(frame_start, cap, settings.vsync);
        next_frame().await;
        if is_quit_requested() {
            return None;
        }
    }
    // Typing collects in a queue that no frame empties; none of it is meant
    // for the next screen.
    while get_char_pressed().is_some() {}
    Some(arena)
}

/// A key, click or touch that skips the splash. Modifier keys, lock keys and
/// shortcuts don't, so switching apps, hiding the game or taking a
/// screenshot leaves it playing.
fn skip_pressed() -> bool {
    let shortcut = [
        KeyCode::LeftSuper,
        KeyCode::RightSuper,
        KeyCode::LeftControl,
        KeyCode::RightControl,
        KeyCode::LeftAlt,
        KeyCode::RightAlt,
    ]
    .into_iter()
    .any(is_key_down);
    (!shortcut && get_keys_pressed().into_iter().any(skips))
        || MOUSE_BUTTONS.into_iter().any(is_mouse_button_pressed)
        || touches()
            .iter()
            .any(|touch| touch.phase == TouchPhase::Started)
}

/// A key or button that could have skipped the splash is still down.
fn held() -> bool {
    get_keys_down().into_iter().any(skips) || MOUSE_BUTTONS.into_iter().any(is_mouse_button_down)
}

/// Whether pressing `key` skips the splash. Unknown keys include media and
/// volume keys on some systems.
fn skips(key: KeyCode) -> bool {
    !matches!(
        key,
        KeyCode::LeftShift
            | KeyCode::RightShift
            | KeyCode::LeftControl
            | KeyCode::RightControl
            | KeyCode::LeftAlt
            | KeyCode::RightAlt
            | KeyCode::LeftSuper
            | KeyCode::RightSuper
            | KeyCode::CapsLock
            | KeyCode::NumLock
            | KeyCode::ScrollLock
            | KeyCode::PrintScreen
            | KeyCode::Unknown
    )
}

/// What the game is running on, and the notices that go with it.
struct Credits {
    line: Option<String>,
    notices: Vec<&'static str>,
}

impl Credits {
    fn prepare(&self, s: f32) {
        if let Some(line) = &self.line {
            let size = LINE_SIZE * s;
            prepare_label(&fitted_label(line, size, screen_width() - 48. * s), size);
        }
        for notice in &self.notices {
            prepare_label(notice, NOTICE_SIZE * s);
        }
    }
}

fn credits(settings: &Settings) -> Credits {
    let metal = graphics::metal();
    let device = hardware::device();
    let name = device
        .as_ref()
        .map(hardware::Device::name)
        .filter(|name| !name.is_empty());
    // Credit an upscaler only where the game's own plan would use it at this
    // window's size: MetalFX only below 100% render scale, for example.
    let dpi = screen_dpi_scale();
    let output = (
        (screen_width() * dpi).round() as u32,
        (screen_height() * dpi).round() as u32,
    );
    let plan = crate::upscale::Plan::new(
        settings,
        output,
        graphics::metalfx_available(),
        settings.upscaler == crate::upscale::DLSS && dlss::available(),
    );
    let metalfx = plan.as_ref().is_some_and(|plan| plan.metalfx);
    let dlss_in_use = plan.as_ref().is_some_and(|plan| plan.dlss.is_some());
    let mut notices = Vec::new();
    let line = if metal {
        notices.push(APPLE);
        let with = if metalfx {
            "Metal and MetalFX"
        } else {
            "Metal"
        };
        Some(match name {
            Some(name) => format!("Running on {name} with {with}"),
            None => format!("Rendered with {with}"),
        })
    } else {
        name.map(|name| {
            if device.as_ref().is_some_and(|device| device.software) {
                format!("Running in software on {name}. Update the graphics driver for full speed.")
            } else if dlss_in_use {
                format!("Running on {name} with NVIDIA DLSS")
            } else {
                format!("Running on {name}")
            }
        })
    };
    // DLSS is part of this build, so it's credited even where it can't run.
    if cfg!(tankyard_dlss) && !metal {
        if !dlss_in_use {
            notices.push("Supports NVIDIA DLSS on GeForce RTX GPUs.");
        }
        notices.push(NVIDIA);
    }
    Credits { line, notices }
}

/// Positions for the current window size.
struct Layout {
    s: f32,
    /// Pixels per arena unit, so sizes and outlines match the game's.
    unit: f32,
    tank: Vec2,
    tank_radius: f32,
    /// From the tank's center to the end of its barrel.
    muzzle: f32,
    left: f32,
    baseline: f32,
    /// The vertical middle of the name's capitals.
    middle: f32,
    size: f32,
    shapes: [Shape; 3],
}

#[derive(Clone, Copy)]
struct Shape {
    at: Vec2,
    sides: u8,
    radius: f32,
    color: Color,
    angle: f32,
}

struct Letter {
    text: String,
    x: f32,
    center: Vec2,
}

impl Layout {
    fn new(ui_scale: f32) -> Self {
        let (w, h) = (screen_width(), screen_height());
        let s = crate::ui_layout::scale(w, h, ui_scale);
        let size = 72. * s;
        // The basic tank is 22 units across its hull, and its barrel reaches
        // 38 units plus 45% of the hull (see `tank_model`).
        let unit = 1.5 * s;
        let tank_radius = 22. * unit;
        let muzzle = (38. + 22. * 0.45) * unit;
        let gap = 18. * s;
        let title = label_width(TITLE, size);
        let tank_x = (w - (tank_radius + muzzle + gap + title)) * 0.5 + tank_radius;
        let middle = h * 0.46;
        let left = tank_x + muzzle + gap;
        let shapes = [
            (0.12, -36., 4, 15., AMBER, 0.5),
            (0.50, 34., 3, 16., RED, -0.2),
            (0.87, -20., 5, 19., PENTAGON, 0.3),
        ]
        .map(|(along, rise, sides, radius, color, angle)| Shape {
            at: vec2(left + title * along, middle + rise * s),
            sides,
            radius: radius * unit,
            color,
            angle,
        });
        Self {
            s,
            unit,
            tank: vec2(tank_x, middle),
            tank_radius,
            muzzle,
            left,
            baseline: middle + size * 0.36,
            middle,
            size,
            shapes,
        }
    }

    fn letters(&self) -> Vec<Letter> {
        TITLE
            .char_indices()
            .filter(|(_, c)| !c.is_whitespace())
            .map(|(at, c)| {
                let text = c.to_string();
                let x = self.left + label_width(&TITLE[..at], self.size);
                let center = vec2(x + label_width(&text, self.size) * 0.5, self.middle);
                Letter { text, x, center }
            })
            .collect()
    }

    /// Each shot, if it was fired before the splash started leaving.
    fn shots(&self, leave_at: f32) -> [Shot; 3] {
        std::array::from_fn(|i| {
            let target = self.shapes[i];
            let dir = (target.at - self.tank).normalize_or_zero();
            let from = self.tank + dir * self.muzzle;
            let impact = target.at - dir * target.radius * 0.7;
            let fire = SHOTS[i];
            let fired = fire < leave_at;
            let flight = from.distance(impact).max(0.) / (BULLET_SPEED * self.s);
            Shot {
                fire,
                fired,
                hit: if fired { fire + flight } else { f32::INFINITY },
                from,
                dir,
                impact,
            }
        })
    }
}

#[derive(Clone, Copy)]
struct Shot {
    fire: f32,
    fired: bool,
    hit: f32,
    from: Vec2,
    dir: Vec2,
    impact: Vec2,
}

struct Frame {
    t: f32,
    leave_at: f32,
    motion: bool,
}

impl Frame {
    fn draw(
        &self,
        arena: &mut Ambient,
        settings: &Settings,
        credits: &Credits,
        tank: Option<&ClassDef>,
    ) {
        let t = self.t;
        let layout = Layout::new(settings.ui_scale);
        prepare_label(TITLE, layout.size);
        credits.prepare(layout.s);
        let leaving = phase(t, self.leave_at, LEAVING);
        // The menu's arena comes up behind the name, and is all the way in,
        // exactly as the menu draws it, when the splash ends.
        let reveal = smoothstep(phase(t, if self.motion { 0.9 } else { 0.35 }, 0.8))
            .max(smoothstep(leaving));
        if reveal > 0. {
            arena.draw(settings);
            draw_rectangle(
                0.,
                0.,
                screen_width(),
                screen_height(),
                alpha(bg_color(), 1. - reveal),
            );
        } else {
            clear_background(bg_color());
        }
        if self.motion {
            self.scene(&layout, tank);
        } else {
            self.calm(&layout, tank);
        }
        let shown = ease_out(phase(t, if self.motion { 0.75 } else { 0.2 }, 0.35));
        let rise = if self.motion {
            (1. - shown) * 6. * layout.s
        } else {
            0.
        };
        draw_credits(credits, shown * (1. - ease_out(leaving)), rise, layout.s);
    }

    /// The scale things shrink by as the splash leaves, `delay` after it starts.
    fn out(&self, delay: f32) -> f32 {
        pop_out(phase(self.t, self.leave_at + delay, 0.16))
    }

    fn scene(&self, layout: &Layout, tank: Option<&ClassDef>) {
        let t = self.t;
        let shots = layout.shots(self.leave_at);
        // A small push along each shot as it lands.
        let shake = shots.iter().fold(Vec2::ZERO, |sum, shot| {
            sum + shot.dir * 3. * layout.s * kick(t - shot.hit, 28.)
        });
        push_model(Mat4::from_translation(shake.extend(0.)));
        self.shapes(layout, &shots);
        self.bullets(layout, &shots);
        if let Some(class) = tank {
            self.tank(layout, &shots, class);
        }
        self.title(layout, &shots);
        pop_model();
    }

    fn shapes(&self, layout: &Layout, shots: &[Shot; 3]) {
        let t = self.t;
        let edge = 2.5 * layout.unit;
        for (i, (shape, shot)) in layout.shapes.iter().zip(shots).enumerate() {
            let color = crate::accessibility::palette(shape.color);
            let turn = if i % 2 == 0 { 0.7 } else { -0.7 };
            let spin = shape.angle + t * turn;
            let at = shape.at + vec2(0., (t * 2.4 + i as f32 * 1.9).sin() * 2.5 * layout.s);
            if t < shot.hit {
                let size =
                    spring(t - 0.02 - i as f32 * 0.05, 3.8, 0.48) * self.out(i as f32 * 0.02);
                render::polygon(at, shape.sides, (shape.radius + edge) * size, spin, OUTLINE);
                render::polygon(at, shape.sides, shape.radius * size, spin, color);
                continue;
            }
            // Destroyed: it flashes white, swells and fades, and throws
            // shards the way the bullet was going.
            let age = t - shot.hit;
            let fade = phase(age, 0., 0.16);
            if fade < 1. {
                let size = 1. + 0.5 * ease_out(fade);
                let flash = mix(color, WHITE, 0.6 * (1. - phase(age, 0., 0.09)));
                let opacity = 1. - fade;
                render::polygon(
                    at,
                    shape.sides,
                    (shape.radius + edge) * size,
                    spin,
                    alpha(OUTLINE, opacity),
                );
                render::polygon(
                    at,
                    shape.sides,
                    shape.radius * size,
                    spin,
                    alpha(flash, opacity),
                );
            }
            self.shards(i, shot, color, layout);
        }
    }

    fn shards(&self, index: usize, shot: &Shot, color: Color, layout: &Layout) {
        let age = self.t - shot.hit;
        let leaving = 1. - phase(self.t, self.leave_at, LEAVING);
        let heading = shot.dir.y.atan2(shot.dir.x);
        for j in 0..8 {
            let seed = (index * 64 + j * 8) as u32;
            let life = 0.3 + noise(seed) * 0.25;
            if age >= life {
                continue;
            }
            // Most fly on with the bullet; a couple scatter anywhere.
            let angle = if j < 6 {
                heading + (noise(seed + 1) - 0.5) * 2.4
            } else {
                noise(seed + 1) * TAU
            };
            let speed = (150. + noise(seed + 2) * 320.) * layout.s;
            // Drag slows them the way it slows the arena's particles.
            let travel = speed * (1. - (-4. * age).exp()) / 4.;
            let at = shot.impact + vec2(angle.cos(), angle.sin()) * travel;
            let size = (3. + noise(seed + 3) * 4.) * layout.unit * (1. - 0.4 * age / life);
            draw_poly(
                at.x,
                at.y,
                3,
                size,
                noise(seed + 4) * 360. + age * 400.,
                alpha(color, (1. - age / life) * leaving),
            );
        }
    }

    fn bullets(&self, layout: &Layout, shots: &[Shot; 3]) {
        let t = self.t;
        let radius = 6.5 * layout.unit;
        let edge = 2. * layout.unit;
        let color = player_color();
        let leaving = 1. - phase(t, self.leave_at, 0.12);
        for shot in shots.iter().filter(|shot| shot.fired && t >= shot.fire) {
            let (at, size, opacity) = if t < shot.hit {
                let travel = (t - shot.fire) * BULLET_SPEED * layout.s;
                (shot.from + shot.dir * travel, 1., 1.)
            } else {
                // Spent: it shrinks and fades where it struck, like the
                // arena's bullets do.
                let spent = phase(t - shot.hit, 0., 0.1);
                if spent >= 1. {
                    continue;
                }
                (shot.impact, 1. - 0.25 * spent, 0.7 * (1. - spent))
            };
            let opacity = opacity * leaving;
            draw_circle(at.x, at.y, (radius + edge) * size, alpha(OUTLINE, opacity));
            draw_circle(at.x, at.y, radius * size, alpha(color, opacity));
        }
    }

    fn tank(&self, layout: &Layout, shots: &[Shot; 3], class: &ClassDef) {
        let t = self.t;
        let appear = t - 0.06;
        let size = spring(appear, 3.4, 0.5) * self.out(0.06);
        if size <= 0.001 {
            return;
        }
        let slide = (1. - spring(appear, 2.6, 0.8)) * 34. * layout.s;
        // It swings to each shape before firing, then settles. Each swing is
        // its own spring, so a new one can start before the last settles.
        let angles = shots.map(|shot| shot.dir.y.atan2(shot.dir.x));
        let swings = [
            (0.08, angles[0]),
            (SHOTS[0] + 0.03, angles[1]),
            (SHOTS[1] + 0.03, angles[2]),
            (SHOTS[2] + 0.12, REST_AIM),
        ];
        let (mut aim, mut from) = (0.9, 0.9);
        for (at, to) in swings {
            aim += (to - from) * spring(t - at, 5., 0.62);
            from = to;
        }
        let recoil = shots
            .iter()
            .filter(|shot| shot.fired)
            .map(|shot| 0.1 * kick(t - shot.fire, 36.))
            .sum();
        render::class_model(
            class,
            layout.tank - vec2(slide, 0.),
            layout.tank_radius * size,
            aim,
            recoil,
            0.,
            player_color(),
            t,
        );
    }

    /// Each letter flies out of the nearest burst and springs into place,
    /// later the farther it is from the burst.
    fn title(&self, layout: &Layout, shots: &[Shot; 3]) {
        let t = self.t;
        let color = text_color();
        let ripple = 1300. * layout.s;
        for (i, letter) in layout.letters().into_iter().enumerate() {
            let (shot, shape) = shots
                .iter()
                .zip(&layout.shapes)
                .min_by(|(_, a), (_, b)| {
                    (a.at.x - letter.center.x)
                        .abs()
                        .total_cmp(&(b.at.x - letter.center.x).abs())
                })
                .expect("three shapes");
            let born = shot.hit + (letter.center.x - shape.at.x).abs() / ripple;
            let age = t - born;
            if age <= 0. || born > self.leave_at {
                continue;
            }
            let grow = spring(age, 4., 0.42) * self.out(i as f32 * 0.014);
            if grow <= 0.001 {
                continue;
            }
            let settle = spring(age, 3.2, 0.72);
            let offset = (shot.impact - letter.center) * (1. - settle);
            let lean = if i % 2 == 0 { 0.6 } else { -0.6 };
            push_model(
                Mat4::from_translation((letter.center + offset).extend(0.))
                    * Mat4::from_rotation_z((1. - settle) * lean)
                    * Mat4::from_scale(vec3(grow, grow, 1.))
                    * Mat4::from_translation((-letter.center).extend(0.)),
            );
            label(
                &letter.text,
                letter.x,
                layout.baseline,
                layout.size,
                alpha(color, phase(age, 0., 0.05)),
            );
            pop_model();
        }
    }

    /// With reduced motion, the name and tank only fade.
    fn calm(&self, layout: &Layout, tank: Option<&ClassDef>) {
        let shown = ease_out(phase(self.t, 0., 0.3)) * (1. - phase(self.t, self.leave_at, LEAVING));
        if let Some(class) = tank {
            render::class_model(
                class,
                layout.tank,
                layout.tank_radius,
                REST_AIM,
                0.,
                0.,
                alpha(player_color(), shown),
                0.,
            );
        }
        label(
            TITLE,
            layout.left,
            layout.baseline,
            layout.size,
            alpha(text_color(), shown),
        );
    }
}

/// The running-on line, with the notices in smaller print beneath it.
fn draw_credits(credits: &Credits, opacity: f32, rise: f32, s: f32) {
    if opacity <= 0. {
        return;
    }
    let (w, h) = (screen_width(), screen_height());
    let small = NOTICE_SIZE * s;
    let notices: Vec<String> = credits
        .notices
        .iter()
        .flat_map(|notice| wrapped_lines(notice, w - 48. * s, small))
        .collect();
    let mut y = h - 18. * s - notices.len().saturating_sub(1) as f32 * small * 1.4 + rise;
    if let Some(line) = &credits.line {
        let size = LINE_SIZE * s;
        let above = if notices.is_empty() {
            0.
        } else {
            small * 1.4 + 6. * s
        };
        center_label(
            &fitted_label(line, size, w - 48. * s),
            w * 0.5,
            y - above,
            size,
            alpha(muted_color(), opacity),
        );
    }
    for notice in &notices {
        center_label(
            notice,
            w * 0.5,
            y,
            small,
            alpha(muted_color(), 0.7 * opacity),
        );
        y += small * 1.4;
    }
}

fn push_model(matrix: Mat4) {
    unsafe { get_internal_gl() }
        .quad_gl
        .push_model_matrix(matrix);
}

fn pop_model() {
    unsafe { get_internal_gl() }.quad_gl.pop_model_matrix();
}

fn phase(t: f32, start: f32, duration: f32) -> f32 {
    ((t - start) / duration).clamp(0., 1.)
}

fn ease_out(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3. - 2. * t)
}

/// A damped spring let go at `t` = 0: 0 before, settling at 1. `frequency`
/// is in hertz; `damping` below 1 overshoots before settling.
fn spring(t: f32, frequency: f32, damping: f32) -> f32 {
    if t <= 0. {
        return 0.;
    }
    let omega = TAU * frequency;
    let damped = omega * (1. - damping * damping).sqrt();
    let decay = (-damping * omega * t).exp();
    1. - decay * ((damped * t).cos() + damping * omega / damped * (damped * t).sin())
}

/// A kick that springs back: 0 at `t` = 0, peaking at 1 after 1/`rate`
/// seconds. The same critically damped shape as barrel recoil.
fn kick(t: f32, rate: f32) -> f32 {
    if t <= 0. {
        0.
    } else {
        rate * t * (1. - rate * t).exp()
    }
}

/// Leaving: a slight swell, then a quick shrink to nothing.
fn pop_out(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    let c = 1.70158;
    1. - ((c + 1.) * t * t * t - c * t * t)
}

/// A fixed pseudo-random number in [0, 1), so every launch looks the same.
fn noise(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn springs_overshoot_then_settle() {
        assert_eq!(spring(-0.1, 4., 0.5), 0.);
        assert!(spring(0.001, 4., 0.5) < 0.01, "starts at rest");
        let peak = (1..200)
            .map(|i| spring(i as f32 * 0.005, 4., 0.5))
            .fold(0., f32::max);
        assert!(peak > 1.1 && peak < 1.25, "{peak}");
        assert!((spring(3., 4., 0.5) - 1.).abs() < 1e-3);
    }

    #[test]
    fn kicks_and_exits_start_and_end_where_they_should() {
        assert_eq!(kick(0., 36.), 0.);
        assert!((kick(1. / 36., 36.) - 1.).abs() < 1e-5);
        assert!(kick(0.3, 36.) < 0.01);
        assert_eq!(pop_out(0.), 1.);
        assert!(pop_out(0.15) > 1., "swells first");
        assert!(pop_out(1.).abs() < 1e-5);
        assert_eq!(smoothstep(0.), 0.);
        assert_eq!(smoothstep(1.), 1.);
    }

    #[test]
    fn keys_skip_but_modifiers_and_system_keys_dont() {
        for key in [KeyCode::Space, KeyCode::Enter, KeyCode::Escape, KeyCode::A] {
            assert!(skips(key), "{key:?}");
        }
        for key in [
            KeyCode::LeftSuper,
            KeyCode::RightShift,
            KeyCode::LeftAlt,
            KeyCode::CapsLock,
            KeyCode::PrintScreen,
            KeyCode::Unknown,
        ] {
            assert!(!skips(key), "{key:?}");
        }
    }

    #[test]
    fn noise_is_fixed_and_in_range() {
        let values: Vec<f32> = (0..256).map(noise).collect();
        assert!(values.iter().all(|v| (0. ..1.).contains(v)));
        assert_eq!(noise(7), noise(7));
        assert_ne!(noise(7), noise(8));
    }
}

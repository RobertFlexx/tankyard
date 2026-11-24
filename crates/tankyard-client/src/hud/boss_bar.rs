//! Screen-space boss health, using the same soft light renderer as the arena.
use super::{Hud, bar_status, boss_banner_layout, grouped};
use crate::{
    accessibility,
    emission::{Emission, Layer, Stop},
    lighting, render,
    ui::*,
    ui_layout,
};
use macroquad::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use tankyard_core::{Boss, BossKind, EntityId};

/// The actual health and the damage trail already followed by `BossFx`.
pub(crate) struct Bar<'a> {
    pub id: EntityId,
    /// Axiom is a tank rather than a `Boss`, so it has no kind.
    pub kind: Option<BossKind>,
    pub name: &'a str,
    pub health: f32,
    pub max_health: f32,
    pub lost: f32,
    pub tint: Color,
    pub marks: &'a [f32],
    pub status: Option<(&'a str, u8)>,
    pub marked: bool,
}

#[derive(Default)]
pub(super) struct Animation {
    key: Option<(EntityId, Option<BossKind>, f32)>,
    at: f64,
    health: f32,
    fill: f32,
    flash: f32,
    alarm: f32,
}

#[derive(Clone, Copy)]
struct Shown {
    fill: f32,
    lost: f32,
    flash: f32,
    alarm: f32,
}

impl Animation {
    fn sample(&mut self, bar: &Bar<'_>, now: f64, calm: bool) -> Shown {
        let health = (bar.health / bar.max_health.max(1.)).clamp(0., 1.);
        let key = (bar.id, bar.kind, bar.max_health);
        let elapsed = now - self.at;
        let alarm = if bar.marked { 1. } else { 0. };
        // Reappearing, changing boss or changing maximum health is not damage.
        if self.key != Some(key) || !(0. ..=0.35).contains(&elapsed) || calm {
            self.fill = health;
            self.flash = 0.;
            self.alarm = alarm;
        } else {
            let dt = elapsed as f32;
            if health > self.health {
                self.fill = health;
                self.flash = 0.;
            } else {
                self.fill = health + (self.fill - health) * (-dt * 20.).exp();
                self.flash *= (-dt * 7.).exp();
                if health < self.health {
                    self.flash = self
                        .flash
                        .max((0.18 + (self.health - health) * 12.).min(1.));
                }
                if self.fill - health < 0.0001 {
                    self.fill = health;
                }
            }
            let rate = if bar.marked { 12. } else { 6. };
            self.alarm = alarm + (self.alarm - alarm) * (-dt * rate).exp();
        }
        self.key = Some(key);
        self.at = now;
        self.health = health;
        let lost = if calm {
            health
        } else {
            bar.lost.clamp(health, 1.).max(self.fill)
        };
        Shown {
            fill: self.fill,
            lost,
            flash: self.flash,
            alarm: self.alarm,
        }
    }
}

impl Hud {
    /// Axiom and the headline boss animate independently, even shown together.
    pub(crate) fn boss_bar(&mut self, bar: &Bar<'_>, x: f32, top: f32, w: f32, s: f32) {
        let now = get_time();
        let calm = accessibility::reduced_motion();
        let shown = self.boss_bars[usize::from(bar.kind.is_some())].sample(bar, now, calm);
        let beat = if calm {
            0.65
        } else {
            // A slow breath, never a flashing on/off warning.
            0.5 - 0.5 * (now as f32 * TAU * 1.1).cos()
        };
        draw(bar, shown, beat, x, top, w, s);
    }

    pub(crate) fn headline_boss_bar(
        &mut self,
        boss: &Boss,
        trail: f32,
        marked: bool,
        top: f32,
        s: f32,
        colorblind: bool,
    ) {
        let layout = boss_banner_layout(screen_width(), s);
        self.boss_bar(
            &Bar {
                id: boss.id,
                kind: Some(boss.kind),
                name: boss.kind.name(),
                health: boss.health,
                max_health: boss.max_health,
                lost: trail,
                tint: render::boss_tint(boss, colorblind),
                marks: &tankyard_core::BOSS_PHASES,
                status: bar_status(boss, marked),
                marked,
            },
            layout.x,
            top,
            layout.w,
            s,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(bar: &Bar<'_>, shown: Shown, beat: f32, x: f32, top: f32, w: f32, s: f32) {
    let center = x + w * 0.5;
    let size = 20. * s;
    let name = fitted_label(bar.name, size, (w - 60. * s).max(40. * s));
    // Numbers stay exact while the colored fill eases between snapshots.
    let numbers = format!(
        "{} / {}",
        grouped(bar.health.max(0.) as u64),
        grouped(bar.max_health as u64)
    );
    let status = bar
        .status
        .map(|(text, urgency)| (fitted_label(text, 13. * s, w), urgency));
    // Growing the font atlas after drawing can erase already queued labels.
    prepare_label(&name, size);
    prepare_label(&numbers, 12. * s);
    if let Some((text, _)) = &status {
        prepare_label(text, 13. * s);
    }
    arcade_center(&name, center, top + 15. * s, size);
    let icon = vec2(
        center - label_width(&name, size) * 0.5 - 27. * s,
        top + 9. * s,
    );
    match bar.kind {
        Some(kind) => tankyard_client::boss_model::boss_model_geometry(
            kind,
            icon,
            8. * s,
            tankyard_client::boss_model::Pose {
                phase: 1,
                ..Default::default()
            },
            None,
            &mut render::draw_model_primitive,
        ),
        None => tankyard_client::boss_model::axiom_geometry(
            icon,
            8. * s,
            0.,
            0.,
            &mut render::draw_model_primitive,
        ),
    }
    let track = Rect::new(x + 3. * s, top + 23. * s, w - 6. * s, 16. * s);
    draw_track(track, shown, bar.tint, bar.marks, beat, s);
    arcade_center(&numbers, center, track.y + track.h * 0.5 + 4.4 * s, 12. * s);
    if let Some((text, urgency)) = status {
        let color = match urgency {
            0 => Color::new(0.74, 0.77, 0.82, 1.),
            1 => AMBER,
            _ => alpha(Color::new(1., 0.42, 0.38, 1.), 0.8 + 0.2 * beat),
        };
        arcade_center_in(&text, center, top + 55. * s, 13. * s, color);
    }
}

fn draw_track(rect: Rect, shown: Shown, tint: Color, marks: &[f32], beat: f32, s: f32) {
    if rect.w <= 0. || rect.h <= 0. {
        return;
    }
    let ink = Color::new(0.15, 0.15, 0.16, 1.);
    let red = Color::new(1., 0.16, 0.18, 1.);
    let rim = (2.8 * s).max(1.5);
    let outer = Rect::new(
        rect.x - rim,
        rect.y - rim,
        rect.w + rim * 2.,
        rect.h + rim * 2.,
    );
    capsule(outer, mix(ink, red, shown.alarm * (0.62 + 0.3 * beat)));
    gradient(rect, Color::new(0.25, 0.26, 0.3, 1.), None);
    let lost = rect.w * shown.lost;
    let fill = rect.w * shown.fill;
    let damage = lost - fill;
    let boundary = (damage > 0.).then_some((rect.x + fill, (3. * s).min(damage * 0.4)));
    gradient(Rect::new(rect.x, rect.y, lost, rect.h), tint, boundary);

    if shown.alarm > 0.001 || damage > 0.25 * s {
        let mut light = Emission::begin(false, 1);
        if shown.alarm > 0.001 {
            // A soft outer halo and a tighter lit rim, not stacked flat outlines.
            let strength = shown.alarm * (0.55 + 0.45 * beat);
            let mut stops = capsule_stops(outer, (9. + 3. * beat) * s, alpha(red, strength * 0.48));
            light.band(Layer::Wash, &stops);
            for stop in &mut stops {
                stop.half = 3. * s;
                stop.color.a = strength * 0.55;
            }
            light.band(Layer::Core, &stops);
        }
        if damage > 0.25 * s {
            let cy = rect.y + rect.h * 0.5;
            let strength = (damage / (4. * s)).min(1.) * (0.55 + 0.45 * shown.flash);
            let at = ui_layout::position(vec2(rect.x + fill + damage * 0.5, cy));
            light.oval(
                Layer::Wash,
                at,
                vec2(damage * 0.5 + 3. * s, 0.),
                (9. + 3. * shown.flash) * s,
                alpha(WHITE, strength * 0.58),
            );
            let edge = ui_layout::position(vec2(rect.x + fill, cy));
            light.spot(Layer::Wash, edge, 13. * s, alpha(WHITE, strength * 0.32));
            light.oval(
                Layer::Core,
                edge,
                vec2(0., rect.h * 0.45),
                3. * s,
                alpha(WHITE, strength * 0.8),
            );
        }
        light.flush(None);
    }

    // Phase marks and labels sit above the light and remain crisp.
    let nick = 4.5 * s;
    for &mark in marks {
        let at = rect.x + rect.w * mark;
        let (high, low) = (outer.y, outer.bottom());
        draw_triangle(
            vec2(at - nick, high),
            vec2(at + nick, high),
            vec2(at, rect.y + nick),
            ink,
        );
        draw_triangle(
            vec2(at - nick, low),
            vec2(at + nick, low),
            vec2(at, rect.bottom() - nick),
            ink,
        );
        draw_rectangle(
            at - 0.75 * s,
            rect.y,
            1.5 * s,
            rect.h,
            Color::new(0.15, 0.15, 0.16, 0.5),
        );
    }
}

const CAP_STEPS: usize = 12;
const OUTLINE_POINTS: usize = (CAP_STEPS + 1) * 2 + 2;

fn capsule_stops(rect: Rect, half: f32, color: Color) -> [Stop; OUTLINE_POINTS] {
    let r = rect.h.min(rect.w) * 0.5;
    std::array::from_fn(|i| {
        // Close on the straight top edge so both end tangents agree; closing
        // on a corner can leave a visible join in a wide soft band.
        let at = if i == 0 || i == OUTLINE_POINTS - 1 {
            vec2(rect.center().x, rect.y)
        } else {
            let (cx, start, step) = if i <= CAP_STEPS + 1 {
                (rect.right() - r, -FRAC_PI_2, i - 1)
            } else {
                (rect.x + r, FRAC_PI_2, i - CAP_STEPS - 2)
            };
            let angle = start + PI * step as f32 / CAP_STEPS as f32;
            vec2(cx + angle.cos() * r, rect.center().y + angle.sin() * r)
        };
        Stop {
            at: ui_layout::position(at),
            half,
            color,
        }
    })
}

const ROWS: usize = 16;
const COLUMNS: usize = 5;
const VERTICES: usize = (ROWS + 1) * COLUMNS;
const INDICES: [u16; ROWS * (COLUMNS - 1) * 6] = {
    let mut indices = [0; ROWS * (COLUMNS - 1) * 6];
    let mut row = 0;
    while row < ROWS {
        let mut col = 0;
        while col < COLUMNS - 1 {
            let at = (row * (COLUMNS - 1) + col) * 6;
            let a = (row * COLUMNS + col) as u16;
            let b = a + COLUMNS as u16;
            indices[at] = a;
            indices[at + 1] = a + 1;
            indices[at + 2] = b + 1;
            indices[at + 3] = a;
            indices[at + 4] = b + 1;
            indices[at + 5] = b;
            col += 1;
        }
        row += 1;
    }
    indices
};

/// A continuous vertical gradient, with an optional soft blend into lost health.
/// The fixed mesh uses the ordinary batcher on both OpenGL and Metal.
fn gradient(rect: Rect, tint: Color, boundary: Option<(f32, f32)>) {
    if rect.w <= 0. || rect.h <= 0. {
        return;
    }
    let mut vertices = gradient_vertices(rect, tint, boundary);
    for vertex in &mut vertices {
        let at = ui_layout::position(vertex.position.truncate());
        vertex.position.x = at.x;
        vertex.position.y = at.y;
    }
    lighting::geometry(&vertices, &INDICES);
}

fn gradient_vertices(rect: Rect, tint: Color, boundary: Option<(f32, f32)>) -> [Vertex; VERTICES] {
    let r = rect.h.min(rect.w) * 0.5;
    std::array::from_fn(|i| {
        let angle = -FRAC_PI_2 + PI * (i / COLUMNS) as f32 / ROWS as f32;
        let y = (angle.sin() + 1.) * 0.5;
        let cap = angle.cos().max(0.) * r;
        let left = rect.x + r - cap;
        let right = rect.right() - r + cap;
        let x = match (i % COLUMNS, boundary) {
            (0, _) => left,
            (4, _) => right,
            (col, Some((edge, blend))) => (edge + (col as f32 - 2.) * blend).clamp(left, right),
            (col, None) => left + (right - left) * col as f32 / (COLUMNS - 1) as f32,
        };
        let pale = boundary.map_or(0., |(edge, blend)| {
            let t = ((x - edge + blend) / (2. * blend).max(0.0001)).clamp(0., 1.);
            t * t * (3. - 2. * t)
        });
        let body = shade(tint, y);
        let lost = mix(WHITE, shade(mix(tint, WHITE, 0.9), y), 0.35);
        Vertex::new(
            x,
            rect.center().y + angle.sin() * r,
            0.,
            0.,
            0.,
            mix(body, lost, pale),
        )
    })
}

fn shade(tint: Color, y: f32) -> Color {
    let (from, to, t) = if y < 0.3 {
        (mix(tint, WHITE, 0.1), mix(tint, WHITE, 0.32), y / 0.3)
    } else if y < 0.6 {
        (mix(tint, WHITE, 0.32), tint, (y - 0.3) / 0.3)
    } else {
        (tint, mix(tint, BLACK, 0.24), (y - 0.6) / 0.4)
    };
    mix(from, to, t * t * (3. - 2. * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar() -> Bar<'static> {
        Bar {
            id: 1,
            kind: Some(BossKind::Sentinel),
            name: "Sentinel",
            health: 100.,
            max_health: 100.,
            lost: 1.,
            tint: BLUE,
            marks: &[],
            status: None,
            marked: false,
        }
    }

    #[test]
    fn damage_eases_without_overshoot_at_every_frame_rate() {
        let mut ends = Vec::new();
        for fps in [30, 60, 144, 240] {
            let mut bar = bar();
            let mut animation = Animation::default();
            animation.sample(&bar, 0., false);
            bar.health = 35.;
            animation.sample(&bar, 0., false);
            let mut before = 1.;
            for frame in 1..=fps / 5 {
                animation.sample(&bar, f64::from(frame) / f64::from(fps), false);
            }
            ends.push(animation.sample(&bar, 0.2, false).fill);
            for frame in (fps / 5 + 1)..=fps {
                bar.lost = 0.35 + 0.65 * (-(frame as f32 / fps as f32) * 5.).exp();
                let shown = animation.sample(&bar, f64::from(frame) / f64::from(fps), false);
                assert!(shown.fill >= 0.35 && shown.fill <= before);
                assert!(shown.lost >= shown.fill && shown.lost <= 1.);
                before = shown.fill;
            }
            assert_eq!(animation.fill, 0.35);
        }
        assert!(
            ends.windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 0.0001)
        );
    }

    #[test]
    fn new_bosses_changed_maximums_and_reappearing_bars_reset_cleanly() {
        let mut bar = bar();
        bar.marked = true;
        let mut animation = Animation::default();
        animation.sample(&bar, 0., false);
        bar.health = 40.;
        animation.sample(&bar, 0.05, false);
        bar.id = 2;
        bar.health = 80.;
        bar.marked = false;
        let shown = animation.sample(&bar, 0.06, false);
        assert_eq!((shown.fill, shown.flash, shown.alarm), (0.8, 0., 0.));
        bar.max_health = 200.;
        assert_eq!(animation.sample(&bar, 0.07, false).fill, 0.4);
        bar.health = 20.;
        assert_eq!(animation.sample(&bar, 5., false).fill, 0.1);
    }

    #[test]
    fn healing_and_reduced_motion_do_not_flash_or_show_false_damage() {
        let mut bar = bar();
        let mut animation = Animation::default();
        animation.sample(&bar, 0., false);
        bar.health = 50.;
        animation.sample(&bar, 0.05, false);
        bar.health = 75.;
        let shown = animation.sample(&bar, 0.06, false);
        assert_eq!((shown.fill, shown.flash), (0.75, 0.));
        bar.health = 20.;
        bar.marked = true;
        let shown = animation.sample(&bar, 0.07, true);
        assert_eq!(
            (shown.fill, shown.lost, shown.flash, shown.alarm),
            (0.2, 0.2, 0., 1.)
        );
    }

    #[test]
    fn targeting_fades_without_changing_health_or_replaying_damage() {
        let mut bar = bar();
        let mut animation = Animation::default();
        animation.sample(&bar, 0., false);
        bar.marked = true;
        let lit = animation.sample(&bar, 0.05, false);
        assert!(lit.alarm > 0. && lit.alarm < 1.);
        bar.marked = false;
        let fading = animation.sample(&bar, 0.1, false);
        assert!(fading.alarm > 0. && fading.alarm < lit.alarm);
        assert_eq!((fading.fill, fading.lost, fading.flash), (1., 1., 0.));
    }

    #[test]
    fn empty_and_tiny_health_never_expand_to_a_full_end_cap() {
        for width in [0., 0.01, 0.1, 1., 8., 16., 100., 600.] {
            let rect = Rect::new(10., 20., width, 16.);
            let vertices = gradient_vertices(rect, BLUE, Some((rect.x + width * 0.4, 0.5)));
            for vertex in &vertices {
                let p = vertex.position;
                assert!(p.is_finite());
                assert!(p.x >= rect.x && p.x <= rect.right());
                assert!(p.y >= rect.y && p.y <= rect.bottom());
            }
            assert!(INDICES.iter().all(|&i| usize::from(i) < vertices.len()));
        }
    }
}

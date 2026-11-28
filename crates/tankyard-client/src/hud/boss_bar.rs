//! Screen-space boss health, using the same soft light renderer as the arena.
use super::{Hud, bar_status, boss_banner_layout, grouped};
use crate::{
    accessibility,
    boss_fx::BossFx,
    emission::{Emission, Layer, Stop},
    lighting, render,
    ui::*,
    ui_layout,
};
use macroquad::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use tankyard_core::{BossKind, EntityId, Snapshot};

/// The actual health and the damage trail already followed by `BossFx`.
struct Bar<'a> {
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

#[derive(Clone, Copy)]
pub(crate) struct Layout {
    area: Rect,
    count: usize,
    columns: usize,
    scale: f32,
    pitch: f32,
    compact: bool,
}

impl Layout {
    pub(crate) fn for_snapshot(snapshot: Option<&Snapshot>, scale: f32) -> Self {
        let count = snapshot.map_or(0, |snapshot| {
            snapshot
                .bosses
                .iter()
                .filter(|boss| boss.health > 0.)
                .count()
                + snapshot
                    .tanks
                    .iter()
                    .filter(|tank| tank.creator_boss && tank.alive)
                    .count()
        });
        Self::new(screen_width(), screen_height(), scale, count)
    }

    fn new(width: f32, height: f32, scale: f32, count: usize) -> Self {
        let area = boss_banner_layout(width, scale);
        let top = 106. * scale;
        let columns = match count {
            0..=1 => 1,
            3 if area.w >= 580. * scale => 3,
            2..=4 => 2,
            5..=12 => 3,
            _ => 4,
        };
        let compact = count > 4;
        let row_height = if compact { 40. } else { 61. };
        let rows = count.div_ceil(columns).max(1) as f32;
        let min_column = if compact { 132. } else { 188. };
        let scale = scale
            .min(area.w / (columns as f32 * (min_column + 10.)))
            .min(height * 0.29 / (rows * row_height));
        Self {
            area: Rect::new(area.x, top, area.w, rows * row_height * scale),
            count,
            columns,
            scale,
            pitch: row_height * scale,
            compact,
        }
    }

    pub(crate) fn height(self) -> f32 {
        self.count.div_ceil(self.columns) as f32 * self.pitch
    }

    fn cell(self, index: usize) -> Rect {
        let row = index / self.columns;
        let column = index % self.columns;
        let in_row = self.columns.min(self.count - row * self.columns);
        let gap = 14. * self.scale;
        let width = (self.area.w - gap * (self.columns - 1) as f32) / self.columns as f32;
        let span = width * in_row as f32 + gap * (in_row - 1) as f32;
        Rect::new(
            self.area.center().x - span * 0.5 + column as f32 * (width + gap),
            self.area.y + row as f32 * self.pitch,
            width,
            self.pitch,
        )
    }

    fn track(self, index: usize) -> Rect {
        let r = self.cell(index);
        let (y, h) = if self.compact { (17., 12.) } else { (23., 16.) };
        Rect::new(
            r.x + 3. * self.scale,
            r.y + y * self.scale,
            r.w - 6. * self.scale,
            h * self.scale,
        )
    }
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
        // NaN would pass `clamp` and later cross the gradient's bounds.
        let health = tankyard_core::within(bar.health / bar.max_health.max(1.), 0., 1.);
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
    pub(crate) fn draw_bosses(
        &mut self,
        snapshot: &Snapshot,
        local: EntityId,
        fx: &BossFx,
        layout: Layout,
        colorblind: bool,
    ) {
        let mut bars = Vec::with_capacity(layout.count);
        for boss in snapshot.bosses.iter().filter(|boss| boss.health > 0.) {
            let marked = boss.target != 0 && boss.target == local;
            bars.push(Bar {
                id: boss.id,
                kind: Some(boss.kind),
                name: if boss.mothership {
                    "Mothership"
                } else {
                    boss.worm_part()
                        .map_or(boss.kind.name(), |part| part.name())
                },
                health: boss.health,
                max_health: boss.max_health,
                lost: fx.health_trail(boss),
                tint: render::boss_tint(boss, colorblind),
                marks: match boss.worm_part() {
                    Some(tankyard_core::WormPart::Whole) => &tankyard_core::sunderworm::WORM_MARKS,
                    Some(_) => &[0.5],
                    None => &tankyard_core::BOSS_PHASES,
                },
                status: bar_status(boss, marked),
                marked,
            });
        }
        for tank in snapshot
            .tanks
            .iter()
            .filter(|tank| tank.creator_boss && tank.alive)
        {
            let health = (tank.health / tank.max_health.max(1.)).clamp(0., 1.);
            bars.push(Bar {
                id: tank.id,
                kind: None,
                name: tankyard_core::creator_boss::NAME,
                health: tank.health,
                max_health: tank.max_health,
                lost: fx.axiom_trail(tank.id, health),
                tint: accessibility::palette(tankyard_client::boss_model::AXIOM_GOLD),
                marks: &tankyard_core::BOSS_PHASES,
                status: None,
                marked: false,
            });
        }
        // Keep a bar's position when its target changes.
        bars.sort_unstable_by_key(|bar| bar.id);
        let now = get_time();
        let calm = accessibility::reduced_motion();
        let beat = if calm {
            0.65
        } else {
            0.5 - 0.5 * (now as f32 * TAU * 1.1).cos()
        };
        if !bars.is_empty() {
            let daylight = if is_dark() { 0. } else { 0.72 };
            let mut light = Emission::begin(daylight, 1);
            for (index, bar) in bars.iter().enumerate() {
                let shown = self
                    .boss_bars
                    .entry(bar.id)
                    .or_default()
                    .sample(bar, now, calm);
                draw_track(
                    layout.track(index),
                    shown,
                    bar.tint,
                    beat,
                    layout.scale,
                    &mut light,
                );
            }
            light.flush(None);
            let _text = text_after_shapes();
            for (index, bar) in bars.iter().enumerate() {
                draw_marks(layout.track(index), bar.marks, layout.scale);
                draw_labels(bar, layout, index, beat);
            }
        }
        self.boss_bars.retain(|_, animation| animation.at == now);
    }
}

fn draw_labels(bar: &Bar<'_>, layout: Layout, index: usize, beat: f32) {
    let r = layout.cell(index);
    let track = layout.track(index);
    let s = layout.scale;
    let center = r.center().x;
    let size = if layout.compact { 14. * s } else { 20. * s };
    let padding = if layout.compact { 8. * s } else { 60. * s };
    let name = fitted_label(bar.name, size, r.w - padding);
    let numbers = format!(
        "{} / {}",
        grouped(bar.health.max(0.) as u64),
        grouped(bar.max_health as u64)
    );
    if !layout.compact {
        let icon = vec2(
            center - label_width(&name, size) * 0.5 - 24. * s,
            r.y + 9. * s,
        );
        match bar.kind {
            Some(kind) => tankyard_client::boss_model::boss_model_geometry(
                kind,
                icon,
                7. * s,
                tankyard_client::boss_model::Pose {
                    phase: 1,
                    ..Default::default()
                },
                None,
                &mut render::draw_model_primitive,
            ),
            None => tankyard_client::boss_model::axiom_geometry(
                icon,
                7. * s,
                0.,
                0.,
                &mut render::draw_model_primitive,
            ),
        }
    }
    arcade_center(
        &name,
        center,
        r.y + if layout.compact { 12. * s } else { 15. * s },
        size,
    );
    let number_size = if layout.compact { 10. * s } else { 12. * s };
    let numbers = fitted_label(&numbers, number_size, track.w - 4. * s);
    arcade_center(
        &numbers,
        center,
        track.center().y + number_size * 0.36,
        number_size,
    );
    if let Some((text, urgency)) = bar.status {
        let (size, baseline) = if layout.compact {
            (9., 38.)
        } else {
            (13., 55.)
        };
        let size = size * s;
        let text = fitted_label(text, size, r.w - 6. * s);
        let color = match urgency {
            0 => Color::new(0.74, 0.77, 0.82, 1.),
            1 => AMBER,
            _ => alpha(Color::new(1., 0.42, 0.38, 1.), 0.8 + 0.2 * beat),
        };
        arcade_center_in(&text, center, r.y + baseline * s, size, color);
    }
}

fn draw_track(rect: Rect, shown: Shown, tint: Color, beat: f32, s: f32, light: &mut Emission) {
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
    }
}

fn draw_marks(rect: Rect, marks: &[f32], s: f32) {
    let ink = Color::new(0.15, 0.15, 0.16, 1.);
    let rim = (2.8 * s).max(1.5);
    // Phase marks and labels sit above the light and remain crisp.
    let nick = 4.5 * s;
    for &mark in marks {
        let at = rect.x + rect.w * mark;
        let (high, low) = (rect.y - rim, rect.bottom() + rim);
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
        // At the ends of a square gradient, `rect.x + r` and
        // `rect.right() - r` are the same point in exact maths but can land a
        // float step apart here. `f32::clamp` panics on crossed bounds, so
        // keep them ordered before using them.
        let (left, right) = {
            let a = rect.x + r - cap;
            let b = rect.right() - r + cap;
            if a <= b { (a, b) } else { (b, a) }
        };
        let x = match (i % COLUMNS, boundary) {
            (0, _) => left,
            (4, _) => right,
            (col, Some((edge, blend))) => {
                tankyard_core::within(edge + (col as f32 - 2.) * blend, left, right)
            }
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

    #[test]
    fn every_boss_gets_a_cell_inside_the_hud_at_common_window_sizes() {
        for (w, h, preference) in [(640., 400., 1.4), (1280., 800., 1.), (2560., 1080., 0.8)] {
            let scale = ui_layout::scale(w, h, preference);
            for count in 0..=17 {
                let layout = Layout::new(w, h, scale, count);
                let mut cells = Vec::<Rect>::new();
                for index in 0..count {
                    let cell = layout.cell(index);
                    assert!(cell.x >= 0. && cell.right() <= w + 0.01);
                    assert!(
                        cell.y >= layout.area.y
                            && cell.bottom() <= layout.area.y + layout.height() + 0.01
                    );
                    assert!(layout.track(index).w > 0.);
                    assert!(cells.iter().all(|other| {
                        cell.intersect(*other)
                            .is_none_or(|shared| shared.w * shared.h < 0.02)
                    }));
                    cells.push(cell);
                }
                assert!(layout.height() <= h * 0.29 + 0.01);
            }
        }
    }

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

    #[test]
    fn broken_health_numbers_still_give_a_drawable_bar() {
        let mut animation = Animation::default();
        for (health, max_health) in [
            (f32::NAN, 100.),
            (50., f32::NAN),
            (f32::INFINITY, 100.),
            (-40., 100.),
            (1e30, 1e-30),
        ] {
            let shown = animation.sample(
                &Bar {
                    health,
                    max_health,
                    ..bar()
                },
                0.5,
                false,
            );
            assert!(shown.fill.is_finite() && (0.0..=1.).contains(&shown.fill));
            assert!(shown.lost.is_finite() && (0.0..=1.).contains(&shown.lost));
        }
    }

    #[test]
    fn a_damage_boundary_at_a_rounded_square_end_stays_drawable() {
        // When the lost-health fill is exactly as wide as it is tall, the two
        // cap ends are the same point. At some x positions `rect.x + r` and
        // `rect.right() - r` round a float step apart; clamping between them
        // used to panic and take the game down on the killing blow.
        let side = 16. * 0.62;
        let rect = Rect::new(758., 100., side, side);
        let vertices = gradient_vertices(rect, BLUE, Some((rect.x + side * 0.5, 1.)));
        for vertex in &vertices {
            let p = vertex.position;
            assert!(p.is_finite());
            assert!(p.x >= rect.x && p.x <= rect.right());
            assert!(p.y >= rect.y && p.y <= rect.bottom());
        }
    }
}

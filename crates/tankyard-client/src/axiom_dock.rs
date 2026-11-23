//! Axiom's attack bar. All twelve attacks sit on the number row, in three rows
//! by what they're for, so a hand can learn them without looking. The server
//! has the last word on every attack; this only keeps a press from asking for
//! something it can already see won't happen.
use crate::hud_widgets::{rounded, rounded_lines};
use crate::ui::{
    RED, alpha, center_label, contains, fitted_label, label, mix, muted_color, panel, right_label,
    surface_color, take_click, text_color, ui_circle as draw_circle,
    ui_circle_lines as draw_circle_lines, ui_line as draw_line, ui_triangle as draw_triangle,
    wrapped_lines,
};
use macroquad::prelude::{BLACK, Color, KeyCode, Rect, Vec2, screen_height, vec2};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};
use tankyard_client::boss_model::{AXIOM_GOLD, AXIOM_TEAL};
use tankyard_core::creator_boss::{ATTACK_COUNT, Attack};
use tankyard_server::creator::APPLIED;

/// The keys in the bar's order, with what each cap says.
pub const KEYS: [(KeyCode, &str); ATTACK_COUNT] = [
    (KeyCode::Key1, "1"),
    (KeyCode::Key2, "2"),
    (KeyCode::Key3, "3"),
    (KeyCode::Key4, "4"),
    (KeyCode::Key5, "5"),
    (KeyCode::Key6, "6"),
    (KeyCode::Key7, "7"),
    (KeyCode::Key8, "8"),
    (KeyCode::Key9, "9"),
    (KeyCode::Key0, "0"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
];

const WIDTH: f32 = 246.;
const HEIGHT: f32 = 225.;
const MARGIN: f32 = 22.;
const COLUMNS: usize = 4;
const CELL: f32 = 52.;
const GAP: f32 = 6.;
const GRID_TOP: f32 = 30.;
const CORNER: f32 = 7.;
/// A sent attack isn't asked for again until the server has had time to
/// answer, so a double tap is one command and not a signed one wasted.
const ASK_AGAIN: f64 = 0.4;
const READY_PING: f32 = 0.5;
const DENIED_FLASH: f32 = 0.38;
const NOTE_LIFE: f32 = 2.8;
/// Replies stop being waited for after this long, in case one never comes.
const REPLY_WAIT: f32 = 3.;
/// The green of the fortress row.
const FORTRESS: Color = Color::new(0.56, 0.87, 0.52, 1.);

/// What the bar needs to know about Axiom right now.
#[derive(Clone, Copy, Debug)]
pub struct Form {
    pub cooldowns: [f32; ATTACK_COUNT],
    pub hull_full: bool,
}

/// What to do about a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Send,
    /// Sent a moment ago and still waiting to hear.
    Sent,
    Cooling,
    HullFull,
}

fn verdict(attack: Attack, remaining: f32, hull_full: bool, since_asked: f64) -> Verdict {
    if remaining > 0. {
        Verdict::Cooling
    } else if attack == Attack::Reforge && hull_full {
        Verdict::HullFull
    } else if since_asked < ASK_AGAIN {
        Verdict::Sent
    } else {
        Verdict::Send
    }
}

/// Which cell of the bar holds `attack`.
fn slot_of(attack: Attack) -> usize {
    Attack::bar()
        .iter()
        .position(|&other| other == attack)
        .unwrap_or(0)
}

/// One of Axiom's colors as words or an icon on a panel. A light panel
/// washes the bright versions out, so there they go deeper.
pub(crate) fn readable(color: Color) -> Color {
    if crate::ui::is_dark() {
        color
    } else {
        mix(color, text_color(), 0.5)
    }
}

fn group_color(slot: usize) -> Color {
    crate::accessibility::palette(match slot / COLUMNS {
        0 => AXIOM_GOLD,
        1 => AXIOM_TEAL,
        _ => FORTRESS,
    })
}

/// Seconds left, as short as it can be said.
fn countdown(seconds: f32) -> String {
    if seconds < 10. {
        format!("{:.1}", (seconds * 10.).ceil() / 10.)
    } else {
        format!("{:.0}", seconds.ceil())
    }
}

/// Where a ray `angle` clockwise from twelve o'clock leaves a rounded rectangle.
fn rim(rect: Rect, radius: f32, angle: f32) -> Vec2 {
    let way = vec2(angle.sin(), -angle.cos());
    let half = vec2(rect.w, rect.h) * 0.5;
    let reach = (half.x / way.x.abs().max(1e-5)).min(half.y / way.y.abs().max(1e-5));
    let hit = way * reach;
    let inner = half - Vec2::splat(radius);
    if hit.x.abs() > inner.x && hit.y.abs() > inner.y {
        let corner = vec2(inner.x * way.x.signum(), inner.y * way.y.signum());
        let along = way.dot(corner);
        let root = (along * along - corner.length_squared() + radius * radius).max(0.);
        return rect.center() + way * (along + root.sqrt());
    }
    rect.center() + hit
}

/// A clock-face shade over `share` of a rounded rectangle, the part still to
/// recharge, shrinking clockwise toward twelve.
fn sweep(rect: Rect, radius: f32, share: f32, color: Color) {
    const STEPS: f32 = 48.;
    let share = share.clamp(0., 1.);
    if share <= 0. {
        return;
    }
    let center = rect.center();
    let first = TAU * (1. - share);
    let count = (share * STEPS).ceil().max(1.);
    let mut last = rim(rect, radius, first);
    for step in 1..=count as usize {
        let next = rim(rect, radius, first + TAU * share * step as f32 / count);
        draw_triangle(center, last, next, color);
        last = next;
    }
}

/// The bar's state between frames.
pub struct AxiomDock {
    ready: [f32; ATTACK_COUNT],
    denied: [f32; ATTACK_COUNT],
    cooling: [bool; ATTACK_COUNT],
    asked: [f64; ATTACK_COUNT],
    note: String,
    note_life: f32,
    /// Attacks sent to a server and not answered yet, and how long the
    /// oldest has been waiting.
    unanswered: u8,
    waited: f32,
}

impl Default for AxiomDock {
    fn default() -> Self {
        Self {
            ready: [0.; ATTACK_COUNT],
            denied: [0.; ATTACK_COUNT],
            cooling: [false; ATTACK_COUNT],
            asked: [f64::NEG_INFINITY; ATTACK_COUNT],
            note: String::new(),
            note_life: 0.,
            unanswered: 0,
            waited: 0.,
        }
    }
}

impl AxiomDock {
    /// Where the bar sits. It takes the corner the upgrade panel leaves free.
    pub fn rect(s: f32) -> Rect {
        Rect::new(
            MARGIN * s,
            screen_height() - (MARGIN + HEIGHT) * s,
            WIDTH * s,
            HEIGHT * s,
        )
    }

    fn cell(r: Rect, slot: usize, s: f32) -> Rect {
        let across = COLUMNS as f32 * CELL + (COLUMNS - 1) as f32 * GAP;
        let left = r.x + (WIDTH - across) * 0.5 * s;
        Rect::new(
            left + (slot % COLUMNS) as f32 * (CELL + GAP) * s,
            r.y + (GRID_TOP + (slot / COLUMNS) as f32 * (CELL + GAP)) * s,
            CELL * s,
            CELL * s,
        )
    }

    /// A press of `attack`. True when it should go to the server.
    pub fn press(&mut self, attack: Attack, form: &Form, now: f64) -> bool {
        let slot = slot_of(attack);
        let remaining = form.cooldowns[attack.index()];
        match verdict(attack, remaining, form.hull_full, now - self.asked[slot]) {
            Verdict::Send => {
                self.asked[slot] = now;
                true
            }
            Verdict::Sent => false,
            Verdict::Cooling => {
                self.denied[slot] = DENIED_FLASH;
                false
            }
            Verdict::HullFull => {
                self.denied[slot] = DENIED_FLASH;
                self.tell("Nothing to reforge.");
                false
            }
        }
    }

    /// An attack went to a server, whose reply should stay quiet.
    pub fn sent(&mut self) {
        if self.unanswered == 0 {
            self.waited = 0.;
        }
        self.unanswered = self.unanswered.saturating_add(1);
    }

    /// A reply from the server. True if it only confirms an attack, which the
    /// arena already shows; anything else is also for the player to read.
    pub fn heard(&mut self, message: &str) -> bool {
        if self.unanswered == 0 {
            return false;
        }
        self.unanswered -= 1;
        self.waited = 0.;
        if message == APPLIED {
            return true;
        }
        self.tell(message);
        false
    }

    /// Say something on the bar's bottom line.
    pub fn tell(&mut self, message: &str) {
        self.note.clear();
        self.note.push_str(message);
        self.note_life = NOTE_LIFE;
    }

    /// Axiom isn't out: forget anything half-shown.
    pub fn rest(&mut self) {
        self.ready = [0.; ATTACK_COUNT];
        self.denied = [0.; ATTACK_COUNT];
        self.cooling = [false; ATTACK_COUNT];
        self.note_life = 0.;
    }

    fn tick(&mut self, cooldowns: &[f32; ATTACK_COUNT], dt: f32) {
        for (slot, attack) in Attack::bar().iter().enumerate() {
            let cooling = cooldowns[attack.index()] > 0.;
            if self.cooling[slot] && !cooling {
                self.ready[slot] = READY_PING;
            }
            self.cooling[slot] = cooling;
            self.ready[slot] = (self.ready[slot] - dt).max(0.);
            self.denied[slot] = (self.denied[slot] - dt).max(0.);
        }
        self.note_life = (self.note_life - dt).max(0.);
        if self.unanswered > 0 {
            self.waited += dt;
            if self.waited > REPLY_WAIT {
                self.unanswered = 0;
            }
        }
    }

    /// Draw the bar. Returns an attack whose cell was clicked and should go
    /// to the server.
    pub fn draw(
        &mut self,
        form: &Form,
        hint: &str,
        (s, dt, now): (f32, f32, f64),
        calm: bool,
    ) -> Option<Attack> {
        self.tick(&form.cooldowns, dt);
        let r = Self::rect(s);
        panel(r);
        label(
            "AXIOM",
            r.x + 11. * s,
            r.y + 20. * s,
            15. * s,
            readable(crate::accessibility::palette(AXIOM_GOLD)),
        );
        right_label(
            "ATTACKS",
            r.x + r.w - 11. * s,
            r.y + 19. * s,
            10. * s,
            muted_color(),
        );
        let mut clicked = None;
        let mut hovered = None;
        for (slot, &attack) in Attack::bar().iter().enumerate() {
            let cell = Self::cell(r, slot, s);
            let over = contains(cell);
            if over {
                hovered = Some(slot);
                if take_click() {
                    clicked = Some(attack);
                }
            }
            self.draw_cell(cell, (slot, attack), form, over, (s, calm));
        }
        let (line, color) = if self.note_life > 0. {
            (
                self.note.as_str(),
                alpha(RED, (self.note_life * 3.).min(1.)),
            )
        } else {
            (hint, muted_color())
        };
        let size = 11. * s;
        center_label(
            &fitted_label(line, size, r.w - 16. * s),
            r.center().x,
            r.y + 214. * s,
            size,
            color,
        );
        if let Some(slot) = hovered {
            tooltip(r, slot, form, s);
        }
        clicked.filter(|&attack| self.press(attack, form, now))
    }

    fn draw_cell(
        &self,
        cell: Rect,
        (slot, attack): (usize, Attack),
        form: &Form,
        over: bool,
        (s, calm): (f32, bool),
    ) {
        let remaining = form.cooldowns[attack.index()];
        let cooling = remaining > 0.;
        let full = attack == Attack::Reforge && form.hull_full;
        let usable = !cooling && !full;
        let color = group_color(slot);
        let denied = self.denied[slot] / DENIED_FLASH;
        // A refused press shakes its head.
        let nudge = if calm || denied <= 0. {
            0.
        } else {
            (denied * PI * 5.).sin() * denied * 3.5 * s
        };
        let cell = Rect::new(cell.x + nudge, cell.y, cell.w, cell.h);
        let radius = CORNER * s;
        let lift = match (usable, over) {
            (true, true) => 0.36,
            (true, false) => 0.2,
            (false, true) => 0.12,
            (false, false) => 0.07,
        };
        rounded(cell, radius, mix(surface_color(), color, lift));
        let ink = if usable {
            readable(color)
        } else {
            alpha(muted_color(), 0.8)
        };
        glyph(
            attack,
            cell.center() + vec2(0., 2. * s),
            15. * s,
            ink,
            1.6 * s,
        );
        if cooling {
            let share = remaining / attack.cooldown().max(0.01);
            sweep(cell, radius, share, alpha(BLACK, 0.5));
            let center = cell.center();
            center_label(
                &countdown(remaining),
                center.x,
                center.y + 7. * s,
                16. * s,
                Color::new(1., 1., 1., 0.96),
            );
        } else if full {
            center_label(
                "FULL",
                cell.center().x,
                cell.y + cell.h - 5. * s,
                9. * s,
                muted_color(),
            );
        }
        let edge = if usable {
            alpha(color, if over { 0.95 } else { 0.5 })
        } else {
            alpha(muted_color(), 0.3)
        };
        rounded_lines(cell, radius, (1.1 * s).max(1.), edge);
        let ready = self.ready[slot] / READY_PING;
        if ready > 0. {
            // A ring that leaves the cell as it comes back.
            let out = if calm { 0. } else { (1. - ready) * 5. * s };
            let ping = Rect::new(
                cell.x - out,
                cell.y - out,
                cell.w + out * 2.,
                cell.h + out * 2.,
            );
            rounded_lines(ping, radius + out, 1.8 * s, alpha(color, ready));
        }
        if denied > 0. {
            rounded_lines(cell, radius, 2. * s, alpha(RED, denied));
        }
        let cap = Rect::new(cell.x + 3. * s, cell.y + 3. * s, 15. * s, 15. * s);
        rounded(cap, 3. * s, alpha(BLACK, 0.55));
        center_label(
            KEYS[slot].1,
            cap.center().x,
            cap.y + 11.5 * s,
            11. * s,
            Color::new(1., 1., 1., if usable { 0.95 } else { 0.6 }),
        );
    }
}

fn tooltip(r: Rect, slot: usize, form: &Form, s: f32) {
    let attack = Attack::bar()[slot];
    let size = 12. * s;
    let lines = wrapped_lines(attack.description(), r.w - 22. * s, size);
    let h = (34. + lines.len() as f32 * 15.) * s;
    let tip = Rect::new(r.x, (r.y - h - 6. * s).max(4. * s), r.w, h);
    panel(tip);
    label(
        attack.name(),
        tip.x + 11. * s,
        tip.y + 19. * s,
        14. * s,
        readable(group_color(slot)),
    );
    let remaining = form.cooldowns[attack.index()];
    let timing = if remaining > 0. {
        format!("{}s left", countdown(remaining))
    } else {
        format!("{:.0}s recharge", attack.cooldown())
    };
    right_label(
        &timing,
        tip.x + tip.w - 11. * s,
        tip.y + 18. * s,
        10. * s,
        muted_color(),
    );
    for (row, line) in lines.iter().enumerate() {
        label(
            line,
            tip.x + 11. * s,
            tip.y + (36. + row as f32 * 15.) * s,
            size,
            text_color(),
        );
    }
}

/// Each attack's mark, drawn `size` from its center to its edge.
fn glyph(attack: Attack, c: Vec2, size: f32, color: Color, stroke: f32) {
    let at = |x: f32, y: f32| c + vec2(x, y) * size;
    let way = |angle: f32| vec2(angle.cos(), angle.sin());
    let line = |a: Vec2, b: Vec2, width: f32| draw_line(a.x, a.y, b.x, b.y, width, color);
    let dot = |p: Vec2, radius: f32| draw_circle(p.x, p.y, radius, color);
    let ring = |p: Vec2, radius: f32, width: f32| draw_circle_lines(p.x, p.y, radius, width, color);
    let fill = |p: Vec2, sides: usize, radius: f32, turn: f32| {
        let corner = |i: usize| p + way(turn + i as f32 * TAU / sides as f32) * radius;
        for i in 1..sides - 1 {
            draw_triangle(corner(0), corner(i), corner(i + 1), color);
        }
    };
    match attack {
        Attack::Starfall => {
            for (x, y, long) in [(-0.74, 0.08, 0.7), (-0.16, 0.62, 0.98), (0.46, 0.0, 0.62)] {
                let head = at(x, y);
                let tail = head + vec2(0.62, -0.78) * long * size;
                line(tail, head, stroke);
                fill(head, 4, stroke * 2.1, 0.);
            }
        }
        Attack::Sunburst => {
            dot(c, size * 0.34);
            for ray in 0..8 {
                let d = way(ray as f32 * TAU / 8.);
                let far = if ray % 2 == 0 { 0.98 } else { 0.76 };
                line(c + d * size * 0.52, c + d * size * far, stroke);
            }
        }
        Attack::CometLance => {
            let head = at(0.5, -0.5);
            let back = vec2(-FRAC_PI_4.cos(), FRAC_PI_4.sin());
            let side = vec2(back.y, -back.x);
            for (offset, length) in [(-0.27, 0.98), (0., 1.42), (0.27, 0.98)] {
                let from = head + side * offset * size;
                line(from, from + back * length * size, stroke);
            }
            dot(head, size * 0.33);
        }
        Attack::MeteorShower => {
            for (x, y, radius) in [(-0.56, -0.08, 0.25), (0.02, 0.56, 0.3), (0.5, -0.34, 0.22)] {
                let rock = at(x, y);
                let back = vec2(0.64, -0.77);
                line(rock, rock + back * size * (radius + 0.5), stroke);
                fill(rock, 6, size * radius, x * 3.);
            }
        }
        Attack::Constellation => {
            let stars = [
                at(-0.8, 0.36),
                at(-0.28, -0.52),
                at(0.18, 0.12),
                at(0.8, -0.62),
                at(0.62, 0.72),
            ];
            for (a, b) in [(0, 1), (1, 2), (2, 3), (2, 4)] {
                line(stars[a], stars[b], stroke * 0.7);
            }
            for star in stars {
                fill(star, 4, stroke * 2.2, FRAC_PI_4 * 2.);
            }
        }
        Attack::Repulse => {
            dot(c, size * 0.2);
            ring(c, size * 0.5, stroke);
            for corner in 0..4 {
                let d = way(FRAC_PI_4 + corner as f32 * FRAC_PI_2);
                let n = vec2(-d.y, d.x);
                let tip = c + d * size * 1.04;
                let base = c + d * size * 0.72;
                line(tip, base + n * size * 0.24, stroke);
                line(tip, base - n * size * 0.24, stroke);
            }
        }
        Attack::GravityWell => {
            let point = |u: f32| c + way(u * TAU * 2.25) * size * (0.98 - 0.86 * u);
            let mut last = point(0.);
            for step in 1..=30 {
                let next = point(step as f32 / 30.);
                line(last, next, stroke);
                last = next;
            }
            dot(c, size * 0.14);
        }
        Attack::RiftDash => {
            for x in [-0.62, 0.02] {
                line(at(x, -0.62), at(x + 0.56, 0.), stroke * 1.35);
                line(at(x + 0.56, 0.), at(x, 0.62), stroke * 1.35);
            }
        }
        Attack::NovaMines => {
            for corner in 0..4 {
                let d = way(FRAC_PI_4 + corner as f32 * FRAC_PI_2);
                let n = vec2(-d.y, d.x);
                let base = c + d * size * 0.44;
                draw_triangle(
                    c + d * size * 1.04,
                    base + n * size * 0.23,
                    base - n * size * 0.23,
                    color,
                );
            }
            ring(c, size * 0.48, stroke);
            dot(c, size * 0.17);
        }
        Attack::StarLattice => {
            let node = |i: usize| c + way((i as f32 + 0.5) * TAU / 8.) * size * 0.98;
            for i in 0..8 {
                line(node(i), node(i + 1), stroke);
                line(node(i), node(i + 3), stroke * 0.55);
            }
        }
        Attack::SolarTurrets => {
            for barrel in 0..3 {
                let d = way(-FRAC_PI_2 + barrel as f32 * TAU / 3.);
                line(c + d * size * 0.3, c + d * size, stroke * 1.9);
            }
            fill(c, 6, size * 0.46, 0.);
        }
        Attack::Reforge => {
            let corner = |i: usize| c + way(-FRAC_PI_2 + i as f32 * TAU / 6.) * size;
            for i in 0..6 {
                line(corner(i), corner(i + 1), stroke);
            }
            line(at(-0.46, 0.), at(0.46, 0.), stroke * 1.6);
            line(at(0., -0.46), at(0., 0.46), stroke * 1.6);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        Form {
            cooldowns: [0.; ATTACK_COUNT],
            hull_full: false,
        }
    }

    #[test]
    fn the_keys_follow_the_bar_and_none_repeats() {
        assert_eq!(KEYS.len(), Attack::bar().len());
        for (slot, attack) in Attack::bar().iter().enumerate() {
            assert_eq!(slot_of(*attack), slot);
            assert_eq!(
                KEYS.iter().filter(|(key, _)| *key == KEYS[slot].0).count(),
                1
            );
        }
    }

    #[test]
    fn a_press_goes_once_and_a_double_tap_is_one_command() {
        let mut dock = AxiomDock::default();
        let form = form();
        assert!(dock.press(Attack::Sunburst, &form, 10.));
        assert!(!dock.press(Attack::Sunburst, &form, 10.1), "a double tap");
        assert!(dock.press(Attack::Starfall, &form, 10.1), "another attack");
        assert!(dock.press(Attack::Sunburst, &form, 10.5), "a real retry");
        assert_eq!(dock.denied, [0.; ATTACK_COUNT]);
    }

    #[test]
    fn a_press_on_cooldown_is_refused_where_the_player_can_see() {
        let mut dock = AxiomDock::default();
        let mut cooling = form();
        cooling.cooldowns[Attack::Repulse.index()] = 3.;
        assert!(!dock.press(Attack::Repulse, &cooling, 10.));
        assert!(dock.denied[slot_of(Attack::Repulse)] > 0.);
        assert!(dock.note_life <= 0., "the cell says it");
    }

    #[test]
    fn reforge_at_full_health_explains_itself_and_cooling_comes_first() {
        let mut dock = AxiomDock::default();
        let mut full = form();
        full.hull_full = true;
        assert!(!dock.press(Attack::Reforge, &full, 10.));
        assert!(dock.note_life > 0.);
        assert_eq!(verdict(Attack::Reforge, 2., true, 9.), Verdict::Cooling);
        assert_eq!(verdict(Attack::Starfall, 0., true, 9.), Verdict::Send);
    }

    #[test]
    fn a_recharged_attack_pings_once() {
        let mut dock = AxiomDock::default();
        let mut cooldowns = [0.; ATTACK_COUNT];
        dock.tick(&cooldowns, 0.01);
        assert_eq!(dock.ready, [0.; ATTACK_COUNT]);
        cooldowns[Attack::Starfall.index()] = 1.;
        dock.tick(&cooldowns, 0.01);
        cooldowns[Attack::Starfall.index()] = 0.;
        dock.tick(&cooldowns, 0.01);
        assert!(dock.ready[slot_of(Attack::Starfall)] > 0.);
        dock.tick(&cooldowns, READY_PING);
        dock.tick(&cooldowns, 0.01);
        assert_eq!(dock.ready, [0.; ATTACK_COUNT]);
    }

    #[test]
    fn only_replies_to_attacks_stay_quiet() {
        let mut dock = AxiomDock::default();
        assert!(!dock.heard(APPLIED), "not ours");
        dock.sent();
        dock.sent();
        assert!(dock.heard(APPLIED));
        assert!(!dock.heard("Wait before sending another command."));
        assert_eq!(dock.note, "Wait before sending another command.");
        assert!(!dock.heard(APPLIED), "nothing left to answer");
    }

    #[test]
    fn a_reply_that_never_comes_stops_being_waited_for() {
        let mut dock = AxiomDock::default();
        dock.sent();
        dock.tick(&[0.; ATTACK_COUNT], REPLY_WAIT + 0.1);
        assert!(!dock.heard(APPLIED));
    }

    #[test]
    fn the_sweep_stays_on_the_rounded_cell_all_the_way_round() {
        let cell = Rect::new(100., 200., 52., 52.);
        let radius = 7.;
        for step in 0..720 {
            let angle = step as f32 / 720. * TAU;
            let p = rim(cell, radius, angle);
            let from = (p - cell.center()).abs();
            assert!(from.x <= 26.01 && from.y <= 26.01, "{angle} {p}");
            let inner = 26. - radius;
            let corner = (from - Vec2::splat(inner)).max(Vec2::ZERO);
            assert!(corner.length() <= radius + 0.01, "{angle} {p}");
            assert!(
                from.x >= 25.99 || from.y >= 25.99 || (corner.length() - radius).abs() < 0.01,
                "{angle} {p}"
            );
        }
    }

    #[test]
    fn countdowns_round_up_and_never_read_zero_while_cooling() {
        assert_eq!(countdown(0.01), "0.1");
        assert_eq!(countdown(2.41), "2.5");
        assert_eq!(countdown(9.96), "10.0");
        assert_eq!(countdown(10.2), "11");
    }
}

//! Axiom, the Worldshaper. It's a player, so it arrives here as a tank, but it
//! wears a boss's light: a gold and teal hull, cannons that flash and kick one
//! at a time, rounds that burn like small stars, and a flare of its own for
//! each of its twelve attacks.
use super::fire::{self, Plume};
use super::{
    BossFx, FIRE, Flash, Flight, Fuel, KICK_RATE, MOST_KICK, Presence, Round, Spray, direction,
    ease_out, follow_trail, perpendicular, settle, turn_between,
};
use crate::emission::{Emission, Layer, Stop};
use crate::render::{View, polygon};
use crate::ui::{alpha, mix};
use macroquad::prelude::*;
use std::f32::consts::{E, FRAC_PI_4, PI, TAU};
use tankyard_client::boss_model::{AXIOM_GOLD, AXIOM_RECOIL, AXIOM_TEAL, AXIOM_TURN};
use tankyard_client::tank_model::OUTLINE;
use tankyard_content::ProjectileKind;
use tankyard_core::creator_boss::{Attack, CANNONS, GRAVITY_REACH, MUZZLE, REPULSE_REACH};
use tankyard_core::{BOSS_PHASES, EntityId, V2};

/// Most attack flares showing at once.
pub(super) const MOST_CASTS: usize = 24;
/// From this size a piercing round is the Comet Lance.
const COMET_RADIUS: f32 = 20.;
/// How quickly a fired cannon runs back out.
const GUN_RETURN: f32 = 11.;
/// How long after leaving the snapshots Axiom can still be credited a death.
/// Long enough for a late reliable event, too short to mistake the ordinary
/// tank underneath for the boss once the form has ended.
const KILL_WINDOW: f32 = 0.5;
/// The white of Axiom's heart.
const PALE: Color = Color::new(1., 0.96, 0.78, 1.);

/// Axiom as this frame shows it.
#[derive(Clone, Copy, Debug)]
pub struct ShownAxiom {
    pub id: EntityId,
    pub pos: V2,
    pub aim: f32,
    pub radius: f32,
    pub health: f32,
    pub max_health: f32,
}

/// One attack's flare.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cast {
    attack: Attack,
    owner: EntityId,
    /// Where the hull was when it went off.
    from: V2,
    aim: f32,
    radius: f32,
    age: f32,
    life: f32,
}

/// Which cannon, counted from the one on `aim`, faces `bearing`.
fn cannon_slot(bearing: f32, aim: f32) -> usize {
    let step = TAU / CANNONS as f32;
    (turn_between(aim, bearing).rem_euclid(TAU) / step).round() as usize % CANNONS
}

/// How hurt Axiom is, on the same scale as a boss's phases.
fn phase_of(share: f32) -> u8 {
    if share < BOSS_PHASES[1] {
        3
    } else if share < BOSS_PHASES[0] {
        2
    } else {
        1
    }
}

fn is_comet(kind: ProjectileKind, radius: f32) -> bool {
    kind == ProjectileKind::Piercing && radius >= COMET_RADIUS
}

/// How long each attack's flare lasts.
fn flare_life(attack: Attack) -> f32 {
    match attack {
        Attack::Starfall => 0.34,
        Attack::CometLance => 0.46,
        Attack::Repulse | Attack::MeteorShower => 0.5,
        Attack::Sunburst => 0.55,
        Attack::NovaMines | Attack::SolarTurrets => 0.6,
        Attack::RiftDash => 0.62,
        Attack::StarLattice => 0.7,
        Attack::Constellation => 0.85,
        Attack::GravityWell => 0.95,
        Attack::Reforge => 1.,
    }
}

impl Presence {
    pub(super) fn axiom(item: &ShownAxiom, color: Color) -> Self {
        let share = (item.health / item.max_health.max(1.)).clamp(0., 1.);
        Self {
            kind: None,
            pos: item.pos,
            radius: item.radius,
            color,
            fuel: Fuel::Star,
            aim: item.aim,
            aim_rate: 0.,
            aim_target: item.aim,
            aim_drive: 0.,
            charge: 0.,
            heat: 0.,
            phase: phase_of(share),
            health: item.health,
            trail: share,
            trail_hold: 0.,
            kick: Vec2::ZERO,
            kick_rate: Vec2::ZERO,
            winding: false,
            unseen: 0.,
            embers: 0.,
            smoke: 0.,
            flame: 0.,
            guns: [0.; CANNONS],
            drive: Vec2::ZERO,
            rail: super::railgun::RailShown::default(),
            exhaust: super::exhaust_for(item.pos, color, Fuel::Star),
        }
    }
}

impl BossFx {
    /// Whether `id` is Axiom, or was a moment ago.
    pub fn is_axiom(&self, id: EntityId) -> bool {
        self.stars
            && self
                .bosses
                .get(&id)
                .is_some_and(|shown| shown.fuel == Fuel::Star && shown.unseen < KILL_WINDOW)
    }
    /// Whether a shot is one of Axiom's. Rounds already flying keep their
    /// look after the form ends; anything fired since then is a normal shot.
    pub fn is_star_round(&self, owner: EntityId, id: EntityId) -> bool {
        self.stars
            && (self
                .bosses
                .get(&owner)
                .is_some_and(|shown| shown.fuel == Fuel::Star && shown.unseen <= 0.)
                || self
                    .trails
                    .get(&id)
                    .is_some_and(|trail| trail.fuel == Fuel::Star))
    }
    /// How far firing has shoved Axiom's hull on screen.
    pub fn axiom_kick(&self, id: EntityId, zoom: f32) -> Vec2 {
        self.bosses
            .get(&id)
            .map_or(Vec2::ZERO, |shown| shown.kick * zoom)
    }
    /// How far back each of Axiom's cannons is, from the one on its aim.
    pub fn axiom_recoil(&self, id: EntityId) -> [f32; CANNONS] {
        self.bosses
            .get(&id)
            .map_or([0.; CANNONS], |shown| shown.guns)
    }
    /// Where the pale stretch on Axiom's health bar ends, as a share.
    pub fn axiom_trail(&self, id: EntityId, share: f32) -> f32 {
        self.bosses.get(&id).map_or(share, |shown| shown.trail)
    }

    /// Follow this frame's Axioms. Runs after [`BossFx::sync`], which ages
    /// everything: one that has left the form stops counting as seen, and its
    /// entry stays briefly so a kill can still find where it fell.
    pub fn sync_axiom(&mut self, shown: impl Iterator<Item = ShownAxiom>, dt: f32) {
        let dt = dt.clamp(0., 0.1);
        let look = self.look;
        let color = crate::accessibility::palette(AXIOM_GOLD);
        for item in shown {
            self.stars = true;
            let known = self.bosses.contains_key(&item.id);
            let presence = self
                .bosses
                .entry(item.id)
                .or_insert_with(|| Presence::axiom(&item, color));
            let most = item.max_health.max(1.);
            let share = (item.health / most).clamp(0., 1.);
            let before = (presence.health / most).clamp(0., 1.);
            let was_phase = presence.phase;
            let moved = item.pos - presence.pos;
            if dt > 0. && moved.length_squared() < 260. * 260. {
                let speed = vec2(moved.x, moved.y) / dt;
                presence.drive += (speed - presence.drive) * (1. - (-dt * 10.).exp());
            } else if dt > 0. {
                presence.drive = Vec2::ZERO;
            }
            presence.unseen = 0.;
            presence.pos = item.pos;
            presence.radius = item.radius;
            presence.color = color;
            presence.fuel = Fuel::Star;
            if item.aim.is_finite() {
                presence.aim = item.aim;
                presence.aim_target = item.aim;
            }
            if share < before {
                presence.heat = (presence.heat + 0.12 + (before - share) * 26.).min(1.);
            }
            (presence.trail, presence.trail_hold) = follow_trail(
                (presence.trail, presence.trail_hold),
                before,
                share,
                dt,
                look.calm,
            );
            presence.health = item.health;
            presence.heat *= (-dt * 6.5).exp();
            presence.phase = phase_of(share);
            let (x, vx) = settle(presence.kick.x, presence.kick_rate.x, KICK_RATE, dt);
            let (y, vy) = settle(presence.kick.y, presence.kick_rate.y, KICK_RATE, dt);
            presence.kick = vec2(x, y);
            presence.kick_rate = vec2(vx, vy);
            let back = (-dt * GUN_RETURN).exp();
            for gun in &mut presence.guns {
                *gun *= back;
                if *gun < 0.004 {
                    *gun = 0.;
                }
            }
            let phase = presence.phase;
            let rate = match phase {
                3 => 24.,
                2 => 8.,
                _ => 0.,
            };
            presence.embers += rate * dt * look.level();
            let due = presence.embers as usize;
            presence.embers -= due as f32;
            if !look.live() {
                continue;
            }
            if !known {
                self.burst(item.pos, Flash::Arrival, item.radius, color, 1.1);
                self.spray(Spray {
                    at: item.pos,
                    heading: None,
                    count: look.many(46.),
                    speed: (160., 620.),
                    life: (0.4, 1.1),
                    size: (1.4, 3.6),
                    color: mix(color, WHITE, 0.3),
                });
            }
            if known && phase > was_phase {
                self.phase_change(item.pos, item.radius, color);
            }
            for spark in 0..due {
                let angle = self.rand() * TAU;
                let from = item.pos + V2::from_angle(angle) * item.radius * self.between((0.5, 1.));
                self.spray(Spray {
                    at: from,
                    heading: Some((angle, 0.6)),
                    count: 1,
                    speed: (34., 130.),
                    life: (0.4, 1.),
                    size: (1.2, 2.8),
                    color: if spark % 3 == 0 { AXIOM_TEAL } else { color },
                });
            }
        }
        if self.stars {
            self.stars = self.bosses.values().any(|shown| shown.fuel == Fuel::Star)
                || self.trails.values().any(|trail| trail.fuel == Fuel::Star)
                || !self.casts.is_empty();
        }
    }

    /// One of Axiom's rounds has just left a cannon. Returns the camera shake.
    pub(super) fn star_launched(&mut self, flight: &Flight) -> f32 {
        let look = self.look;
        let Some(presence) = self.bosses.get_mut(&flight.owner) else {
            return 0.;
        };
        let heading = vec2(flight.vel.x, flight.vel.y).normalize_or_zero();
        let comet = is_comet(flight.kind, flight.radius);
        let (shove, shake) = if comet {
            (2.8, 1.3)
        } else {
            ((flight.radius / 30.).clamp(0.06, 0.34), 0.)
        };
        let (color, hull) = (presence.color, presence.radius);
        if !look.calm {
            presence.kick_rate -= heading * shove * 60.;
            let most = hull * MOST_KICK * KICK_RATE * E;
            if presence.kick_rate.length() > most {
                presence.kick_rate = presence.kick_rate.normalize_or_zero() * most;
            }
            let bearing = (flight.at - presence.pos).angle();
            presence.guns[cannon_slot(bearing, presence.aim)] = 1.;
        }
        if !look.live() || flight.kind.is_drone() {
            return shake;
        }
        self.fueled_burst(
            flight.at,
            Flash::Muzzle(heading),
            Fuel::Star,
            flight.radius * if comet { 1.8 } else { 1.1 },
            color,
            if comet { 0.28 } else { 0.15 },
        );
        if comet {
            self.spray(Spray {
                at: flight.at,
                heading: Some((heading.y.atan2(heading.x), 0.34)),
                count: look.many(30.),
                speed: (360., 1100.),
                life: (0.2, 0.55),
                size: (1.5, 3.6),
                color: mix(color, WHITE, 0.3),
            });
        }
        shake
    }

    /// One of Axiom's rounds is gone.
    pub(super) fn star_landed(&mut self, flight: &Flight, struck: bool, color: Color) {
        let look = self.look;
        let (at, radius, kind) = (flight.at, flight.radius, flight.kind);
        if is_comet(kind, radius) {
            self.fueled_burst(at, Flash::Blast, Fuel::Star, radius * 1.5, color, 0.66);
            for (tint, count) in [(mix(color, WHITE, 0.25), 34.), (AXIOM_TEAL, 16.)] {
                self.spray(Spray {
                    at,
                    heading: None,
                    count: look.many(count),
                    speed: (130., 690.),
                    life: (0.3, 0.9),
                    size: (1.4, 3.4),
                    color: tint,
                });
            }
            self.scar(at, radius * 2.2);
            return;
        }
        if kind == ProjectileKind::Explosive {
            self.fueled_burst(at, Flash::Blast, Fuel::Star, radius * 1.7, color, 0.6);
            self.spray(Spray {
                at,
                heading: None,
                count: look.many(22.),
                speed: (120., 430.),
                life: (0.28, 0.75),
                size: (1.4, 3.),
                color: mix(color, FIRE, 0.35),
            });
            self.plume(Plume {
                at,
                scatter: radius * 0.5,
                heading: None,
                speed: (30., 300.),
                size: (radius * 0.4, radius),
                life: (0.22, 0.55),
                count: look.many(20.),
                burn: fire::STREAM,
            });
            self.scar(at, radius * 2.);
            return;
        }
        if kind == ProjectileKind::Mine {
            self.fueled_burst(at, Flash::Hit, Fuel::Star, radius * 1.3, color, 0.34);
            return;
        }
        if kind.is_drone() || !struck {
            self.burst(at, Flash::Fizzle, radius, color, 0.2);
            return;
        }
        self.fueled_burst(at, Flash::Hit, Fuel::Star, radius, color, 0.3);
        self.spray(Spray {
            at,
            heading: None,
            count: look.many(5.),
            speed: (70., 260.),
            life: (0.16, 0.4),
            size: (1., 2.2),
            color: if flight.id.is_multiple_of(3) {
                AXIOM_TEAL
            } else {
                mix(color, WHITE, 0.25)
            },
        });
    }

    /// Whether a round of this kind leaves fire behind it as it flies.
    pub(super) fn star_burns(kind: ProjectileKind) -> bool {
        kind == ProjectileKind::Explosive
    }
    /// Whether a round is big enough to light the arena around it.
    pub(super) fn star_heavy(kind: ProjectileKind, radius: f32) -> bool {
        is_comet(kind, radius)
    }

    /// Axiom has cast `attack`, from `at`. Returns the camera shake, or `None`
    /// if this wasn't Axiom at all.
    pub fn axiom_cast(&mut self, owner: EntityId, attack: u32, at: V2) -> Option<f32> {
        if !self.is_axiom(owner) {
            return None;
        }
        let presence = *self.bosses.get(&owner)?;
        let attack = *Attack::ALL.get(usize::try_from(attack).ok()?)?;
        let shake = match attack {
            Attack::CometLance => 1.2,
            Attack::RiftDash => 0.9,
            Attack::Repulse | Attack::Sunburst => 0.7,
            _ => 0.25,
        };
        let look = self.look;
        if !look.live() {
            return Some(shake);
        }
        if self.casts.len() >= MOST_CASTS {
            self.casts.remove(0);
        }
        self.casts.push(Cast {
            attack,
            owner,
            from: at,
            aim: presence.aim,
            radius: presence.radius,
            age: 0.,
            life: flare_life(attack),
        });
        let (hull, aim, color) = (presence.radius, presence.aim, presence.color);
        let mouth = at + V2::from_angle(aim) * hull * MUZZLE;
        match attack {
            Attack::MeteorShower => self.plume(Plume {
                at: mouth,
                scatter: hull * 0.14,
                heading: Some((aim, 0.3)),
                speed: (120., 640.),
                size: (hull * 0.07, hull * 0.17),
                life: (0.16, 0.42),
                count: look.many(26.),
                burn: fire::STREAM,
            }),
            Attack::Reforge => {
                for _ in 0..look.many(44.) {
                    let angle = self.rand() * TAU;
                    let from = at + V2::from_angle(angle) * hull * self.between((0.9, 1.4));
                    self.spray(Spray {
                        at: from,
                        heading: Some((angle + PI, 0.25)),
                        count: 1,
                        speed: (120., 340.),
                        life: (0.4, 0.85),
                        size: (1.3, 3.),
                        color: mix(color, Color::new(0.62, 1., 0.66, 1.), 0.45),
                    });
                }
            }
            Attack::Repulse => self.spray(Spray {
                at,
                heading: None,
                count: look.many(54.),
                speed: (420., 980.),
                life: (0.2, 0.5),
                size: (1.3, 3.),
                color: mix(AXIOM_TEAL, WHITE, 0.2),
            }),
            Attack::RiftDash => {
                for _ in 0..look.many(30.) {
                    let along = self.rand();
                    let side = self.rand() * TAU;
                    let from =
                        at + V2::from_angle(aim) * along * 500. + V2::from_angle(side) * hull * 0.7;
                    self.spray(Spray {
                        at: from,
                        heading: Some((aim + PI, 0.5)),
                        count: 1,
                        speed: (60., 260.),
                        life: (0.3, 0.7),
                        size: (1.2, 2.8),
                        color: AXIOM_TEAL,
                    });
                }
            }
            _ => {}
        }
        Some(shake)
    }

    pub(super) fn run_casts(&mut self, dt: f32) {
        for cast in &mut self.casts {
            cast.age += dt;
        }
        self.casts.retain(|cast| cast.age < cast.life);
    }

    pub(super) fn draw_casts(&self, light: &mut Emission, view: &View, level: f32) {
        for cast in &self.casts {
            // A flare stays with the hull; the dash also keeps where it began.
            let center = self
                .bosses
                .get(&cast.owner)
                .map_or(cast.from, |shown| shown.pos);
            draw_cast(light, view, cast, center, level.min(1.));
        }
    }

    /// The lights Axiom and its flares throw onto a dark arena.
    pub(super) fn axiom_lights(&self, view: &View, out: &mut Vec<crate::lighting::Light>) {
        use crate::lighting::{Light, gather};
        let z = view.zoom;
        for shown in self.bosses.values() {
            if shown.fuel != Fuel::Star || shown.unseen > 0. {
                continue;
            }
            let lit = 0.34 + 0.5 * shown.heat;
            gather(
                out,
                Light::Spot {
                    at: view.screen(shown.pos),
                    radius: (shown.radius * 3. + 180.) * z,
                    color: alpha(mix(shown.color, WHITE, 0.2), lit),
                },
            );
        }
        for cast in &self.casts {
            let fade = 1. - (cast.age / cast.life).clamp(0., 1.);
            let center = self
                .bosses
                .get(&cast.owner)
                .map_or(cast.from, |shown| shown.pos);
            gather(
                out,
                Light::Spot {
                    at: view.screen(center),
                    radius: (cast.radius * 3.4 + 200.) * z,
                    color: alpha(
                        mix(AXIOM_GOLD, WHITE, 0.3),
                        0.6 * fade * fade * self.look.level().min(1.),
                    ),
                },
            );
        }
    }

    /// Axiom's own light: a white-gold heart, its rings picked out, a seam
    /// along each cannon that flares as that cannon fires, and gold leaking
    /// from a damaged hull. Everything lines up with `boss_model::axiom_model`.
    pub fn axiom_glow(
        &self,
        light: &mut Emission,
        id: EntityId,
        p: Vec2,
        r: f32,
        aim: f32,
        time: f32,
    ) {
        let Some(shown) = self.bosses.get(&id) else {
            return;
        };
        let level = self.look.level();
        let calm = self.look.calm;
        let gold = shown.color;
        let hurt = shown.heat;
        let turn = time * AXIOM_TURN;
        let pulse = if calm {
            0.5
        } else if shown.phase >= 3 {
            0.5 + 0.5 * (time * 4.8).sin()
        } else {
            0.5 + 0.5 * (time * 1.4).sin()
        };
        // Lines of light stay on at low settings: they are the model's detail.
        let lines = level.max(0.4);
        light.spot(
            Layer::Wash,
            p,
            r * 2.3,
            alpha(gold, (0.07 + 0.025 * pulse) * level + 0.09 * hurt * level),
        );
        light.spot(
            Layer::Wash,
            p,
            r * (0.5 + 0.05 * pulse),
            alpha(light.hot(gold, 0.2), (0.2 + 0.06 * pulse) * lines),
        );
        light.spot(
            Layer::Core,
            p,
            r * (0.15 + 0.02 * pulse),
            alpha(light.hot(gold, 0.8), (0.58 + 0.14 * pulse) * lines),
        );
        light.glint(
            p,
            r * (0.7 + 0.08 * pulse),
            turn * 3. + FRAC_PI_4,
            alpha(light.hot(gold, 0.9), (0.26 + 0.1 * pulse) * lines),
        );
        // The rings, each on the angle its armor turns at.
        for (sides, size, angle, tint, strength) in [
            (6, 0.78, -turn, gold, 0.5 + 0.1 * pulse),
            (6, 0.47, turn, AXIOM_TEAL, 0.58 + 0.12 * pulse),
            (12, 0.94, turn, gold, 0.26),
        ] {
            let strength = (strength + 0.45 * hurt) * lines;
            light.outline(
                Layer::Wash,
                p,
                sides,
                r * size,
                angle,
                (r * 0.045).max(1.2),
                alpha(tint, strength * 0.3),
            );
            light.outline(
                Layer::Core,
                p,
                sides,
                r * size,
                angle,
                (r * 0.008).max(0.8),
                alpha(light.hot(tint, 0.35), strength.min(0.95)),
            );
        }
        if !calm {
            for side in [0., PI] {
                light.arc(
                    Layer::Core,
                    p,
                    r * 0.57,
                    (r * 0.012).max(0.9),
                    turn * 6. + side,
                    1.15,
                    alpha(light.hot(AXIOM_TEAL, 0.5), 0.62 * lines),
                );
            }
        }
        for (index, &kick) in shown.guns.iter().enumerate() {
            let d = direction(aim + index as f32 * TAU / CANNONS as f32);
            let thrown = kick * r * AXIOM_RECOIL;
            let mouth = p + d * (r * MUZZLE - thrown);
            light.streak(
                Layer::Core,
                p + d * (r * 1.06 - thrown),
                mouth - d * r * 0.1,
                (r * (0.011 + 0.012 * kick)).max(0.7),
                alpha(gold, 0.),
                alpha(light.hot(gold, 0.4), (0.24 + 0.72 * kick) * lines),
            );
            light.spot(
                Layer::Wash,
                mouth,
                r * (0.11 + 0.15 * kick),
                alpha(gold, (0.12 + 0.56 * kick) * level),
            );
            light.spot(
                Layer::Core,
                mouth,
                r * (0.036 + 0.05 * kick),
                alpha(light.hot(gold, 0.9), (0.5 + 0.48 * kick) * lines),
            );
        }
        if level <= 0. {
            return;
        }
        if shown.phase >= 2 {
            // Light leaks through the plates of a damaged hull, flaring and
            // dying on its own, in the gold and teal the armor is holding in.
            let (strength, patches) = if shown.phase >= 3 {
                (0.5 + 0.2 * pulse, 8)
            } else {
                (0.34, 4)
            };
            let seed = (id % 97) as f32;
            for patch in 0..patches {
                let n = patch as f32 + seed;
                let angle = n * 2.399_963 + turn;
                let reach = r * (0.5 + 0.42 * (n * 0.618_034).fract());
                let flare = if calm {
                    0.6
                } else {
                    0.4 + 0.6 * (0.5 + 0.5 * (time * (1.8 + n % 3.) + n * 1.9).sin())
                };
                let at = p + direction(angle) * reach;
                let tint = if patch % 3 == 2 { AXIOM_TEAL } else { gold };
                light.spot(
                    Layer::Wash,
                    at,
                    r * (0.2 + 0.08 * flare),
                    alpha(tint, strength * 0.7 * flare * level),
                );
                light.spot(
                    Layer::Core,
                    at,
                    r * 0.05,
                    alpha(light.hot(tint, 0.6), (strength + 0.3) * flare * level),
                );
            }
        }
        if hurt > 0.02 {
            light.spot(
                Layer::Wash,
                p,
                r * 1.5,
                alpha(light.hot(gold, 0.45), 0.3 * hurt * level),
            );
        }
        // A faint wake behind it as it drives.
        let drive = shown.drive;
        if !calm && drive.length_squared() > 196. {
            let speed = drive.length();
            let way = drive / speed;
            let strength = ((speed - 14.) / 200.).min(1.);
            light.streak(
                Layer::Wash,
                p - way * r * (1.3 + 0.7 * strength),
                p - way * r * 0.6,
                r * (0.16 + 0.1 * strength),
                alpha(gold, 0.),
                alpha(light.hot(AXIOM_TEAL, 0.2), (0.1 + 0.2 * strength) * level),
            );
        }
    }

    /// One of Axiom's rounds. Each has a solid body, so it can be seen and
    /// dodged whatever the settings.
    pub(super) fn star_round(&self, light: &mut Emission, view: &View, round: &Round) {
        let Round {
            p,
            r,
            heading,
            kind,
            color,
            opacity,
            age,
            z,
            ..
        } = *round;
        let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
        let n = perpendicular(d);
        let level = self.look.level().max(0.55);
        let o = opacity * level.min(1.);
        let rim = (1.6 * z).max(1.);
        let clock = self.flicker(round);
        let spin = heading.y.atan2(heading.x) + clock * 1.6;
        if is_comet(kind, r / z.max(0.001)) {
            comet(light, round, o);
            return;
        }
        let path = self.path_of(view, round);
        self.trail(light, view, round, &path, 1.);
        match kind {
            ProjectileKind::Piercing => {
                // A falling shard of light with a white spine.
                let tip = p + d * r * 2.6;
                let tail = p - d * r * 1.9;
                let sliver = |grow: f32, tint: Color| {
                    draw_triangle(tip + d * grow, p + n * (r * 0.62 + grow), tail, tint);
                    draw_triangle(tip + d * grow, p - n * (r * 0.62 + grow), tail, tint);
                };
                sliver(rim * 1.4, alpha(mix(color, OUTLINE, 0.78), opacity));
                sliver(0., alpha(color, opacity));
                light.oval(
                    Layer::Wash,
                    p,
                    d * r * 3.4,
                    r * 1.4,
                    alpha(light.hot(color, 0.15), 0.36 * o),
                );
                light.beam(
                    Layer::Core,
                    tail,
                    tip + d * r * 0.3,
                    (r * 0.3).max(1.1),
                    alpha(light.hot(color, 0.92), 0.95 * o),
                );
            }
            ProjectileKind::Homing => {
                // A small star that turns as it hunts.
                star(
                    p,
                    r * 1.75 + rim,
                    r * 0.62 + rim,
                    spin,
                    alpha(OUTLINE, opacity),
                );
                star(p, r * 1.75, r * 0.62, spin, alpha(AXIOM_TEAL, opacity));
                light.spot(Layer::Wash, p, r * 2.7, alpha(AXIOM_TEAL, 0.36 * o));
                light.spot(
                    Layer::Core,
                    p,
                    r * 0.6,
                    alpha(light.hot(AXIOM_TEAL, 0.9), 0.95 * o),
                );
                light.glint(p, r * 2.3, spin, alpha(light.hot(AXIOM_TEAL, 0.8), 0.6 * o));
            }
            ProjectileKind::Explosive => {
                // A meteor: dark rock with gold in its cracks.
                let tumble = spin * 0.7;
                polygon(p, 7, r + rim, tumble, alpha(OUTLINE, opacity));
                polygon(
                    p,
                    7,
                    r,
                    tumble,
                    alpha(Color::new(0.33, 0.27, 0.24, 1.), opacity),
                );
                let beat = 0.5 + 0.5 * (age * (9. + age * 4.)).sin();
                light.spot(
                    Layer::Wash,
                    p,
                    r * 2.9,
                    alpha(mix(color, FIRE, 0.45), (0.3 + 0.2 * beat) * o),
                );
                for crack in 0..3 {
                    let way = direction(tumble + crack as f32 * TAU / 3.);
                    light.beam(
                        Layer::Core,
                        p,
                        p + way * r * 0.82,
                        (r * 0.12).max(0.9),
                        alpha(light.hot(color, 0.6), (0.65 + 0.3 * beat) * o),
                    );
                }
                light.spot(
                    Layer::Core,
                    p,
                    r * 0.36,
                    alpha(light.hot(color, 1.), 0.95 * o),
                );
            }
            ProjectileKind::Bullet => {
                // Repulse's wave: light, quick and teal.
                polygon(p, 0, r + rim, 0., alpha(OUTLINE, opacity));
                polygon(p, 0, r, 0., alpha(AXIOM_TEAL, opacity));
                light.oval(
                    Layer::Wash,
                    p - d * r * 0.3,
                    d * r * 2.6,
                    r * 1.8,
                    alpha(AXIOM_TEAL, 0.4 * o),
                );
                light.spot(
                    Layer::Core,
                    p,
                    r * 0.8,
                    alpha(light.hot(AXIOM_TEAL, 0.9), 0.85 * o),
                );
            }
            _ => {
                // The cannons' shell and Sunburst: a gold ball of starlight.
                polygon(p, 0, r + rim, 0., alpha(mix(color, OUTLINE, 0.78), opacity));
                polygon(p, 0, r, 0., alpha(color, opacity));
                polygon(p, 0, r * 0.68, 0., alpha(PALE, opacity));
                light.oval(
                    Layer::Wash,
                    p - d * r * 0.4,
                    d * r * 3.1,
                    r * 2.25,
                    alpha(light.hot(color, 0.08), 0.42 * o),
                );
                light.spot(
                    Layer::Core,
                    p,
                    r * 0.98,
                    alpha(light.hot(color, 0.95), 0.86 * o),
                );
                light.glint(p, r * 2.5, spin, alpha(light.hot(color, 0.85), 0.5 * o));
            }
        }
    }
}

/// A filled four-pointed star, `long` to its points and `short` to its waist.
fn star(p: Vec2, long: f32, short: f32, turn: f32, color: Color) {
    let waist = short * 0.42;
    for point in 0..4 {
        let way = direction(turn + point as f32 * PI / 2.);
        let side = perpendicular(way);
        draw_triangle(p + way * long, p + side * waist, p - side * waist, color);
    }
}

/// The Comet Lance in flight: a white head, a gold body and a long teal tail.
fn comet(light: &mut Emission, round: &Round, o: f32) {
    let Round {
        p,
        r,
        heading,
        color,
        opacity,
        age,
        z,
        ..
    } = *round;
    let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
    let n = perpendicular(d);
    let rim = (1.8 * z).max(1.);
    polygon(p, 0, r + rim, 0., alpha(mix(color, OUTLINE, 0.8), opacity));
    polygon(p, 0, r, 0., alpha(color, opacity));
    polygon(p, 0, r * 0.7, 0., alpha(PALE, opacity));
    // Never reaching further back than it has flown, so it can't poke out
    // through the back of the cannon at launch.
    let reach = (age * heading.length() * z).max(r * 0.8).min(r * 10.);
    let point = |back: f32, half: f32, tint: Color, strength: f32| Stop {
        at: p - d * back,
        half,
        color: alpha(tint, strength * o),
    };
    light.spot(Layer::Wash, p, r * 4., alpha(color, 0.34 * o));
    light.band(
        Layer::Wash,
        &[
            point(-r * 0.95, r * 0.1, color, 0.),
            point(-r * 0.2, r * 1.12, color, 0.62),
            point(reach * 0.34, r * 0.92, mix(color, AXIOM_TEAL, 0.35), 0.5),
            point(reach * 0.7, r * 0.5, AXIOM_TEAL, 0.32),
            point(reach, r * 0.08, AXIOM_TEAL, 0.),
        ],
    );
    light.band(
        Layer::Core,
        &[
            point(-r * 0.78, r * 0.08, PALE, 0.),
            point(-r * 0.1, r * 0.66, WHITE, 0.96),
            point(reach * 0.3, r * 0.34, PALE, 0.66),
            point(reach * 0.78, r * 0.05, color, 0.),
        ],
    );
    for side in [-1., 1.] {
        light.streak(
            Layer::Core,
            p - d * reach * 0.76 + n * side * r * 1.25,
            p + d * r * 0.1 + n * side * r * 0.92,
            (r * 0.06).max(0.9),
            alpha(AXIOM_TEAL, 0.),
            alpha(light.hot(AXIOM_TEAL, 0.5), 0.8 * o),
        );
    }
    light.spot(
        Layer::Core,
        p + d * r * 0.3,
        r * 0.66,
        alpha(light.hot(color, 1.), o),
    );
    light.glint(
        p + d * r * 0.3,
        r * 3.2,
        heading.y.atan2(heading.x) + FRAC_PI_4,
        alpha(light.hot(color, 0.9), 0.55 * o),
    );
}

/// How each attack announces itself.
fn draw_cast(light: &mut Emission, view: &View, cast: &Cast, center: V2, level: f32) {
    let z = view.zoom;
    let p = view.screen(center);
    let r = cast.radius * z;
    let t = (cast.age / cast.life).clamp(0., 1.);
    let fade = 1. - t;
    let grow = ease_out(t);
    let gold = AXIOM_GOLD;
    let teal = AXIOM_TEAL;
    let ahead = direction(cast.aim);
    let mouth = p + ahead * r * MUZZLE;
    // Where this attack's shots appeared, for the ones that deploy around it.
    let deployed = cast.attack.spawn_reach(cast.radius) * z;
    let thin = (r * 0.009).max(0.9);
    match cast.attack {
        Attack::Starfall => {
            for lance in 0..9 {
                let way = direction(cast.aim + (lance as f32 - 4.) * 0.065);
                let from = p + way * r * 1.1;
                light.streak(
                    Layer::Core,
                    from,
                    from + way * r * (0.5 + 1.7 * grow),
                    thin * 1.5,
                    alpha(gold, 0.),
                    alpha(light.hot(gold, 0.8), 0.85 * fade * fade * level),
                );
            }
            light.oval(
                Layer::Wash,
                mouth + ahead * r * 0.3,
                ahead * r * 0.9,
                r * 0.36,
                alpha(gold, 0.32 * fade * fade * level),
            );
        }
        Attack::Sunburst => {
            let ring = r * (1. + 1.3 * grow);
            light.ring(
                Layer::Core,
                p,
                ring,
                (r * 0.018 * fade).max(1.),
                alpha(light.hot(gold, 0.7), 0.85 * fade * fade * level),
            );
            light.ring(
                Layer::Wash,
                p,
                ring,
                r * 0.13 * fade + 1.,
                alpha(gold, 0.3 * fade * level),
            );
            for ray in 0..24 {
                let way = direction(cast.aim + ray as f32 * TAU / 24.);
                light.streak(
                    Layer::Core,
                    p + way * ring * 0.8,
                    p + way * ring,
                    thin,
                    alpha(gold, 0.),
                    alpha(light.hot(gold, 0.6), 0.8 * fade * level),
                );
            }
        }
        Attack::CometLance => {
            let length = r * (1.4 + 5. * grow);
            light.streak(
                Layer::Wash,
                mouth + ahead * length,
                mouth,
                r * 0.2 * fade + 1.,
                alpha(teal, 0.),
                alpha(gold, 0.55 * fade * level),
            );
            light.beam(
                Layer::Core,
                mouth,
                mouth + ahead * length,
                (r * 0.05 * fade).max(1.),
                alpha(light.hot(gold, 1.), 0.95 * fade * fade * level),
            );
            light.spot(
                Layer::Wash,
                mouth,
                r * (0.5 + 0.7 * grow),
                alpha(gold, 0.6 * fade * fade * level),
            );
            light.glint(
                mouth,
                r * (1.5 - 0.4 * t),
                cast.aim,
                alpha(light.hot(gold, 1.), 0.9 * fade * level),
            );
        }
        Attack::MeteorShower => {
            light.oval(
                Layer::Wash,
                mouth + ahead * r * 0.5,
                ahead * r * (0.7 + 0.5 * grow),
                r * 0.42,
                alpha(mix(gold, FIRE, 0.5), 0.52 * fade * fade * level),
            );
            light.spot(
                Layer::Core,
                mouth,
                r * 0.22 * (1. + grow),
                alpha(light.hot(gold, 0.9), 0.82 * fade.powi(3) * level),
            );
        }
        Attack::Constellation => {
            let ring = r * 1.2;
            let shown = (t / 0.3).min(1.);
            light.outline(
                Layer::Core,
                p,
                12,
                ring,
                cast.aim,
                thin,
                alpha(light.hot(teal, 0.4), 0.72 * fade * shown * level),
            );
            for node in 0..12 {
                // The stars light in turn around the ring.
                let due = node as f32 / 12. * 0.28;
                let on = ((t - due) / 0.12).clamp(0., 1.);
                let at = p + direction(cast.aim + node as f32 * TAU / 12.) * ring;
                light.spot(
                    Layer::Wash,
                    at,
                    r * 0.11,
                    alpha(teal, 0.42 * on * fade * level),
                );
                light.spot(
                    Layer::Core,
                    at,
                    r * 0.036,
                    alpha(light.hot(teal, 0.9), on * fade * level),
                );
            }
        }
        Attack::Repulse => {
            // Out to exactly where it clears shots.
            let ring = (cast.radius + (REPULSE_REACH - cast.radius).max(0.) * grow) * z;
            light.ring(
                Layer::Core,
                p,
                ring,
                (r * 0.024 * fade).max(1.),
                alpha(light.hot(teal, 0.7), 0.9 * fade * level),
            );
            light.ring(
                Layer::Wash,
                p,
                ring,
                r * 0.2 * fade + 1.,
                alpha(teal, 0.36 * fade * level),
            );
            light.spot(
                Layer::Wash,
                p,
                ring,
                alpha(teal, 0.12 * fade * fade * level),
            );
        }
        Attack::GravityWell => {
            for wave in 0..3 {
                let u = ((t - wave as f32 * 0.16) / 0.68).clamp(0., 1.);
                if u <= 0. {
                    continue;
                }
                let inward = u * u;
                let reach = GRAVITY_REACH + (cast.radius - GRAVITY_REACH) * inward;
                let strength = (u * PI).sin() * level;
                let radius = reach * z;
                light.ring(
                    Layer::Core,
                    p,
                    radius,
                    thin,
                    alpha(light.hot(teal, 0.4), 0.55 * strength),
                );
                for side in [0., PI] {
                    light.arc(
                        Layer::Core,
                        p,
                        radius,
                        thin * 2.,
                        cast.aim + inward * 2.6 + side + wave as f32,
                        1.1,
                        alpha(light.hot(gold, 0.4), 0.7 * strength),
                    );
                }
            }
            light.spot(
                Layer::Wash,
                p,
                r * 1.35,
                alpha(teal, 0.24 * (t * PI).sin() * level),
            );
        }
        Attack::RiftDash => {
            let start = view.screen(cast.from);
            light.streak(
                Layer::Wash,
                start,
                p,
                r * 0.56 * fade + 1.,
                alpha(teal, 0.),
                alpha(teal, 0.5 * fade * level),
            );
            light.streak(
                Layer::Core,
                start,
                p,
                (r * 0.06 * fade).max(1.),
                alpha(gold, 0.),
                alpha(light.hot(gold, 0.8), 0.92 * fade * level),
            );
            for (at, from, to) in [(start, 1., 0.45), (p, 0.8, 1.25)] {
                light.ring(
                    Layer::Core,
                    at,
                    r * (from + (to - from) * grow),
                    (r * 0.016 * fade).max(1.),
                    alpha(light.hot(teal, 0.5), 0.8 * fade * fade * level),
                );
            }
        }
        Attack::NovaMines => {
            for mine in 0..12 {
                let way = direction(cast.aim + mine as f32 * TAU / 12.);
                let at = p + way * deployed;
                light.streak(
                    Layer::Core,
                    p + way * r * 0.95,
                    at,
                    thin,
                    alpha(gold, 0.),
                    alpha(light.hot(gold, 0.6), 0.65 * fade * fade * level),
                );
                light.ring(
                    Layer::Core,
                    at,
                    r * (0.05 + 0.1 * grow),
                    thin,
                    alpha(light.hot(gold, 0.5), 0.85 * fade * level),
                );
            }
        }
        Attack::StarLattice => {
            let strength = (t / 0.14).min(1.) * fade * level;
            light.outline(
                Layer::Core,
                p,
                8,
                deployed,
                cast.aim,
                thin * 1.3,
                alpha(light.hot(teal, 0.45), 0.85 * strength),
            );
            light.outline(
                Layer::Wash,
                p,
                8,
                deployed,
                cast.aim,
                r * 0.05 + 1.,
                alpha(teal, 0.3 * strength),
            );
            for node in 0..8 {
                let at = p + direction(cast.aim + node as f32 * TAU / 8.) * deployed;
                light.spot(
                    Layer::Core,
                    at,
                    r * 0.05,
                    alpha(light.hot(gold, 0.8), strength),
                );
            }
        }
        Attack::SolarTurrets => {
            for sentry in 0..6 {
                let way = direction(cast.aim + sentry as f32 * TAU / 6.);
                let at = p + way * deployed;
                light.beam(
                    Layer::Core,
                    p + way * r * MUZZLE,
                    at,
                    thin * 1.4,
                    alpha(light.hot(gold, 0.7), 0.8 * fade * fade * level),
                );
                light.spot(
                    Layer::Wash,
                    at,
                    r * (0.12 + 0.12 * grow),
                    alpha(gold, 0.5 * fade * fade * level),
                );
            }
        }
        Attack::Reforge => {
            let mend = Color::new(0.62, 1., 0.66, 1.);
            for (lead, size) in [(0., 1.7), (0.18, 1.45)] {
                let u = ((t - lead) / (1. - lead)).clamp(0., 1.);
                let radius = r * (size - (size - 0.9) * ease_out(u));
                let strength = (u * PI).sin() * level;
                light.outline(
                    Layer::Core,
                    p,
                    6,
                    radius,
                    cast.aim,
                    thin * 1.5,
                    alpha(light.hot(mix(gold, mend, 0.5), 0.4), 0.8 * strength),
                );
            }
            let swell = (t * PI).sin();
            light.spot(
                Layer::Wash,
                p,
                r * 1.5,
                alpha(mix(gold, mend, 0.35), 0.3 * swell * level),
            );
            light.spot(
                Layer::Core,
                p,
                r * 0.36 * swell,
                alpha(light.hot(gold, 0.9), 0.75 * swell * level),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axiom(id: EntityId) -> ShownAxiom {
        ShownAxiom {
            id,
            pos: V2::new(500., 500.),
            aim: 0.,
            radius: 180.,
            health: 120_000.,
            max_health: 120_000.,
        }
    }
    fn sync(fx: &mut BossFx, item: ShownAxiom, dt: f32) {
        fx.sync(std::iter::empty(), dt);
        fx.sync_axiom(std::iter::once(item), dt);
    }
    fn shot(id: EntityId, owner: EntityId, at: V2, radius: f32, kind: ProjectileKind) -> Flight {
        Flight {
            id,
            owner,
            at,
            vel: V2::new(860., 0.),
            radius,
            kind,
            damage: 180.,
        }
    }

    #[test]
    fn a_shot_finds_the_cannon_it_left_from() {
        let step = TAU / CANNONS as f32;
        for aim in [0., 0.4, -2.9, 3.1] {
            for slot in 0..CANNONS {
                let bearing = aim + slot as f32 * step + 0.08;
                assert_eq!(cannon_slot(bearing, aim), slot, "{aim} {slot}");
                let wrapped = bearing - TAU;
                assert_eq!(cannon_slot(wrapped, aim), slot, "{aim} {slot} wrapped");
            }
        }
        assert!(cannon_slot(f32::NAN, 0.) < CANNONS);
    }

    #[test]
    fn axiom_gets_hurt_on_the_same_scale_as_a_boss() {
        assert_eq!(phase_of(1.), 1);
        assert_eq!(phase_of(BOSS_PHASES[0]), 1);
        assert_eq!(phase_of(BOSS_PHASES[0] - 0.01), 2);
        assert_eq!(phase_of(BOSS_PHASES[1] - 0.01), 3);
        assert_eq!(phase_of(0.), 3);
    }

    #[test]
    fn firing_kicks_only_the_cannon_that_fired_and_it_runs_back_out() {
        let mut fx = BossFx::default();
        let item = axiom(7);
        sync(&mut fx, item, 1. / 60.);
        let mouth = item.pos + V2::from_angle(TAU / 6.) * 232.;
        fx.launched(&shot(9, 7, mouth, 11., ProjectileKind::Shell));
        let guns = fx.axiom_recoil(7);
        assert_eq!(guns[1], 1.);
        assert!(guns.iter().enumerate().all(|(i, gun)| i == 1 || *gun == 0.));
        for _ in 0..90 {
            sync(&mut fx, item, 1. / 60.);
        }
        assert_eq!(fx.axiom_recoil(7), [0.; CANNONS]);
        assert!(fx.axiom_kick(7, 1.).length() < 0.05);
    }

    #[test]
    fn the_comet_lance_is_axioms_and_never_draws_as_bastions_slug() {
        let mut fx = BossFx::default();
        let item = axiom(7);
        sync(&mut fx, item, 1. / 60.);
        let lance = shot(9, 7, V2::new(800., 500.), 25., ProjectileKind::Piercing);
        assert!(lance.damage >= tankyard_core::BASTION_ROUND_DAMAGE);
        let shake = fx.launched(&lance);
        assert!(shake > 1., "{shake}");
        fx.track(&lance);
        assert_eq!(fx.trails[&9].fuel, Fuel::Star);
        assert!(fx.trails[&9].heavy, "a comet lights the arena");
        // An ordinary cannon shot scaled up to the same damage stays light.
        let shell = shot(10, 7, V2::new(800., 520.), 11., ProjectileKind::Shell);
        assert_eq!(fx.launched(&shell), 0.);
        fx.track(&shell);
        assert!(!fx.trails[&10].heavy);
        let scars = fx.scars.len();
        fx.landed(&lance, true);
        assert_eq!(fx.scars.len(), scars + 1);
    }

    #[test]
    fn rounds_keep_their_look_after_the_form_ends_and_new_ones_do_not() {
        let mut fx = BossFx::default();
        let item = axiom(7);
        sync(&mut fx, item, 1. / 60.);
        assert!(fx.is_boss(7) && fx.is_axiom(7));
        let flying = shot(9, 7, V2::new(800., 500.), 11., ProjectileKind::Shell);
        fx.track(&flying);
        assert!(fx.is_star_round(7, 9));
        // The form ends: nothing shows it this frame.
        fx.sync(std::iter::empty(), 1. / 60.);
        fx.sync_axiom(std::iter::empty(), 1. / 60.);
        assert!(!fx.is_boss(7), "its next shots are a normal tank's");
        assert!(fx.is_axiom(7), "a kill can still find it");
        assert!(fx.is_star_round(7, 9), "already flying");
        assert!(!fx.is_star_round(7, 10), "fired since");
        fx.track(&flying);
        // Long gone, and the round it left behind still lands as its own.
        for _ in 0..120 {
            fx.sync(std::iter::empty(), 1. / 30.);
            fx.sync_axiom(std::iter::empty(), 1. / 30.);
            fx.track(&flying);
            fx.update(1. / 60.);
        }
        assert!(!fx.is_axiom(7));
        assert!(fx.is_star_round(7, 9));
        let bursts = fx.bursts.len();
        fx.landed(&flying, true);
        assert!(fx.bursts.len() > bursts);
        assert!(!fx.is_star_round(7, 9));
    }

    #[test]
    fn every_attack_has_a_flare_and_they_stay_bounded() {
        let mut fx = BossFx::default();
        let item = axiom(7);
        sync(&mut fx, item, 1. / 60.);
        assert_eq!(fx.axiom_cast(99, 0, item.pos), None, "not Axiom");
        assert_eq!(
            fx.axiom_cast(7, Attack::ALL.len() as u32, item.pos),
            None,
            "not an attack"
        );
        for attack in Attack::ALL {
            assert!(flare_life(attack) > 0.);
            assert!(fx.axiom_cast(7, attack.index() as u32, item.pos).is_some());
        }
        assert_eq!(fx.casts.len(), Attack::ALL.len());
        for _ in 0..200 {
            fx.axiom_cast(7, 5, item.pos);
        }
        assert_eq!(fx.casts.len(), MOST_CASTS);
        for _ in 0..40 {
            fx.update(0.05);
        }
        assert!(fx.casts.is_empty());
    }

    #[test]
    fn reduced_motion_leaves_axiom_still_with_no_flares() {
        let mut fx = BossFx::default();
        fx.set_look(super::super::Look {
            strength: 1.,
            calm: true,
        });
        let item = axiom(7);
        sync(&mut fx, item, 1. / 60.);
        let mouth = item.pos + V2::new(232., 0.);
        fx.launched(&shot(9, 7, mouth, 11., ProjectileKind::Shell));
        assert_eq!(fx.axiom_recoil(7), [0.; CANNONS]);
        assert!(fx.axiom_cast(7, 1, item.pos).is_some());
        assert!(fx.casts.is_empty() && fx.bursts.is_empty() && fx.sparks.is_empty());
    }

    #[test]
    fn taking_damage_trails_and_crossing_a_phase_flashes_once() {
        let mut fx = BossFx::default();
        let mut item = axiom(7);
        sync(&mut fx, item, 1. / 60.);
        fx.bursts.clear();
        item.health = item.max_health * (BOSS_PHASES[0] - 0.02);
        sync(&mut fx, item, 1. / 60.);
        sync(&mut fx, item, 1. / 60.);
        let phases = fx
            .bursts
            .iter()
            .filter(|burst| burst.flash == Flash::Phase)
            .count();
        assert_eq!(phases, 1);
        assert!(
            fx.axiom_trail(7, 0.) > BOSS_PHASES[0] - 0.02,
            "pale stretch"
        );
    }
}

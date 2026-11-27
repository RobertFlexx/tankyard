//! A parry meeting a boss's heaviest fire: the flash at the point of contact,
//! the strike running back up the line, and the blast when it gets there, all
//! in the fire that boss really burns. Ions from the Railgunner, flame from
//! the Siege Engine, hard steel-blue from Bastion.
//!
//! The host decides the clash and what it deals; this only shows it. With
//! reduced motion or Effects at zero only the words over the tank remain.
use super::fire::{self, Plume, ramp};
use super::railgun::{along, ion, lay, stream};
use super::shots::bolt;
use super::{
    BossFx, FIRE, Flash, Fuel, KICK_RATE, MOST_KICK, Spray, direction, ease_out, perpendicular,
};
use crate::emission::{Emission, Layer, Stop};
use crate::lighting::Light;
use crate::map_sounds::{BossCue, falloff};
use crate::render::View;
use crate::ui::{AMBER, alpha, mix};
use macroquad::prelude::*;
use std::f32::consts::{E, PI, TAU};
use tankyard_client::boss_model::RAIL_FRINGE;
use tankyard_core::{
    BEAM_ROUND_RADIUS, BossKind, ClashStyle, EntityId, RAIL_MUZZLE, V2, clash_names,
};

/// Clashes on screen at once, more than a fight will ever have.
const MOST_CLASHES: usize = 6;
/// Rounds of a stream remembered from the frame they vanished.
const MOST_GONE: usize = 160;
/// Points along the line a strike runs back on.
const SPINE: usize = 40;
/// How long the flash at the point of contact lasts.
const CONTACT_SECONDS: f32 = 0.66;
/// How long a clash goes on after the strike has landed.
const AFTER_SECONDS: f32 = 0.62;
/// How far the contact's flash reaches at full size.
const REACH: f32 = 118.;
/// How long sparks and flame hang almost still at the moment of the hit, and
/// how slowly they run while they do.
const HOLD_SECONDS: f32 = 0.075;
const HOLD_RATE: f32 = 0.12;
/// Seconds between the blasts that follow a strike down the hull.
const SHOCK_GAP: f32 = 0.055;
/// How long the blast where a strike lands is on screen.
const LAND_SECONDS: f32 = 0.56;
/// Heavy rounds' blasts held back at once.
const MOST_BLASTS: usize = 24;
/// How far from the parry a held blast can be and still be the round it took.
/// A drawn round trails the host's by up to a few ticks of very fast flight.
const CLAIM_RANGE: f32 = 420.;

/// A clash under way.
#[derive(Clone, Copy, Debug)]
pub(super) struct Clash {
    style: ClashStyle,
    boss: EntityId,
    /// Where the parry met the round.
    at: V2,
    /// The way back to the boss from there.
    back: V2,
    /// The line the strike runs along, from `at` to where it lands.
    spine: [V2; SPINE],
    count: u8,
    age: f32,
    flight: f32,
    landed: bool,
    /// How many of the blasts after the landing have gone off.
    shocks: u8,
    /// Whether the local player landed it.
    mine: bool,
    damage: u32,
    seed: f32,
}

/// A round of a plasma stream that has just vanished. It's held for a frame:
/// if a clash took it, the strike draws it going instead.
#[derive(Clone, Copy, Debug)]
pub(super) struct GoneRound {
    owner: EntityId,
    id: EntityId,
    at: V2,
    radius: f32,
    due: bool,
}

/// The blast a heavy round sets off where it lands.
#[derive(Clone, Copy, Debug)]
pub(super) enum Blast {
    /// The Railgunner's cannon, at this strength.
    Rail(f32),
    Siege,
    Bastion,
    Spectrum,
}

/// A heavy round's blast, held for a frame or two. A round that was parried
/// doesn't go off: the clash takes its place.
#[derive(Clone, Copy, Debug)]
pub(super) struct HeldBlast {
    owner: EntityId,
    at: V2,
    radius: f32,
    color: Color,
    blast: Blast,
    due: bool,
}

/// The blades of light a hit throws out: how many, how far in reaches of the
/// clash, how wide, how far from square to the line of fire they stray, and
/// how long they last.
#[derive(Clone, Copy)]
struct Cut {
    blades: u8,
    reach: f32,
    width: f32,
    stray: f32,
    life: f32,
}

fn cut_of(style: ClashStyle) -> Cut {
    match style {
        ClashStyle::Beam => Cut {
            blades: 12,
            reach: 4.9,
            width: 0.075,
            stray: 1.05,
            life: 0.27,
        },
        ClashStyle::Rail => Cut {
            blades: 9,
            reach: 5.6,
            width: 0.055,
            stray: 0.7,
            life: 0.22,
        },
        ClashStyle::Siege => Cut {
            blades: 9,
            reach: 3.3,
            width: 0.14,
            stray: 1.3,
            life: 0.31,
        },
        ClashStyle::Bastion => Cut {
            blades: 6,
            reach: 3.,
            width: 0.08,
            stray: 0.,
            life: 0.2,
        },
    }
}

/// How big a clash is, by the gun it answered.
fn size_of(style: ClashStyle) -> f32 {
    match style {
        ClashStyle::Beam => 1.,
        ClashStyle::Rail => 1.1,
        ClashStyle::Siege => 0.8,
        ClashStyle::Bastion => 0.56,
    }
}

/// The color of a clash `t` of the way from its heart to its edge.
fn heat(style: ClashStyle, t: f32) -> Color {
    match style {
        ClashStyle::Beam | ClashStyle::Rail => ion(t),
        ClashStyle::Siege => ramp(t * 0.62),
        ClashStyle::Bastion => fire::steel(t),
    }
}

fn fuel_of(style: ClashStyle) -> Fuel {
    match style {
        ClashStyle::Beam | ClashStyle::Rail => Fuel::Ion,
        ClashStyle::Siege => Fuel::Fire,
        ClashStyle::Bastion => Fuel::Plasma,
    }
}

/// How many blasts chase a strike down the hull.
fn shocks_of(style: ClashStyle) -> u8 {
    match style {
        ClashStyle::Beam => 6,
        ClashStyle::Rail => 4,
        ClashStyle::Siege => 3,
        ClashStyle::Bastion => 2,
    }
}

/// What the player reads over their tank when they land one.
pub fn headline(style: ClashStyle) -> &'static str {
    match style {
        ClashStyle::Beam => "BEAM BROKEN",
        ClashStyle::Rail => "NOT TODAY",
        ClashStyle::Siege => "RETURN TO SENDER",
        ClashStyle::Bastion => "BOUNCED",
    }
}

/// The color of the words and the number for a clash.
pub fn clash_color(style: ClashStyle) -> Color {
    heat(style, 0.3)
}

/// `line` laid out again in evenly spaced points, so a strike runs along it
/// at a steady speed however unevenly the rounds were spread.
fn even(line: &[V2]) -> ([V2; SPINE], u8) {
    let mut out = [line.first().copied().unwrap_or(V2::ZERO); SPINE];
    let total: f32 = line.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
    if line.len() < 2 || !total.is_finite() || total < 1. {
        out[1] = line.last().copied().unwrap_or(out[0]);
        return (out, 2);
    }
    let count = ((total / 55.) as usize + 2).clamp(2, SPINE);
    let mut from = 0;
    let mut walked = 0.;
    for (index, point) in out[..count].iter_mut().enumerate() {
        let wanted = total * index as f32 / (count - 1) as f32;
        while from + 2 < line.len() && walked + line[from].distance(line[from + 1]) < wanted {
            walked += line[from].distance(line[from + 1]);
            from += 1;
        }
        let span = line[from].distance(line[from + 1]).max(1e-3);
        *point = line[from].lerp(line[from + 1], ((wanted - walked) / span).clamp(0., 1.));
    }
    (out, count as u8)
}

impl BossFx {
    /// Where the camera is, so far-off clashes don't shake it.
    pub fn set_camera(&mut self, at: V2) {
        self.camera = at;
    }

    /// How much time boss effects get this frame: a hard hit holds them
    /// nearly still for an instant.
    pub(super) fn held(&mut self, dt: f32) -> f32 {
        if self.hold <= 0. {
            return dt;
        }
        self.hold -= dt;
        dt * HOLD_RATE
    }

    /// Note a vanished round of a plasma stream.
    pub(super) fn stream_round_gone(&mut self, owner: EntityId, id: EntityId, at: V2, radius: f32) {
        if self.gone.len() < MOST_GONE {
            self.gone.push(GoneRound {
                owner,
                id,
                at,
                radius,
                due: false,
            });
        }
    }

    /// Hold a heavy round's blast until any clash that took it has arrived.
    pub(super) fn hold_blast(
        &mut self,
        owner: EntityId,
        at: V2,
        radius: f32,
        color: Color,
        blast: Blast,
    ) {
        let held = HeldBlast {
            owner,
            at,
            radius,
            color,
            blast,
            due: false,
        };
        if self.blasts.len() < MOST_BLASTS {
            self.blasts.push(held);
        } else {
            self.set_off(&held);
        }
    }

    fn set_off(&mut self, held: &HeldBlast) {
        match held.blast {
            Blast::Rail(strength) => self.rail_blast(held.at, held.radius, strength),
            Blast::Siege => self.detonate(held.at, held.radius),
            Blast::Bastion => self.slug_impact(held.at, held.radius, held.color),
            Blast::Spectrum => {
                self.fueled_burst(
                    held.at,
                    Flash::Hit,
                    Fuel::Spectrum,
                    held.radius * 1.6,
                    held.color,
                    0.32,
                );
                self.burst(held.at, Flash::Phase, held.radius * 2.4, held.color, 0.52);
                self.spray(Spray {
                    at: held.at,
                    heading: None,
                    count: self.look.many(22.),
                    speed: (120., 580.),
                    life: (0.18, 0.52),
                    size: (1.4, 3.),
                    color: held.color,
                });
            }
        }
    }

    /// Rounds nothing claimed were just hits, so they go off like hits.
    fn settle_gone(&mut self) {
        let mut index = 0;
        while index < self.gone.len() {
            if self.gone[index].due {
                let round = self.gone.swap_remove(index);
                let silk = self
                    .bosses
                    .get(&round.owner)
                    .filter(|p| p.kind == Some(BossKind::Jester))
                    .map(|p| p.round_color(round.id, tankyard_content::ProjectileKind::Piercing));
                self.stream_hit(round.at, round.radius, silk);
            } else {
                self.gone[index].due = true;
                index += 1;
            }
        }
        let mut index = 0;
        while index < self.blasts.len() {
            if self.blasts[index].due {
                let held = self.blasts.swap_remove(index);
                self.set_off(&held);
            } else {
                self.blasts[index].due = true;
                index += 1;
            }
        }
    }

    /// Where a strike at `boss` lands: the muzzle it came out of, or the
    /// side of the hull facing the parry.
    fn landing(&self, style: ClashStyle, boss: EntityId, from: V2) -> Option<V2> {
        let shown = self.bosses.get(&boss)?;
        Some(match style {
            ClashStyle::Beam | ClashStyle::Rail => {
                let muzzle = if shown.kind == Some(BossKind::Jester) {
                    tankyard_core::encounters::CARD_MUZZLE
                } else {
                    RAIL_MUZZLE
                };
                shown.pos + V2::from_angle(shown.aim) * shown.radius * muzzle
            }
            ClashStyle::Siege | ClashStyle::Bastion => {
                shown.pos + (from - shown.pos).normalized() * shown.radius * 0.72
            }
        })
    }

    /// How far a blast at `at` should reach the camera, 0 to 1.
    fn nearness(&self, at: V2, mine: bool) -> f32 {
        if mine {
            1.
        } else {
            falloff(at.distance(self.camera))
        }
    }

    /// The boss a clash event's id bits name. They're only the low bits, so
    /// if two bosses share them it's the nearer one with the gun that fired.
    pub fn clash_boss(&self, style: ClashStyle, bits: u32, at: V2) -> EntityId {
        let gun = match style {
            ClashStyle::Beam | ClashStyle::Rail => BossKind::Railgunner,
            ClashStyle::Siege => BossKind::SiegeEngine,
            ClashStyle::Bastion => BossKind::Bastion,
        };
        self.bosses
            .iter()
            .filter(|(id, _)| clash_names(bits, **id))
            .min_by(|a, b| {
                let carries = |kind| {
                    kind == Some(gun)
                        || kind == Some(BossKind::Jester)
                        || (kind == Some(BossKind::Carillon) && style == ClashStyle::Bastion)
                };
                carries(b.1.kind)
                    .cmp(&carries(a.1.kind))
                    .then(a.1.pos.distance(at).total_cmp(&b.1.pos.distance(at)))
            })
            .map_or(0, |(id, _)| *id)
    }

    /// A confirmed parry met the named boss's big gun.
    pub fn clash(&mut self, style: ClashStyle, at: V2, boss: EntityId, damage: u32, mine: bool) {
        let cue = match style {
            ClashStyle::Beam => BossCue::ClashBeam,
            ClashStyle::Rail => BossCue::ClashIon,
            ClashStyle::Siege => BossCue::ClashFire,
            ClashStyle::Bastion => BossCue::ClashSteel,
        };
        self.push_cue(cue, at);
        // The round that was parried doesn't also go off.
        self.blasts
            .retain(|held| held.owner != boss || held.at.distance(at) > CLAIM_RANGE);
        let landing = self.landing(style, boss, at);
        let target = landing.unwrap_or(at);
        let mut line = vec![at];
        if style == ClashStyle::Beam && self.look.live() {
            // The rounds of the stream, newest nearest the gun, up to the
            // parry: that's the line, however far it had bent.
            let mut rounds: Vec<_> = self
                .gone
                .iter()
                .filter(|round| round.owner == boss)
                .map(|round| (round.id, round.at))
                .collect();
            self.gone.retain(|round| round.owner != boss);
            rounds.sort_unstable_by_key(|round| round.0);
            let run = target.distance(at);
            line.extend(
                rounds
                    .into_iter()
                    .map(|(_, point)| point)
                    .filter(|point| point.distance(target) < run - 16.),
            );
        }
        line.push(target);
        let (spine, count) = even(&line);
        let distance: f32 = line.windows(2).map(|p| p[0].distance(p[1])).sum();
        let flight = if landing.is_some() {
            style.flight(distance.max(at.distance(target)))
        } else {
            0.
        };
        if self.clashes.len() >= MOST_CLASHES {
            self.clashes.remove(0);
        }
        let seed = self.rand() * 97.;
        let clash = Clash {
            style,
            boss,
            at,
            back: (spine[1] - at).normalized(),
            spine,
            count,
            age: 0.,
            flight,
            landed: landing.is_none(),
            shocks: 0,
            mine,
            damage,
            seed,
        };
        self.clashes.push(clash);
        if !self.look.live() {
            return;
        }
        self.clash_contact(&clash);
        let near = self.nearness(at, mine);
        self.pending_shake += 2.6 * size_of(style) * near;
        if near > 0.3 {
            self.hold = self.hold.max(HOLD_SECONDS * size_of(style).sqrt());
        }
    }

    /// Everything thrown off at the point of contact.
    fn clash_contact(&mut self, clash: &Clash) {
        let look = self.look;
        let (at, style) = (clash.at, clash.style);
        let size = size_of(style);
        let angle = clash.back.y.atan2(clash.back.x);
        match style {
            ClashStyle::Beam | ClashStyle::Rail => {
                for out in [-1., 1.] {
                    self.spray(Spray {
                        at,
                        heading: Some((angle + out * 1.32, 0.64)),
                        count: look.many(104. * size),
                        speed: (640., 2700.),
                        life: (0.16, 0.62),
                        size: (1.5, 4.2),
                        color: ion(0.18),
                    });
                }
                self.spray(Spray {
                    at,
                    heading: Some((angle, 0.56)),
                    count: look.many(58. * size),
                    speed: (480., 2100.),
                    life: (0.14, 0.46),
                    size: (1.3, 3.2),
                    color: mix(RAIL_FRINGE, WHITE, 0.25),
                });
                self.spray(Spray {
                    at,
                    heading: None,
                    count: look.many(70. * size),
                    speed: (50., 340.),
                    life: (0.6, 1.5),
                    size: (1.1, 2.7),
                    color: ion(0.62),
                });
                self.scar(at, 46. * size);
            }
            ClashStyle::Siege => {
                self.plume(Plume {
                    at,
                    scatter: 18.,
                    heading: None,
                    speed: (60., 720.),
                    size: (28., 72.),
                    life: (0.28, 0.85),
                    count: look.many(92.),
                    burn: fire::DENSE,
                });
                for out in [-1., 1.] {
                    self.plume(Plume {
                        at,
                        scatter: 8.,
                        heading: Some((angle + out * 1.25, 0.34)),
                        speed: (420., 1450.),
                        size: (15., 42.),
                        life: (0.16, 0.5),
                        count: look.many(46.),
                        burn: fire::STREAM,
                    });
                    self.spray(Spray {
                        at,
                        heading: Some((angle + out * 1.25, 0.5)),
                        count: look.many(44.),
                        speed: (420., 1700.),
                        life: (0.24, 0.8),
                        size: (1.6, 4.2),
                        color: mix(FIRE, AMBER, 0.45),
                    });
                }
                self.spray(Spray {
                    at,
                    heading: None,
                    count: look.many(60.),
                    speed: (50., 300.),
                    life: (0.7, 1.6),
                    size: (1.2, 2.8),
                    color: mix(FIRE, AMBER, 0.2),
                });
                for _ in 0..look.many(9.) {
                    let turn = self.rand() * TAU;
                    let from = at + V2::from_angle(turn) * 36. * self.rand();
                    let puff = self.between((44., 84.));
                    let life = self.between((0.9, 1.7));
                    self.smoke(from, turn, (30., 110.), puff, life);
                }
                self.scar(at, 66.);
            }
            ClashStyle::Bastion => {
                for out in [-1., 1.] {
                    self.spray(Spray {
                        at,
                        heading: Some((angle + out * 1.2, 0.42)),
                        count: look.many(44.),
                        speed: (380., 1500.),
                        life: (0.14, 0.44),
                        size: (1.3, 3.2),
                        color: fire::steel(0.2),
                    });
                }
                self.spray(Spray {
                    at,
                    heading: None,
                    count: look.many(26.),
                    speed: (40., 240.),
                    life: (0.4, 0.95),
                    size: (1., 2.2),
                    color: fire::steel(0.5),
                });
                for _ in 0..look.many(3.) {
                    let turn = self.rand() * TAU;
                    let puff = self.between((26., 44.));
                    let life = self.between((0.5, 0.9));
                    self.smoke(at, turn, (24., 90.), puff, life);
                }
                self.scar(at, 34.);
            }
        }
    }

    /// Run clashes on by `dt` of real time: they follow the host's strike,
    /// whatever the sparks are doing.
    pub(super) fn run_clashes(&mut self, dt: f32) {
        self.settle_gone();
        let mut index = 0;
        while index < self.clashes.len() {
            let mut clash = self.clashes[index];
            let before = clash.age;
            clash.age += dt;
            if !clash.landed
                && let Some(point) = self.landing(clash.style, clash.boss, clash.at)
            {
                clash.spine[usize::from(clash.count) - 1] = point;
            }
            if !clash.landed {
                self.feed_strike(&clash, before);
            }
            if !clash.landed && clash.age >= clash.flight {
                clash.landed = true;
                self.clash_lands(&clash);
            }
            while self.look.live()
                && clash.landed
                && clash.flight > 0.
                && clash.shocks < shocks_of(clash.style)
                && clash.age >= clash.flight + f32::from(clash.shocks + 1) * SHOCK_GAP
            {
                self.clash_shock(&clash);
                clash.shocks += 1;
            }
            if clash.age >= (clash.flight + AFTER_SECONDS).max(CONTACT_SECONDS) {
                self.clashes.remove(index);
            } else {
                self.clashes[index] = clash;
                index += 1;
            }
        }
    }

    /// A burning shell leaves fire all the way back.
    fn feed_strike(&mut self, clash: &Clash, before: f32) {
        if !self.look.live() || clash.style != ClashStyle::Siege || clash.flight <= 0. {
            return;
        }
        let count = usize::from(clash.count);
        let (from, to) = (
            (before / clash.flight).clamp(0., 1.),
            (clash.age / clash.flight).clamp(0., 1.),
        );
        let (a, b) = (
            along(&clash.spine[..count], from),
            along(&clash.spine[..count], to),
        );
        let moved = a.distance(b);
        let puffs = ((moved / 9.) as usize).min(26);
        let back = (a - b).angle();
        for step in 0..puffs {
            let at = a.lerp(b, (step as f32 + 0.5) / puffs as f32);
            self.plume(Plume {
                at,
                scatter: 14.,
                heading: Some((back, 0.5)),
                speed: (30., 260.),
                size: (17., 36.),
                life: (0.18, 0.52),
                count: (2. * self.look.level()).ceil() as usize,
                burn: fire::STREAM,
            });
        }
    }

    /// The strike has reached the boss.
    fn clash_lands(&mut self, clash: &Clash) {
        let look = self.look;
        let style = clash.style;
        let size = size_of(style);
        let at = clash.spine[usize::from(clash.count) - 1];
        self.push_cue(BossCue::Backlash, at);
        let way = (at - clash.spine[usize::from(clash.count) - 2]).normalized();
        if let Some(presence) = self.bosses.get_mut(&clash.boss) {
            presence.heat = 1.;
            if look.live() {
                presence.kick_rate += vec2(way.x, way.y) * 11. * size * 60.;
                let most = presence.radius * MOST_KICK * KICK_RATE * E;
                if presence.kick_rate.length() > most {
                    presence.kick_rate = presence.kick_rate.normalize_or_zero() * most;
                }
                if matches!(style, ClashStyle::Beam | ClashStyle::Rail) {
                    presence.rail.recoil = 1.;
                }
            }
        }
        if clash.mine && clash.damage > 0 {
            self.strikes.push((at, clash.damage, clash_color(style)));
        }
        if !look.live() {
            return;
        }
        let near = self.nearness(at, clash.mine);
        self.pending_shake += 2.1 * size * near;
        let angle = way.y.atan2(way.x);
        match style {
            ClashStyle::Beam | ClashStyle::Rail => {
                self.spray(Spray {
                    at,
                    heading: Some((angle + PI, 0.82)),
                    count: look.many(96. * size),
                    speed: (420., 2200.),
                    life: (0.2, 0.7),
                    size: (1.5, 4.),
                    color: ion(0.2),
                });
                self.spray(Spray {
                    at,
                    heading: None,
                    count: look.many(64. * size),
                    speed: (60., 460.),
                    life: (0.5, 1.4),
                    size: (1.2, 3.),
                    color: mix(ion(0.5), RAIL_FRINGE, 0.4),
                });
            }
            ClashStyle::Siege => {
                self.fireball(at, 46.);
            }
            ClashStyle::Bastion => {
                self.slug_debris(at, 40.);
            }
        }
    }

    /// One of the blasts that follow a strike down the hull.
    fn clash_shock(&mut self, clash: &Clash) {
        let look = self.look;
        let Some((pos, aim, radius)) = self
            .bosses
            .get(&clash.boss)
            .map(|shown| (shown.pos, shown.aim, shown.radius))
        else {
            return;
        };
        let step = f32::from(clash.shocks);
        let style = clash.style;
        let spot = match style {
            ClashStyle::Beam | ClashStyle::Rail => {
                // Back down the barrel, muzzle to breech.
                let run = RAIL_MUZZLE - 0.34 * (step + 1.);
                let side = (clash.seed + step * 2.3).sin() * 0.16;
                pos + V2::from_angle(aim) * radius * run
                    + V2::from_angle(aim + PI * 0.5) * radius * side
            }
            ClashStyle::Siege | ClashStyle::Bastion => {
                let turn = clash.seed * 3. + step * 2.1;
                pos + V2::from_angle(turn) * radius * (0.32 + 0.16 * step)
            }
        };
        let blast = radius * (0.22 + 0.05 * (clash.seed + step).sin());
        let tint = heat(style, 0.34);
        self.fueled_burst(spot, Flash::Bloom, fuel_of(style), blast, tint, 0.4);
        self.spray(Spray {
            at: spot,
            heading: None,
            count: look.many(24.),
            speed: (140., 840.),
            life: (0.18, 0.56),
            size: (1.2, 3.2),
            color: heat(style, 0.26),
        });
        if style == ClashStyle::Siege {
            self.plume(Plume {
                at: spot,
                scatter: blast * 0.5,
                heading: None,
                speed: (30., 280.),
                size: (blast * 0.5, blast * 1.2),
                life: (0.24, 0.6),
                count: look.many(22.),
                burn: fire::STREAM,
            });
        }
        let near = self.nearness(spot, clash.mine);
        self.pending_shake += 0.35 * near;
    }

    /// The numbers for strikes the local player landed, ready to show.
    pub fn take_strikes(&mut self) -> Vec<(V2, u32, Color)> {
        std::mem::take(&mut self.strikes)
    }

    pub(super) fn draw_clashes(&self, light: &mut Emission, view: &View, level: f32) {
        if !self.look.live() {
            return;
        }
        let level = level.min(1.);
        for clash in &self.clashes {
            if !clash.landed {
                self.draw_strike(light, view, clash, level);
            }
            if clash.age < CONTACT_SECONDS
                && view.visible(clash.at, REACH * size_of(clash.style) * 8.)
            {
                draw_contact(light, view, clash, level);
            }
            if clash.landed && clash.flight > 0. {
                let since = clash.age - clash.flight;
                let at = clash.spine[usize::from(clash.count).clamp(2, SPINE) - 1];
                if since < LAND_SECONDS && view.visible(at, REACH * size_of(clash.style) * 8.) {
                    draw_landing(light, view, clash, at, since, level);
                }
            }
        }
    }

    /// The strike on its way back up the line.
    fn draw_strike(&self, light: &mut Emission, view: &View, clash: &Clash, level: f32) {
        let count = usize::from(clash.count).clamp(2, SPINE);
        let mut points = [Vec2::ZERO; SPINE];
        for (out, point) in points.iter_mut().zip(&clash.spine[..count]) {
            *out = view.screen(*point);
        }
        let spine = &points[..count];
        let u = (clash.age / clash.flight.max(0.01)).clamp(0., 1.);
        let head = along(spine, u);
        let way = (along(spine, (u + 0.03).min(1.)) - along(spine, (u - 0.03).max(0.)))
            .normalize_or_zero();
        let side = perpendicular(way);
        let style = clash.style;
        let r = REACH * size_of(style) * view.zoom;
        let pulse = 0.86 + 0.14 * (clash.age * 74. + clash.seed).sin();
        let white = light.hot(heat(style, 0.06), 1.);
        let body = heat(style, 0.36);
        let rim = heat(style, 0.78);
        // The part it has already passed burns out behind it.
        let reached = (u * (count - 1) as f32) as usize;
        let mut behind = [Vec2::ZERO; SPINE];
        behind[..=reached].copy_from_slice(&spine[..=reached]);
        behind[reached + 1] = head;
        let passed = &behind[..reached + 2];
        let (age, flight) = (clash.age, clash.flight);
        let burn = move |at: f32| (1. - (age - flight * u * at) / 0.2).clamp(0., 1.);
        lay(light, Layer::Wash, passed, |_, at| {
            (
                r * 0.42 * (0.4 + 0.6 * at),
                alpha(rim, 0.5 * burn(at) * level),
                0.,
            )
        });
        lay(light, Layer::Core, passed, |_, at| {
            (
                (r * 0.075 * (0.3 + 0.7 * at)).max(1.),
                alpha(body, burn(at).powi(2) * level),
                0.,
            )
        });
        match style {
            ClashStyle::Beam => {
                // What's left of the stream, from the gun down to the surge.
                let mut rest = [Vec2::ZERO; SPINE + 1];
                let mut length = 0;
                for point in spine[reached + 1..].iter().rev() {
                    rest[length] = *point;
                    length += 1;
                }
                rest[length] = head;
                length += 1;
                let half = BEAM_ROUND_RADIUS * 1.3 * view.zoom;
                stream(
                    light,
                    &rest[..length],
                    half,
                    clash.age + 1.,
                    level,
                    true,
                    self.bosses
                        .get(&clash.boss)
                        .is_some_and(|p| p.kind == Some(BossKind::Jester)),
                );
                for fork in 0..3 {
                    let seed = clash.seed + fork as f32 * 3.1 + (clash.age * 40.).floor() * 1.9;
                    let out = way * -(0.4 + 0.5 * (seed * 2.1).sin().abs())
                        + side * (seed * 3.7).sin() * 1.1;
                    bolt(
                        light,
                        head,
                        head + out * r * 0.9,
                        (r * 0.012).max(0.8),
                        alpha(ion(0.12), 0.9 * level),
                        seed,
                    );
                }
                light.oval(
                    Layer::Wash,
                    head - way * r * 0.42,
                    way * r * 1.9 * pulse,
                    r * 0.52,
                    alpha(rim, 0.6 * level),
                );
                light.oval(
                    Layer::Wash,
                    head,
                    way * r * 0.82,
                    r * 0.34,
                    alpha(body, 0.9 * level),
                );
                light.oval(
                    Layer::Core,
                    head,
                    way * r * 0.74 * pulse,
                    r * 0.15,
                    alpha(white, level),
                );
                light.glint(head, r * 1.25 * pulse, clash.age * 9., alpha(white, level));
            }
            ClashStyle::Rail => {
                light.oval(
                    Layer::Wash,
                    head - way * r * 0.9,
                    way * r * 2.5,
                    r * 0.32,
                    alpha(rim, 0.62 * level),
                );
                light.oval(
                    Layer::Core,
                    head - way * r * 0.1,
                    way * r * 1.05,
                    r * 0.11,
                    alpha(white, level),
                );
                light.spot(Layer::Wash, head, r * 0.62, alpha(body, 0.85 * level));
                light.glint(head, r * 0.9 * pulse, clash.seed, alpha(white, level));
            }
            ClashStyle::Siege => {
                // The shell itself, on fire.
                light.spot(
                    Layer::Smoke,
                    head - way * r * 0.2,
                    r * 0.86,
                    Color::new(0.05, 0.025, 0.02, 0.3 * level),
                );
                light.oval(
                    Layer::Wash,
                    head - way * r * 0.5,
                    way * r * 1.36,
                    r * 0.62 * pulse,
                    alpha(ramp(0.42), 0.82 * level),
                );
                light.oval(
                    Layer::Core,
                    head - way * r * 0.14,
                    way * r * 0.7,
                    r * 0.34,
                    alpha(ramp(0.14), level),
                );
                light.spot(Layer::Core, head, r * 0.24 * pulse, alpha(white, level));
            }
            ClashStyle::Bastion => {
                light.oval(
                    Layer::Wash,
                    head - way * r * 0.6,
                    way * r * 1.7,
                    r * 0.4,
                    alpha(rim, 0.6 * level),
                );
                light.oval(
                    Layer::Core,
                    head,
                    way * r * 0.74,
                    r * 0.19,
                    alpha(white, level),
                );
                light.outline(
                    Layer::Core,
                    head,
                    6,
                    r * 0.52 * pulse,
                    clash.age * 6.,
                    (r * 0.02).max(0.8),
                    alpha(body, 0.86 * level),
                );
            }
        }
    }

    /// Light a clash throws across a dark arena.
    pub(super) fn clash_lights(&self, view: &View, out: &mut Vec<Light>) {
        let z = view.zoom;
        let level = self.look.level().min(1.);
        for clash in &self.clashes {
            let size = size_of(clash.style);
            let tint = mix(heat(clash.style, 0.4), WHITE, 0.2);
            if clash.age < CONTACT_SECONDS {
                let fade = 1. - clash.age / CONTACT_SECONDS;
                let reach = 1000. * size;
                if view.visible(clash.at, reach) {
                    crate::lighting::gather(
                        out,
                        Light::Spot {
                            at: view.screen(clash.at),
                            radius: reach * z,
                            color: alpha(tint, fade * fade * level),
                        },
                    );
                }
            }
            let count = usize::from(clash.count).clamp(2, SPINE);
            let (spot, strength, reach) = if clash.landed {
                let since = clash.age - clash.flight;
                (
                    clash.spine[count - 1],
                    (1. - since / 0.5).max(0.).powi(2),
                    820. * size,
                )
            } else {
                let u = (clash.age / clash.flight.max(0.01)).clamp(0., 1.);
                (along(&clash.spine[..count], u), 0.85, 520. * size)
            };
            if strength > 0.01 && view.visible(spot, reach) {
                crate::lighting::gather(
                    out,
                    Light::Spot {
                        at: view.screen(spot),
                        radius: reach * z,
                        color: alpha(tint, strength * level),
                    },
                );
            }
        }
    }
}

/// The heart of a hit: hot in the middle and falling away through the color
/// of the gun, over a blink of dark so it has something to be bright against.
fn heart(light: &mut Emission, p: Vec2, r: f32, t: f32, span: f32, style: ClashStyle, level: f32) {
    let u = (t / span).clamp(0., 1.);
    let (fade, grow) = (1. - u, ease_out(u));
    let punch = (1. - t / 0.1).max(0.);
    let edge = heat(style, 0.84);
    let body = heat(style, 0.4);
    let white = light.hot(heat(style, 0.05), 1.);
    if punch > 0. {
        light.spot(
            Layer::Smoke,
            p,
            r * 6.4,
            Color::new(0.012, 0.014, 0.026, 0.38 * punch * level),
        );
    }
    // Wide light goes first, so the air clears and doesn't sit there like fog.
    light.spot(
        Layer::Wash,
        p,
        r * (3.6 + 2.2 * grow),
        alpha(edge, 0.2 * fade.powi(5) * level),
    );
    light.spot(
        Layer::Wash,
        p,
        r * (1.75 + 1.25 * grow),
        alpha(body, 0.6 * fade.powi(3) * level),
    );
    light.spot(
        Layer::Core,
        p,
        r * (0.62 + 0.36 * grow),
        alpha(body, 0.58 * fade * fade * level),
    );
    light.spot(
        Layer::Core,
        p,
        r * (0.26 + 0.22 * punch + 0.05 * grow),
        alpha(white, (0.44 * fade + 0.56 * punch) * level),
    );
}

/// A blade of light from `from` out along `way`, tapering to nothing.
fn blade(
    light: &mut Emission,
    from: Vec2,
    way: Vec2,
    length: f32,
    half: f32,
    style: ClashStyle,
    strength: f32,
) {
    if strength < 0.01 || length < 2. {
        return;
    }
    let (edge, body) = (heat(style, 0.8), heat(style, 0.34));
    let white = light.hot(heat(style, 0.05), 1.);
    let stop = |along: f32, width: f32, color: Color, share: f32| Stop {
        at: from + way * length * along,
        half: width,
        color: alpha(color, share * strength),
    };
    light.band(
        Layer::Wash,
        &[
            stop(0., half * 2.7, body, 0.52),
            stop(0.36, half * 2.1, edge, 0.34),
            stop(1., 0., edge, 0.),
        ],
    );
    light.band(
        Layer::Core,
        &[
            stop(0., half * 0.55, white, 1.),
            stop(0.13, half, white, 1.),
            stop(0.46, half * 0.5, body, 0.92),
            stop(1., 0., edge, 0.),
        ],
    );
}

/// The blades a hit throws out, mostly square to the line of fire, as a jet
/// spreads when it meets a plate. `d` points back along that line.
fn blades(light: &mut Emission, p: Vec2, d: Vec2, r: f32, t: f32, clash: &Clash, level: f32) {
    let style = clash.style;
    let cut = cut_of(style);
    let v = t / cut.life;
    if v >= 1. {
        return;
    }
    let facing = d.y.atan2(d.x);
    let out = 0.62 + 0.38 * ease_out((t / 0.05).min(1.));
    let fall = (1. - v).powf(1.3);
    let count = usize::from(cut.blades);
    for index in 0..count {
        let seed = clash.seed + index as f32 * 1.73;
        let one = (seed * 12.9898).sin() * 0.5 + 0.5;
        let two = (seed * 78.233).sin() * 0.5 + 0.5;
        let angle = if style == ClashStyle::Bastion {
            // Bastion's plate breaks along its six sides.
            facing + (index as f32 + 0.5) / count as f32 * TAU + (one - 0.5) * 0.12
        } else {
            let side = if index % 2 == 0 { 1. } else { -1. };
            facing + side * (PI * 0.5 + (one - 0.5) * 2. * cut.stray)
        };
        let way = direction(angle);
        let length = r * cut.reach * (0.42 + 0.58 * two * two) * out;
        let half = r * cut.width * (0.6 + 0.4 * one) * fall.max(0.18);
        let flicker = 0.88 + 0.12 * ((t * 53.).floor() * 1.9 + seed).sin();
        blade(
            light,
            p + way * r * 0.08,
            way,
            length,
            half,
            style,
            fall * flicker * level,
        );
    }
}

/// One shock front. Its edge wanders and burns unevenly, brightest where the
/// force went, so it reads as a blast and not a drawn circle.
#[allow(clippy::too_many_arguments)]
fn front(
    light: &mut Emission,
    p: Vec2,
    radius: f32,
    half: f32,
    wash: f32,
    d: Vec2,
    style: ClashStyle,
    strength: f32,
    seed: f32,
) {
    const SEGMENTS: usize = 60;
    if strength < 0.01 || radius < 2. {
        return;
    }
    let (edge, hot) = (heat(style, 0.8), heat(style, 0.2));
    let clear = Stop {
        at: p,
        half: 0.,
        color: Color::new(0., 0., 0., 0.),
    };
    let (mut thin, mut wide) = ([clear; SEGMENTS + 1], [clear; SEGMENTS + 1]);
    let mut shade = [clear; SEGMENTS + 1];
    for index in 0..=SEGMENTS {
        let turn = index as f32 / SEGMENTS as f32 * TAU;
        let way = direction(turn);
        // Square to the line of fire is where the blast went, so it runs a
        // little further there and burns brighter.
        let square = 1. - way.dot(d).abs();
        let wander = 0.94 + 0.11 * square;
        let burn = (0.26 + 0.74 * square) * (0.8 + 0.2 * (turn * 5. + seed * 2.).sin());
        let at = p + way * radius * wander;
        thin[index] = Stop {
            at,
            half: (half * (0.35 + 0.65 * burn)).max(0.5),
            color: alpha(hot, strength * strength * burn),
        };
        // Lit air just behind the edge.
        wide[index] = Stop {
            at: at - way * wash * 0.72,
            half: wash * (0.55 + 0.45 * burn),
            color: alpha(edge, 0.17 * strength * burn),
        };
        // A dark line just inside it, where the air has been pushed thin.
        // Against that the bright edge stands up like a real pressure wave.
        shade[index] = Stop {
            at: at - way * (half * 3. + 2.),
            half: half * 2.4 + 1.2,
            color: Color::new(0.02, 0.025, 0.05, 0.24 * strength * burn),
        };
    }
    light.band(Layer::Smoke, &shade);
    light.band(Layer::Wash, &wide);
    light.band(Layer::Core, &thin);
}

/// Hairline streaks blown straight out from a hit.
fn speed_lines(light: &mut Emission, p: Vec2, r: f32, t: f32, clash: &Clash, level: f32) {
    let blown = (1. - t / 0.24).max(0.);
    if blown <= 0. {
        return;
    }
    let body = heat(clash.style, 0.4);
    let white = light.hot(heat(clash.style, 0.05), 1.);
    let lines = if light.rich() { 15 } else { 8 };
    for line in 0..lines {
        let seed = clash.seed + line as f32;
        let angle = line as f32 / lines as f32 * TAU + (seed * 12.9898).sin() * 0.3 + clash.seed;
        let way = direction(angle);
        let scatter = 0.5 + 0.5 * ((seed * 78.233).sin() * 0.5 + 0.5);
        let weight = (seed * 37.719).sin() * 0.5 + 0.5;
        let inner = r * (0.8 + 2.2 * (1. - blown)) * scatter;
        let outer = inner + r * (1.9 * blown + 0.2) * scatter;
        light.streak(
            Layer::Core,
            p + way * outer,
            p + way * inner,
            (r * (0.01 + 0.016 * weight)).max(0.6),
            alpha(body, 0.),
            alpha(white, (0.42 + 0.48 * weight) * blown * blown * level),
        );
    }
}

/// The flash where the parry met the round.
fn draw_contact(light: &mut Emission, view: &View, clash: &Clash, level: f32) {
    let style = clash.style;
    let t = clash.age;
    let r = REACH * size_of(style) * view.zoom;
    let p = view.screen(clash.at);
    let d = vec2(clash.back.x, clash.back.y);
    let n = perpendicular(d);
    let u = (t / CONTACT_SECONDS).clamp(0., 1.);
    let grow = ease_out(u);
    let flare = (1. - t / 0.3).max(0.);
    let body = heat(style, 0.4);
    let white = light.hot(heat(style, 0.05), 1.);
    heart(light, p, r, t, CONTACT_SECONDS, style, level);
    // A long flare across the line of fire, and a shorter one along it.
    let bar = flare * flare;
    light.oval(
        Layer::Wash,
        p,
        n * r * (4.6 + 1.6 * grow),
        r * 0.2,
        alpha(body, 0.42 * bar * level),
    );
    light.oval(
        Layer::Core,
        p,
        n * r * (4.2 + 1.4 * grow),
        (r * 0.026).max(1.1),
        alpha(white, bar * level),
    );
    light.oval(
        Layer::Core,
        p,
        d * r * (1.9 + 0.8 * grow),
        (r * 0.02).max(0.9),
        alpha(white, 0.7 * bar * level),
    );
    blades(light, p, d, r, t, clash, level);
    // The parry's edge, bowed toward the round it caught.
    let facing = d.y.atan2(d.x);
    let center = p - d * r * 0.9;
    light.arc(
        Layer::Wash,
        center,
        r * 0.98,
        r * 0.18 * (1. - u) + 1.,
        facing - 1.12,
        2.24,
        alpha(body, 0.56 * flare * level),
    );
    light.arc(
        Layer::Core,
        center,
        r * 0.98,
        (r * 0.04 * (0.4 + flare)).max(1.),
        facing - 1.05,
        2.1,
        alpha(white, flare * level),
    );
    // One shock front, not a stack of them.
    let v = (t / 0.5).clamp(0., 1.);
    if v < 1. {
        let fall = (1. - v) * (1. - v);
        front(
            light,
            p,
            r * (0.5 + 4.3 * ease_out(v)),
            r * 0.05 * (0.4 + fall),
            r * 0.42 * (0.5 + fall),
            d,
            style,
            fall * level,
            clash.seed,
        );
    }
    speed_lines(light, p, r, t, clash, level);
    sheets(light, p, (d, n), r, clash, level);
    match style {
        ClashStyle::Beam | ClashStyle::Rail if flare > 0. => {
            // Forks of ions leaping off the contact.
            let beat = (t * 34.).floor();
            for fork in 0..6 {
                let seed = clash.seed + fork as f32 * 2.7 + beat * 1.37;
                let aside = if fork % 2 == 0 { 1.57 } else { -1.57 };
                let turn = facing + aside + (seed * 3.1).sin() * 1.1;
                let way = direction(turn);
                let length = r * (1. + 1.9 * ((seed * 7.3).sin() * 0.5 + 0.5)) * (0.5 + 0.5 * grow);
                bolt(
                    light,
                    p + way * r * 0.2,
                    p + way * length,
                    (r * 0.012).max(0.8),
                    alpha(heat(style, 0.14), flare * level),
                    seed,
                );
            }
        }
        ClashStyle::Bastion => {
            // The plate the slug glanced off, breaking up as it opens.
            let plate = (t / 0.36).clamp(0., 1.);
            if plate < 1. {
                let fall = (1. - plate).powi(2);
                light.outline(
                    Layer::Core,
                    p,
                    6,
                    r * (0.86 + 1.5 * ease_out(plate)),
                    facing + plate * 0.3,
                    (r * 0.024 * (0.5 + fall)).max(0.9),
                    alpha(heat(style, 0.14), fall * level),
                );
            }
        }
        _ => {}
    }
}

/// The strike going off on the boss: the same shape as the contact, thrown
/// back out the way the strike came in.
fn draw_landing(light: &mut Emission, view: &View, clash: &Clash, at: V2, t: f32, level: f32) {
    let style = clash.style;
    let r = REACH * size_of(style) * view.zoom * 1.12;
    let p = view.screen(at);
    let count = usize::from(clash.count).clamp(2, SPINE);
    let back = (clash.spine[count - 2] - at).normalized();
    let d = vec2(back.x, back.y);
    let n = perpendicular(d);
    let flare = (1. - t / 0.26).max(0.);
    let body = heat(style, 0.4);
    let white = light.hot(heat(style, 0.05), 1.);
    heart(light, p, r, t, LAND_SECONDS, style, level);
    let bar = flare * flare;
    light.oval(
        Layer::Wash,
        p,
        n * r * 3.6,
        r * 0.18,
        alpha(body, 0.36 * bar * level),
    );
    light.oval(
        Layer::Core,
        p,
        n * r * 3.3,
        (r * 0.024).max(1.),
        alpha(white, 0.9 * bar * level),
    );
    blades(light, p, d, r, t, clash, level);
    let v = (t / 0.46).clamp(0., 1.);
    if v < 1. {
        let fall = (1. - v) * (1. - v);
        front(
            light,
            p,
            r * (0.46 + 3.7 * ease_out(v)),
            r * 0.05 * (0.4 + fall),
            r * 0.4 * (0.5 + fall),
            d,
            style,
            fall * level,
            clash.seed * 1.7,
        );
    }
    speed_lines(light, p, r, t, clash, level);
}

/// The round's force sheeting off the parry to either side and curling back
/// toward the gun, as a jet does against a plate.
fn sheets(light: &mut Emission, p: Vec2, (d, n): (Vec2, Vec2), r: f32, clash: &Clash, level: f32) {
    const POINTS: usize = 15;
    let style = clash.style;
    let t = clash.age;
    let strength = (1. - t / 0.46).max(0.);
    if strength <= 0. {
        return;
    }
    let reach = r * (0.5 + 3.5 * ease_out((t / 0.42).min(1.)));
    let (half, wide) = match style {
        ClashStyle::Beam => (0.15, 0.4),
        ClashStyle::Rail => (0.1, 0.3),
        ClashStyle::Siege => (0.21, 0.52),
        ClashStyle::Bastion => (0.07, 0.2),
    };
    let edge = heat(style, 0.76);
    let body = heat(style, 0.3);
    let white = light.hot(heat(style, 0.04), 1.);
    for side in [-1_f32, 1.] {
        let mut spine = [Vec2::ZERO; POINTS];
        for (index, point) in spine.iter_mut().enumerate() {
            let v = index as f32 / (POINTS - 1) as f32;
            let ripple = (v * 7. - t * 22. + side * clash.seed).sin() * r * 0.07 * v;
            *point = p + n * side * reach * v + d * (reach * 0.44 * v * v + ripple);
        }
        let taper = |v: f32| (1. - v).powf(0.72) * (v / 0.05).clamp(0.45, 1.);
        lay(light, Layer::Wash, &spine, |_, v| {
            (
                (r * wide * taper(v)).max(0.5),
                alpha(edge, 0.46 * strength * (1. - v * 0.6) * level),
                0.,
            )
        });
        lay(light, Layer::Core, &spine, |_, v| {
            (
                (r * half * taper(v)).max(0.5),
                alpha(body, 0.88 * strength * (1. - v * 0.5) * level),
                0.,
            )
        });
        lay(light, Layer::Core, &spine, |_, v| {
            (
                (r * half * 0.34 * taper(v)).max(0.5),
                alpha(white, strength * (1. - v * 0.7) * level),
                0.,
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Look, Shown};
    use super::*;
    use tankyard_core::{Boss, RAIL_RADIUS};

    fn boss(kind: BossKind) -> Boss {
        let mut world = tankyard_core::World::new(
            tankyard_core::WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        world.add_player("Pilot", false);
        assert_ne!(world.spawn_boss(kind), 0);
        let mut boss = world.bosses.remove(0);
        boss.id = 41;
        boss.pos = V2::new(1000., 1000.);
        boss.aim = 0.;
        boss.arrival = 0.;
        boss
    }

    fn sync(fx: &mut BossFx, boss: &Boss) {
        fx.sync(
            std::iter::once(Shown {
                tick: 0,
                body: None,
                body_time: crate::boss_fx::BodyTime::Local(1.),
                boss,
                pos: boss.pos,
                aim: boss.aim,
                color: WHITE,
            }),
            1. / 60.,
        );
    }

    #[test]
    fn every_clash_runs_its_course_and_leaves_nothing_behind() {
        for (kind, style) in [
            (BossKind::Railgunner, ClashStyle::Beam),
            (BossKind::Railgunner, ClashStyle::Rail),
            (BossKind::SiegeEngine, ClashStyle::Siege),
            (BossKind::Bastion, ClashStyle::Bastion),
        ] {
            let mut fx = BossFx::default();
            let boss = boss(kind);
            sync(&mut fx, &boss);
            let at = boss.pos + V2::new(1300., 0.);
            fx.clash(style, at, 41, 4200, true);
            assert_eq!(fx.clashes.len(), 1);
            assert!(
                fx.take_shake() > 0.,
                "{style:?} shakes the player who hit it"
            );
            assert!(!fx.sparks.is_empty() || !fx.flames.is_empty(), "{style:?}");
            assert!(fx.held(1. / 60.) < 1. / 60., "the hit holds for a moment");
            let mut landed = false;
            for _ in 0..240 {
                sync(&mut fx, &boss);
                fx.update(1. / 60.);
                let cues = fx.take_cues();
                landed |= cues.iter().any(|(cue, _)| *cue == BossCue::Backlash);
            }
            assert!(landed, "{style:?} reached the boss");
            assert!(fx.clashes.is_empty(), "{style:?}");
            let strikes = fx.take_strikes();
            assert_eq!(strikes.len(), 1);
            assert_eq!(strikes[0].1, 4200);
            assert!(fx.take_strikes().is_empty());
        }
    }

    #[test]
    fn a_weaker_gun_makes_a_smaller_clash() {
        let sizes = ClashStyle::ALL.map(size_of);
        // Rail recoil is now larger than the sustained beam's clash.
        assert!(
            sizes[1] > sizes[0] && sizes[0] > sizes[2] && sizes[2] > sizes[3],
            "{sizes:?}"
        );
        let shocks = ClashStyle::ALL.map(shocks_of);
        assert!(
            shocks.windows(2).all(|pair| pair[0] > pair[1]),
            "{shocks:?}"
        );
    }

    #[test]
    fn a_beam_goes_back_along_the_way_it_had_bent() {
        let mut fx = BossFx::default();
        let boss = boss(BossKind::Railgunner);
        sync(&mut fx, &boss);
        let muzzle = boss.pos + V2::new(RAIL_RADIUS * RAIL_MUZZLE, 0.);
        let at = muzzle + V2::new(1000., 0.);
        // A bowed stream, newest rounds nearest the gun.
        for step in 1..14 {
            let v = step as f32 / 14.;
            let bow = (v * PI).sin() * 160.;
            fx.stream_round_gone(41, 1000 - step, muzzle + V2::new(1000. * v, bow), 11.);
        }
        fx.clash(ClashStyle::Beam, at, 41, 100, false);
        assert!(fx.gone.is_empty(), "the clash took the stream");
        let clash = fx.clashes[0];
        let count = usize::from(clash.count);
        assert!(count > 8);
        assert_eq!(clash.spine[0], at);
        assert!(clash.spine[count - 1].distance(muzzle) < 1.);
        let bowed = clash.spine[..count].iter().map(|p| p.y - muzzle.y);
        assert!(bowed.fold(0., f32::max) > 120., "it follows the bend");
        let gaps: Vec<f32> = clash.spine[..count]
            .windows(2)
            .map(|pair| pair[0].distance(pair[1]))
            .collect();
        let (low, high) = gaps
            .iter()
            .fold((f32::MAX, 0_f32), |(a, b), &gap| (a.min(gap), b.max(gap)));
        assert!(high < low * 1.8, "evenly spaced: {low}..{high}");
        // The rounds it took don't each spark as a hit afterwards.
        let bursts = fx.bursts.len();
        fx.update(1. / 60.);
        fx.update(1. / 60.);
        assert!(fx.bursts.len() <= bursts + 1);
    }

    #[test]
    fn stream_rounds_nobody_claimed_still_spark_as_hits() {
        let mut fx = BossFx::default();
        fx.stream_round_gone(41, 7, V2::new(10., 10.), 11.);
        assert!(fx.bursts.is_empty());
        fx.update(1. / 60.);
        assert!(fx.bursts.is_empty(), "held for one frame");
        fx.update(1. / 60.);
        assert_eq!(fx.bursts.len(), 1);
        assert!(fx.gone.is_empty());
    }

    #[test]
    fn reduced_motion_keeps_the_sound_and_the_number_only() {
        let mut fx = BossFx::default();
        fx.set_look(Look {
            strength: 1.,
            calm: true,
        });
        let boss = boss(BossKind::SiegeEngine);
        sync(&mut fx, &boss);
        fx.clash(
            ClashStyle::Siege,
            boss.pos + V2::new(800., 0.),
            41,
            900,
            true,
        );
        assert_eq!(fx.clashes.len(), 1, "only the strike timer is retained");
        assert!(fx.sparks.is_empty() && fx.flames.is_empty());
        assert_eq!(fx.take_shake(), 0.);
        assert!(
            fx.take_cues()
                .iter()
                .any(|(cue, _)| *cue == BossCue::ClashFire)
        );
        for _ in 0..120 {
            // The game reapplies this setting before every update.
            fx.set_look(Look {
                strength: 1.,
                calm: true,
            });
            fx.update(1. / 60.);
            assert!(fx.sparks.is_empty() && fx.flames.is_empty());
            assert_eq!(fx.take_shake(), 0.);
        }
        let strikes = fx.take_strikes();
        assert_eq!(strikes.len(), 1);
        assert_eq!(strikes[0].1, 900);
        assert!(fx.clashes.is_empty());
    }

    #[test]
    fn a_far_off_clash_leaves_the_camera_alone() {
        let mut fx = BossFx::default();
        let boss = boss(BossKind::Bastion);
        sync(&mut fx, &boss);
        fx.set_camera(V2::new(9000., 9000.));
        fx.clash(
            ClashStyle::Bastion,
            boss.pos + V2::new(500., 0.),
            41,
            0,
            false,
        );
        assert_eq!(fx.take_shake(), 0.);
        assert_eq!(fx.hold, 0.);
    }

    #[test]
    fn bosses_sharing_low_id_bits_are_told_apart_by_their_gun_then_distance() {
        let mut fx = BossFx::default();
        let rail = boss(BossKind::Railgunner);
        let mut siege = boss(BossKind::SiegeEngine);
        siege.id = rail.id + 8192;
        siege.pos += V2::new(1400., 300.);
        let mut other_rail = rail.clone();
        other_rail.id = rail.id + 2 * 8192;
        other_rail.pos += V2::new(-1800., 900.);
        fx.sync(
            [&rail, &siege, &other_rail].into_iter().map(|boss| Shown {
                tick: 0,
                body: None,
                body_time: crate::boss_fx::BodyTime::Local(1.),
                boss,
                pos: boss.pos,
                aim: boss.aim,
                color: WHITE,
            }),
            1. / 60.,
        );
        let bits = (rail.id & 8191) as u32;
        // Next to the Railgunner, but only the Siege Engine fires this round.
        let at = rail.pos + V2::new(350., 0.);
        assert_eq!(fx.clash_boss(ClashStyle::Siege, bits, at), siege.id);
        // Two guns of the right kind: the one nearer the parry.
        assert_eq!(fx.clash_boss(ClashStyle::Rail, bits, at), rail.id);
        let there = other_rail.pos + V2::new(200., 0.);
        assert_eq!(fx.clash_boss(ClashStyle::Beam, bits, there), other_rail.id);
        // Whoever fired is gone: no boss, and no strike to draw.
        assert_eq!(fx.clash_boss(ClashStyle::Rail, bits + 1, at), 0);
        fx.clash(ClashStyle::Rail, at, siege.id, 1234, true);
        let clash = fx.clashes[0];
        assert_eq!(clash.boss, siege.id);
    }

    #[test]
    fn disabling_effects_during_a_strike_keeps_its_damage_feedback() {
        let mut fx = BossFx::default();
        let boss = boss(BossKind::Railgunner);
        sync(&mut fx, &boss);
        fx.clash(
            ClashStyle::Rail,
            boss.pos + V2::new(1200., 0.),
            boss.id,
            1234,
            true,
        );
        for _ in 0..120 {
            fx.set_look(Look {
                strength: 0.,
                calm: false,
            });
            fx.update(1. / 60.);
            assert!(fx.sparks.is_empty() && fx.flames.is_empty());
            assert_eq!(fx.take_shake(), 0.);
        }
        assert_eq!(fx.take_strikes()[0].1, 1234);
        assert!(fx.clashes.is_empty());
    }

    #[test]
    fn clashes_are_capped_and_a_missing_boss_is_harmless() {
        let mut fx = BossFx::default();
        for n in 0..(MOST_CLASHES * 3) {
            fx.clash(ClashStyle::Rail, V2::new(n as f32, 5.), 4242, 10, false);
        }
        assert_eq!(fx.clashes.len(), MOST_CLASHES);
        for _ in 0..120 {
            fx.update(1. / 60.);
        }
        assert!(fx.clashes.is_empty());
    }

    #[test]
    fn a_line_is_laid_out_evenly_and_keeps_its_ends() {
        let line = [V2::new(0., 0.), V2::new(10., 0.), V2::new(1000., 0.)];
        let (points, count) = even(&line);
        let count = usize::from(count);
        assert_eq!(points[0], line[0]);
        assert!(points[count - 1].distance(line[2]) < 0.01);
        for pair in points[..count].windows(2) {
            assert!((pair[1].x - pair[0].x - 1000. / (count - 1) as f32).abs() < 0.1);
        }
        for degenerate in [&[][..], &[V2::new(5., 5.)][..], &[V2::ZERO, V2::ZERO][..]] {
            let (points, count) = even(degenerate);
            assert_eq!(count, 2);
            assert!(points[0].x.is_finite());
        }
    }
}

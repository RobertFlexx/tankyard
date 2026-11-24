//! A parry that lands on a boss's heaviest fire. An ordinary parry sends a
//! round back. A clash sends back what was in it: the strike runs up the line
//! to whoever fired it and goes off there, and a beam caught this way breaks.
//!
//! It goes out as an ordinary parry event with a value, which clients from
//! before this release ignore, so the protocol doesn't change. The host alone
//! decides the hit and the damage; clients only draw what the value says.
use crate::railgunner::is_beam_round;
use crate::*;
use tankyard_content::{Ability, ProjectileKind};

/// How long after a clash the same boss can't be clashed with again, so two
/// players catching one volley can't stack it.
const CLASH_WAIT: f32 = 0.45;
/// How much of a clash a bot's parry lands. They time it better than anyone
/// should, and a lobby of them shouldn't melt a boss.
const BOT_SHARE: f32 = 0.4;
/// What the Railgunner's second, lighter shot is worth against its first.
const ECHO_SHARE: f32 = 0.5;
/// Seconds a struck boss loses before its next big attack.
const REEL_SECONDS: f32 = 0.9;
/// How many more seconds a broken beam leaves the vents open.
const BROKEN_VENT: f32 = 0.7;
/// The shortest and longest a strike takes to get back.
const FLIGHT: (f32, f32) = (0.05, 0.6);
/// The Railgunner's rail and beam only turn for a parry made at the last
/// moment: within this many seconds of pressing it.
pub const RAIL_PARRY_WINDOW: f32 = 0.075;
/// And only for a tank turned toward them, within this many radians.
pub const RAIL_PARRY_ARC: f32 = 0.85;

const STYLE_BITS: u32 = 3;
const BOSS_BITS: u32 = 13;
/// Damage goes on the wire in steps of this much.
const DAMAGE_STEP: f32 = 10.;

/// Which big gun was parried, which sets how hard it hits back and how it
/// looks doing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClashStyle {
    /// The Railgunner's plasma stream.
    Beam,
    /// The Railgunner's cannon.
    Rail,
    /// The Siege Engine's burning shell.
    Siege,
    /// Bastion's steel slug.
    Bastion,
}

impl ClashStyle {
    pub const ALL: [Self; 4] = [Self::Beam, Self::Rail, Self::Siege, Self::Bastion];

    /// What it takes from the boss, as a share of the boss's full health.
    pub const fn share(self) -> f32 {
        match self {
            Self::Beam => 0.1,
            Self::Rail => 0.09,
            Self::Siege => 0.07,
            Self::Bastion => 0.04,
        }
    }
    /// How fast the strike runs back, in units a second.
    pub const fn speed(self) -> f32 {
        match self {
            Self::Beam => 6200.,
            Self::Rail => 4600.,
            Self::Siege => 2600.,
            Self::Bastion => 2200.,
        }
    }
    /// How hard the blow rocks the boss, in units a second.
    const fn knock(self) -> f32 {
        match self {
            Self::Beam => 150.,
            Self::Rail => 130.,
            Self::Siege => 110.,
            Self::Bastion => 70.,
        }
    }
    /// Seconds to get back across `distance`.
    pub fn flight(self, distance: f32) -> f32 {
        (distance.max(0.) / self.speed()).clamp(FLIGHT.0, FLIGHT.1)
    }
}

/// A strike on its way to a boss. Only the host runs it, so it never travels
/// in a snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClashWork {
    /// Seconds until this boss can be clashed with again.
    pub wait: f32,
    /// Seconds until the strike lands; none is on its way at zero.
    pub strike: f32,
    pub damage: f32,
    /// Who sent it.
    pub by: EntityId,
}

/// The value a clash's parry event carries.
pub fn clash_value(style: ClashStyle, boss: EntityId, damage: f32) -> u32 {
    let code = style as u32 + 1;
    let boss = boss as u32 & ((1 << BOSS_BITS) - 1);
    let damage = (damage / DAMAGE_STEP).round().clamp(0., 65_535.) as u32;
    code | boss << STYLE_BITS | damage << (STYLE_BITS + BOSS_BITS)
}

/// What a parry event's `value` says about its clash, if it was one: the
/// style, the low bits of the boss's id, and roughly how much it dealt.
pub fn clash_of(value: u32) -> Option<(ClashStyle, u32, u32)> {
    let code = value & ((1 << STYLE_BITS) - 1);
    let style = *ClashStyle::ALL.get((code as usize).checked_sub(1)?)?;
    let boss = (value >> STYLE_BITS) & ((1 << BOSS_BITS) - 1);
    let damage = (value >> (STYLE_BITS + BOSS_BITS)) * DAMAGE_STEP as u32;
    Some((style, boss, damage))
}

/// Whether the low bits a clash event carries name the boss `id`.
pub fn clash_names(bits: u32, id: EntityId) -> bool {
    id as u32 & ((1 << BOSS_BITS) - 1) == bits
}

/// How a round from a boss of `kind` clashes, and what share of the full blow
/// it is worth. Only each boss's big gun does.
fn style_of(
    kind: BossKind,
    round: ProjectileKind,
    vel: V2,
    damage: f32,
) -> Option<(ClashStyle, f32)> {
    let shell = round == ProjectileKind::Shell;
    match kind {
        BossKind::Railgunner if is_beam_round(round, vel) => Some((ClashStyle::Beam, 1.)),
        BossKind::Railgunner if shell => Some((
            ClashStyle::Rail,
            if damage >= LETHAL_DAMAGE {
                1.
            } else {
                ECHO_SHARE
            },
        )),
        BossKind::SiegeEngine if shell && damage >= LETHAL_DAMAGE => Some((ClashStyle::Siege, 1.)),
        BossKind::Bastion if shell && damage >= BASTION_ROUND_DAMAGE => {
            Some((ClashStyle::Bastion, 1.))
        }
        _ => None,
    }
}

impl World {
    /// Whether `defender`'s parry is too late, or turned the wrong way, for
    /// the Railgunner's rail or beam round `projectile`. Those go straight
    /// through anything but a parry made at the last moment facing them.
    pub(crate) fn misses_rail(&self, projectile: usize, defender: usize, ability: Ability) -> bool {
        let shot = &self.projectiles[projectile];
        let railgunner = self
            .bosses
            .iter()
            .any(|boss| boss.id == shot.owner && boss.kind == BossKind::Railgunner);
        if !railgunner || style_of(BossKind::Railgunner, shot.kind, shot.vel, shot.damage).is_none()
        {
            return false;
        }
        let tank = &self.tanks[defender];
        let held = ability.parry_window().unwrap_or(0.) - tank.shield;
        let toward = (shot.vel * -1.).normalized();
        let facing = V2::from_angle(tank.aim).dot(toward) >= RAIL_PARRY_ARC.cos();
        held > RAIL_PARRY_WINDOW || !facing
    }

    /// Answers `projectile` with a clash if it's a boss's big gun and that
    /// boss isn't still reeling from the last one. The round is spent, and
    /// so is the rest of a beam.
    pub(crate) fn clash(
        &mut self,
        projectile: usize,
        defender: usize,
        impact: V2,
        ability: Ability,
    ) -> bool {
        let shot = &self.projectiles[projectile];
        let (owner, round, vel, round_damage) = (shot.owner, shot.kind, shot.vel, shot.damage);
        let Some(boss) = self
            .bosses
            .iter_mut()
            .find(|boss| boss.id == owner && boss.health > 0.)
        else {
            return false;
        };
        if boss.clash.wait > 0. {
            return false;
        }
        let Some((style, weight)) = style_of(boss.kind, round, vel, round_damage) else {
            return false;
        };
        let tank = &mut self.tanks[defender];
        let (by, team) = (tank.id, tank.team);
        let share = style.share() * weight * if tank.bot { BOT_SHARE } else { 1. };
        let healing = match ability {
            Ability::CounterParry | Ability::VolleyParry => 0.12,
            _ => 0.08,
        };
        tank.health = (tank.health + tank.max_health * healing).min(tank.max_health);
        let damage = boss.max_health * share;
        let flight = style.flight(impact.distance(boss.pos));
        boss.clash = ClashWork {
            wait: flight.max(CLASH_WAIT),
            strike: flight,
            damage,
            by,
        };
        boss.vel += (boss.pos - impact).normalized() * style.knock();
        boss.attack_timer = (boss.attack_timer + REEL_SECONDS).min(30.);
        boss.timer = boss.timer.max(REEL_SECONDS * 0.7);
        match style {
            ClashStyle::Beam => boss.break_beam(BROKEN_VENT),
            ClashStyle::Rail => boss.rail.echo = 0.,
            ClashStyle::Siege | ClashStyle::Bastion => {}
        }
        let spent = &mut self.projectiles[projectile];
        spent.lifetime = 0.;
        spent.penetration = 0.;
        if style == ClashStyle::Beam {
            for rest in &mut self.projectiles {
                if rest.owner == owner && is_beam_round(rest.kind, rest.vel) {
                    rest.lifetime = 0.;
                    rest.penetration = 0.;
                }
            }
        }
        let value = clash_value(style, owner, damage);
        self.event(EventKind::Parry, impact, by, value, team);
        true
    }

    /// One tick of the strikes on their way back.
    pub(crate) fn land_clashes(&mut self) {
        for index in 0..self.bosses.len() {
            let boss = &mut self.bosses[index];
            boss.clash.wait = (boss.clash.wait - DT).max(0.);
            if boss.clash.strike <= 0. {
                continue;
            }
            boss.clash.strike -= DT;
            if boss.clash.strike > 0. {
                continue;
            }
            boss.clash.strike = 0.;
            if boss.health <= 0. {
                continue;
            }
            let (damage, by) = (boss.clash.damage, boss.clash.by);
            boss.health -= damage;
            boss.last_attacker = by;
            let (id, pos, team) = (boss.id, boss.pos, boss.team);
            self.record_damage(id, by, damage);
            self.event(EventKind::Impact, pos, id, damage as u32, team);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;

    const TANK: V2 = V2::new(2400., 1500.);
    const BOSS: V2 = V2::new(1200., 1500.);

    fn duel(kind: BossKind) -> World {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                spawn_protection: 0.,
                dynamic_arena: false,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.walls.clear();
        let player = world.add_player("Pilot", false);
        assert!(world.sandbox_class(player, "riposte"));
        world.tanks[0].level = 45;
        world.tanks[0].pos = V2::new(100., 100.);
        assert_ne!(world.spawn_boss(kind), 0);
        world.tanks[0].pos = TANK;
        world.tanks[0].vel = V2::ZERO;
        // Facing the boss, just as the parry starts.
        world.tanks[0].aim = std::f32::consts::PI;
        world.tanks[0].shield = Ability::Parry.parry_window().unwrap();
        let boss = &mut world.bosses[0];
        boss.pos = BOSS;
        boss.vel = V2::ZERO;
        boss.arrival = 0.;
        boss.timer = 25.;
        boss.attack_timer = 25.;
        world
    }

    fn round(world: &mut World, kind: ProjectileKind, speed: f32, damage: f32) -> usize {
        let (owner, team) = (world.bosses[0].id, world.bosses[0].team);
        let shot = world.simple_projectile(
            owner,
            team,
            TANK - V2::new(60., 0.),
            V2::new(speed, 0.),
            kind,
            damage,
            12.,
            1.,
            8.,
        );
        world.projectiles.push(shot);
        world.projectiles.len() - 1
    }

    fn fly_home(world: &mut World) {
        for _ in 0..((FLIGHT.1 / DT) as usize + 2) {
            world.land_clashes();
        }
    }

    #[test]
    fn a_parried_beam_breaks_and_the_strike_lands_on_the_railgunner() {
        let mut world = duel(BossKind::Railgunner);
        world.bosses[0].rail.beam = 3.;
        world.bosses[0].tactic = BossTactic::Charge;
        world.bosses[0].tactic_time = 3.;
        world.tanks[0].health = 10.;
        let parried = round(
            &mut world,
            ProjectileKind::Piercing,
            BEAM_ROUND_SPEED,
            BEAM_ROUND_DAMAGE,
        );
        for _ in 0..12 {
            round(
                &mut world,
                ProjectileKind::Piercing,
                BEAM_ROUND_SPEED,
                BEAM_ROUND_DAMAGE,
            );
        }
        let full = world.bosses[0].max_health;
        assert!(world.parry_projectile(parried, 0, TANK));
        assert!(
            world.projectiles.iter().all(|p| p.lifetime <= 0.),
            "the whole stream goes with it"
        );
        assert!(world.tanks[0].health > 10., "a parry still heals");
        let boss = &world.bosses[0];
        assert!(
            !boss.beaming() && boss.venting(),
            "it's left with its fins open"
        );
        assert_eq!(boss.health, full, "the strike hasn't got there yet");
        let event = world
            .events
            .iter()
            .find(|event| matches!(event.kind, EventKind::Parry))
            .unwrap();
        let (style, bits, shown) = clash_of(event.value).unwrap();
        assert_eq!(style, ClashStyle::Beam);
        assert!(clash_names(bits, world.bosses[0].id));
        assert!((shown as f32 - full * 0.1).abs() <= DAMAGE_STEP);
        assert_eq!(event.pos, TANK);
        fly_home(&mut world);
        let boss = &world.bosses[0];
        assert!((boss.health - full * 0.9).abs() < 1., "{}", boss.health);
        assert_eq!(boss.last_attacker, world.tanks[0].id);
    }

    #[test]
    fn the_railgunner_only_yields_to_a_last_moment_parry_facing_it() {
        let window = Ability::Parry.parry_window().unwrap();
        for (shield, aim, kind, speed, damage, yields) in [
            // Late: the rail goes through.
            (
                window - RAIL_PARRY_WINDOW - 0.02,
                std::f32::consts::PI,
                ProjectileKind::Shell,
                1700.,
                LETHAL_DAMAGE,
                false,
            ),
            // Turned away: so does the beam.
            (
                window,
                0.,
                ProjectileKind::Piercing,
                BEAM_ROUND_SPEED,
                BEAM_ROUND_DAMAGE,
                false,
            ),
            // On time and facing it: a clash.
            (
                window - RAIL_PARRY_WINDOW * 0.5,
                std::f32::consts::PI + 0.5,
                ProjectileKind::Shell,
                1700.,
                LETHAL_DAMAGE,
                true,
            ),
            // Its small guns still take any parry.
            (window - 0.15, 0., ProjectileKind::Bullet, 500., 20., true),
        ] {
            let mut world = duel(BossKind::Railgunner);
            world.tanks[0].shield = shield;
            world.tanks[0].aim = aim;
            let index = round(&mut world, kind, speed, damage);
            assert_eq!(
                world.parry_projectile(index, 0, TANK),
                yields,
                "{kind:?} with {shield} left facing {aim}"
            );
        }
        // Nothing else asks this much.
        let mut world = duel(BossKind::SiegeEngine);
        world.tanks[0].shield = window - 0.15;
        world.tanks[0].aim = 0.;
        let index = round(&mut world, ProjectileKind::Shell, 900., LETHAL_DAMAGE);
        assert!(world.parry_projectile(index, 0, TANK));
    }

    #[test]
    fn the_heavier_the_gun_the_harder_it_comes_back() {
        let shares = ClashStyle::ALL.map(ClashStyle::share);
        assert!(
            shares.windows(2).all(|pair| pair[0] > pair[1]),
            "{shares:?}"
        );
        for (kind, proj, speed, damage, style) in [
            (
                BossKind::Railgunner,
                ProjectileKind::Shell,
                RAIL_ROUND_SPEED,
                LETHAL_DAMAGE,
                ClashStyle::Rail,
            ),
            (
                BossKind::SiegeEngine,
                ProjectileKind::Shell,
                SIEGE_ROUND_SPEED,
                LETHAL_DAMAGE,
                ClashStyle::Siege,
            ),
            (
                BossKind::Bastion,
                ProjectileKind::Shell,
                BASTION_ROUND_SPEED,
                BASTION_ROUND_DAMAGE,
                ClashStyle::Bastion,
            ),
        ] {
            let mut world = duel(kind);
            let index = round(&mut world, proj, speed, damage);
            let full = world.bosses[0].max_health;
            assert!(world.parry_projectile(index, 0, TANK));
            assert!(world.projectiles[index].lifetime <= 0., "{kind:?}");
            fly_home(&mut world);
            let dealt = full - world.bosses[0].health;
            assert!(
                (dealt - full * style.share()).abs() < 1.,
                "{kind:?} {dealt}"
            );
        }
    }

    #[test]
    fn one_volley_cannot_stack_a_clash() {
        let mut world = duel(BossKind::SiegeEngine);
        let first = round(&mut world, ProjectileKind::Shell, 1500., LETHAL_DAMAGE);
        let second = round(&mut world, ProjectileKind::Shell, 1500., LETHAL_DAMAGE);
        let full = world.bosses[0].max_health;
        assert!(world.parry_projectile(first, 0, TANK));
        let strike = world.bosses[0].clash;
        assert!(world.parry_projectile(second, 0, TANK), "still a parry");
        assert_eq!(world.bosses[0].clash, strike, "but not a second clash");
        let back = &world.projectiles[second];
        assert_eq!(back.owner, world.tanks[0].id);
        assert!(back.damage <= 240., "{}", back.damage);
        fly_home(&mut world);
        let dealt = full - world.bosses[0].health;
        assert!((dealt - full * ClashStyle::Siege.share()).abs() < 1.);
    }

    #[test]
    fn the_small_guns_still_get_an_ordinary_parry() {
        for (kind, proj, damage) in [
            (BossKind::Sentinel, ProjectileKind::Piercing, 28.),
            (BossKind::Railgunner, ProjectileKind::Piercing, 46.),
            (BossKind::SiegeEngine, ProjectileKind::Explosive, 40.),
            (BossKind::Bastion, ProjectileKind::Explosive, 36.),
        ] {
            let mut world = duel(kind);
            let index = round(&mut world, proj, 700., damage);
            assert!(world.parry_projectile(index, 0, TANK), "{kind:?}");
            assert_eq!(world.bosses[0].clash, ClashWork::default(), "{kind:?}");
            assert_eq!(world.projectiles[index].owner, world.tanks[0].id);
            let event = world
                .events
                .iter()
                .find(|event| matches!(event.kind, EventKind::Parry))
                .unwrap();
            assert_eq!(event.value, 0, "{kind:?}");
        }
    }

    #[test]
    fn a_bots_clash_lands_lighter() {
        let mut world = duel(BossKind::SiegeEngine);
        world.tanks[0].bot = true;
        let index = round(&mut world, ProjectileKind::Shell, 1500., LETHAL_DAMAGE);
        let full = world.bosses[0].max_health;
        assert!(world.parry_projectile(index, 0, TANK));
        fly_home(&mut world);
        let dealt = full - world.bosses[0].health;
        assert!((dealt - full * ClashStyle::Siege.share() * BOT_SHARE).abs() < 1.);
    }

    #[test]
    fn the_rail_echo_is_worth_half_and_a_clash_cancels_it() {
        let mut world = duel(BossKind::Railgunner);
        world.bosses[0].rail.echo = 0.3;
        let index = round(
            &mut world,
            ProjectileKind::Shell,
            RAIL_ROUND_SPEED,
            RAIL_ECHO_DAMAGE,
        );
        let full = world.bosses[0].max_health;
        assert!(world.parry_projectile(index, 0, TANK));
        assert_eq!(world.bosses[0].rail.echo, 0.);
        fly_home(&mut world);
        let dealt = full - world.bosses[0].health;
        assert!((dealt - full * ClashStyle::Rail.share() * ECHO_SHARE).abs() < 1.);
    }

    #[test]
    fn a_clash_can_finish_a_boss_and_credits_whoever_landed_it() {
        let mut world = duel(BossKind::SiegeEngine);
        let index = round(&mut world, ProjectileKind::Shell, 1500., LETHAL_DAMAGE);
        world.bosses[0].health = 5.;
        assert!(world.parry_projectile(index, 0, TANK));
        let by = world.tanks[0].id;
        world.tanks[0].shield = 0.;
        for _ in 0..((FLIGHT.1 / DT) as usize + 4) {
            world.step();
        }
        assert!(world.bosses.is_empty(), "it went down");
        assert!(world.tanks.iter().any(|t| t.id == by && t.score > 0));
    }

    #[test]
    fn a_strike_on_a_boss_already_dead_does_nothing() {
        let mut world = duel(BossKind::Bastion);
        let index = round(
            &mut world,
            ProjectileKind::Shell,
            BASTION_ROUND_SPEED,
            BASTION_ROUND_DAMAGE,
        );
        assert!(world.parry_projectile(index, 0, TANK));
        world.bosses[0].health = -1.;
        world.events.clear();
        fly_home(&mut world);
        assert_eq!(world.bosses[0].health, -1.);
        assert!(world.events.is_empty());
    }

    #[test]
    fn a_clash_value_survives_the_wire_and_nothing_else_reads_as_one() {
        for style in ClashStyle::ALL {
            for (boss, damage) in [(1, 0.), (8191, 655_340.), (8192, 12_340.), (u64::MAX, 1e9)] {
                let value = clash_value(style, boss, damage);
                let (read, bits, shown) = clash_of(value).unwrap();
                assert_eq!(read, style);
                assert!(clash_names(bits, boss));
                assert!(shown as f32 <= 655_350.);
                assert!((shown as f32 - damage.min(655_350.)).abs() <= DAMAGE_STEP);
            }
        }
        assert_eq!(clash_of(0), None, "an ordinary parry");
        assert_eq!(clash_of(5), None);
        assert_eq!(clash_of(7), None);
    }

    #[test]
    fn a_strike_always_takes_a_moment_and_never_long() {
        for style in ClashStyle::ALL {
            for distance in [-5., 0., 1., 900., 2400., 1e9] {
                let flight = style.flight(distance);
                assert!((FLIGHT.0..=FLIGHT.1).contains(&flight), "{flight}");
            }
        }
    }
}

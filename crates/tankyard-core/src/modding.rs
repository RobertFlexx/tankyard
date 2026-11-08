//! Authoritative behavior API. JSON hooks and trusted Rust extensions use the
//! same validated command buffer. Nothing here receives administration access.
use crate::*;
use std::collections::{BTreeMap, HashMap};
use tankyard_mods::{
    API_VERSION, Condition, Effect, LoadedMods, MAX_COMMANDS, Target as Selection, Trigger,
};

#[derive(Clone, Copy, Debug)]
pub struct HookEvent {
    pub trigger: Trigger,
    pub actor: EntityId,
    pub position: V2,
    pub tick: u64,
    /// The other participant: a kill's victim, a death's killer, or the
    /// attacker that caused damage. Zero when there is none.
    pub target: EntityId,
    /// Damage dealt, level reached, XP awarded or projectile kind.
    pub value: f32,
}
impl HookEvent {
    pub fn new(trigger: Trigger, actor: EntityId, position: V2, tick: u64) -> Self {
        Self {
            trigger,
            actor,
            position,
            tick,
            target: 0,
            value: 0.,
        }
    }
}

pub struct Commands {
    queued: Vec<(EntityId, Effect)>,
}
impl Default for Commands {
    fn default() -> Self {
        Self {
            queued: Vec::with_capacity(MAX_COMMANDS),
        }
    }
}
impl Commands {
    pub fn push(&mut self, actor: EntityId, effect: Effect) -> Result<(), String> {
        if self.queued.len() >= MAX_COMMANDS {
            return Err("hook command budget exceeded".into());
        }
        self.queued.push((actor, effect));
        Ok(())
    }
}
pub trait ServerPlugin: Send {
    fn id(&self) -> &str;
    fn api_version(&self) -> u32 {
        API_VERSION
    }
    fn version(&self) -> &str {
        "1"
    }
    /// Skip events a plugin never handles; scripts register handlers lazily.
    fn handles(&self, _trigger: Trigger) -> bool {
        true
    }
    fn on_event(
        &mut self,
        world: &World,
        event: HookEvent,
        commands: &mut Commands,
    ) -> Result<(), String>;
    /// Bounded diagnostic lines, collected once per tick.
    fn drain_log(&mut self) -> Vec<String> {
        Vec::new()
    }
}
struct Module {
    id: String,
    hooks: Vec<tankyard_mods::Hook>,
    plugin: Option<Box<dyn ServerPlugin>>,
    counters: BTreeMap<(EntityId, String), f64>,
    last_run: BTreeMap<(usize, EntityId), u64>,
    commands: Commands,
    disabled: bool,
    command_tick: u64,
    commands_this_tick: usize,
    operations_left: usize,
}
#[derive(Default)]
pub struct ModRuntime {
    modules: Vec<Module>,
    seen: HashMap<EntityId, (u32, bool)>,
    round: Option<u32>,
    pub errors: Vec<String>,
    /// Recent script output, oldest first.
    pub log: std::collections::VecDeque<String>,
}
impl ModRuntime {
    pub fn new(mods: &LoadedMods) -> Self {
        Self {
            // Script packs are installed as plugins by the script runtime.
            modules: mods
                .behaviors
                .iter()
                .filter(|m| m.scripts.is_empty())
                .map(|m| Module {
                    id: m.info.id.clone(),
                    hooks: m.hooks.clone(),
                    plugin: None,
                    counters: BTreeMap::new(),
                    last_run: BTreeMap::new(),
                    commands: Commands::default(),
                    disabled: false,
                    command_tick: 0,
                    commands_this_tick: 0,
                    operations_left: 10_000,
                })
                .collect(),
            ..Default::default()
        }
    }
    pub fn install(&mut self, plugin: Box<dyn ServerPlugin>) -> Result<(), String> {
        if plugin.api_version() != API_VERSION
            || !tankyard_mods::identifier(plugin.id())
            || self.modules.len() >= tankyard_mods::MAX_MODS
            || self.modules.iter().any(|m| m.id == plugin.id())
        {
            return Err("invalid, duplicate, or incompatible server plugin".into());
        }
        self.modules.push(Module {
            id: plugin.id().into(),
            hooks: vec![],
            plugin: Some(plugin),
            counters: BTreeMap::new(),
            last_run: BTreeMap::new(),
            commands: Commands::default(),
            disabled: false,
            command_tick: 0,
            commands_this_tick: 0,
            operations_left: 10_000,
        });
        Ok(())
    }
    pub fn before_step(&mut self, world: &mut World) {
        if self.modules.is_empty() {
            return;
        }
        if self.round != Some(world.match_state.round) {
            self.round = Some(world.match_state.round);
            self.seen.clear();
            for m in &mut self.modules {
                m.last_run.clear();
                m.counters.clear();
            }
            self.dispatch(
                world,
                HookEvent::new(Trigger::RoundStart, 0, V2::ZERO, world.tick),
            );
        }
        let spawned: Vec<_> = world
            .tanks
            .iter()
            .filter(|t| {
                t.alive
                    && self
                        .seen
                        .get(&t.id)
                        .is_none_or(|(deaths, alive)| !*alive || *deaths != t.deaths)
            })
            .map(|t| (t.id, t.pos))
            .collect();
        self.seen
            .retain(|id, _| world.tanks.iter().any(|t| t.id == *id));
        for t in &world.tanks {
            self.seen.insert(t.id, (t.deaths, t.alive));
        }
        for (actor, position) in spawned {
            self.dispatch(
                world,
                HookEvent::new(Trigger::Spawn, actor, position, world.tick),
            );
        }
        if !world.match_state.finished {
            self.dispatch(
                world,
                HookEvent::new(Trigger::Tick, 0, V2::ZERO, world.tick),
            );
        }
    }
    pub fn after_step(&mut self, world: &mut World) {
        if self.modules.is_empty() {
            return;
        }
        // Only build events some enabled pack listens for.
        let mut wanted = [false; Trigger::ALL.len()];
        for (slot, trigger) in wanted.iter_mut().zip(Trigger::ALL) {
            *slot = self.modules.iter().any(|m| !m.disabled && m.wants(trigger));
        }
        let wants = |trigger: Trigger| {
            Trigger::ALL
                .iter()
                .position(|t| *t == trigger)
                .is_some_and(|index| wanted[index])
        };
        // Process gameplay outcomes before cosmetic shot floods.
        let ordered = world
            .events
            .iter()
            .filter(|e| !matches!(e.kind, EventKind::Shot))
            .chain(
                world
                    .events
                    .iter()
                    .filter(|e| matches!(e.kind, EventKind::Shot)),
            );
        let mut events: Vec<_> = ordered
            .take(1024)
            .filter_map(|e| {
                let (trigger, target) = match e.kind {
                    EventKind::Shot => (Trigger::Shot, 0),
                    EventKind::Kill => (
                        Trigger::Kill,
                        e.elimination.as_ref().map_or(0, |victim| victim.victim),
                    ),
                    EventKind::LevelUp => (Trigger::LevelUp, 0),
                    EventKind::Evolve => (Trigger::Evolve, 0),
                    EventKind::Capture => (Trigger::Capture, 0),
                    EventKind::Victory => (Trigger::RoundEnd, 0),
                    EventKind::Destroy
                        if wants(Trigger::Death)
                            && world.tanks.iter().any(|t| t.id == e.subject) =>
                    {
                        let killer = world
                            .events
                            .iter()
                            .find(|kill| {
                                matches!(kill.kind, EventKind::Kill)
                                    && kill
                                        .elimination
                                        .as_ref()
                                        .is_some_and(|victim| victim.victim == e.subject)
                            })
                            .map_or(0, |kill| kill.subject);
                        (Trigger::Death, killer)
                    }
                    _ => return None,
                };
                if !wants(trigger) {
                    return None;
                }
                Some(HookEvent {
                    trigger,
                    actor: e.subject,
                    position: e.pos,
                    tick: e.tick,
                    target,
                    value: e.value as f32,
                })
            })
            .collect();
        if wants(Trigger::Damage) {
            events.extend(world.damage_log.iter().take(1024).map(|record| HookEvent {
                trigger: Trigger::Damage,
                actor: record.victim,
                position: record.position,
                tick: world.tick,
                target: record.attacker,
                value: record.amount,
            }));
        }
        for e in events {
            self.dispatch(world, e);
        }
        for module in &mut self.modules {
            if let Some(plugin) = module.plugin.as_mut() {
                for line in plugin.drain_log().into_iter().take(64) {
                    if self.log.len() >= 200 {
                        self.log.pop_front();
                    }
                    self.log.push_back(format!("[{}] {line}", module.id));
                }
            }
        }
    }
    pub fn dispatch(&mut self, world: &mut World, event: HookEvent) {
        for module in &mut self.modules {
            if module.disabled || !module.wants(event.trigger) {
                continue;
            }
            if module.command_tick != event.tick {
                module.command_tick = event.tick;
                module.commands_this_tick = 0;
                module.operations_left = 10_000;
            }
            module.commands.queued.clear();
            let result = if let Some(plugin) = module.plugin.as_mut() {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    plugin.on_event(world, event, &mut module.commands)
                }))
                .unwrap_or_else(|_| Err("plugin panicked".into()))
            } else {
                evaluate(module, world, event)
            };
            let result = result.and_then(|()| validate_commands(module, world));
            if let Err(error) = result {
                module.disabled = true;
                module.commands.queued.clear();
                if self.errors.len() < tankyard_mods::MAX_MODS {
                    self.errors.push(format!("{} disabled: {error}", module.id));
                }
                continue;
            }
            module.commands_this_tick += module.commands.queued.len();
            for (actor, effect) in module.commands.queued.drain(..) {
                if let Effect::Counter { name, add } = effect {
                    let value = module.counters.entry((actor, name)).or_default();
                    *value = (*value + add).clamp(-1e9, 1e9);
                } else {
                    apply(world, actor, effect);
                }
            }
        }
    }
}
impl Module {
    fn wants(&self, trigger: Trigger) -> bool {
        match &self.plugin {
            Some(plugin) => plugin.handles(trigger),
            None => self.hooks.iter().any(|hook| hook.event == trigger),
        }
    }
}
/// Bosses named by scripts and JSON hooks, by display or enum name.
pub fn boss_by_name(name: &str) -> Option<BossKind> {
    let wanted = name.trim().to_ascii_lowercase().replace([' ', '_'], "");
    BossKind::ALL.into_iter().find(|kind| {
        kind.name().to_ascii_lowercase().replace(' ', "") == wanted
            || format!("{kind:?}").to_ascii_lowercase() == wanted
    })
}
fn validate_commands(module: &Module, world: &World) -> Result<(), String> {
    if module.commands_this_tick + module.commands.queued.len() > MAX_COMMANDS {
        return Err("per-tick command budget exceeded".into());
    }
    let mut counters = std::collections::HashSet::new();
    for (actor, effect) in &module.commands.queued {
        effect.validate(&world.content)?;
        if let Effect::SpawnBoss { boss } = effect
            && boss_by_name(boss).is_none()
        {
            return Err(format!("unknown boss {boss:?}"));
        }
        if let Effect::SetTeamScore { team, .. } = effect
            && *team > world.config.mode.teams()
        {
            return Err("team score outside this mode's teams".into());
        }
        if let Effect::Counter { name, .. } = effect
            && !module.counters.contains_key(&(*actor, name.clone()))
        {
            counters.insert((*actor, name));
        }
    }
    if counters.len() + module.counters.len() > 4096 {
        return Err("counter budget exceeded".into());
    }
    Ok(())
}
fn evaluate(module: &mut Module, world: &World, event: HookEvent) -> Result<(), String> {
    for (index, hook) in module.hooks.iter().enumerate() {
        if hook.event != event.trigger
            || (hook.event == Trigger::Tick && !event.tick.is_multiple_of(hook.every_ticks))
        {
            continue;
        }
        spend(&mut module.operations_left)?;
        let actors: Vec<_> = match hook.target {
            Selection::Actor => vec![event.actor],
            Selection::AllPlayers => world.tanks.iter().map(|t| t.id).collect(),
            Selection::AlivePlayers => world
                .tanks
                .iter()
                .filter(|t| t.alive)
                .map(|t| t.id)
                .collect(),
        };
        for actor in actors {
            spend(&mut module.operations_left)?;
            if module
                .last_run
                .get(&(index, actor))
                .is_some_and(|last| event.tick.saturating_sub(*last) < hook.cooldown_ticks)
            {
                continue;
            }
            let tank = world.tanks.iter().find(|t| t.id == actor);
            let mut matches = true;
            for c in &hook.conditions {
                if !condition(
                    c,
                    tank,
                    actor,
                    &module.counters,
                    &mut module.operations_left,
                )? {
                    matches = false;
                    break;
                }
            }
            if !matches {
                continue;
            }
            for effect in &hook.actions {
                module.commands.push(actor, effect.clone())?;
            }
            if module.last_run.len() >= 16384 && !module.last_run.contains_key(&(index, actor)) {
                return Err("hook cooldown budget exceeded".into());
            }
            module.last_run.insert((index, actor), event.tick);
        }
    }
    Ok(())
}
fn condition(
    c: &Condition,
    tank: Option<&Tank>,
    actor: EntityId,
    counters: &BTreeMap<(EntityId, String), f64>,
    operations: &mut usize,
) -> Result<bool, String> {
    spend(operations)?;
    let result = match c {
        Condition::HealthBelow { fraction } => {
            tank.is_some_and(|t| t.alive && t.health / t.max_health < *fraction)
        }
        Condition::LevelAtLeast { level } => tank.is_some_and(|t| t.level >= *level),
        Condition::ClassIs { class } => tank.is_some_and(|t| &t.class == class),
        Condition::TeamIs { team } => tank.is_some_and(|t| t.team == *team),
        Condition::IsBot { value } => tank.is_some_and(|t| t.bot == *value),
        Condition::CounterAtLeast { name, value } => {
            counters.get(&(actor, name.clone())).copied().unwrap_or(0.0) >= *value
        }
        Condition::Any { conditions } => {
            let mut found = false;
            for c in conditions {
                if condition(c, tank, actor, counters, operations)? {
                    found = true;
                    break;
                }
            }
            found
        }
        Condition::Not { condition: c } => !condition(c, tank, actor, counters, operations)?,
    };
    Ok(result)
}
fn spend(operations: &mut usize) -> Result<(), String> {
    if *operations == 0 {
        return Err("hook operation budget exceeded".into());
    }
    *operations -= 1;
    Ok(())
}
fn apply(world: &mut World, actor: EntityId, effect: Effect) {
    let index = world.tanks.iter().position(|t| t.id == actor);
    match effect {
        Effect::Control {
            movement,
            aim,
            fire,
            ability,
        } => {
            if let Some(i) = index {
                let t = &mut world.tanks[i];
                t.input.movement = V2::new(movement[0], movement[1]);
                t.input.aim = aim;
                t.input.fire = fire;
                t.input.ability = ability;
            }
        }
        Effect::Heal { amount } => {
            if let Some(i) = index
                && world.tanks[i].alive
            {
                let t = &mut world.tanks[i];
                t.health = (t.health + amount).min(t.max_health);
            }
        }
        Effect::Damage { amount } => {
            if let Some(i) = index {
                world.damage_tank(i, amount, 0, V2::ZERO);
            }
        }
        Effect::Award { xp, score } => world.award(actor, xp, score),
        Effect::SetTeam { team } => {
            if let Some(i) = index {
                world.tanks[i].team = team;
            }
        }
        Effect::SetClass { class } => {
            if let Some(i) = index {
                let t = &mut world.tanks[i];
                t.class = class;
                t.cooldowns.clear();
                t.barrel_shots.clear();
                t.barrel_aims.clear();
                t.charge = 0.0;
                world.refresh_stats(actor, false);
            }
        }
        Effect::Impulse { velocity } => {
            if let Some(i) = index {
                world.tanks[i].vel =
                    (world.tanks[i].vel + V2::new(velocity[0], velocity[1])).clamp_length(2000.0);
            }
        }
        Effect::Teleport { position } => {
            if let Some(i) = index {
                let t = &mut world.tanks[i];
                t.pos = V2::new(
                    position[0] * world.config.width,
                    position[1] * world.config.height,
                );
                t.vel = V2::ZERO;
                physics::constrain_tank(t, &world.config, &world.walls);
            }
        }
        Effect::Announce { message } => world.match_state.announcement = message,
        Effect::FinishRound { team, reason } => world.finish(team, &reason),
        Effect::SpawnTarget {
            position,
            sides,
            health,
            xp,
        } => {
            let pos = V2::new(
                position[0] * world.config.width,
                position[1] * world.config.height,
            );
            if world.targets.len() < 10_000
                && world
                    .walls
                    .iter()
                    .all(|wall| physics::circle_rect(pos, 20.0, *wall).is_none())
            {
                let id = world.id();
                world.targets.push(Target {
                    id,
                    pos,
                    vel: V2::ZERO,
                    radius: 20.0,
                    health,
                    max_health: health,
                    kind: TargetKind::Custom(sides),
                    angle: 0.0,
                    xp,
                });
            }
        }
        Effect::Fire {
            speed,
            damage,
            radius,
            lifetime,
            angle,
        } => {
            if let Some(i) = index
                && world.tanks[i].alive
                && world.projectiles.len() < world.config.max_projectiles
            {
                let t = &world.tanks[i];
                let direction = V2::from_angle(t.aim + angle);
                let pos = t.pos + direction * (t.radius + radius + 2.0);
                if world
                    .walls
                    .iter()
                    .all(|wall| physics::sweep_rect(t.pos, pos, radius, *wall).is_none())
                {
                    let p = world.simple_projectile(
                        t.id,
                        t.team,
                        pos,
                        direction * speed,
                        tankyard_content::ProjectileKind::Bullet,
                        damage,
                        radius,
                        lifetime,
                        1.0,
                    );
                    world.projectiles.push(p);
                }
            }
        }
        Effect::SetLevel { level } => {
            if index.is_some() {
                world.set_level_unchecked(actor, level);
                world.config.modded = true;
            }
        }
        Effect::RaiseStat { stat } => {
            if let Some(i) = index {
                let stat = usize::from(stat);
                if world.tanks[i].alive && world.tanks[i].stats[stat] < 7 {
                    world.tanks[i].points = world.tanks[i].points.saturating_add(1).min(38);
                    world.upgrade_stat(actor, stat);
                }
            }
        }
        Effect::SetScales {
            speed,
            reload,
            damage,
        } => {
            if let Some(i) = index {
                let t = &mut world.tanks[i];
                t.speed_scale = speed;
                t.reload_scale = reload;
                t.damage_scale = damage;
            }
        }
        Effect::SetArmor { multiplier } => {
            if index.is_some() {
                if (multiplier - 1.).abs() < f32::EPSILON {
                    world.incoming_damage.remove(&actor);
                } else if world.incoming_damage.len() < 4096
                    || world.incoming_damage.contains_key(&actor)
                {
                    world.incoming_damage.insert(actor, multiplier);
                }
            }
        }
        Effect::SetShield { seconds } => {
            if let Some(i) = index
                && world.tanks[i].alive
            {
                world.tanks[i].shield = seconds;
            }
        }
        Effect::SetProtection { seconds } => {
            if let Some(i) = index
                && world.tanks[i].alive
            {
                world.tanks[i].protection = seconds;
            }
        }
        Effect::SetVelocity { velocity } => {
            if let Some(i) = index {
                world.tanks[i].vel = V2::new(velocity[0], velocity[1]).clamp_length(2000.0);
            }
        }
        Effect::Kill => {
            if let Some(i) = index
                && world.tanks[i].alive
            {
                // Environmental death: resolved by the normal death phase.
                world.tanks[i].health = -1.;
            }
        }
        Effect::SetScore { score } => {
            if let Some(i) = index {
                world.tanks[i].score = score;
            }
        }
        Effect::SetTeamScore { team, score } => {
            if let Some(slot) = world
                .match_state
                .scores
                .get_mut(usize::from(team.max(1)) - 1)
            {
                *slot = score;
            }
        }
        Effect::SpawnBoss { boss } => {
            if let Some(kind) = boss_by_name(&boss)
                && world.bosses.len() < 12
            {
                world.spawn_boss(kind);
            }
        }
        Effect::ClearProjectiles => {
            world.projectiles.retain(|p| {
                crate::team_bases::base_guard_team(p.owner).is_some()
                    || (actor != 0 && p.owner != actor)
            });
        }
        Effect::Counter { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_flood_cannot_exceed_a_modules_per_tick_command_budget() {
        let mut w = world();
        let actor = w.add_player("Pilot", false);
        let hook = tankyard_mods::Hook {
            id: "reward".into(),
            event: Trigger::Shot,
            target: Selection::Actor,
            every_ticks: 1,
            cooldown_ticks: 0,
            conditions: vec![],
            actions: vec![Effect::Award { xp: 0, score: 1 }],
        };
        let mut loaded = LoadedMods::default();
        loaded.behaviors.push(tankyard_mods::LoadedMod {
            info: tankyard_mods::ModInfo {
                id: "reward".into(),
                name: "reward".into(),
                version: "1".into(),
                behavior_digest: 1,
            },
            hooks: vec![hook],
            scripts: Default::default(),
        });
        let mut runtime = ModRuntime::new(&loaded);
        for _ in 0..MAX_COMMANDS + 20 {
            runtime.dispatch(&mut w, HookEvent::new(Trigger::Shot, actor, V2::ZERO, 5));
        }
        assert_eq!(w.tanks[0].score, MAX_COMMANDS as u32);
        assert_eq!(runtime.errors.len(), 1);
    }
    fn world() -> World {
        World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        )
    }
    struct BadPlugin;
    impl ServerPlugin for BadPlugin {
        fn id(&self) -> &str {
            "bad_plugin"
        }
        fn on_event(
            &mut self,
            _world: &World,
            e: HookEvent,
            c: &mut Commands,
        ) -> Result<(), String> {
            c.push(e.actor, Effect::Heal { amount: 50.0 })?;
            c.push(
                e.actor,
                Effect::SetClass {
                    class: "missing".into(),
                },
            )
        }
    }
    #[test]
    fn invalid_native_commands_are_quarantined_before_any_effect_is_applied() {
        let mut w = world();
        let id = w.add_player("Pilot", false);
        w.tanks[0].health = 20.0;
        let mut runtime = ModRuntime::default();
        runtime.install(Box::new(BadPlugin)).unwrap();
        runtime.dispatch(&mut w, HookEvent::new(Trigger::Kill, id, V2::ZERO, 1));
        assert_eq!(w.tanks[0].health, 20.0);
        assert_eq!(runtime.errors.len(), 1);
        runtime.dispatch(&mut w, HookEvent::new(Trigger::Kill, id, V2::ZERO, 2));
        assert_eq!(runtime.errors.len(), 1);
    }
    #[test]
    fn hooks_change_gameplay_and_respect_per_actor_cooldowns() {
        let mut w = world();
        let id = w.add_player("Pilot", false);
        w.tanks[0].health = 20.0;
        let hook = tankyard_mods::Hook {
            id: "heal_on_kill".into(),
            event: Trigger::Kill,
            target: Selection::Actor,
            every_ticks: 1,
            cooldown_ticks: 60,
            conditions: vec![],
            actions: vec![
                Effect::Heal { amount: 10.0 },
                Effect::Award { xp: 20, score: 5 },
            ],
        };
        let mut mods = LoadedMods::default();
        mods.behaviors.push(tankyard_mods::LoadedMod {
            info: tankyard_mods::ModInfo {
                id: "vampire".into(),
                name: "Vampire".into(),
                version: "1".into(),
                behavior_digest: 1,
            },
            hooks: vec![hook],
            scripts: Default::default(),
        });
        let mut runtime = ModRuntime::new(&mods);
        for tick in [1, 20, 61] {
            runtime.dispatch(&mut w, HookEvent::new(Trigger::Kill, id, V2::ZERO, tick));
        }
        assert_eq!(w.tanks[0].health, 40.0);
        assert_eq!(w.tanks[0].score, 10);
        assert_eq!(w.tanks[0].xp, 40);
    }
    #[test]
    fn downloaded_rules_keep_prediction_equal_to_server_movement() {
        let mut bundle = tankyard_mods::GameBundle::builtin();
        bundle.rules.movement_multiplier = 1.5;
        bundle.rules.acceleration = 12.0;
        let mut w = World::with_bundle(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            &bundle,
        )
        .unwrap();
        let id = w.add_player("Pilot", false);
        let mut predicted = w.tanks[0].clone();
        for sequence in 1..90 {
            let input = PlayerInput {
                sequence,
                movement: V2::new(1.0, 0.2).normalized(),
                ..Default::default()
            };
            w.set_input(id, input);
            predicted.input = input;
            physics::predict_tank(
                &mut predicted,
                &w.content,
                &w.config,
                &w.walls,
                w.tick + 1,
                DT,
            );
            w.step();
            assert!(predicted.pos.distance(w.tanks[0].pos) < 0.01);
        }
    }

    struct Recorder(std::sync::Arc<std::sync::Mutex<Vec<HookEvent>>>);
    impl ServerPlugin for Recorder {
        fn id(&self) -> &str {
            "recorder"
        }
        fn handles(&self, trigger: Trigger) -> bool {
            matches!(trigger, Trigger::Damage | Trigger::Kill | Trigger::Death)
        }
        fn on_event(&mut self, _: &World, e: HookEvent, c: &mut Commands) -> Result<(), String> {
            self.0.lock().unwrap().push(e);
            if e.trigger == Trigger::Damage && e.target != 0 {
                c.push(e.actor, Effect::SetArmor { multiplier: 0.5 })?;
            }
            Ok(())
        }
    }

    #[test]
    fn damage_events_name_the_attacker_and_new_effects_are_bounded() {
        let mut w = world();
        let victim = w.add_player("Victim", false);
        let attacker = w.add_player("Attacker", false);
        for tank in &mut w.tanks {
            tank.protection = 0.;
            tank.spawn_immunity = false;
        }
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut runtime = ModRuntime::default();
        runtime.install(Box::new(Recorder(events.clone()))).unwrap();
        let before = w.tanks[0].health;
        w.step_with(|world| runtime.before_step(world));
        w.damage_tank(0, 10., attacker, V2::ZERO);
        runtime.after_step(&mut w);
        assert_eq!(w.tanks[0].health, before - 10.);
        {
            let seen = events.lock().unwrap();
            let damage = seen.iter().find(|e| e.trigger == Trigger::Damage).unwrap();
            assert_eq!((damage.actor, damage.target), (victim, attacker));
            assert_eq!(damage.value, 10.);
        }
        // The armor command from the damage handler halves the next hit.
        w.damage_tank(0, 10., attacker, V2::ZERO);
        assert_eq!(w.tanks[0].health, before - 15.);
        let content = &w.content;
        for invalid in [
            Effect::SetScales {
                speed: 0.1,
                reload: 1.,
                damage: 1.,
            },
            Effect::SetArmor {
                multiplier: f32::NAN,
            },
            Effect::SetShield { seconds: 31. },
            Effect::SetLevel { level: 0 },
            Effect::RaiseStat { stat: 8 },
            Effect::SetTeamScore { team: 0, score: 1 },
        ] {
            assert!(invalid.validate(content).is_err(), "{invalid:?}");
        }
        assert_eq!(
            boss_by_name("Guardian of the Pentagons"),
            Some(BossKind::Guardian)
        );
        assert_eq!(
            boss_by_name("fallen_booster"),
            Some(BossKind::FallenBooster)
        );
        assert_eq!(boss_by_name("nope"), None);
        apply(&mut w, victim, Effect::Kill);
        w.step_with(|world| runtime.before_step(world));
        runtime.after_step(&mut w);
        assert!(!w.tanks[0].alive);
        let seen = events.lock().unwrap();
        let death = seen.iter().find(|e| e.trigger == Trigger::Death).unwrap();
        assert_eq!(death.actor, victim);
    }
}

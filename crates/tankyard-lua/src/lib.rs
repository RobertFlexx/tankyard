//! Sandboxed LuaJIT behavior packs.
//!
//! Each pack runs in its own LuaJIT state, recreated every round. Scripts see
//! only computation libraries and the `tankyard` API: no files, processes,
//! network, native code, bytecode or debug access. World data is lent to a
//! script for the duration of one event, and every change it requests passes
//! through the same validated command buffer as JSON hooks and native
//! extensions. Instruction, memory and command budgets stop runaway scripts;
//! a failing pack is disabled without applying its partial batch.
mod sandbox;

use mlua::{Function, Lua, LuaSerdeExt, Table, Value, Variadic};
use sandbox::Sandbox;
use std::{path::PathBuf, sync::Arc};
use tankyard_content::{ClassDef, Content, MapDef};
use tankyard_core::{
    EntityId, Tank, World, base_guard_team,
    modding::{Commands, HookEvent, ModRuntime, ServerPlugin},
};
use tankyard_mods::{
    Effect, GameRules, LoadedMods, ModInfo, ScriptDefinitions, ScriptPack, Trigger,
};

pub use sandbox::MEMORY_LIMIT;
/// Instructions one event handler may run.
pub const CALL_INSTRUCTIONS: u64 = 500_000;
/// Instructions all of a pack's handlers may run in one simulation tick.
pub const TICK_INSTRUCTIONS: u64 = 1_000_000;
/// Instructions for loading a pack's scripts.
pub const LOAD_INSTRUCTIONS: u64 = 20_000_000;
const MAX_QUERY_RESULTS: usize = 256;
const STAT_NAMES: [&str; 8] = [
    "health",
    "regen",
    "body",
    "speed",
    "penetration",
    "damage",
    "reload",
    "movement",
];
const PRELUDE: &str = include_str!("prelude.lua");

/// Load packs, running script packs' definition phase in the sandbox.
pub fn load(paths: &[PathBuf]) -> Result<LoadedMods, String> {
    tankyard_mods::load_with(paths, Some(&mut define))
}

/// Install a fresh script state for every script pack. Call once per round.
pub fn install(runtime: &mut ModRuntime, loaded: &LoadedMods, seed: u64) -> Result<(), String> {
    for behavior in loaded.behaviors.iter().filter(|b| !b.scripts.is_empty()) {
        let seed = seed ^ tankyard_mods::digest(behavior.info.id.as_bytes());
        runtime.install(Box::new(LuaPlugin::new(
            &behavior.info,
            &behavior.scripts,
            &loaded.bundle.content,
            seed,
        )?))?;
    }
    Ok(())
}

struct Script {
    sandbox: Sandbox,
    native: Table,
    host: Table,
}

impl Script {
    /// `defining` is true only while definitions are collected; the catalog
    /// copies stay readable in both phases so top-level code runs the same.
    fn new(
        info: &ModInfo,
        scripts: &ScriptPack,
        seed: u64,
        catalog: &Content,
        defining: bool,
    ) -> Result<Self, String> {
        let context = |e: mlua::Error| format!("{}: {}", info.id, clean_error(&e));
        let sandbox = Sandbox::new(seed).map_err(|e| format!("{}: {e}", info.id))?;
        sandbox
            .install_require(Arc::new(scripts.sources.clone()))
            .map_err(context)?;
        let lua = &sandbox.lua;
        install_catalog(lua, catalog).map_err(context)?;
        let native = lua.create_table().map_err(context)?;
        let triggers = lua.create_table().map_err(context)?;
        for trigger in Trigger::ALL {
            triggers.raw_set(trigger.name(), true).map_err(context)?;
        }
        sandbox.budget.begin(LOAD_INSTRUCTIONS);
        let host: Table = lua
            .load(PRELUDE)
            .set_name("=tankyard")
            .set_mode(mlua::chunk::ChunkMode::Text)
            .set_environment(lua.globals())
            .call((native.clone(), triggers, defining))
            .map_err(context)?;
        for entry in &scripts.entries {
            let source = scripts
                .sources
                .get(entry)
                .ok_or_else(|| format!("{}: missing script {entry}", info.id))?;
            sandbox.run(entry, source).map_err(context)?;
        }
        Ok(Self {
            sandbox,
            native,
            host,
        })
    }
}

/// Copies of the current catalog, so packs can extend the existing tree.
fn install_catalog(lua: &Lua, content: &Content) -> mlua::Result<()> {
    let classes = Arc::new(content.classes.clone());
    let maps = Arc::new(tankyard_content::builtin_maps());
    let globals = lua.globals();
    globals.raw_set(
        "__tankyard_base_class",
        lua.create_function(move |lua, id: String| {
            match classes.iter().find(|class| class.id == id) {
                Some(class) => lua.to_value(class),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    globals.raw_set(
        "__tankyard_base_map",
        lua.create_function(
            move |lua, id: String| match maps.iter().find(|map| map.id == id) {
                Some(map) => lua.to_value(map),
                None => Ok(Value::Nil),
            },
        )?,
    )?;
    Ok(())
}

fn define(
    info: &ModInfo,
    scripts: &ScriptPack,
    content: &Content,
) -> Result<ScriptDefinitions, String> {
    let script = Script::new(info, scripts, 0, content, true)?;
    let lua = &script.sandbox.lua;
    let fail = |what: &str, e: mlua::Error| format!("{}: {what}: {}", info.id, clean_error(&e));
    let table: Table = script
        .host
        .get::<Function>("definitions")
        .and_then(|f| f.call(()))
        .map_err(|e| fail("definitions", e))?;
    let mut result = ScriptDefinitions::default();
    let classes: Table = table.get("classes").map_err(|e| fail("classes", e))?;
    for (index, value) in classes.sequence_values::<Value>().enumerate() {
        let value = value.map_err(|e| fail("classes", e))?;
        let class: ClassDef = lua
            .from_value(value)
            .map_err(|e| fail(&format!("define_class #{}", index + 1), e))?;
        result.classes.push(class);
    }
    let maps: Table = table.get("maps").map_err(|e| fail("maps", e))?;
    for (index, value) in maps.sequence_values::<Value>().enumerate() {
        let value = value.map_err(|e| fail("maps", e))?;
        let map: MapDef = lua
            .from_value(value)
            .map_err(|e| fail(&format!("define_map #{}", index + 1), e))?;
        result.maps.push(map);
    }
    let rules: Value = table.get("rules").map_err(|e| fail("rules", e))?;
    if !rules.is_nil() {
        let rules: GameRules = lua.from_value(rules).map_err(|e| fail("set_rules", e))?;
        rules.validate().map_err(|e| format!("{}: {e}", info.id))?;
        result.rules = Some(rules);
    }
    result.mode_name = table.get("mode_name").map_err(|e| fail("mode_name", e))?;
    let textures: Table = table.get("textures").map_err(|e| fail("textures", e))?;
    for value in textures.sequence_values::<Table>() {
        let texture = value.map_err(|e| fail("textures", e))?;
        let id: String = texture.get("id").map_err(|e| fail("texture", e))?;
        let file: String = texture.get("file").map_err(|e| fail("texture", e))?;
        let bytes = scripts
            .images
            .get(&file)
            .ok_or_else(|| format!("{}: texture file {file} is not a PNG in this pack", info.id))?;
        result.textures.push(
            tankyard_mods::decode_texture(&id, bytes).map_err(|e| format!("{}: {e}", info.id))?,
        );
    }
    Ok(result)
}

/// A pack's runtime state for one round.
pub struct LuaPlugin {
    id: String,
    version: String,
    script: Script,
    dispatch: Function,
    handled: [bool; Trigger::ALL.len()],
    tick: u64,
    tick_used: u64,
}

impl LuaPlugin {
    pub fn new(
        info: &ModInfo,
        scripts: &ScriptPack,
        catalog: &Content,
        seed: u64,
    ) -> Result<Self, String> {
        let script = Script::new(info, scripts, seed, catalog, false)?;
        let fail = |e: mlua::Error| format!("{}: {}", info.id, clean_error(&e));
        let dispatch: Function = script.host.get("dispatch").map_err(fail)?;
        let events: Table = script
            .host
            .get::<Function>("events")
            .and_then(|f| f.call(()))
            .map_err(fail)?;
        let mut handled = [false; Trigger::ALL.len()];
        for name in events.sequence_values::<String>() {
            let name = name.map_err(fail)?;
            if let Some(trigger) = Trigger::from_name(&name) {
                handled[trigger_index(trigger)] = true;
            }
        }
        Ok(Self {
            id: info.id.clone(),
            version: info.version.clone(),
            script,
            dispatch,
            handled,
            tick: u64::MAX,
            tick_used: 0,
        })
    }
}

fn trigger_index(trigger: Trigger) -> usize {
    Trigger::ALL
        .iter()
        .position(|candidate| *candidate == trigger)
        .unwrap_or(0)
}

impl ServerPlugin for LuaPlugin {
    fn id(&self) -> &str {
        &self.id
    }
    fn version(&self) -> &str {
        &self.version
    }
    fn handles(&self, trigger: Trigger) -> bool {
        self.handled[trigger_index(trigger)]
    }
    fn drain_log(&mut self) -> Vec<String> {
        self.script.sandbox.drain_log()
    }
    fn on_event(
        &mut self,
        world: &World,
        event: HookEvent,
        commands: &mut Commands,
    ) -> Result<(), String> {
        if world.tick != self.tick {
            self.tick = world.tick;
            self.tick_used = 0;
        }
        let remaining = TICK_INSTRUCTIONS.saturating_sub(self.tick_used);
        if remaining == 0 {
            return Err("script exceeded its per-tick instruction budget".into());
        }
        let budget = self.script.sandbox.budget.clone();
        budget.begin(remaining.min(CALL_INSTRUCTIONS));
        let lua = &self.script.sandbox.lua;
        let native = &self.script.native;
        let dispatch = &self.dispatch;
        let result = lua.scope(|scope| {
            let read = scope.create_function(
                move |lua, (op, a, b, c): (String, Value, Value, Value)| {
                    read(lua, world, &op, &a, &b, &c)
                },
            )?;
            let emit =
                scope.create_function_mut(|lua, (op, args): (String, Variadic<Value>)| {
                    let (actor, effect) = emit(lua, world, &op, &args)?;
                    commands
                        .push(actor, effect)
                        .map_err(mlua::Error::RuntimeError)
                })?;
            native.raw_set("read", read)?;
            native.raw_set("emit", emit)?;
            let event_table = lua.create_table_with_capacity(0, 7)?;
            event_table.raw_set("type", event.trigger.name())?;
            event_table.raw_set("actor", lua_id(event.actor))?;
            event_table.raw_set("target", lua_id(event.target))?;
            event_table.raw_set("value", event.value)?;
            event_table.raw_set("x", event.position.x)?;
            event_table.raw_set("y", event.position.y)?;
            event_table.raw_set("tick", event.tick as f64)?;
            let outcome = dispatch.call::<()>((event.trigger.name(), event_table));
            native.raw_set("read", Value::Nil)?;
            native.raw_set("emit", Value::Nil)?;
            outcome
        });
        self.tick_used = self.tick_used.saturating_add(budget.used());
        if let Some(reason) = budget.reason() {
            return Err(reason);
        }
        result.map_err(|e| clean_error(&e))
    }
}

/// Entity IDs are exact in Lua numbers; reserved owners read as zero.
fn lua_id(id: EntityId) -> f64 {
    if id >= (1 << 53) { 0. } else { id as f64 }
}

fn clean_error(error: &mlua::Error) -> String {
    let mut text = match error {
        mlua::Error::CallbackError { cause, .. } => return clean_error(cause),
        mlua::Error::WithContext { cause, .. } => return clean_error(cause),
        other => other.to_string(),
    };
    if let Some(at) = text.find("\nstack traceback:") {
        text.truncate(at);
    }
    let text: String = text
        .replace("runtime error: ", "")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(400)
        .collect();
    text.replace('\n', " ")
}

fn rt(message: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(message.into())
}

fn number(value: Option<&Value>, what: &str) -> mlua::Result<f64> {
    match value {
        Some(Value::Number(n)) if n.is_finite() => Ok(*n),
        Some(Value::Integer(n)) => Ok(*n as f64),
        _ => Err(rt(format!("{what} must be a finite number"))),
    }
}
fn optional_number(value: Option<&Value>, what: &str) -> mlua::Result<Option<f64>> {
    match value {
        None | Some(Value::Nil) => Ok(None),
        value => number(value, what).map(Some),
    }
}
fn entity(value: Option<&Value>) -> mlua::Result<EntityId> {
    let id = number(value, "tank id")?;
    if id < 0. || id.fract() != 0. || id >= (1_u64 << 53) as f64 {
        return Err(rt("tank id must be a whole number"));
    }
    Ok(id as EntityId)
}
fn count(value: Option<&Value>, what: &str, limit: f64) -> mlua::Result<u32> {
    let n = number(value, what)?;
    if !(0. ..=limit).contains(&n) {
        return Err(rt(format!("{what} must be between 0 and {limit}")));
    }
    Ok(n.floor() as u32)
}
fn text(value: Option<&Value>, limit: usize) -> String {
    let raw = match value {
        Some(Value::String(s)) => s.to_string_lossy(),
        _ => String::new(),
    };
    let clean: String = raw.chars().filter(|c| !c.is_control()).collect();
    let mut out = String::new();
    for c in clean.trim().chars() {
        if out.len() + c.len_utf8() > limit {
            break;
        }
        out.push(c);
    }
    if out.is_empty() { "…".into() } else { out }
}

fn normalized(world: &World, x: f64, y: f64) -> mlua::Result<[f32; 2]> {
    let position = [
        (x / f64::from(world.config.width)) as f32,
        (y / f64::from(world.config.height)) as f32,
    ];
    if position.iter().all(|v| (0.0..=1.0).contains(v)) {
        Ok(position)
    } else {
        Err(rt("position is outside the arena"))
    }
}

fn find_tank(world: &World, id: EntityId) -> Option<&Tank> {
    world.tanks.iter().find(|tank| tank.id == id)
}

fn emit(lua: &Lua, world: &World, op: &str, args: &[Value]) -> mlua::Result<(EntityId, Effect)> {
    let _ = lua;
    let arg = |index: usize| args.get(index);
    let f = |index: usize, what: &str| number(arg(index), what).map(|v| v as f32);
    Ok(match op {
        "heal" => (
            entity(arg(0))?,
            Effect::Heal {
                amount: f(1, "amount")?,
            },
        ),
        "damage" => (
            entity(arg(0))?,
            Effect::Damage {
                amount: f(1, "amount")?,
            },
        ),
        "kill" => (entity(arg(0))?, Effect::Kill),
        "award" => (
            entity(arg(0))?,
            Effect::Award {
                xp: count(arg(1), "xp", 1_000_000.)?,
                score: count(arg(2), "score", 1_000_000.)?,
            },
        ),
        "set_class" => (
            entity(arg(0))?,
            Effect::SetClass {
                class: text(arg(1), 48),
            },
        ),
        "set_team" => (
            entity(arg(0))?,
            Effect::SetTeam {
                team: count(arg(1), "team", 4.)? as u8,
            },
        ),
        "set_level" => (
            entity(arg(0))?,
            Effect::SetLevel {
                level: count(arg(1), "level", 255.)? as u8,
            },
        ),
        "raise_stat" => {
            let stat = match arg(1) {
                Some(Value::String(name)) => {
                    let name = name.to_string_lossy();
                    STAT_NAMES
                        .iter()
                        .position(|candidate| *candidate == name)
                        .ok_or_else(|| rt(format!("unknown stat {name:?}")))?
                }
                value => {
                    let index = count(value, "stat", 8.)? as usize;
                    if index == 0 {
                        return Err(rt("stats are numbered 1 to 8"));
                    }
                    index - 1
                }
            };
            (entity(arg(0))?, Effect::RaiseStat { stat: stat as u8 })
        }
        "set_scales" => {
            let id = entity(arg(0))?;
            let tank = find_tank(world, id);
            let current = |value: Option<f64>, fallback: f32| value.map_or(fallback, |v| v as f32);
            (
                id,
                Effect::SetScales {
                    speed: current(
                        optional_number(arg(1), "speed")?,
                        tank.map_or(1., |t| t.speed_scale),
                    ),
                    reload: current(
                        optional_number(arg(2), "reload")?,
                        tank.map_or(1., |t| t.reload_scale),
                    ),
                    damage: current(
                        optional_number(arg(3), "damage")?,
                        tank.map_or(1., |t| t.damage_scale),
                    ),
                },
            )
        }
        "set_armor" => (
            entity(arg(0))?,
            Effect::SetArmor {
                multiplier: f(1, "multiplier")?,
            },
        ),
        "set_shield" => (
            entity(arg(0))?,
            Effect::SetShield {
                seconds: f(1, "seconds")?,
            },
        ),
        "set_protection" => (
            entity(arg(0))?,
            Effect::SetProtection {
                seconds: f(1, "seconds")?,
            },
        ),
        "push" => (
            entity(arg(0))?,
            Effect::Impulse {
                velocity: [f(1, "x velocity")?, f(2, "y velocity")?],
            },
        ),
        "set_velocity" => (
            entity(arg(0))?,
            Effect::SetVelocity {
                velocity: [f(1, "x velocity")?, f(2, "y velocity")?],
            },
        ),
        "teleport" => (
            entity(arg(0))?,
            Effect::Teleport {
                position: normalized(world, number(arg(1), "x")?, number(arg(2), "y")?)?,
            },
        ),
        "fire" => (
            entity(arg(0))?,
            Effect::Fire {
                speed: f(1, "speed")?,
                damage: f(2, "damage")?,
                radius: f(3, "radius")?,
                lifetime: f(4, "lifetime")?,
                angle: f(5, "angle")?,
            },
        ),
        "control" => {
            let id = entity(arg(0))?;
            let movement = tankyard_core::V2::new(f(1, "x")?, f(2, "y")?).clamp_length(1.);
            let aim = match optional_number(arg(3), "aim")? {
                Some(aim) => aim as f32,
                None => find_tank(world, id).map_or(0., |t| t.aim),
            };
            let flag = |index: usize| matches!(arg(index), Some(Value::Boolean(true)));
            (
                id,
                Effect::Control {
                    movement: [movement.x, movement.y],
                    aim: (aim + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI,
                    fire: flag(4),
                    ability: flag(5),
                },
            )
        }
        "spawn_shape" => (
            0,
            Effect::SpawnTarget {
                position: normalized(world, number(arg(0), "x")?, number(arg(1), "y")?)?,
                sides: count(arg(2), "sides", 12.)? as u8,
                health: f(3, "health")?,
                xp: count(arg(4), "xp", 1_000_000.)?,
            },
        ),
        "spawn_boss" => (
            0,
            Effect::SpawnBoss {
                boss: text(arg(0), 48),
            },
        ),
        "clear_projectiles" => (
            match arg(0) {
                Some(Value::Nil) | None => 0,
                value => entity(value)?,
            },
            Effect::ClearProjectiles,
        ),
        "announce" => (
            0,
            Effect::Announce {
                message: text(arg(0), 160),
            },
        ),
        "finish_round" => {
            let team = count(arg(0), "team", 4.)? as u8;
            (
                0,
                Effect::FinishRound {
                    team: (team > 0).then_some(team),
                    reason: text(arg(1), 160),
                },
            )
        }
        "set_score" => (
            entity(arg(0))?,
            Effect::SetScore {
                score: count(arg(1), "score", 1_000_000_000.)?,
            },
        ),
        "set_team_score" => (
            0,
            Effect::SetTeamScore {
                team: count(arg(0), "team", 4.)? as u8,
                score: count(arg(1), "score", 1_000_000.)?,
            },
        ),
        _ => return Err(rt(format!("unknown command {op:?}"))),
    })
}

fn tank_table(lua: &Lua, world: &World, tank: &Tank) -> mlua::Result<Table> {
    let t = lua.create_table_with_capacity(0, 28)?;
    t.raw_set("id", lua_id(tank.id))?;
    t.raw_set("name", tank.name.as_str())?;
    t.raw_set("class", tank.class.as_str())?;
    t.raw_set("team", tank.team)?;
    t.raw_set("x", tank.pos.x)?;
    t.raw_set("y", tank.pos.y)?;
    t.raw_set("vx", tank.vel.x)?;
    t.raw_set("vy", tank.vel.y)?;
    t.raw_set("aim", tank.aim)?;
    t.raw_set("radius", tank.radius)?;
    t.raw_set("health", tank.health)?;
    t.raw_set("max_health", tank.max_health)?;
    t.raw_set("level", tank.level)?;
    t.raw_set("xp", tank.xp)?;
    t.raw_set("score", tank.score)?;
    t.raw_set("kills", tank.kills)?;
    t.raw_set("deaths", tank.deaths)?;
    t.raw_set("alive", tank.alive)?;
    t.raw_set("bot", tank.bot)?;
    t.raw_set("points", tank.points)?;
    t.raw_set("shield", tank.shield)?;
    t.raw_set("protection", tank.protection)?;
    t.raw_set("speed_scale", tank.speed_scale)?;
    t.raw_set("reload_scale", tank.reload_scale)?;
    t.raw_set("damage_scale", tank.damage_scale)?;
    t.raw_set(
        "in_base",
        world
            .config
            .base_protected(tank.team, tank.pos, tank.radius),
    )?;
    let stats = lua.create_table_with_capacity(8, 0)?;
    for (index, value) in tank.stats.iter().enumerate() {
        stats.raw_set(index + 1, *value)?;
    }
    t.raw_set("stats", stats)?;
    Ok(t)
}

fn rect_table(lua: &Lua, rect: tankyard_core::Rect) -> mlua::Result<Table> {
    let t = lua.create_table_with_capacity(0, 4)?;
    t.raw_set("x", rect.x)?;
    t.raw_set("y", rect.y)?;
    t.raw_set("w", rect.w)?;
    t.raw_set("h", rect.h)?;
    Ok(t)
}

fn read(
    lua: &Lua,
    world: &World,
    op: &str,
    a: &Value,
    b: &Value,
    c: &Value,
) -> mlua::Result<Value> {
    let list = |capacity: usize| lua.create_table_with_capacity(capacity.min(4096), 0);
    Ok(match op {
        "tick" => Value::Number(world.tick as f64),
        "time" => Value::Number(f64::from(world.match_state.elapsed)),
        "arena" => {
            let t = lua.create_table()?;
            t.raw_set("width", world.config.width)?;
            t.raw_set("height", world.config.height)?;
            t.raw_set("mode", world.config.mode.name())?;
            t.raw_set("mode_key", format!("{:?}", world.config.mode))?;
            t.raw_set("teams", world.config.mode.teams())?;
            t.raw_set("map", world.config.map.as_str())?;
            t.raw_set("round", world.match_state.round)?;
            t.raw_set("elapsed", world.match_state.elapsed)?;
            t.raw_set("finished", world.match_state.finished)?;
            t.raw_set("winner", world.match_state.winner)?;
            let scores = list(4)?;
            for (index, score) in world.match_state.scores.iter().enumerate() {
                scores.raw_set(index + 1, *score)?;
            }
            t.raw_set("scores", scores)?;
            t.raw_set("bounds", rect_table(lua, world.config.bounds())?)?;
            Value::Table(t)
        }
        "tanks" | "players" => {
            let humans = op == "players";
            let t = list(world.tanks.len())?;
            let mut index = 0;
            for tank in world.tanks.iter().filter(|tank| !humans || !tank.bot) {
                index += 1;
                t.raw_set(index, tank_table(lua, world, tank)?)?;
            }
            Value::Table(t)
        }
        "tank" => {
            let id = entity(Some(a))?;
            match find_tank(world, id) {
                Some(tank) => Value::Table(tank_table(lua, world, tank)?),
                None => Value::Nil,
            }
        }
        "bosses" => {
            let t = list(world.bosses.len())?;
            for (index, boss) in world.bosses.iter().enumerate() {
                let b = lua.create_table()?;
                b.raw_set("id", lua_id(boss.id))?;
                b.raw_set("kind", boss.kind.name())?;
                b.raw_set("x", boss.pos.x)?;
                b.raw_set("y", boss.pos.y)?;
                b.raw_set("radius", boss.radius)?;
                b.raw_set("health", boss.health)?;
                b.raw_set("max_health", boss.max_health)?;
                b.raw_set("team", boss.team)?;
                t.raw_set(index + 1, b)?;
            }
            Value::Table(t)
        }
        "walls" => {
            let t = list(world.walls.len())?;
            for (index, wall) in world.walls.iter().take(4096).enumerate() {
                t.raw_set(index + 1, rect_table(lua, *wall)?)?;
            }
            Value::Table(t)
        }
        "objectives" => {
            let t = list(world.objectives.len())?;
            for (index, objective) in world.objectives.iter().enumerate() {
                let o = lua.create_table()?;
                o.raw_set("id", lua_id(objective.id))?;
                o.raw_set("x", objective.pos.x)?;
                o.raw_set("y", objective.pos.y)?;
                o.raw_set("radius", objective.radius)?;
                o.raw_set("team", objective.team)?;
                o.raw_set("progress", objective.progress)?;
                o.raw_set("contested", objective.contested)?;
                o.raw_set("carrier", objective.carrier.map_or(0., lua_id))?;
                t.raw_set(index + 1, o)?;
            }
            Value::Table(t)
        }
        "classes" => {
            let t = list(world.content.classes.len())?;
            for (index, class) in world.content.classes.iter().enumerate() {
                t.raw_set(index + 1, class.id.as_str())?;
            }
            Value::Table(t)
        }
        "class" => {
            let Value::String(id) = a else {
                return Err(rt("class id must be a string"));
            };
            let id = id.to_string_lossy();
            match world.content.class(&id) {
                Some(class) => {
                    let t = lua.create_table()?;
                    t.raw_set("id", class.id.as_str())?;
                    t.raw_set("name", class.name.as_str())?;
                    t.raw_set("tier", class.tier)?;
                    t.raw_set("level", class.min_level)?;
                    t.raw_set("ability", class.ability.name())?;
                    t.raw_set("description", class.description.as_str())?;
                    t.raw_set("drones", class.has_drones())?;
                    let upgrades = list(class.upgrades.len())?;
                    for (index, next) in class.upgrades.iter().enumerate() {
                        upgrades.raw_set(index + 1, next.as_str())?;
                    }
                    t.raw_set("upgrades", upgrades)?;
                    let parents = list(class.parents.len())?;
                    for (index, parent) in class.parents.iter().enumerate() {
                        parents.raw_set(index + 1, parent.as_str())?;
                    }
                    t.raw_set("parents", parents)?;
                    Value::Table(t)
                }
                None => Value::Nil,
            }
        }
        "base" => {
            let team = count(Some(a), "team", 4.)? as u8;
            match world.config.team_base(team) {
                Some(base) => Value::Table(rect_table(lua, base)?),
                None => Value::Nil,
            }
        }
        "shapes" | "projectiles" => {
            let (x, y, radius) = (
                number(Some(a), "x")? as f32,
                number(Some(b), "y")? as f32,
                number(Some(c), "radius")? as f32,
            );
            let center = tankyard_core::V2::new(x, y);
            let reach = radius.clamp(0., 5000.).powi(2);
            let t = list(32)?;
            let mut index = 0;
            if op == "shapes" {
                for shape in world
                    .targets
                    .iter()
                    .filter(|s| s.health > 0. && (s.pos - center).length_squared() <= reach)
                    .take(MAX_QUERY_RESULTS)
                {
                    index += 1;
                    let s = lua.create_table()?;
                    s.raw_set("id", lua_id(shape.id))?;
                    s.raw_set("x", shape.pos.x)?;
                    s.raw_set("y", shape.pos.y)?;
                    s.raw_set("radius", shape.radius)?;
                    s.raw_set("health", shape.health)?;
                    s.raw_set("max_health", shape.max_health)?;
                    s.raw_set("sides", tankyard_core::target_sides(shape.kind))?;
                    s.raw_set("xp", shape.xp)?;
                    t.raw_set(index, s)?;
                }
            } else {
                for shot in world
                    .projectiles
                    .iter()
                    .filter(|p| p.lifetime > 0. && (p.pos - center).length_squared() <= reach)
                    .take(MAX_QUERY_RESULTS)
                {
                    index += 1;
                    let p = lua.create_table()?;
                    p.raw_set("id", lua_id(shot.id))?;
                    p.raw_set("owner", lua_id(shot.owner))?;
                    p.raw_set("guardian", base_guard_team(shot.owner).is_some())?;
                    p.raw_set("team", shot.team)?;
                    p.raw_set("kind", format!("{:?}", shot.kind))?;
                    p.raw_set("x", shot.pos.x)?;
                    p.raw_set("y", shot.pos.y)?;
                    p.raw_set("vx", shot.vel.x)?;
                    p.raw_set("vy", shot.vel.y)?;
                    p.raw_set("radius", shot.radius)?;
                    p.raw_set("damage", shot.damage)?;
                    p.raw_set("lifetime", shot.lifetime)?;
                    t.raw_set(index, p)?;
                }
            }
            Value::Table(t)
        }
        _ => return Err(rt(format!("unknown query {op:?}"))),
    })
}

#[cfg(test)]
mod tests;

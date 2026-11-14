//! Arena Workshop plugins: Luau tools and checks for the map editor.
//!
//! A pack lists its workshop scripts under `editor` in `mod.json`. They run
//! in the same sandbox as behavior scripts (computation only, with step and
//! memory budgets) and only ever see a copy of the map being edited. A tool
//! hands back a whole map, which the editor validates and applies as one
//! undo step, so a failing or misbehaving tool changes nothing.
use crate::{clean_error, sandbox, sandbox::Sandbox};
use mlua::{Function, LuaSerdeExt, Table, Value, serde::SerializeOptions};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tankyard_content::MapDef;

/// Steps one tool run, or one pack's checks together, may take.
pub const TOOL_INSTRUCTIONS: u64 = 20_000_000;
const MAX_PARAMS: usize = 12;
const MAX_FINDINGS: usize = 64;
const PRELUDE: &str = include_str!("editor_prelude.lua");

/// The map lists a plugin names objects by, as in `mod.json`.
pub const LISTS: [&str; 8] = [
    "walls",
    "blocks",
    "zones",
    "decor",
    "triggers",
    "spawn_regions",
    "team_bases",
    "objective_positions",
];

/// An object as a plugin names it: one of [`LISTS`] and a zero-based index.
pub type ObjectRef = (&'static str, usize);

/// A control a tool asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Param {
    Number {
        min: f32,
        max: f32,
        step: f32,
        default: f32,
    },
    Integer {
        min: i32,
        max: i32,
        default: i32,
    },
    Toggle(bool),
    Choice {
        options: Vec<String>,
        default: usize,
    },
    Text {
        limit: usize,
        default: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamSpec {
    pub id: String,
    pub name: String,
    pub kind: Param,
}
impl ParamSpec {
    pub fn default_value(&self) -> ParamValue {
        match &self.kind {
            Param::Number { default, .. } => ParamValue::Number(*default),
            Param::Integer { default, .. } => ParamValue::Integer(*default),
            Param::Toggle(default) => ParamValue::Toggle(*default),
            Param::Choice { default, .. } => ParamValue::Choice(*default),
            Param::Text { default, .. } => ParamValue::Text(default.clone()),
        }
    }
}

/// What the author set a control to.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Number(f32),
    Integer(i32),
    Toggle(bool),
    Choice(usize),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub description: String,
    pub params: Vec<ParamSpec>,
}

/// What the editor tells a tool about the moment it runs.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub selection: Vec<ObjectRef>,
    /// The pointer in world units, when it is over the canvas.
    pub pointer: Option<[f32; 2]>,
    pub grid: f32,
    /// The part of the arena the canvas shows: x, y, width and height in
    /// world units.
    pub view: Option<[f32; 4]>,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub map: MapDef,
    /// The objects the tool picked out, if it chose any.
    pub selection: Option<Vec<ObjectRef>>,
    pub messages: Vec<String>,
    /// Lines the tool printed.
    pub log: Vec<String>,
}

/// Something a plugin check noticed.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub text: String,
    pub object: Option<ObjectRef>,
}

pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub tools: Vec<Tool>,
    pub checks: Vec<String>,
    /// Why the plugin stopped, after a runaway script.
    pub stopped: Option<String>,
    sandbox: Sandbox,
    host: Table,
}

/// Every workshop plugin from a set of packs. Packs without editor scripts
/// are skipped; a pack that fails to load is reported and left out.
#[derive(Default)]
pub struct Plugins {
    pub plugins: Vec<Plugin>,
    pub errors: Vec<String>,
}

impl Plugins {
    pub fn load(paths: &[PathBuf]) -> Self {
        let mut plugins = Self::default();
        for path in paths {
            match Plugin::load(path) {
                Ok(Some(plugin)) => plugins.plugins.push(plugin),
                Ok(None) => {}
                Err(error) => plugins.errors.push(error),
            }
        }
        plugins
    }

    /// Run tool `tool` of plugin `plugin` on a copy of `map`.
    pub fn run(
        &mut self,
        plugin: usize,
        tool: usize,
        map: &MapDef,
        params: &[ParamValue],
        context: &Context,
    ) -> Result<Outcome, String> {
        let plugin = self.plugins.get_mut(plugin).ok_or("no such plugin")?;
        plugin.run(tool, map, params, context)
    }

    /// Every plugin's checks on `map`, by plugin name. A check that fails
    /// reports its error as a finding.
    pub fn check(&mut self, map: &MapDef) -> Vec<(String, Finding)> {
        let mut found = Vec::new();
        for plugin in &mut self.plugins {
            for finding in plugin.check(map) {
                found.push((plugin.name.clone(), finding));
            }
        }
        found
    }
}

impl Plugin {
    fn load(path: &std::path::Path) -> Result<Option<Self>, String> {
        let (manifest, pack) = tankyard_mods::read_pack(path)?;
        if manifest.editor.is_empty() {
            return Ok(None);
        }
        let id = manifest.id.clone();
        let context = |e: mlua::Error| format!("{id}: {}", clean_error(&e));
        let mut sources = BTreeMap::new();
        for (name, data) in pack.files() {
            if name.ends_with(".lua") {
                let text = std::str::from_utf8(data)
                    .map_err(|_| format!("{id}: {name}: Lua files must be UTF-8 text"))?;
                sources.insert(name.to_owned(), text.to_owned());
            }
        }
        let globals = sandbox::compile_globals(sources.values().map(String::as_str));
        let seed = tankyard_mods::digest(id.as_bytes());
        let sandbox = Sandbox::new(seed, &globals, false).map_err(|e| format!("{id}: {e}"))?;
        sandbox
            .install_require(Arc::new(sources.clone()))
            .map_err(context)?;
        sandbox.budget.begin(crate::LOAD_INSTRUCTIONS);
        let host: Table = sandbox
            .host_chunk("=editor", PRELUDE)
            .set_environment(sandbox.lua.globals())
            .call(())
            .map_err(context)?;
        for entry in &manifest.editor {
            let source = sources
                .get(entry)
                .ok_or_else(|| format!("{id}: missing script {entry}"))?;
            sandbox.run(entry, source).map_err(context)?;
        }
        let tools = host
            .get::<Function>("tools")
            .and_then(|f| f.call::<Table>(()))
            .and_then(|list| {
                list.sequence_values::<Table>()
                    .map(|spec| read_tool(&spec?))
                    .collect::<mlua::Result<Vec<Tool>>>()
            })
            .map_err(context)?;
        let checks = host
            .get::<Function>("checks")
            .and_then(|f| f.call::<Table>(()))
            .and_then(|list| {
                list.sequence_values::<Table>()
                    .map(|spec| spec?.get::<String>("name"))
                    .collect::<mlua::Result<Vec<String>>>()
            })
            .map_err(context)?;
        if tools.is_empty() && checks.is_empty() {
            return Err(format!("{id}: its editor scripts add no tools or checks"));
        }
        Ok(Some(Self {
            id,
            name: manifest.name,
            version: manifest.version,
            tools,
            checks,
            stopped: None,
            sandbox,
            host,
        }))
    }

    fn map_value(&self, map: &MapDef) -> mlua::Result<Value> {
        // Missing optional settings read as nil, as they would in mod.json.
        self.sandbox
            .lua
            .to_value_with(map, SerializeOptions::new().serialize_none_to_null(false))
    }

    fn halted(&mut self, error: mlua::Error) -> String {
        let text = clean_error(&error);
        if self.sandbox.budget.poisoned() {
            let reason = self
                .sandbox
                .budget
                .reason()
                .unwrap_or_else(|| "it exceeded its budget".into());
            self.stopped = Some(reason.clone());
            return format!(
                "{} stopped: {reason}. Reload plugins to try again.",
                self.name
            );
        }
        text
    }

    fn run(
        &mut self,
        index: usize,
        map: &MapDef,
        params: &[ParamValue],
        context: &Context,
    ) -> Result<Outcome, String> {
        if let Some(reason) = &self.stopped {
            return Err(format!(
                "{} stopped: {reason}. Reload plugins to try again.",
                self.name
            ));
        }
        let tool = self.tools.get(index).ok_or("no such tool")?.clone();
        self.sandbox.budget.begin(TOOL_INSTRUCTIONS);
        let result = self.call_tool(index, &tool, map, params, context);
        let log = self.sandbox.drain_log();
        match result {
            Ok((map, selection, messages)) => Ok(Outcome {
                map,
                selection,
                messages,
                log,
            }),
            Err(error) => Err(format!("{}: {}", tool.name, self.halted(error))),
        }
    }

    #[allow(clippy::type_complexity)]
    fn call_tool(
        &self,
        index: usize,
        tool: &Tool,
        map: &MapDef,
        params: &[ParamValue],
        context: &Context,
    ) -> mlua::Result<(MapDef, Option<Vec<ObjectRef>>, Vec<String>)> {
        let lua = &self.sandbox.lua;
        let values = lua.create_table()?;
        for (spec, value) in tool.params.iter().zip(params) {
            let value = match (&spec.kind, value) {
                (Param::Number { .. }, ParamValue::Number(v)) => Value::Number(f64::from(*v)),
                (Param::Integer { .. }, ParamValue::Integer(v)) => Value::Integer((*v).into()),
                (Param::Toggle(_), ParamValue::Toggle(v)) => Value::Boolean(*v),
                (Param::Choice { options, .. }, ParamValue::Choice(v)) => {
                    Value::String(lua.create_string(options.get(*v).map_or("", String::as_str))?)
                }
                (Param::Text { .. }, ParamValue::Text(v)) => Value::String(lua.create_string(v)?),
                _ => spec_default(lua, spec)?,
            };
            values.raw_set(spec.id.as_str(), value)?;
        }
        let ctx = lua.create_table()?;
        let selection = lua.create_table()?;
        for &(list, at) in &context.selection {
            let entry = lua.create_table()?;
            entry.raw_set("list", list)?;
            entry.raw_set("index", at + 1)?;
            selection.raw_push(entry)?;
        }
        ctx.raw_set("selection", selection)?;
        if let Some([x, y]) = context.pointer {
            let pointer = lua.create_table()?;
            pointer.raw_set("x", x)?;
            pointer.raw_set("y", y)?;
            ctx.raw_set("pointer", pointer)?;
        }
        ctx.raw_set("grid", context.grid)?;
        if let Some([x, y, w, h]) = context.view {
            let view = lua.create_table()?;
            view.raw_set("x", x)?;
            view.raw_set("y", y)?;
            view.raw_set("w", w)?;
            view.raw_set("h", h)?;
            ctx.raw_set("view", view)?;
        }
        let run: Function = self.host.get("run")?;
        let (map, chosen, messages): (Value, Value, Table) =
            run.call((index + 1, self.map_value(map)?, values, ctx))?;
        let map: MapDef = lua.from_value(map)?;
        let selection = match chosen {
            Value::Table(list) => Some(
                list.sequence_values::<Table>()
                    .filter_map(|entry| {
                        let entry = entry.ok()?;
                        let list: String = entry.get("list").ok()?;
                        let at: i64 = entry.get("index").ok()?;
                        let list = LISTS.into_iter().find(|name| *name == list)?;
                        Some((list, usize::try_from(at.checked_sub(1)?).ok()?))
                    })
                    .collect(),
            ),
            _ => None,
        };
        let messages = messages
            .sequence_values::<String>()
            .filter_map(Result::ok)
            .map(|text| one_line(&text, 160))
            .filter(|text| !text.is_empty())
            .collect();
        Ok((map, selection, messages))
    }

    fn check(&mut self, map: &MapDef) -> Vec<Finding> {
        if self.checks.is_empty() || self.stopped.is_some() {
            return Vec::new();
        }
        self.sandbox.budget.begin(TOOL_INSTRUCTIONS);
        let mut found = Vec::new();
        for index in 0..self.checks.len() {
            let name = self.checks[index].clone();
            match self.call_check(index, map) {
                Ok(list) => found.extend(list),
                Err(error) => {
                    let text = self.halted(error);
                    found.push(Finding {
                        text: format!("Check {name} failed: {text}"),
                        object: None,
                    });
                    if self.stopped.is_some() {
                        break;
                    }
                }
            }
        }
        self.sandbox.drain_log();
        found.truncate(MAX_FINDINGS);
        found
    }

    fn call_check(&self, index: usize, map: &MapDef) -> mlua::Result<Vec<Finding>> {
        let check: Function = self.host.get("check")?;
        let result: Value = check.call((index + 1, self.map_value(map)?))?;
        let Value::Table(list) = result else {
            return Ok(Vec::new());
        };
        let mut found = Vec::new();
        for entry in list.sequence_values::<Value>().take(MAX_FINDINGS) {
            let finding = match entry? {
                Value::String(text) => Finding {
                    text: one_line(&text.to_string_lossy(), 200),
                    object: None,
                },
                Value::Table(entry) => {
                    let text: String = entry.get("text")?;
                    let list: Option<String> = entry.get("list")?;
                    let at: Option<i64> = entry.get("index")?;
                    let object = list
                        .and_then(|list| LISTS.into_iter().find(|name| *name == list))
                        .zip(at.and_then(|at| usize::try_from(at.checked_sub(1)?).ok()));
                    Finding {
                        text: one_line(&text, 200),
                        object,
                    }
                }
                _ => continue,
            };
            if !finding.text.is_empty() {
                found.push(finding);
            }
        }
        Ok(found)
    }
}

fn one_line(text: &str, limit: usize) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(limit)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn spec_default(lua: &mlua::Lua, spec: &ParamSpec) -> mlua::Result<Value> {
    Ok(match spec.default_value() {
        ParamValue::Number(v) => Value::Number(f64::from(v)),
        ParamValue::Integer(v) => Value::Integer(v.into()),
        ParamValue::Toggle(v) => Value::Boolean(v),
        ParamValue::Choice(v) => match &spec.kind {
            Param::Choice { options, .. } => Value::String(lua.create_string(&options[v])?),
            _ => Value::Nil,
        },
        ParamValue::Text(v) => Value::String(lua.create_string(v)?),
    })
}

fn read_tool(spec: &Table) -> mlua::Result<Tool> {
    let id: String = spec.get("id")?;
    let fail = |what: String| mlua::Error::RuntimeError(format!("tool {id}: {what}"));
    let mut params = Vec::new();
    if let Some(list) = spec.get::<Option<Table>>("params")? {
        for entry in list.sequence_values::<Table>() {
            if params.len() == MAX_PARAMS {
                return Err(fail(format!("at most {MAX_PARAMS} params")));
            }
            params.push(read_param(&entry?).map_err(|e| fail(clean_error(&e)))?);
        }
    }
    let mut ids: Vec<&str> = params.iter().map(|p: &ParamSpec| p.id.as_str()).collect();
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(fail("two params share an id".into()));
    }
    Ok(Tool {
        name: spec.get("name")?,
        description: spec
            .get::<Option<String>>("description")?
            .unwrap_or_default(),
        id,
        params,
    })
}

fn read_param(entry: &Table) -> mlua::Result<ParamSpec> {
    let id: String = entry.get("id")?;
    let name: String = entry
        .get::<Option<String>>("name")?
        .unwrap_or_else(|| id.clone());
    let fail = |what: &str| mlua::Error::RuntimeError(format!("param {id}: {what}"));
    if id.is_empty() || id.len() > 32 || name.is_empty() || name.chars().count() > 40 {
        return Err(fail(
            "needs an id of up to 32 characters and a name of up to 40",
        ));
    }
    let kind: String = entry
        .get::<Option<String>>("type")?
        .unwrap_or_else(|| "number".into());
    let finite = |key: &str, fallback: f64| -> mlua::Result<f64> {
        let value = entry.get::<Option<f64>>(key)?.unwrap_or(fallback);
        if value.is_finite() && value.abs() <= 1e7 {
            Ok(value)
        } else {
            Err(fail(&format!("{key} must be a number within ±10,000,000")))
        }
    };
    let kind = match kind.as_str() {
        "number" | "integer" => {
            let (min, max) = (finite("min", 0.)?, finite("max", 100.)?);
            if min >= max {
                return Err(fail("min must be below max"));
            }
            let default = finite("default", min)?.clamp(min, max);
            if kind == "integer" {
                Param::Integer {
                    min: min.ceil() as i32,
                    max: max.floor() as i32,
                    default: default.round() as i32,
                }
            } else {
                let step = finite("step", ((max - min) / 100.).max(0.01))?;
                if step <= 0. {
                    return Err(fail("step must be above zero"));
                }
                Param::Number {
                    min: min as f32,
                    max: max as f32,
                    step: step as f32,
                    default: default as f32,
                }
            }
        }
        "toggle" => Param::Toggle(entry.get::<Option<bool>>("default")?.unwrap_or(false)),
        "choice" => {
            let options: Vec<String> = entry.get("options")?;
            if options.is_empty()
                || options.len() > 24
                || options
                    .iter()
                    .any(|o| o.is_empty() || o.chars().count() > 32)
            {
                return Err(fail("needs 1 to 24 options of up to 32 characters"));
            }
            let chosen: Option<String> = entry.get("default")?;
            let default = chosen
                .and_then(|chosen| options.iter().position(|o| *o == chosen))
                .unwrap_or(0);
            Param::Choice { options, default }
        }
        "text" => {
            let limit = finite("limit", 64.)?.clamp(1., 200.) as usize;
            let default: String = entry.get::<Option<String>>("default")?.unwrap_or_default();
            Param::Text {
                default: default.chars().take(limit).collect(),
                limit,
            }
        }
        other => return Err(fail(&format!("unknown type {other:?}"))),
    };
    Ok(ParamSpec { id, name, kind })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pack on disk with one editor script.
    fn plugin_pack(name: &str, script: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tankyard-editor-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("mod.json"),
            format!(
                r#"{{"api_version":2,"id":"{name}","name":"{name}","version":"1","editor":["tools.lua"]}}"#
            ),
        )
        .unwrap();
        std::fs::write(dir.join("tools.lua"), script).unwrap();
        dir
    }

    const ROW: &str = r#"
editor.tool {
  id = "row",
  name = "Row of blocks",
  params = {
    { id = "count", name = "Blocks", type = "integer", min = 1, max = 20, default = 3 },
    { id = "gap", name = "Gap", type = "number", min = 0, max = 400, step = 10, default = 100 },
    { id = "style", name = "Style", type = "choice", options = { "slate", "garden" }, default = "garden" },
  },
  run = function(map, params, ctx)
    local x = ctx.pointer and ctx.pointer.x or 200
    for i = 1, params.count do
      table.insert(map.blocks, {
        rect = editor.rect(map, x + (i - 1) * params.gap, 300, 40, 40),
        style = params.style,
      })
      ctx.select("blocks", #map.blocks)
    end
    ctx.say(("Placed %d"):format(params.count))
  end,
}
editor.check {
  id = "lonely",
  name = "Lonely blocks",
  run = function(map)
    local found = {}
    for i, block in ipairs(map.blocks) do
      if block.style == "garden" then
        found[#found + 1] = { text = "A garden block", list = "blocks", index = i }
      end
    end
    return found
  end,
}
"#;

    #[test]
    fn tools_edit_a_copy_and_report_what_they_chose() {
        let mut plugins = Plugins::load(&[plugin_pack("rows", ROW)]);
        assert!(plugins.errors.is_empty(), "{:?}", plugins.errors);
        let tool = &plugins.plugins[0].tools[0];
        assert_eq!(tool.params.len(), 3);
        assert_eq!(tool.params[2].default_value(), ParamValue::Choice(1));
        let map = tankyard_content::builtin_maps().remove(0);
        let before = map.blocks.len();
        let params = [
            ParamValue::Integer(4),
            ParamValue::Number(80.),
            ParamValue::Choice(1),
        ];
        let context = Context {
            pointer: Some([500., 500.]),
            grid: 40.,
            ..Default::default()
        };
        let outcome = plugins.run(0, 0, &map, &params, &context).unwrap();
        assert_eq!(outcome.map.blocks.len(), before + 4);
        assert_eq!(map.blocks.len(), before, "the original is untouched");
        let added = &outcome.map.blocks[before + 1];
        assert!((added.rect[0] * map.width - 580.).abs() < 0.01);
        assert_eq!(added.style, tankyard_content::BlockStyle::Garden);
        let selection = outcome.selection.unwrap();
        assert_eq!(selection.first(), Some(&("blocks", before)));
        assert_eq!(selection.len(), 4);
        assert_eq!(outcome.messages, vec!["Placed 4".to_owned()]);
        let found = plugins.check(&outcome.map);
        assert_eq!(found.len(), 4);
        assert_eq!(found[0].1.object, Some(("blocks", before)));
    }

    #[test]
    fn free_groups_skip_moving_groups_and_their_watchers() {
        use tankyard_content::{MapTrigger, TriggerAction, TriggerWhen};
        const PICK: &str = r#"
editor.tool {
  id = "pick",
  name = "Pick a group",
  run = function(map, params, ctx)
    ctx.say(tostring(editor.free_group(map)))
  end,
}
"#;
        let mut plugins = Plugins::load(&[plugin_pack("pick", PICK)]);
        assert!(plugins.errors.is_empty(), "{:?}", plugins.errors);
        let mut map = tankyard_content::builtin_maps().remove(0);
        map.triggers = vec![
            MapTrigger {
                action: TriggerAction::Fade,
                group: 1,
                ..Default::default()
            },
            MapTrigger {
                when: TriggerWhen::Arrived,
                action: TriggerAction::Signal,
                source: 2,
                group: 7,
                ..Default::default()
            },
            MapTrigger {
                action: TriggerAction::Tint,
                group: 3,
                ..Default::default()
            },
            MapTrigger {
                action: TriggerAction::Stop,
                group: 4,
                ..Default::default()
            },
        ];
        let outcome = plugins.run(0, 0, &map, &[], &Context::default()).unwrap();
        assert_eq!(outcome.messages, vec!["5".to_owned()]);
    }

    #[test]
    fn broken_tools_and_runaway_loops_change_nothing() {
        let mut plugins = Plugins::load(&[plugin_pack(
            "broken",
            r#"
editor.tool { id = "bad", name = "Bad", run = function(map) map.width = "wide" end }
editor.tool { id = "spin", name = "Spin", run = function() while true do end end }
"#,
        )]);
        let map = tankyard_content::builtin_maps().remove(0);
        let error = plugins
            .run(0, 0, &map, &[], &Context::default())
            .unwrap_err();
        assert!(error.starts_with("Bad:"), "{error}");
        let error = plugins
            .run(0, 1, &map, &[], &Context::default())
            .unwrap_err();
        assert!(error.contains("stopped"), "{error}");
        assert!(plugins.run(0, 0, &map, &[], &Context::default()).is_err());
    }

    #[test]
    fn packs_without_editor_scripts_are_skipped_and_bad_specs_reported() {
        let plain =
            std::env::temp_dir().join(format!("tankyard-editor-plain-{}", std::process::id()));
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(
            plain.join("mod.json"),
            r#"{"api_version":1,"id":"plain","name":"Plain","version":"1"}"#,
        )
        .unwrap();
        let bad = plugin_pack(
            "bad_spec",
            r#"editor.tool { id = "x", name = "X", params = { { id = "n", type = "number", min = 5, max = 1 } }, run = function() end }"#,
        );
        let plugins = Plugins::load(&[plain, bad]);
        assert!(plugins.plugins.is_empty());
        assert_eq!(plugins.errors.len(), 1);
        assert!(
            plugins.errors[0].contains("min must be below max"),
            "{:?}",
            plugins.errors
        );
    }

    #[test]
    fn the_plugin_guide_example_runs_as_written() {
        let guide = include_str!("../../../docs/editor-plugins.md");
        let script = guide
            .split("```lua\n")
            .nth(1)
            .and_then(|rest| rest.split("```").next())
            .unwrap();
        let mut plugins = Plugins::load(&[plugin_pack("guide", script)]);
        assert!(plugins.errors.is_empty(), "{:?}", plugins.errors);
        let map = tankyard_content::builtin_maps().remove(0);
        let before = map.blocks.len();
        let params = [
            ParamValue::Integer(4),
            ParamValue::Number(10.),
            ParamValue::Number(40.),
            ParamValue::Choice(1),
        ];
        // Near the arena's far corner, the row is pulled back inside.
        let context = Context {
            pointer: Some([map.width - 5., map.height - 5.]),
            grid: 40.,
            ..Default::default()
        };
        let outcome = plugins.run(0, 0, &map, &params, &context).unwrap();
        assert_eq!(outcome.map.blocks.len(), before + 4);
        assert!(outcome.map.blocks[before..].iter().all(|block| {
            block.rect[0] >= 0.
                && block.rect[1] >= 0.
                && block.rect[0] + block.rect[2] <= 1.0001
                && block.rect[1] + block.rect[3] <= 1.0001
        }));
        assert_eq!(outcome.selection.unwrap().len(), 4);
        assert_eq!(outcome.messages, vec!["Placed 4 blocks.".to_owned()]);
        tankyard_content::validate_maps(std::slice::from_ref(&outcome.map)).unwrap();
        let found = plugins.check(&outcome.map);
        assert_eq!(found.len(), 4, "{found:?}");
        assert_eq!(found[0].1.object, Some(("blocks", before)));
    }

    #[test]
    fn bundled_toolkit_tools_keep_maps_valid() {
        let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods/workshop_toolkit");
        let mut plugins = Plugins::load(&[pack]);
        assert!(plugins.errors.is_empty(), "{:?}", plugins.errors);
        let mut map = tankyard_content::builtin_maps().remove(0);
        map.blocks.push(tankyard_content::MapBlock {
            rect: [0.2, 0.2, 0.03, 0.03],
            style: Default::default(),
            color: None,
            group: 0,
        });
        let block = map.blocks.len() - 1;
        let context = Context {
            selection: vec![("blocks", block)],
            pointer: Some([map.width * 0.5, map.height * 0.5]),
            grid: 40.,
            view: Some([0., 0., map.width, map.height]),
        };
        let tools = plugins.plugins[0].tools.clone();
        assert_eq!(tools.len(), 5);
        for (index, tool) in tools.iter().enumerate() {
            let values: Vec<ParamValue> =
                tool.params.iter().map(ParamSpec::default_value).collect();
            let outcome = plugins
                .run(0, index, &map, &values, &context)
                .unwrap_or_else(|e| panic!("{}: {e}", tool.id));
            tankyard_content::validate_map_features(&outcome.map)
                .unwrap_or_else(|e| panic!("{}: {e}", tool.id));
            if tool.id != "snap" {
                assert!(
                    outcome.map.blocks.len() > map.blocks.len(),
                    "{} adds blocks",
                    tool.id
                );
            }
        }
        let mut squeezed = map.clone();
        let mut twin = squeezed.blocks[block].clone();
        twin.rect[0] += 0.03 + 30. / map.width;
        squeezed.blocks.push(twin);
        let found = plugins.check(&squeezed);
        assert!(
            found
                .iter()
                .any(|(_, f)| f.text.contains("30-unit gap is too narrow for any tank")),
            "{found:?}"
        );
        // A row of touching blocks has no gaps, even though the first and
        // third stand a block apart.
        let mut row = map.clone();
        row.blocks.clear();
        for i in 0..5 {
            let mut cell = map.blocks[block].clone();
            let x = 520. + 40. * i as f32;
            cell.rect = [
                x / map.width,
                320. / map.height,
                40. / map.width,
                40. / map.height,
            ];
            row.blocks.push(cell);
        }
        let found = plugins.check(&row);
        assert!(
            !found.iter().any(|(_, f)| f.text.contains("gap")),
            "{found:?}"
        );
        // A floor packed with 1600 blocks stays inside the check's budget.
        let mut packed = map.clone();
        packed.blocks.clear();
        for i in 0..1600 {
            let mut cell = map.blocks[block].clone();
            let (x, y) = (40. * (i % 40) as f32, 40. * (i / 40) as f32);
            cell.rect = [
                x / map.width,
                y / map.height,
                40. / map.width,
                40. / map.height,
            ];
            packed.blocks.push(cell);
        }
        let found = plugins.check(&packed);
        assert!(plugins.plugins[0].stopped.is_none(), "{found:?}");
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn bundled_logic_kit_wires_working_logic() {
        use tankyard_content::{MapDef, TriggerAction, TriggerWhen};
        use tankyard_core::{LogicEffect, MapLogic, Occupant, V2, WorldConfig};
        let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods/logic_kit");
        let mut plugins = Plugins::load(&[pack]);
        assert!(plugins.errors.is_empty(), "{:?}", plugins.errors);
        let tools = plugins.plugins[0].tools.clone();
        let tool = |id: &str| tools.iter().position(|t| t.id == id).unwrap();
        let defaults = |index: usize| -> Vec<ParamValue> {
            tools[index]
                .params
                .iter()
                .map(ParamSpec::default_value)
                .collect()
        };
        let mut map = tankyard_content::builtin_maps().remove(0);
        map.triggers.clear();
        map.blocks.push(tankyard_content::MapBlock {
            rect: [0.45, 0.2, 0.1, 0.03],
            style: Default::default(),
            color: None,
            group: 0,
        });
        let door = map.blocks.len() - 1;
        let middle = [map.width * 0.5, map.height * 0.5];
        let context = |selection: Vec<ObjectRef>| Context {
            selection,
            pointer: Some(middle),
            grid: 40.,
            view: None,
        };
        // Run a map's logic with tanks standing at `stops`, one per second.
        let run = |map: &MapDef, stops: &[V2], seconds: f32| {
            let config = WorldConfig {
                width: map.width,
                height: map.height,
                ..WorldConfig::default()
            };
            let (mut logic, mut hidden, mut effects) = (MapLogic::default(), 0, Vec::new());
            let mut now = 0.;
            for tick in 0..(seconds * 60.) as usize {
                now += tankyard_core::DT;
                let at = stops.get(tick / 60).copied();
                let tanks: Vec<_> = at
                    .map(|pos| Occupant {
                        id: 1,
                        pos,
                        bot: false,
                        team: 1,
                    })
                    .into_iter()
                    .collect();
                logic.step(
                    map,
                    &config,
                    now,
                    &mut hidden,
                    &tanks,
                    &mut || 0.5,
                    &mut effects,
                );
            }
            (logic, hidden, effects)
        };
        let centre = |map: &MapDef, index: usize| {
            let r = map.triggers[index].rect;
            V2::new(
                (r[0] + r[2] * 0.5) * map.width,
                (r[1] + r[3] * 0.5) * map.height,
            )
        };

        let switch = tool("switch_door");
        let outcome = plugins
            .run(
                0,
                switch,
                &map,
                &defaults(switch),
                &context(vec![("blocks", door)]),
            )
            .unwrap();
        let built = outcome.map;
        tankyard_content::validate_map_features(&built).unwrap();
        let group = built.blocks[door].group;
        assert_ne!(group, 0, "the door gets its own group");
        assert_eq!(built.triggers.len(), 4, "three switches and a goal");
        let away = V2::new(10., 10.);
        let stops = |order: &[usize]| -> Vec<V2> {
            order
                .iter()
                .flat_map(|&i| [centre(&built, i), away])
                .collect()
        };
        let (_, hidden, _) = run(&built, &stops(&[0, 1]), 5.);
        assert_eq!(
            hidden & MapDef::group_bit(group),
            0,
            "two of three keep it shut"
        );
        let (_, hidden, _) = run(&built, &stops(&[2, 0, 1]), 7.);
        assert_ne!(hidden & MapDef::group_bit(group), 0, "all three open it");
        let none = plugins
            .run(0, switch, &map, &defaults(switch), &context(vec![]))
            .unwrap();
        assert_eq!(none.map, map, "nothing selected changes nothing");
        assert_eq!(
            none.messages,
            vec!["Select the blocks that make the door first.".to_owned()]
        );

        let countdown = tool("countdown");
        let mut values = defaults(countdown);
        values[0] = ParamValue::Integer(5);
        let built = plugins
            .run(0, countdown, &map, &values, &context(vec![]))
            .unwrap()
            .map;
        tankyard_content::validate_map_features(&built).unwrap();
        let (logic, _, effects) = run(&built, &[], 7.);
        assert_eq!(logic.counters[0], 0);
        assert_eq!(effects, vec![LogicEffect::Announce("Time is up!".into())]);

        let sequence = tool("sequence");
        let mut steps = map.clone();
        for group in [1, 2, 3] {
            steps.triggers.push(tankyard_content::MapTrigger {
                when: TriggerWhen::Every,
                action: TriggerAction::Hide,
                rect: [0.1, 0.1, 0.05, 0.05],
                group,
                seconds: 1000.,
                ..Default::default()
            });
        }
        let chosen = vec![("triggers", 0), ("triggers", 1), ("triggers", 2)];
        let built = plugins
            .run(0, sequence, &steps, &defaults(sequence), &context(chosen))
            .unwrap()
            .map;
        tankyard_content::validate_map_features(&built).unwrap();
        let plate = centre(&built, 3);
        let bits = |groups: &[u8]| groups.iter().map(|&g| MapDef::group_bit(g)).sum::<u32>();
        let (_, hidden, _) = run(&built, &[plate, away], 1.6);
        assert_eq!(
            hidden,
            bits(&[1, 2]),
            "the first two acted, the third waits a second more"
        );
        let (_, hidden, _) = run(&built, &[plate, away, away], 2.6);
        assert_eq!(hidden, bits(&[1, 2, 3]));

        let lever = tool("lever_bank");
        let outcome = plugins
            .run(
                0,
                lever,
                &map,
                &defaults(lever),
                &context(vec![("blocks", door)]),
            )
            .unwrap();
        let levers = outcome.map;
        tankyard_content::validate_map_features(&levers).unwrap();
        let group = levers.blocks[door].group;
        let gate = levers
            .triggers
            .iter()
            .find(|t| t.only_if.is_some())
            .expect("a gate");
        assert_eq!(gate.conditions().count(), 3, "one condition per lever");
        assert!(gate.otherwise > 0, "a shut gate closes the door");
        // Levers are the triggers that flip counters, in order.
        let lever_at: Vec<V2> = levers
            .triggers
            .iter()
            .enumerate()
            .filter(|(_, t)| t.action == TriggerAction::Calculate)
            .map(|(i, _)| centre(&levers, i))
            .collect();
        assert_eq!(lever_at.len(), 3);
        let visit = |order: &[usize]| -> Vec<V2> {
            order.iter().flat_map(|&i| [lever_at[i], away]).collect()
        };
        let open = |hidden: u32| hidden & MapDef::group_bit(group) != 0;
        let (_, hidden, _) = run(&levers, &visit(&[0, 1]), 4.5);
        assert!(!open(hidden), "two of three levers keep it shut");
        let (_, hidden, _) = run(&levers, &visit(&[0, 1, 2]), 6.5);
        assert!(open(hidden), "all three open it");
        let (_, hidden, _) = run(&levers, &visit(&[0, 1, 2, 1]), 8.5);
        assert!(!open(hidden), "turning one back shuts it again");
        let mut values = defaults(lever);
        values[1] = ParamValue::Choice(2);
        let xor = plugins
            .run(0, lever, &map, &values, &context(vec![("blocks", door)]))
            .unwrap();
        assert_eq!(
            xor.messages,
            vec![format!(
                "Door is group {group}. 3 levers; it opens when just one is on."
            )]
        );
        let (_, hidden, _) = run(&xor.map, &visit(&[1]), 2.5);
        assert!(open(hidden), "one lever opens an XOR door");
        let (_, hidden, _) = run(&xor.map, &visit(&[1, 2]), 4.5);
        assert!(!open(hidden), "two shut it");

        let mut unread = map.clone();
        unread.triggers.push(tankyard_content::MapTrigger {
            when: TriggerWhen::Every,
            action: TriggerAction::Count,
            rect: [0.1, 0.1, 0.05, 0.05],
            group: 6,
            ..Default::default()
        });
        let found = plugins.check(&unread);
        assert!(
            found
                .iter()
                .any(|(_, f)| f.text.contains("Counter 6 changes, but nothing reads it")),
            "{found:?}"
        );
        assert!(
            plugins.check(&levers).is_empty(),
            "levers read their counters"
        );
        assert!(plugins.check(&built).is_empty());
    }
}

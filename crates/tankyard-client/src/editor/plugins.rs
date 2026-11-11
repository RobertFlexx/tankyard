//! Workshop plugins from enabled packs: loading them, running a tool as one
//! undo step, and adding their checks to the Check tab.
use super::Editor;
use crate::settings::Settings;
use macroquad::prelude::vec2;
use std::collections::HashMap;
use tankyard_client::editor_document::{Key, Problem};
use tankyard_core::V2;
use tankyard_lua::editor::{Context, ObjectRef, ParamValue, Plugins};

/// Plugin state the editor keeps between frames.
#[derive(Default)]
pub struct Workshop {
    /// Loaded on first use, and again after Reload.
    pub plugins: Option<Plugins>,
    /// Packs waiting for review in Mods.
    pub held: Vec<String>,
    /// Each tool's control values, by plugin and tool id.
    pub values: HashMap<(String, String), Vec<ParamValue>>,
    /// The tool Ctrl+R runs again.
    pub last: Option<(usize, usize)>,
    /// What the last run said, printed or failed with.
    pub report: Vec<String>,
}

pub fn key_ref(key: Key) -> ObjectRef {
    match key {
        Key::Wall(i) => ("walls", i),
        Key::Block(i) => ("blocks", i),
        Key::Zone(i) => ("zones", i),
        Key::Decor(i) => ("decor", i),
        Key::Trigger(i) => ("triggers", i),
        Key::Spawn(i) => ("spawn_regions", i),
        Key::Base(i) => ("team_bases", i),
        Key::Objective(i) => ("objective_positions", i),
    }
}
pub fn ref_key((list, i): ObjectRef) -> Option<Key> {
    Some(match list {
        "walls" => Key::Wall(i),
        "blocks" => Key::Block(i),
        "zones" => Key::Zone(i),
        "decor" => Key::Decor(i),
        "triggers" => Key::Trigger(i),
        "spawn_regions" => Key::Spawn(i),
        "team_bases" => Key::Base(i),
        "objective_positions" => Key::Objective(i),
        _ => return None,
    })
}

impl Editor {
    /// Load plugins the first time they are needed.
    pub(super) fn ensure_plugins(&mut self, settings: &Settings) {
        if self.workshop.plugins.is_some() {
            return;
        }
        let ready = crate::mod_library::editor_paths(settings);
        let plugins = Plugins::load(&ready.paths);
        for error in &plugins.errors {
            log::warn!("workshop plugin: {error}");
        }
        self.workshop.held = ready.held;
        self.workshop.plugins = Some(plugins);
        self.workshop.last = None;
    }

    pub(super) fn reload_plugins(&mut self) {
        self.workshop.plugins = None;
        self.workshop.report.clear();
        self.say("Plugins reloaded.");
    }

    /// The control values a tool runs with.
    pub(super) fn tool_values(&self, plugin: usize, tool: usize) -> Vec<ParamValue> {
        let Some(plugins) = &self.workshop.plugins else {
            return Vec::new();
        };
        let (pack, spec) = (
            &plugins.plugins[plugin],
            &plugins.plugins[plugin].tools[tool],
        );
        let defaults = || spec.params.iter().map(|p| p.default_value()).collect();
        self.workshop
            .values
            .get(&(pack.id.clone(), spec.id.clone()))
            .filter(|values| values.len() == spec.params.len())
            .cloned()
            .unwrap_or_else(defaults)
    }

    /// Run a tool on the document, keeping its result as one undo step when
    /// the workshop can keep the map it made.
    pub(super) fn run_tool(&mut self, plugin: usize, tool: usize, pointer: Option<V2>) {
        let values = self.tool_values(plugin, tool);
        let context = Context {
            selection: self.selection.iter().copied().map(key_ref).collect(),
            pointer: pointer
                .or(self.last_pointer)
                .map(|p| [p.x, p.y])
                .or(Some([self.view.center.x, self.view.center.y])),
            grid: self.grid,
            view: self.visible_area(),
        };
        let Some(plugins) = &mut self.workshop.plugins else {
            return;
        };
        let Some(name) = plugins
            .plugins
            .get(plugin)
            .and_then(|p| p.tools.get(tool))
            .map(|t| t.name.clone())
        else {
            return;
        };
        let result = plugins.run(plugin, tool, &self.doc.map, &values, &context);
        self.workshop.last = Some((plugin, tool));
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.say(error.clone());
                self.workshop.report = vec![error];
                return;
            }
        };
        let mut doc = self.doc.clone();
        doc.map = outcome.map;
        if let Err(error) = doc.check_draft() {
            let error = format!("{name} made a map the workshop cannot keep: {error}");
            self.say(error.clone());
            self.workshop.report = vec![error];
            return;
        }
        let changed = doc != self.doc;
        if changed {
            self.apply(|editor| editor.doc = doc);
        }
        if let Some(chosen) = outcome.selection {
            let keys = chosen
                .into_iter()
                .filter_map(ref_key)
                .filter(|&key| self.doc.object(key).is_some())
                .collect();
            self.set_selection(keys);
            self.mode = super::Mode::Edit;
        }
        self.say(match outcome.messages.first() {
            Some(message) => message.clone(),
            None if changed => format!("{name} done. Ctrl+Z undoes it; Ctrl+R runs it again."),
            None => format!("{name} changed nothing."),
        });
        self.workshop.report = outcome.messages.into_iter().chain(outcome.log).collect();
    }

    /// The part of the arena the canvas shows, in world units.
    fn visible_area(&self) -> Option<[f32; 4]> {
        let area = self.canvas_area;
        if area.w <= 0. || area.h <= 0. {
            return None;
        }
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        let a = self.view.world(vec2(area.x, area.y));
        let b = self.view.world(vec2(area.right(), area.bottom()));
        let (x0, y0) = (a.x.clamp(0., w), a.y.clamp(0., h));
        let (x1, y1) = (b.x.clamp(0., w), b.y.clamp(0., h));
        (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
    }

    /// Ctrl+R: the last tool again, with the same settings.
    pub(super) fn repeat_tool(&mut self, pointer: Option<V2>) {
        match self.workshop.last {
            Some((plugin, tool)) => self.run_tool(plugin, tool, pointer),
            None => self.say("Run a tool from the Tools tab first; Ctrl+R repeats it."),
        }
    }

    /// Plugin checks as Check tab notes. They advise; they never block.
    pub(super) fn plugin_problems(&mut self) -> Vec<Problem> {
        let Some(plugins) = &mut self.workshop.plugins else {
            return Vec::new();
        };
        plugins
            .check(&self.doc.map)
            .into_iter()
            .map(|(plugin, finding)| Problem {
                key: finding
                    .object
                    .and_then(ref_key)
                    .filter(|&key| self.doc.object(key).is_some()),
                text: format!("{plugin}: {}", finding.text),
                blocking: false,
            })
            .collect()
    }
}

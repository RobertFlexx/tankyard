use tankyard_ai::{BotController, Difficulty};
use tankyard_core::{World, WorldConfig};

pub struct LocalSession {
    pub world: World,
    pub bots: BotController,
    pub bundle: tankyard_mods::GameBundle,
    pub mods: tankyard_core::modding::ModRuntime,
}

impl LocalSession {
    pub fn new(config: WorldConfig, bot_count: usize) -> Self {
        Self::with_mods(config, bot_count, tankyard_mods::LoadedMods::default())
            .expect("built-in session rules must be valid")
    }
    pub fn with_mods(
        config: WorldConfig,
        bot_count: usize,
        mut loaded: tankyard_mods::LoadedMods,
    ) -> Result<Self, String> {
        if !loaded.rules_override {
            loaded.bundle.rules = config.rules;
        }
        let mut ids = std::collections::HashSet::new();
        for behavior in &loaded.behaviors {
            if !ids.insert(&behavior.info.id) {
                return Err("duplicate behavior module".into());
            }
            if !loaded.bundle.mods.iter().any(|m| m.id == behavior.info.id) {
                loaded.bundle.mods.push(behavior.info.clone());
            }
            tankyard_mods::validate_hooks(&behavior.hooks, &loaded.bundle.content)?;
        }
        loaded.bundle.validate()?;
        let mut world = World::with_bundle(config, &loaded.bundle)?;
        for i in 0..bot_count.min(128) {
            world.add_player(&format!("Vector {:02}", i + 1), true);
        }
        Ok(Self {
            world,
            bots: BotController::new(Difficulty::Normal),
            mods: tankyard_core::modding::ModRuntime::new(&loaded),
            bundle: loaded.bundle,
        })
    }
    pub fn install_plugin(
        &mut self,
        plugin: Box<dyn tankyard_core::modding::ServerPlugin>,
    ) -> Result<(), String> {
        let id = plugin.id().to_owned();
        let version = plugin.version().to_owned();
        if version.is_empty() || version.len() > 32 || version.chars().any(char::is_control) {
            return Err("invalid native plugin version".into());
        }
        let mut bundle = self.bundle.clone();
        bundle.mods.push(tankyard_mods::ModInfo {
            id,
            name: "Server extension".into(),
            version,
            behavior_digest: 0,
        });
        bundle.validate()?;
        self.mods.install(plugin)?;
        self.world.config.modded = true;
        self.bundle = bundle;
        Ok(())
    }
    pub fn step(&mut self) {
        self.bots.update(&mut self.world);
        self.world.step_with(|world| self.mods.before_step(world));
        self.mods.after_step(&mut self.world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packs_without_rule_profiles_keep_the_configured_physics() {
        let mut config = WorldConfig::default();
        config.rules.movement_multiplier = 1.3;
        let mut loaded = tankyard_mods::LoadedMods::default();
        loaded.bundle.mods.push(tankyard_mods::ModInfo {
            id: "skin".into(),
            name: "Skin".into(),
            version: "1".into(),
            behavior_digest: 0,
        });
        let session = LocalSession::with_mods(config.clone(), 0, loaded.clone()).unwrap();
        assert_eq!(session.world.config.rules.movement_multiplier, 1.3);
        loaded.bundle.rules.movement_multiplier = 1.7;
        loaded.rules_override = true;
        let session = LocalSession::with_mods(config, 0, loaded).unwrap();
        assert_eq!(session.world.config.rules.movement_multiplier, 1.7);
    }
    #[test]
    fn round_start_effects_keep_victory_events_and_bot_controls_reach_physics() {
        let mut loaded = tankyard_mods::LoadedMods::default();
        let json = r#"{"id":"start","event":"round_start","actions":[{"type":"finish_round","team":null,"reason":"Finished"}]}"#;
        loaded.behaviors.push(tankyard_mods::LoadedMod {
            info: tankyard_mods::ModInfo {
                id: "ending".into(),
                name: "Ending".into(),
                version: "1".into(),
                behavior_digest: 0,
            },
            hooks: vec![serde_json::from_str(json).unwrap()],
        });
        let mut session = LocalSession::with_mods(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            1,
            loaded,
        )
        .unwrap();
        session.step();
        assert!(session.world.match_state.finished);
        assert!(
            session
                .world
                .events
                .iter()
                .any(|e| matches!(e.kind, tankyard_core::EventKind::Victory))
        );
        let mut loaded = tankyard_mods::LoadedMods::default();
        let json = r#"{"id":"drive","event":"tick","target":"alive_players","actions":[{"type":"control","movement":[1,0],"aim":0,"fire":false,"ability":false}]}"#;
        loaded.behaviors.push(tankyard_mods::LoadedMod {
            info: tankyard_mods::ModInfo {
                id: "drive".into(),
                name: "Drive".into(),
                version: "1".into(),
                behavior_digest: 0,
            },
            hooks: vec![serde_json::from_str(json).unwrap()],
        });
        let mut session = LocalSession::with_mods(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            1,
            loaded,
        )
        .unwrap();
        session.step();
        assert!(session.world.tanks[0].vel.x > 0.0);
        assert_eq!(session.world.tanks[0].vel.y, 0.0);
        assert!(!session.world.tanks[0].input.fire);
    }
}

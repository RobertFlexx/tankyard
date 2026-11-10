//! The map editor's portable document and operations, independent of the UI.
use serde::{Deserialize, Serialize};
use tankyard_content::*;
use tankyard_mods::{GameRules, PackFiles};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format: u32,
    pub map: MapDef,
    pub rules: GameRules,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Wall(usize),
    Block(usize),
    Zone(usize),
    Decor(usize),
    Trigger(usize),
    Spawn(usize),
    Base(usize),
    Objective(usize),
}
#[derive(Clone, Debug)]
pub enum Object {
    Wall([f32; 4]),
    Block(MapBlock),
    Zone(MapZone),
    Decor(MapDecor),
    Trigger(MapTrigger),
    Spawn(SpawnRegion),
    Base(TeamBase),
    Objective([f32; 2]),
}
impl Object {
    pub fn rect(&self) -> [f32; 4] {
        match self {
            Self::Wall(r) => *r,
            Self::Block(o) => o.rect,
            Self::Zone(o) => o.rect,
            Self::Decor(o) => o.rect,
            Self::Trigger(o) => o.rect,
            Self::Spawn(o) => o.rect,
            Self::Base(o) => [o.position[0] - 0.01, o.position[1] - 0.01, 0.02, 0.02],
            Self::Objective(p) => [p[0] - 0.01, p[1] - 0.01, 0.02, 0.02],
        }
    }
    pub fn set_rect(&mut self, mut r: [f32; 4]) {
        r[2] = r[2].clamp(0.001, 1.);
        r[3] = r[3].clamp(0.001, 1.);
        r[0] = r[0].clamp(0., 1. - r[2]);
        r[1] = r[1].clamp(0., 1. - r[3]);
        match self {
            Self::Wall(rect) => *rect = r,
            Self::Block(o) => o.rect = r,
            Self::Zone(o) => o.rect = r,
            Self::Decor(o) => o.rect = r,
            Self::Trigger(o) => o.rect = r,
            Self::Spawn(o) => o.rect = r,
            Self::Base(o) => o.position = [r[0] + r[2] * 0.5, r[1] + r[3] * 0.5],
            Self::Objective(p) => *p = [r[0] + r[2] * 0.5, r[1] + r[3] * 0.5],
        }
    }
    pub fn group(&self) -> u8 {
        match self {
            Self::Block(o) => o.group,
            Self::Zone(o) => o.group,
            Self::Decor(o) => o.group,
            Self::Trigger(o) => o.group,
            _ => 0,
        }
    }
    pub fn set_group(&mut self, group: u8) {
        match self {
            Self::Block(o) => o.group = group,
            Self::Zone(o) => o.group = group,
            Self::Decor(o) => o.group = group,
            Self::Trigger(o) => o.group = group,
            _ => {}
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Wall(_) => "Wall",
            Self::Block(_) => "Block",
            Self::Zone(o) => o.kind.name(),
            Self::Decor(o) => o.kind.name(),
            Self::Trigger(_) => "Trigger",
            Self::Spawn(_) => "Spawn region",
            Self::Base(_) => "Team base",
            Self::Objective(_) => "Objective",
        }
    }
}
impl Default for Document {
    fn default() -> Self {
        let mut map = builtin_maps().remove(0);
        map.id = "my_arena".into();
        map.name = "My arena".into();
        map.description = "An arena made in Tank Yard's map editor.".into();
        map.width = 3600.;
        map.height = 2600.;
        map.capacity = 16;
        map.walls.clear();
        map.team_bases.clear();
        map.objective_positions.clear();
        map.procedural = MapProcedure::None;
        map.spawn_regions = vec![
            SpawnRegion {
                team: 0,
                rect: [0.05, 0.05, 0.15, 0.15],
            },
            SpawnRegion {
                team: 0,
                rect: [0.80, 0.80, 0.15, 0.15],
            },
        ];
        Self {
            format: 1,
            map,
            rules: Default::default(),
        }
    }
}
impl Document {
    pub fn from_map(map: MapDef) -> Self {
        Self {
            map,
            ..Default::default()
        }
    }
    /// Drafts may be temporarily unplayable, but imports still have strict size
    /// and coordinate limits. Full navigation validation runs on play/export.
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Editor document exceeds 2 MiB.".into());
        }
        let doc: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if doc.format != 1
            || doc.map.id.len() > 48
            || doc.map.name.len() > 64
            || doc.map.description.len() > 2048
            || !(900. ..=16000.).contains(&doc.map.width)
            || !(700. ..=16000.).contains(&doc.map.height)
            || doc.map.walls.len() + doc.map.blocks.len() > 2048
            || doc.map.zones.len() > 256
            || doc.map.decor.len() > 1024
            || doc.map.triggers.len() > 128
            || doc.map.spawn_regions.len() > 32
            || doc.map.team_bases.len() > 4
            || doc.map.objective_positions.len() > 32
        {
            return Err("Unsupported editor format or document limits exceeded.".into());
        }
        for key in doc.keys() {
            let r = doc.object(key).unwrap().rect();
            if r.iter().any(|v| !v.is_finite())
                || r[2] <= 0.
                || r[3] <= 0.
                || r[0] < -0.02
                || r[1] < -0.02
                || r[0] + r[2] > 1.02
                || r[1] + r[3] > 1.02
            {
                return Err("Invalid editor object coordinates.".into());
            }
        }
        doc.rules.validate()?;
        // Unfinished trigger text and targets are editable. Validate their
        // behavior at export/playtest, and validate drawn features on import.
        let mut preview_map = doc.map.clone();
        preview_map.triggers.clear();
        validate_map_features(&preview_map)?;
        Ok(doc)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.rules.validate()?;
        validate_maps(std::slice::from_ref(&self.map))
    }
    /// Turn seeded maze/garden cover into editable blocks when copying a
    /// procedural preset. Existing authored blocks remain separate objects.
    pub fn bake_procedure(&mut self) -> Result<(), String> {
        if self.map.procedural == MapProcedure::None {
            return Ok(());
        }
        let mut bundle = tankyard_mods::GameBundle::builtin();
        let mut map = self.map.clone();
        map.blocks.clear();
        bundle.maps.retain(|m| m.id != map.id);
        bundle.maps.push(map);
        bundle.rules = self.rules;
        let world = tankyard_core::World::with_bundle(
            tankyard_core::WorldConfig {
                width: self.map.width,
                height: self.map.height,
                map: self.map.id.clone(),
                mode: tankyard_core::Mode::Sandbox,
                dynamic_arena: false,
                target_count: 0,
                ..Default::default()
            },
            &bundle,
        )?;
        self.map.walls = world
            .walls
            .iter()
            .map(|r| {
                [
                    r.x / self.map.width,
                    r.y / self.map.height,
                    r.w / self.map.width,
                    r.h / self.map.height,
                ]
            })
            .collect();
        self.map.procedural = MapProcedure::None;
        Ok(())
    }
    pub fn keys(&self) -> Vec<Key> {
        let m = &self.map;
        (0..m.walls.len())
            .map(Key::Wall)
            .chain((0..m.blocks.len()).map(Key::Block))
            .chain((0..m.zones.len()).map(Key::Zone))
            .chain((0..m.decor.len()).map(Key::Decor))
            .chain((0..m.triggers.len()).map(Key::Trigger))
            .chain((0..m.spawn_regions.len()).map(Key::Spawn))
            .chain((0..m.team_bases.len()).map(Key::Base))
            .chain((0..m.objective_positions.len()).map(Key::Objective))
            .collect()
    }
    pub fn object(&self, key: Key) -> Option<Object> {
        let m = &self.map;
        Some(match key {
            Key::Wall(i) => Object::Wall(*m.walls.get(i)?),
            Key::Block(i) => Object::Block(m.blocks.get(i)?.clone()),
            Key::Zone(i) => Object::Zone(m.zones.get(i)?.clone()),
            Key::Decor(i) => Object::Decor(m.decor.get(i)?.clone()),
            Key::Trigger(i) => Object::Trigger(m.triggers.get(i)?.clone()),
            Key::Spawn(i) => Object::Spawn(m.spawn_regions.get(i)?.clone()),
            Key::Base(i) => Object::Base(m.team_bases.get(i)?.clone()),
            Key::Objective(i) => Object::Objective(*m.objective_positions.get(i)?),
        })
    }
    pub fn replace(&mut self, key: Key, object: Object) {
        let m = &mut self.map;
        match (key, object) {
            (Key::Wall(i), Object::Wall(o)) if i < m.walls.len() => m.walls[i] = o,
            (Key::Block(i), Object::Block(o)) if i < m.blocks.len() => m.blocks[i] = o,
            (Key::Zone(i), Object::Zone(o)) if i < m.zones.len() => m.zones[i] = o,
            (Key::Decor(i), Object::Decor(o)) if i < m.decor.len() => m.decor[i] = o,
            (Key::Trigger(i), Object::Trigger(o)) if i < m.triggers.len() => m.triggers[i] = o,
            (Key::Spawn(i), Object::Spawn(o)) if i < m.spawn_regions.len() => {
                m.spawn_regions[i] = o
            }
            (Key::Base(i), Object::Base(o)) if i < m.team_bases.len() => m.team_bases[i] = o,
            (Key::Objective(i), Object::Objective(o)) if i < m.objective_positions.len() => {
                m.objective_positions[i] = o
            }
            _ => {}
        }
    }
    pub fn insert(&mut self, object: Object) -> Option<Key> {
        let m = &mut self.map;
        Some(match object {
            Object::Wall(o) if m.walls.len() + m.blocks.len() < 2048 => {
                m.walls.push(o);
                Key::Wall(m.walls.len() - 1)
            }
            Object::Block(o) if m.walls.len() + m.blocks.len() < 2048 => {
                m.blocks.push(o);
                Key::Block(m.blocks.len() - 1)
            }
            Object::Zone(o) if m.zones.len() < 256 => {
                m.zones.push(o);
                Key::Zone(m.zones.len() - 1)
            }
            Object::Decor(o)
                if m.decor.len() < 1024
                    && (o.kind != DecorKind::Emitter
                        || m.decor
                            .iter()
                            .filter(|d| d.kind == DecorKind::Emitter)
                            .count()
                            < 48) =>
            {
                m.decor.push(o);
                Key::Decor(m.decor.len() - 1)
            }
            Object::Trigger(o) if m.triggers.len() < 128 => {
                m.triggers.push(o);
                Key::Trigger(m.triggers.len() - 1)
            }
            Object::Spawn(o) if m.spawn_regions.len() < 32 => {
                m.spawn_regions.push(o);
                Key::Spawn(m.spawn_regions.len() - 1)
            }
            Object::Base(o)
                if m.team_bases.len() < 4 && !m.team_bases.iter().any(|b| b.team == o.team) =>
            {
                m.team_bases.push(o);
                Key::Base(m.team_bases.len() - 1)
            }
            Object::Objective(o) if m.objective_positions.len() < 32 => {
                m.objective_positions.push(o);
                Key::Objective(m.objective_positions.len() - 1)
            }
            _ => return None,
        })
    }
    pub fn delete(&mut self, keys: &[Key]) {
        // Reverse source order keeps all indices valid while removing a batch.
        for key in self
            .keys()
            .into_iter()
            .rev()
            .filter(|key| keys.contains(key))
        {
            match key {
                Key::Wall(i) => {
                    self.map.walls.remove(i);
                }
                Key::Block(i) => {
                    self.map.blocks.remove(i);
                }
                Key::Zone(i) => {
                    self.map.zones.remove(i);
                }
                Key::Decor(i) => {
                    self.map.decor.remove(i);
                }
                Key::Trigger(i) => {
                    self.map.triggers.remove(i);
                }
                Key::Spawn(i) => {
                    self.map.spawn_regions.remove(i);
                }
                Key::Base(i) => {
                    self.map.team_bases.remove(i);
                }
                Key::Objective(i) => {
                    self.map.objective_positions.remove(i);
                }
            }
        }
    }
    pub fn pack(&self) -> Result<PackFiles, String> {
        self.validate()?;
        let manifest = serde_json::json!({"api_version":1, "id":self.map.id,
            "name":self.map.name,"version":"1.0.0", "description":self.map.description,
            "maps":[self.map],"rules":self.rules});
        let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
        // Export and playtest pass exactly the same manifest path as any pack.
        let pack = PackFiles::from_files([
            ("mod.json".into(), bytes),
            (
                "editor.json".into(),
                serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
            ),
        ])?;
        tankyard_mods::parse_manifest(&pack)?;
        Ok(pack)
    }
    pub fn loaded(&self) -> Result<tankyard_mods::LoadedMods, String> {
        self.validate()?;
        let mut loaded = tankyard_mods::LoadedMods::default();
        loaded.bundle.maps.retain(|map| map.id != self.map.id);
        loaded.bundle.maps.push(self.map.clone());
        loaded.bundle.rules = self.rules;
        loaded.rules_override = true;
        loaded.bundle.validate()?;
        Ok(loaded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_deletion_preserves_unselected_indices_and_roundtrips() {
        let mut doc = Document::default();
        for i in 0..4 {
            doc.insert(Object::Wall([0.1 + i as f32 * 0.1, 0.4, 0.03, 0.03]));
        }
        doc.delete(&[Key::Wall(0), Key::Wall(2)]);
        assert_eq!(doc.map.walls.len(), 2);
        assert_eq!(doc.map.walls[0][0], 0.2);
        assert_eq!(
            Document::from_json(&serde_json::to_vec(&doc).unwrap()).unwrap(),
            doc
        );
        let zip = doc.pack().unwrap().to_zip().unwrap();
        let pack = PackFiles::from_zip(&zip).unwrap();
        assert_eq!(
            tankyard_mods::parse_manifest(&pack).unwrap().maps[0],
            doc.map
        );
    }
    #[test]
    fn moving_an_object_keeps_it_inside_the_arena() {
        let mut obj = Object::Wall([0., 0., 0.2, 0.2]);
        obj.set_rect([0.95, -0.3, 0.2, 0.2]);
        assert_eq!(obj.rect(), [0.8, 0., 0.2, 0.2]);
    }
}

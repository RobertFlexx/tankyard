//! Gameplay definitions shared by the server, client, encyclopedia and bots.
//!
//! The embedded JSON is the source of truth. A community content pack can use
//! exactly the same format; validate it before constructing a world.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

mod maps;
pub use maps::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectileKind {
    Bullet,
    Shell,
    Piercing,
    Scatter,
    Drone,
    Trap,
    Mine,
    Explosive,
    Fragment,
    Homing,
    Barrier,
    Turret,
    Minion,
}

impl ProjectileKind {
    pub fn is_drone(self) -> bool {
        matches!(self, Self::Drone | Self::Minion)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirePattern {
    Normal,
    Alternating,
    Burst,
    Charge,
    Auto,
    Spin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ability {
    None,
    Dash,
    Shield,
    Heal,
    Nova,
    Recall,
    Cloak,
    Overdrive,
    Blink,
    Fortify,
    Magnet,
    Parry,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Barrel {
    #[serde(default)]
    pub profile: BarrelProfile,
    pub offset: [f32; 2],
    pub angle: f32,
    pub length: f32,
    pub width: f32,
    pub cooldown: f32,
    pub delay: f32,
    pub recoil: f32,
    pub pattern: FirePattern,
    pub projectile: ProjectileKind,
    pub speed: f32,
    pub damage: f32,
    pub radius: f32,
    pub lifetime: f32,
    pub penetration: f32,
    pub count: u8,
    pub spread: f32,
    pub ricochets: u8,
    /// Automatic turret targeting cone in radians; absent means a full circle.
    #[serde(default)]
    pub target_arc: Option<f32>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarrelProfile {
    #[default]
    Auto,
    Rectangle,
    Trapezoid,
    Trap,
    Rocket,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyStyle {
    #[default]
    Auto,
    Circle,
    Smasher,
    Landmine,
    Spike,
}
fn body_scale_default() -> f32 {
    1.0
}
fn drone_limit_default() -> u8 {
    8
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassDef {
    #[serde(default)]
    pub body_style: BodyStyle,
    #[serde(default = "body_scale_default")]
    pub body_scale: f32,
    #[serde(default)]
    pub body_texture: Option<String>,
    pub id: String,
    pub name: String,
    pub description: String,
    pub tier: u8,
    pub min_level: u8,
    pub parents: Vec<String>,
    pub upgrades: Vec<String>,
    /// Health, regeneration, body damage, shot speed, penetration, shot damage,
    /// reload speed and movement speed, in the same order as the upgrade HUD.
    pub multipliers: [f32; 8],
    /// Zero denotes a circular hull; three through sixteen denote polygons.
    pub hull_sides: u8,
    pub barrels: Vec<Barrel>,
    #[serde(default = "drone_limit_default")]
    pub drone_limit: u8,
    pub ability: Ability,
    pub ability_cooldown: f32,
    pub passive_regen: f32,
    pub strengths: String,
    pub weaknesses: String,
    pub ai_range: f32,
}

impl ClassDef {
    /// A behavior fingerprint deliberately ignores names, geometry and balance
    /// numbers. A cosmetic reskin or damage-only edit cannot count as a class.
    /// Mount positions, shot patterns, projectile composition, ricochets,
    /// deployable count and abilities all change how a tank is played.
    pub fn behavior_fingerprint(&self) -> String {
        let mut mounts = self
            .barrels
            .iter()
            .map(|b| {
                format!(
                    "{:?}:{:?}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                    b.projectile,
                    b.pattern,
                    (b.angle * 1000.).round() as i32,
                    (b.offset[0] * 10.).round() as i32,
                    (b.offset[1] * 10.).round() as i32,
                    b.count,
                    (b.spread * 1000.).round() as i32,
                    b.ricochets,
                    u8::from(b.delay > 0.),
                    u8::from(b.recoil >= 75.),
                    (b.target_arc.unwrap_or(std::f32::consts::TAU) * 1000.0).round() as i32,
                )
            })
            .collect::<Vec<_>>();
        mounts.sort_unstable();
        format!(
            "{:?}:{}:{}:{}:{}",
            self.ability,
            u8::from(self.passive_regen > 0.),
            u8::from(self.multipliers[2] >= 2.),
            if self.has_drones() {
                self.drone_limit
            } else {
                0
            },
            mounts.join("|")
        )
    }

    pub fn has_deployables(&self) -> bool {
        self.barrels.iter().any(|b| {
            matches!(
                b.projectile,
                ProjectileKind::Drone
                    | ProjectileKind::Minion
                    | ProjectileKind::Trap
                    | ProjectileKind::Mine
                    | ProjectileKind::Barrier
                    | ProjectileKind::Turret
            )
        })
    }

    pub fn has_drones(&self) -> bool {
        self.barrels
            .iter()
            .any(|barrel| barrel.projectile.is_drone())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub classes: Vec<ClassDef>,
}

impl Content {
    pub fn builtin() -> Self {
        let content: Self =
            serde_json::from_str(include_str!("../../../assets/content/classes.json"))
                .expect("the compiled-in class catalog must be valid JSON");
        content
            .validate()
            .expect("the compiled-in class catalog must pass validation");
        content
    }

    pub fn from_json(source: &str) -> Result<Self, String> {
        let content: Self =
            serde_json::from_str(source).map_err(|e| format!("invalid content JSON: {e}"))?;
        content.validate()?;
        Ok(content)
    }

    pub fn class(&self, id: &str) -> Option<&ClassDef> {
        self.classes.iter().find(|c| c.id == id)
    }

    pub fn validate(&self) -> Result<(), String> {
        self.validate_inner(false)
    }
    /// Mods may intentionally add skins or alternate balance profiles.
    pub fn validate_modded(&self) -> Result<(), String> {
        self.validate_inner(true)
    }
    fn validate_inner(&self, variants: bool) -> Result<(), String> {
        if self.classes.is_empty() || self.classes.len() > 1024 {
            return Err("the catalog must contain between 1 and 1024 classes".into());
        }
        let mut ids = HashMap::with_capacity(self.classes.len());
        let mut names = HashSet::with_capacity(self.classes.len());
        let mut profiles = HashMap::with_capacity(self.classes.len());
        for (index, class) in self.classes.iter().enumerate() {
            if class.id.is_empty()
                || class.id.len() > 48
                || !class
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(format!("invalid class identifier {:?}", class.id));
            }
            if ids.insert(class.id.as_str(), index).is_some() {
                return Err(format!("duplicate class identifier {}", class.id));
            }
            if class.name.trim().is_empty()
                || class.name.len() > 64
                || class.name.chars().any(char::is_control)
            {
                return Err(format!("{}: invalid display name", class.id));
            }
            if !names.insert(class.name.to_lowercase()) {
                return Err(format!("duplicate class display name {}", class.name));
            }
            if [&class.description, &class.strengths, &class.weaknesses]
                .iter()
                .any(|text| {
                    text.trim().is_empty()
                        || text.len() > 2048
                        || text.chars().any(char::is_control)
                })
            {
                return Err(format!("{}: missing or oversized playstyle text", class.id));
            }
            let required_level = match class.tier {
                0 => 1,
                1 => 15,
                2 => 30,
                3 => 45,
                _ => return Err(format!("{}: tier must be 0..3", class.id)),
            };
            if class.min_level != required_level {
                return Err(format!(
                    "{}: tier {} requires level {required_level}",
                    class.id, class.tier
                ));
            }
            if class.multipliers.iter().any(|&m| !in_range(m, 0.1, 5.)) {
                return Err(format!(
                    "{}: stat multipliers must be finite and 0.1..5",
                    class.id
                ));
            }
            if class.hull_sides != 0 && !(3..=16).contains(&class.hull_sides) {
                return Err(format!(
                    "{}: hull must be a circle or a 3..16 polygon",
                    class.id
                ));
            }
            if !in_range(class.body_scale, 0.5, 2.0)
                || class.body_texture.as_ref().is_some_and(|id| {
                    id.is_empty()
                        || id.len() > 48
                        || !id
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                })
            {
                return Err(format!(
                    "{}: invalid body scale or texture identifier",
                    class.id
                ));
            }
            if !in_range(class.ability_cooldown, 0.5, 120.)
                || !in_range(class.passive_regen, 0., 10.)
                || !in_range(class.ai_range, 30., 2000.)
            {
                return Err(format!("{}: invalid ability or AI parameters", class.id));
            }
            if class.barrels.len() > 16 || (class.barrels.is_empty() && class.multipliers[2] < 2.) {
                return Err(format!(
                    "{}: a class needs a weapon or a melee specialization",
                    class.id
                ));
            }
            if class.drone_limit > 64 || (class.has_drones() && class.drone_limit == 0) {
                return Err(format!(
                    "{}: drone limit must be 1..64 for a drone class",
                    class.id
                ));
            }
            for (mount, barrel) in class.barrels.iter().enumerate() {
                validate_barrel(barrel).map_err(|e| format!("{} barrel {mount}: {e}", class.id))?;
            }
            let profile = class.behavior_fingerprint();
            if let Some(previous) = profiles.insert(profile, class.id.as_str())
                && !variants
            {
                return Err(format!(
                    "{} and {previous} have identical mechanical profiles",
                    class.id
                ));
            }
        }
        let root_index = *ids.get("vector").ok_or("missing initial class vector")?;
        if self.classes[root_index].tier != 0 || !self.classes[root_index].parents.is_empty() {
            return Err("vector must be the sole tier-0 root with no parents".into());
        }
        for class in &self.classes {
            if class.id != "vector" && (class.tier == 0 || class.parents.is_empty()) {
                return Err(format!("{}: non-root class must have parents", class.id));
            }
            if class.parents.len() > 16 || class.upgrades.len() > 32 {
                return Err(format!("{}: oversized upgrade adjacency list", class.id));
            }
            check_unique_edges(&class.id, &class.parents)?;
            check_unique_edges(&class.id, &class.upgrades)?;
            for parent in &class.parents {
                let parent_index = *ids
                    .get(parent.as_str())
                    .ok_or_else(|| format!("{}: unknown parent {parent}", class.id))?;
                let parent_class = &self.classes[parent_index];
                if parent_class.tier + 1 != class.tier {
                    return Err(format!(
                        "{}: parent {parent} must be in the preceding tier",
                        class.id
                    ));
                }
                if !parent_class.upgrades.contains(&class.id) {
                    return Err(format!(
                        "{}: parent {parent} lacks reciprocal upgrade edge",
                        class.id
                    ));
                }
            }
            for destination in &class.upgrades {
                let next_index = *ids
                    .get(destination.as_str())
                    .ok_or_else(|| format!("{}: unknown upgrade {destination}", class.id))?;
                if !self.classes[next_index].parents.contains(&class.id) {
                    return Err(format!(
                        "{}: upgrade {destination} lacks reciprocal parent edge",
                        class.id
                    ));
                }
            }
        }
        let mut visited = vec![false; self.classes.len()];
        let mut active = vec![false; self.classes.len()];
        self.visit(root_index, &ids, &mut visited, &mut active)?;
        if let Some(index) = visited.iter().position(|&seen| !seen) {
            return Err(format!(
                "{} is unreachable from vector",
                self.classes[index].id
            ));
        }
        Ok(())
    }

    fn visit(
        &self,
        index: usize,
        ids: &HashMap<&str, usize>,
        visited: &mut [bool],
        active: &mut [bool],
    ) -> Result<(), String> {
        if active[index] {
            return Err(format!("upgrade cycle at {}", self.classes[index].id));
        }
        if visited[index] {
            return Ok(());
        }
        active[index] = true;
        for next in &self.classes[index].upgrades {
            self.visit(ids[next.as_str()], ids, visited, active)?;
        }
        active[index] = false;
        visited[index] = true;
        Ok(())
    }
}

fn in_range(value: f32, min: f32, max: f32) -> bool {
    value.is_finite() && value >= min && value <= max
}

fn validate_barrel(b: &Barrel) -> Result<(), &'static str> {
    if b.offset.iter().any(|&v| !in_range(v, -100., 100.))
        || !in_range(b.angle, -std::f32::consts::TAU, std::f32::consts::TAU)
        || !in_range(b.length, 4., 100.)
        || !in_range(b.width, 2., 50.)
    {
        return Err("invalid mount geometry");
    }
    if b.target_arc
        .is_some_and(|arc| !in_range(arc, 0.05, std::f32::consts::TAU))
    {
        return Err("invalid automatic turret arc");
    }
    if !in_range(b.cooldown, 0.04, 30.)
        || !in_range(b.delay, 0., b.cooldown)
        || !in_range(b.recoil, 0., 250.)
    {
        return Err("invalid firing timing or recoil");
    }
    if !in_range(b.speed, 0., 2000.)
        || !in_range(b.damage, 0.1, 500.)
        || !in_range(b.radius, 1., 50.)
        || !in_range(b.lifetime, 0.1, 60.)
        || !in_range(b.penetration, 0.1, 1000.)
        || !(1..=12).contains(&b.count)
        || !in_range(b.spread, 0., std::f32::consts::TAU)
        || b.ricochets > 8
    {
        return Err("invalid projectile parameters");
    }
    if b.speed == 0.
        && !matches!(
            b.projectile,
            ProjectileKind::Barrier | ProjectileKind::Turret | ProjectileKind::Mine
        )
    {
        return Err("moving projectiles require a nonzero speed");
    }
    Ok(())
}

fn check_unique_edges(id: &str, edges: &[String]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for edge in edges {
        if edge == id || !seen.insert(edge) {
            return Err(format!(
                "{id}: self-reference or duplicate upgrade edge {edge}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn familiar_paths_remain_connected_and_mechanically_distinct() {
        let content = Content::builtin();
        let basic = content.class("vector").unwrap();
        assert_eq!(basic.name, "Basic Tank");
        for (id, expected) in [
            ("fork", "Twin"),
            ("lance", "Sniper"),
            ("spray", "Machine Gun"),
            ("orbit", "Flank Guard"),
            ("hive", "Overseer"),
            ("anchor", "Trapper"),
            ("hammer", "Destroyer"),
        ] {
            let class = content.class(id).unwrap();
            assert!(basic.upgrades.contains(&class.id), "missing {expected}");
            assert_eq!(class.name, expected);
        }
        assert_ne!(
            content.class("spray").unwrap().behavior_fingerprint(),
            content.class("vector").unwrap().behavior_fingerprint()
        );
        content.validate().unwrap();
    }

    #[test]
    fn classic_and_original_new_evolution_branches_coexist() {
        let content = Content::builtin();
        let basic = content.class("vector").unwrap();
        assert_eq!(
            basic.upgrades[..4]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["fork", "lance", "spray", "orbit"]
        );
        for (id, parent, tier) in [
            ("gunner", "spray", 2),
            ("tri_angle", "orbit", 2),
            ("triplet", "trident", 3),
            ("hybrid", "breach", 3),
            ("necromancer", "flock", 3),
            ("streamliner", "gunner", 3),
            ("afterburner", "tri_angle", 3),
            ("auto_three", "orbit", 2),
            ("auto_five", "auto_three", 3),
            ("manager", "flock", 3),
            ("gunner_trapper", "gunner", 3),
        ] {
            let class = content.class(id).expect("new class must exist");
            assert_eq!(class.tier, tier);
            assert!(class.parents.contains(&parent.to_owned()));
            assert!(
                content
                    .class(parent)
                    .unwrap()
                    .upgrades
                    .contains(&id.to_owned())
            );
        }
        assert_eq!(content.classes.len(), 92);
        content.validate().unwrap();
    }

    #[test]
    fn complete_catalog_has_distinct_reachable_classes() {
        let content = Content::builtin();
        assert!(content.classes.len() >= 60);
        content.validate().unwrap();
        assert_eq!(content.class("vector").unwrap().upgrades.len(), 8);
        assert!(content.classes.iter().filter(|c| c.tier == 3).count() >= 30);
        for class in &content.classes {
            assert_eq!(class.min_level, [1, 15, 30, 45][class.tier as usize]);
        }
    }

    #[test]
    fn catalog_roundtrips_without_a_second_definition_source() {
        let content = Content::builtin();
        let encoded = serde_json::to_string(&content).unwrap();
        let decoded = Content::from_json(&encoded).unwrap();
        assert_eq!(decoded.classes.len(), content.classes.len());
        for (before, after) in content.classes.iter().zip(decoded.classes.iter()) {
            assert_eq!(before.behavior_fingerprint(), after.behavior_fingerprint());
        }
    }

    #[test]
    fn cosmetic_and_damage_only_duplicates_are_rejected() {
        let mut content = Content::builtin();
        let mut duplicate = content.classes[0].clone();
        duplicate.id = "cosmetic_clone".into();
        duplicate.name = "Cosmetic Clone".into();
        duplicate.barrels[0].damage *= 1.5;
        duplicate.hull_sides = 8;
        content.classes.push(duplicate);
        assert!(
            content
                .validate()
                .unwrap_err()
                .contains("identical mechanical")
        );
    }

    #[test]
    fn invalid_edges_and_numbers_fail_without_panicking() {
        let mut content = Content::builtin();
        content.classes[0].upgrades.push("missing".into());
        assert!(content.validate().is_err());
        let mut content = Content::builtin();
        content.classes[0].barrels[0].damage = f32::NAN;
        assert!(content.validate().is_err());
        let mut content = Content::builtin();
        content.classes[1].parents.clear();
        assert!(content.validate().is_err());
        for invalid in [f32::NAN, 0.0, -1.0, 7.0] {
            let mut content = Content::builtin();
            content.classes[0].barrels[0].target_arc = Some(invalid);
            assert!(content.validate().is_err());
        }
    }

    #[test]
    fn all_supported_projectiles_patterns_and_abilities_are_used() {
        let content = Content::builtin();
        for kind in [
            ProjectileKind::Bullet,
            ProjectileKind::Shell,
            ProjectileKind::Piercing,
            ProjectileKind::Scatter,
            ProjectileKind::Drone,
            ProjectileKind::Trap,
            ProjectileKind::Mine,
            ProjectileKind::Explosive,
            ProjectileKind::Fragment,
            ProjectileKind::Homing,
            ProjectileKind::Barrier,
            ProjectileKind::Turret,
            ProjectileKind::Minion,
        ] {
            assert!(
                content
                    .classes
                    .iter()
                    .flat_map(|c| &c.barrels)
                    .any(|b| b.projectile == kind)
            );
        }
        for pattern in [
            FirePattern::Normal,
            FirePattern::Alternating,
            FirePattern::Burst,
            FirePattern::Charge,
            FirePattern::Auto,
            FirePattern::Spin,
        ] {
            assert!(
                content
                    .classes
                    .iter()
                    .flat_map(|c| &c.barrels)
                    .any(|b| b.pattern == pattern)
            );
        }
        for ability in [
            Ability::None,
            Ability::Dash,
            Ability::Shield,
            Ability::Heal,
            Ability::Nova,
            Ability::Recall,
            Ability::Cloak,
            Ability::Overdrive,
            Ability::Blink,
            Ability::Fortify,
            Ability::Magnet,
            Ability::Parry,
        ] {
            assert!(content.classes.iter().any(|c| c.ability == ability));
        }
    }
}

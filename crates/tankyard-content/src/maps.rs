//! Original arena presets; normalized geometry respects server size overrides.
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapProcedure {
    #[default]
    None,
    Maze,
    Dense,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnRegion {
    /// Zero means an unteamed spawn region; one through four select a faction.
    pub team: u8,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamBase {
    pub team: u8,
    pub position: [f32; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Suggested dimensions. Explicit server arena dimensions take precedence.
    pub width: f32,
    pub height: f32,
    /// Recommended capacity; the server's player limit remains authoritative.
    pub capacity: u16,
    pub recommended_modes: Vec<String>,
    /// Normalized x, y, width and height, each within the unit arena rectangle.
    pub walls: Vec<[f32; 4]>,
    pub procedural: MapProcedure,
    pub objective_positions: Vec<[f32; 2]>,
    pub spawn_regions: Vec<SpawnRegion>,
    pub team_bases: Vec<TeamBase>,
    pub background: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapCatalog {
    maps: Vec<MapDef>,
}

pub fn builtin_maps() -> Vec<MapDef> {
    maps_from_json(include_str!("../../../assets/content/maps.json"))
        .expect("compiled-in arena definitions must be valid")
}

pub fn maps_from_json(source: &str) -> Result<Vec<MapDef>, String> {
    let catalog: MapCatalog =
        serde_json::from_str(source).map_err(|error| format!("invalid map JSON: {error}"))?;
    validate_maps(&catalog.maps)?;
    Ok(catalog.maps)
}

pub fn validate_maps(maps: &[MapDef]) -> Result<(), String> {
    if maps.is_empty() || maps.len() > 256 {
        return Err("a map catalog needs between 1 and 256 arenas".into());
    }
    // Wire size alone does not bound geometric work: a compact catalog can
    // request hundreds of large navigation grids against thousands of walls.
    // Reject the whole catalog before starting any connectivity traversal.
    let mut geometry_work = 0_u64;
    for map in maps {
        if !range(map.width, 900., 16000.)
            || !range(map.height, 700., 16000.)
            || map.walls.len() > 2048
        {
            return Err(format!(
                "{}: invalid dimensions or excessive geometry",
                map.id
            ));
        }
        geometry_work += (map.width / 80.).ceil() as u64
            * (map.height / 80.).ceil() as u64
            * map.walls.len().max(1) as u64;
        if geometry_work > 16_000_000 {
            return Err("map catalog exceeds the geometry validation budget".into());
        }
    }
    let mut ids = HashSet::new();
    for map in maps {
        if map.id.is_empty()
            || map.id.len() > 48
            || !map
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || !ids.insert(map.id.as_str())
        {
            return Err(format!("invalid or duplicate map identifier {}", map.id));
        }
        if map.name.trim().is_empty()
            || map.name.len() > 64
            || map.name.chars().any(char::is_control)
            || map.description.trim().is_empty()
            || map.description.len() > 2048
            || map.description.chars().any(char::is_control)
        {
            return Err(format!("{}: invalid arena display text", map.id));
        }
        if !range(map.width, 900., 16000.)
            || !range(map.height, 700., 16000.)
            || !(2..=128).contains(&map.capacity)
        {
            return Err(format!("{}: invalid dimensions or capacity", map.id));
        }
        if map.recommended_modes.is_empty() || map.recommended_modes.len() > 14 {
            return Err(format!(
                "{}: missing or oversized mode recommendations",
                map.id
            ));
        }
        let mut modes = HashSet::new();
        for mode in &map.recommended_modes {
            let canonical = mode.replace('-', "_");
            if !matches!(
                canonical.as_str(),
                "ffa"
                    | "two_teams"
                    | "four_teams"
                    | "maze"
                    | "domination"
                    | "survival"
                    | "tag"
                    | "mothership"
                    | "capture_core"
                    | "boss_rush"
                    | "deathmatch"
                    | "sandbox"
                    | "random_events"
                    | "coop_pve"
            ) || !modes.insert(canonical)
            {
                return Err(format!(
                    "{}: invalid or duplicate mode reference {mode}",
                    map.id
                ));
            }
        }
        if map.walls.len() > 2048
            || map.objective_positions.len() > 32
            || map.spawn_regions.len() > 32
            || map.team_bases.len() > 4
        {
            return Err(format!("{}: excessive arena geometry", map.id));
        }
        for &wall in &map.walls {
            validate_rect(wall).map_err(|error| format!("{} wall: {error}", map.id))?;
        }
        for &position in &map.objective_positions {
            validate_point(position).map_err(|error| format!("{} objective: {error}", map.id))?;
            if !point_clear(position[0] * map.width, position[1] * map.height, map, 42.) {
                return Err(format!("{}: objective overlaps a wall or boundary", map.id));
            }
        }
        if map.spawn_regions.is_empty() {
            return Err(format!("{}: arena needs a spawn region", map.id));
        }
        for region in &map.spawn_regions {
            validate_rect(region.rect).map_err(|error| format!("{} spawn: {error}", map.id))?;
            let [x, y, w, h] = region.rect;
            if region.team > 4
                || w * map.width < 80.
                || h * map.height < 80.
                || !point_clear(
                    (x + w * 0.5) * map.width,
                    (y + h * 0.5) * map.height,
                    map,
                    37.,
                )
            {
                return Err(format!(
                    "{}: spawn region lacks hull clearance or has invalid team",
                    map.id
                ));
            }
        }
        let mut teams = HashSet::new();
        for base in &map.team_bases {
            validate_point(base.position).map_err(|error| format!("{} base: {error}", map.id))?;
            if !(1..=4).contains(&base.team) || !teams.insert(base.team) {
                return Err(format!("{}: invalid or duplicate team base", map.id));
            }
            if !point_clear(
                base.position[0] * map.width,
                base.position[1] * map.height,
                map,
                75.,
            ) {
                return Err(format!("{}: team base lacks clearance", map.id));
            }
            if !map.spawn_regions.iter().any(|region| {
                let [x, y, w, h] = region.rect;
                region.team == base.team
                    && base.position[0] >= x
                    && base.position[0] <= x + w
                    && base.position[1] >= y
                    && base.position[1] <= y + h
            }) {
                return Err(format!(
                    "{}: team base has no matching spawn region",
                    map.id
                ));
            }
        }
        if !matches!(
            map.background.as_str(),
            "slate" | "teal" | "sand" | "violet"
        ) {
            return Err(format!(
                "{}: unknown background palette {}",
                map.id, map.background
            ));
        }
        // Procedural generators validate their generated geometry in core. This
        // pass verifies the complete static geometry declared by every preset.
        validate_connectivity(map)?;
    }
    Ok(())
}

fn range(value: f32, min: f32, max: f32) -> bool {
    value.is_finite() && value >= min && value <= max
}

fn validate_point(position: [f32; 2]) -> Result<(), &'static str> {
    if position.into_iter().all(|v| range(v, 0., 1.)) {
        Ok(())
    } else {
        Err("positions must be finite normalized coordinates")
    }
}

fn validate_rect([x, y, w, h]: [f32; 4]) -> Result<(), &'static str> {
    if range(x, 0., 1.)
        && range(y, 0., 1.)
        && range(w, 0.001, 1.)
        && range(h, 0.001, 1.)
        && x + w <= 1.00001
        && y + h <= 1.00001
    {
        Ok(())
    } else {
        Err("rectangles must be finite, positive and contained in the unit arena")
    }
}

fn point_clear(x: f32, y: f32, map: &MapDef, radius: f32) -> bool {
    if x < radius || y < radius || x > map.width - radius || y > map.height - radius {
        return false;
    }
    map.walls.iter().all(|&[wx, wy, w, h]| {
        let (wx, wy, w, h) = (
            wx * map.width,
            wy * map.height,
            w * map.width,
            h * map.height,
        );
        let nearest_x = x.clamp(wx, wx + w);
        let nearest_y = y.clamp(wy, wy + h);
        (x - nearest_x).powi(2) + (y - nearest_y).powi(2) >= radius * radius
    })
}

fn segment_clear(a: [f32; 2], b: [f32; 2], map: &MapDef) -> bool {
    map.walls.iter().all(|&[x, y, w, h]| {
        let mins = [x * map.width - 30., y * map.height - 30.];
        let maxs = [(x + w) * map.width + 30., (y + h) * map.height + 30.];
        let mut entry: f32 = 0.;
        let mut exit: f32 = 1.;
        for axis in 0..2 {
            let delta = b[axis] - a[axis];
            if delta.abs() < 0.001 {
                if a[axis] < mins[axis] || a[axis] > maxs[axis] {
                    return true;
                }
            } else {
                let first = (mins[axis] - a[axis]) / delta;
                let second = (maxs[axis] - a[axis]) / delta;
                entry = entry.max(first.min(second));
                exit = exit.min(first.max(second));
                if entry > exit {
                    return true;
                }
            }
        }
        false
    })
}

fn validate_connectivity(map: &MapDef) -> Result<(), String> {
    let cols = (map.width / 80.).ceil() as usize;
    let rows = (map.height / 80.).ceil() as usize;
    let center = |index: usize| {
        [
            (index % cols) as f32 * 80. + 40.,
            (index / cols) as f32 * 80. + 40.,
        ]
    };
    let free = (0..cols * rows)
        .map(|index| {
            let [x, y] = center(index);
            // Keep samples strictly outside the link test's expanded walls.
            // Exact tangency must not create an isolated line of "free" cells.
            point_clear(x, y, map, 37.)
        })
        .collect::<Vec<_>>();
    let Some(start) = free.iter().position(|&valid| valid) else {
        return Err(format!("{}: no navigable arena cells", map.id));
    };
    let mut visited = vec![false; free.len()];
    let mut queue = VecDeque::from([start]);
    visited[start] = true;
    while let Some(index) = queue.pop_front() {
        let (x, y) = (index % cols, index / cols);
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx >= cols || ny >= rows {
                continue;
            }
            let next = ny * cols + nx;
            if !visited[next] && free[next] && segment_clear(center(index), center(next), map) {
                visited[next] = true;
                queue.push_back(next);
            }
        }
    }
    if free
        .iter()
        .zip(&visited)
        .any(|(&free, &seen)| free && !seen)
    {
        return Err(format!(
            "{}: static arena has disconnected navigable regions",
            map.id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_catalogs_cannot_request_unbounded_geometry_validation_work() {
        let mut map = builtin_maps().remove(0);
        map.width = 16000.;
        map.height = 16000.;
        map.walls = vec![[0.45, 0.45, 0.001, 0.001]; 2048];
        assert!(
            validate_maps(&[map.clone()])
                .unwrap_err()
                .contains("budget")
        );
        map.walls.truncate(64);
        let maps: Vec<_> = (0..8)
            .map(|index| {
                let mut copy = map.clone();
                copy.id = format!("arena_{index}");
                copy
            })
            .collect();
        assert!(validate_maps(&maps).unwrap_err().contains("budget"));
    }

    #[test]
    fn all_eight_arenas_have_valid_clear_connected_geometry() {
        let maps = builtin_maps();
        assert_eq!(maps.len(), 8);
        validate_maps(&maps).unwrap();
        assert!(maps.iter().any(|map| map.procedural == MapProcedure::Maze));
        assert!(maps.iter().any(|map| map.procedural == MapProcedure::Dense));
        assert_eq!(
            maps.iter()
                .find(|map| map.id == "domination")
                .unwrap()
                .objective_positions
                .len(),
            3
        );
    }

    #[test]
    fn malformed_references_geometry_and_sealed_maps_fail() {
        let mut maps = builtin_maps();
        maps[0].recommended_modes = vec!["imaginary".into()];
        assert!(validate_maps(&maps).is_err());
        let mut maps = builtin_maps();
        maps[0].width = f32::INFINITY;
        assert!(validate_maps(&maps).is_err());
        let mut maps = builtin_maps();
        maps[0].walls = vec![[0.49, 0., 0.02, 1.]];
        maps[0].spawn_regions[0].rect = [0.02, 0.02, 0.2, 0.2];
        assert!(validate_maps(&maps).unwrap_err().contains("disconnected"));
        let mut maps = builtin_maps();
        maps.push(maps[0].clone());
        assert!(validate_maps(&maps).unwrap_err().contains("duplicate"));
    }

    #[test]
    fn map_catalog_roundtrips_through_the_public_loader() {
        let catalog = MapCatalog {
            maps: builtin_maps(),
        };
        let json = serde_json::to_string(&catalog).unwrap();
        let maps = maps_from_json(&json).unwrap();
        assert_eq!(maps.len(), 8);
    }
}

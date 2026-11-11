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
    /// Groups the editor will not select, as a mask. Editing only.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub locked: u32,
}
fn is_zero(mask: &u32) -> bool {
    *mask == 0
}
/// An object's place in the document. Keys order as the editor draws:
/// walls, blocks, pads, decorations, triggers, then markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
#[derive(Clone, Debug, PartialEq)]
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
    /// The group this object belongs to. A trigger belongs to its `member`
    /// group (hidden means switched off); the group it acts on is separate.
    pub fn group(&self) -> u8 {
        match self {
            Self::Block(o) => o.group,
            Self::Zone(o) => o.group,
            Self::Decor(o) => o.group,
            Self::Trigger(o) => o.member,
            _ => 0,
        }
    }
    pub fn set_group(&mut self, group: u8) {
        match self {
            Self::Block(o) => o.group = group,
            Self::Zone(o) => o.group = group,
            Self::Decor(o) => o.group = group,
            Self::Trigger(o) => o.member = group,
            _ => {}
        }
    }
    /// Whether the object can join a group at all.
    pub fn groupable(&self) -> bool {
        matches!(
            self,
            Self::Block(_) | Self::Zone(_) | Self::Decor(_) | Self::Trigger(_)
        )
    }
    /// Team bases and objectives are points; their rectangle is a handle.
    pub fn is_point(&self) -> bool {
        matches!(self, Self::Base(_) | Self::Objective(_))
    }
    /// Same variant and, for pads and decorations, the same kind.
    pub fn same_kind(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Zone(a), Self::Zone(b)) => a.kind == b.kind,
            (Self::Decor(a), Self::Decor(b)) => a.kind == b.kind,
            (Self::Trigger(a), Self::Trigger(b)) => a.action == b.action,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
    /// Copy everything but position and size from `source`, when it is the
    /// same variant. Returns whether anything could be copied.
    pub fn paste_style(&mut self, source: &Self) -> bool {
        let rect = self.rect();
        match (&mut *self, source) {
            (Self::Block(a), Self::Block(b)) => *a = b.clone(),
            (Self::Zone(a), Self::Zone(b)) => *a = b.clone(),
            (Self::Decor(a), Self::Decor(b)) => *a = b.clone(),
            (Self::Trigger(a), Self::Trigger(b)) => *a = b.clone(),
            (Self::Spawn(a), Self::Spawn(b)) => a.team = b.team,
            _ => return false,
        }
        self.set_rect(rect);
        true
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
/// A quarter turn or mirror applied to a selection about its center.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transform {
    /// Clockwise on screen.
    TurnRight,
    TurnLeft,
    FlipX,
    FlipY,
}
impl Transform {
    fn turns(self) -> bool {
        matches!(self, Self::TurnRight | Self::TurnLeft)
    }
    /// A point in world units about `center`.
    fn point(self, p: [f32; 2], center: [f32; 2]) -> [f32; 2] {
        let d = [p[0] - center[0], p[1] - center[1]];
        let d = match self {
            Self::TurnRight => [-d[1], d[0]],
            Self::TurnLeft => [d[1], -d[0]],
            Self::FlipX => [-d[0], d[1]],
            Self::FlipY => [d[0], -d[1]],
        };
        [center[0] + d[0], center[1] + d[1]]
    }
    /// A direction in degrees (0 right, 90 down).
    fn angle(self, degrees: f32) -> f32 {
        match self {
            Self::TurnRight => degrees + 90.,
            Self::TurnLeft => degrees - 90.,
            Self::FlipX => 180. - degrees,
            Self::FlipY => -degrees,
        }
        .rem_euclid(360.)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}
/// Something the author should fix (`blocking`) or may want to know.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub key: Option<Key>,
    pub text: String,
    pub blocking: bool,
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
            locked: 0,
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
        if doc.format != 1 {
            return Err("Unsupported editor format or document limits exceeded.".into());
        }
        doc.check_draft()?;
        Ok(doc)
    }
    /// The limits every draft keeps, playable or not: sizes, counts,
    /// coordinates, rules and drawn features. Imports and plugin tools pass
    /// through here; full validation runs on playtest and export.
    pub fn check_draft(&self) -> Result<(), String> {
        let doc = self;
        if doc.map.id.len() > 48
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
        validate_map_features(&preview_map)
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
        self.items().map(|(key, _, _)| key).collect()
    }
    /// Every object's key, rectangle and group in drawing order, without
    /// copying any object.
    pub fn items(&self) -> impl DoubleEndedIterator<Item = (Key, [f32; 4], u8)> + '_ {
        let m = &self.map;
        let point = |p: [f32; 2]| [p[0] - 0.01, p[1] - 0.01, 0.02, 0.02];
        (m.walls
            .iter()
            .enumerate()
            .map(|(i, r)| (Key::Wall(i), *r, 0)))
        .chain(
            m.blocks
                .iter()
                .enumerate()
                .map(|(i, o)| (Key::Block(i), o.rect, o.group)),
        )
        .chain(
            m.zones
                .iter()
                .enumerate()
                .map(|(i, o)| (Key::Zone(i), o.rect, o.group)),
        )
        .chain(
            m.decor
                .iter()
                .enumerate()
                .map(|(i, o)| (Key::Decor(i), o.rect, o.group)),
        )
        .chain(
            m.triggers
                .iter()
                .enumerate()
                .map(|(i, o)| (Key::Trigger(i), o.rect, o.member)),
        )
        .chain(
            m.spawn_regions
                .iter()
                .enumerate()
                .map(|(i, o)| (Key::Spawn(i), o.rect, 0)),
        )
        .chain(
            m.team_bases
                .iter()
                .enumerate()
                .map(move |(i, o)| (Key::Base(i), point(o.position), 0)),
        )
        .chain(
            m.objective_positions
                .iter()
                .enumerate()
                .map(move |(i, p)| (Key::Objective(i), point(*p), 0)),
        )
    }
    pub fn len(&self) -> usize {
        let m = &self.map;
        m.walls.len()
            + m.blocks.len()
            + m.zones.len()
            + m.decor.len()
            + m.triggers.len()
            + m.spawn_regions.len()
            + m.team_bases.len()
            + m.objective_positions.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// One object's rectangle without copying it.
    pub fn rect(&self, key: Key) -> Option<[f32; 4]> {
        let m = &self.map;
        let point = |p: [f32; 2]| [p[0] - 0.01, p[1] - 0.01, 0.02, 0.02];
        Some(match key {
            Key::Wall(i) => *m.walls.get(i)?,
            Key::Block(i) => m.blocks.get(i)?.rect,
            Key::Zone(i) => m.zones.get(i)?.rect,
            Key::Decor(i) => m.decor.get(i)?.rect,
            Key::Trigger(i) => m.triggers.get(i)?.rect,
            Key::Spawn(i) => m.spawn_regions.get(i)?.rect,
            Key::Base(i) => point(m.team_bases.get(i)?.position),
            Key::Objective(i) => point(*m.objective_positions.get(i)?),
        })
    }
    /// The smallest rectangle around the objects of `keys`.
    pub fn bounds(&self, keys: &[Key]) -> Option<[f32; 4]> {
        let mut rects = keys.iter().filter_map(|&key| self.rect(key));
        let first = rects.next()?;
        let (mut lo, mut hi) = (
            [first[0], first[1]],
            [first[0] + first[2], first[1] + first[3]],
        );
        for r in rects {
            lo = [lo[0].min(r[0]), lo[1].min(r[1])];
            hi = [hi[0].max(r[0] + r[2]), hi[1].max(r[1] + r[3])];
        }
        Some([lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]])
    }
    fn edit_each(&mut self, keys: &[Key], mut change: impl FnMut(&mut Object, [f32; 4])) {
        for &key in keys {
            if let Some(mut object) = self.object(key) {
                let rect = object.rect();
                change(&mut object, rect);
                self.replace(key, object);
            }
        }
    }
    /// Move the objects of `keys` by a normalized offset, as far as the whole
    /// selection stays inside the arena. Returns whether anything moved.
    pub fn nudge(&mut self, keys: &[Key], dx: f32, dy: f32) -> bool {
        let Some(b) = self.bounds(keys) else {
            return false;
        };
        let dx = dx.clamp(-b[0], (1. - b[0] - b[2]).max(0.));
        let dy = dy.clamp(-b[1], (1. - b[1] - b[3]).max(0.));
        if dx == 0. && dy == 0. {
            return false;
        }
        self.edit_each(keys, |object, r| {
            object.set_rect([r[0] + dx, r[1] + dy, r[2], r[3]]);
        });
        true
    }
    /// Turn or mirror the objects of `keys` about the selection's center, in
    /// world units so non-square arenas keep their proportions. Pad
    /// directions, portal exits and decoration angles turn with them.
    pub fn transform(&mut self, keys: &[Key], transform: Transform) {
        let Some(b) = self.bounds(keys) else {
            return;
        };
        let (w, h) = (self.map.width, self.map.height);
        self.transform_about(
            keys,
            transform,
            [(b[0] + b[2] * 0.5) * w, (b[1] + b[3] * 0.5) * h],
        );
    }
    /// Copies of the objects of `keys` mirrored across the arena's middle
    /// line (FlipX) or its horizon (FlipY); turns mirror like flips. Returns
    /// the copies' keys, which stop at a limit.
    pub fn mirror_copy(&mut self, keys: &[Key], axis: Transform) -> Vec<Key> {
        let objects: Vec<Object> = keys.iter().filter_map(|&key| self.object(key)).collect();
        let copies: Vec<Key> = objects
            .into_iter()
            .filter_map(|object| self.insert(object))
            .collect();
        let center = [self.map.width * 0.5, self.map.height * 0.5];
        self.transform_about(&copies, axis, center);
        copies
    }
    /// Stretch the objects of `originals` from the bounds `from` to `to`.
    /// Points move with the stretch but keep their size.
    pub fn stretch(&mut self, originals: &[(Key, Object)], from: [f32; 4], to: [f32; 4]) {
        let scale = [to[2] / from[2].max(1e-6), to[3] / from[3].max(1e-6)];
        for (key, original) in originals {
            let r = original.rect();
            let mut object = original.clone();
            if object.is_point() {
                let mid = [
                    to[0] + (r[0] + r[2] * 0.5 - from[0]) * scale[0],
                    to[1] + (r[1] + r[3] * 0.5 - from[1]) * scale[1],
                ];
                object.set_rect([mid[0] - r[2] * 0.5, mid[1] - r[3] * 0.5, r[2], r[3]]);
            } else {
                object.set_rect([
                    to[0] + (r[0] - from[0]) * scale[0],
                    to[1] + (r[1] - from[1]) * scale[1],
                    r[2] * scale[0],
                    r[3] * scale[1],
                ]);
            }
            self.replace(*key, object);
        }
    }
    /// Turn or mirror the objects of `keys` about `center`, in world units.
    fn transform_about(&mut self, keys: &[Key], transform: Transform, center: [f32; 2]) {
        let (w, h) = (self.map.width, self.map.height);
        let mirror = !transform.turns();
        self.edit_each(keys, |object, r| {
            let mid = transform.point([(r[0] + r[2] * 0.5) * w, (r[1] + r[3] * 0.5) * h], center);
            let mut size = [r[2] * w, r[3] * h];
            // Directional decorations turn by angle; everything else, and the
            // symmetric decorations, by swapping width and height.
            let by_angle = matches!(object, Object::Decor(d)
                if matches!(d.kind, DecorKind::Stripes | DecorKind::Arrow | DecorKind::Label));
            if transform.turns() && !by_angle {
                size.swap(0, 1);
            }
            object.set_rect([
                (mid[0] - size[0] * 0.5) / w,
                (mid[1] - size[1] * 0.5) / h,
                size[0] / w,
                size[1] / h,
            ]);
            match object {
                Object::Zone(zone) if zone.kind == ZoneKind::Conveyor => {
                    zone.angle = transform.angle(zone.angle);
                }
                Object::Zone(zone) if zone.kind == ZoneKind::Portal => {
                    let exit = transform.point([zone.exit[0] * w, zone.exit[1] * h], center);
                    zone.exit = [(exit[0] / w).clamp(0., 1.), (exit[1] / h).clamp(0., 1.)];
                }
                Object::Decor(decor) => {
                    match decor.kind {
                        DecorKind::Arrow => decor.rotation = transform.angle(decor.rotation),
                        DecorKind::Emitter => {
                            let aim = decor.effect.map_or(0., |effect| effect.direction);
                            decor.rotation =
                                (transform.angle(decor.rotation + aim) - aim).rem_euclid(360.);
                        }
                        _ if mirror => decor.rotation = (-decor.rotation).rem_euclid(360.),
                        _ if by_angle => decor.rotation = transform.angle(decor.rotation),
                        _ => {}
                    }
                    if mirror {
                        decor.spin = -decor.spin;
                    }
                }
                _ => {}
            }
        });
    }
    /// Grow or shrink the objects of `keys` about the selection's center.
    pub fn scale(&mut self, keys: &[Key], factor: f32) {
        let Some(b) = self.bounds(keys) else {
            return;
        };
        if !factor.is_finite() || factor <= 0. {
            return;
        }
        let center = [b[0] + b[2] * 0.5, b[1] + b[3] * 0.5];
        self.edit_each(keys, |object, r| {
            let size = if object.is_point() {
                [r[2], r[3]]
            } else {
                [r[2] * factor, r[3] * factor]
            };
            let mid = [
                center[0] + (r[0] + r[2] * 0.5 - center[0]) * factor,
                center[1] + (r[1] + r[3] * 0.5 - center[1]) * factor,
            ];
            object.set_rect([
                mid[0] - size[0] * 0.5,
                mid[1] - size[1] * 0.5,
                size[0],
                size[1],
            ]);
        });
    }
    /// Line the objects of `keys` up with an edge or center of the selection.
    pub fn align(&mut self, keys: &[Key], align: Align) {
        let Some(b) = self.bounds(keys) else {
            return;
        };
        self.edit_each(keys, |object, mut r| {
            match align {
                Align::Left => r[0] = b[0],
                Align::CenterX => r[0] = b[0] + (b[2] - r[2]) * 0.5,
                Align::Right => r[0] = b[0] + b[2] - r[2],
                Align::Top => r[1] = b[1],
                Align::CenterY => r[1] = b[1] + (b[3] - r[3]) * 0.5,
                Align::Bottom => r[1] = b[1] + b[3] - r[3],
            }
            object.set_rect(r);
        });
    }
    /// Move the objects of `keys` to the end of their lists (drawn on top and,
    /// for triggers, run last) or the start. Returns their new keys.
    pub fn order(&mut self, keys: &[Key], front: bool) -> Vec<Key> {
        fn reorder<T>(items: &mut Vec<T>, chosen: &[usize], front: bool) -> std::ops::Range<usize> {
            let (mut picked, mut rest) = (Vec::new(), Vec::new());
            for (index, item) in items.drain(..).enumerate() {
                if chosen.contains(&index) {
                    picked.push(item);
                } else {
                    rest.push(item);
                }
            }
            let (count, others) = (picked.len(), rest.len());
            if front {
                rest.append(&mut picked);
                *items = rest;
                others..others + count
            } else {
                picked.append(&mut rest);
                *items = picked;
                0..count
            }
        }
        let indices = |wanted: fn(Key) -> Option<usize>| -> Vec<usize> {
            keys.iter().filter_map(|&key| wanted(key)).collect()
        };
        let m = &mut self.map;
        let mut moved = Vec::new();
        macro_rules! list {
            ($items:expr, $variant:ident) => {
                let chosen = indices(|key| match key {
                    Key::$variant(i) => Some(i),
                    _ => None,
                });
                if !chosen.is_empty() {
                    moved.extend(reorder(&mut $items, &chosen, front).map(Key::$variant));
                }
            };
        }
        list!(m.walls, Wall);
        list!(m.blocks, Block);
        list!(m.zones, Zone);
        list!(m.decor, Decor);
        list!(m.triggers, Trigger);
        list!(m.spawn_regions, Spawn);
        list!(m.team_bases, Base);
        list!(m.objective_positions, Objective);
        moved
    }
    /// Add `objects` so the top-left of their bounds lands at `at` (clamped
    /// inside the arena). Returns the keys added, which stop at a limit.
    pub fn place(&mut self, objects: &[Object], at: [f32; 2]) -> Vec<Key> {
        let mut rects = objects.iter().map(Object::rect);
        let Some(first) = rects.next() else {
            return Vec::new();
        };
        let (mut lo, mut hi) = (
            [first[0], first[1]],
            [first[0] + first[2], first[1] + first[3]],
        );
        for r in rects {
            lo = [lo[0].min(r[0]), lo[1].min(r[1])];
            hi = [hi[0].max(r[0] + r[2]), hi[1].max(r[1] + r[3])];
        }
        let size = [hi[0] - lo[0], hi[1] - lo[1]];
        let at = [
            at[0].clamp(0., (1. - size[0]).max(0.)),
            at[1].clamp(0., (1. - size[1]).max(0.)),
        ];
        let mut added = Vec::new();
        for object in objects {
            let mut object = object.clone();
            let r = object.rect();
            object.set_rect([r[0] - lo[0] + at[0], r[1] - lo[1] + at[1], r[2], r[3]]);
            match self.insert(object) {
                Some(key) => added.push(key),
                None => continue,
            }
        }
        added
    }
    /// Groups anything refers to, as a mask: members, trigger targets and
    /// the groups hidden at the start.
    pub fn used_groups(&self) -> u32 {
        let m = &self.map;
        let mut mask = m.starting_mask();
        for (_, _, group) in self.items() {
            mask |= MapDef::group_bit(group);
        }
        for trigger in &m.triggers {
            if trigger.action.targets_group() {
                mask |= MapDef::group_bit(trigger.group);
            }
        }
        mask
    }
    /// The lowest group nothing uses yet.
    pub fn next_free_group(&self) -> Option<u8> {
        let used = self.used_groups();
        (1..=MAP_GROUPS).find(|&group| MapDef::group_bit(group) & used == 0)
    }
    /// Every object in `group`.
    pub fn in_group(&self, group: u8) -> Vec<Key> {
        self.items()
            .filter(|&(_, _, g)| g == group)
            .map(|(key, _, _)| key)
            .collect()
    }
    /// Every object of the same kind as `key`.
    pub fn like(&self, key: Key) -> Vec<Key> {
        let Some(model) = self.object(key) else {
            return Vec::new();
        };
        self.items()
            .map(|(key, _, _)| key)
            .filter(|&other| {
                std::mem::discriminant(&other) == std::mem::discriminant(&key)
                    && self.object(other).is_some_and(|o| o.same_kind(&model))
            })
            .collect()
    }
    /// Specific problems with keys to show them, blocking ones first.
    pub fn problems(&self) -> Vec<Problem> {
        let m = &self.map;
        let mut problems = Vec::new();
        let mut note = |key: Option<Key>, text: String, blocking: bool| {
            problems.push(Problem {
                key,
                text,
                blocking,
            })
        };
        for (index, trigger) in m.triggers.iter().enumerate() {
            let key = Some(Key::Trigger(index));
            let name = format!("Trigger {}", index + 1);
            if let Err(error) = validate_trigger(trigger) {
                note(key, format!("{name}: {error}."), true);
                continue;
            }
            let members = |group: u8| self.items().any(|(_, _, g)| g == group);
            if trigger.action.targets_group() && !members(trigger.group) {
                note(
                    key,
                    format!("{name} acts on group {}, which is empty.", trigger.group),
                    false,
                );
            }
            let senders = |channel: u8| {
                m.triggers.iter().any(|t| match t.action {
                    TriggerAction::Signal => t.group == channel,
                    TriggerAction::Random => (i32::from(t.group)..i32::from(t.group) + t.amount)
                        .contains(&i32::from(channel)),
                    _ => false,
                })
            };
            let counted = |counter: u8| {
                m.triggers
                    .iter()
                    .any(|t| t.action.targets_counter() && t.group == counter)
            };
            match trigger.when {
                TriggerWhen::Signal if !senders(trigger.source) => note(
                    key,
                    format!(
                        "{name} waits for signal {}, which nothing sends.",
                        trigger.source
                    ),
                    false,
                ),
                TriggerWhen::Counter if !counted(trigger.source) => note(
                    key,
                    format!(
                        "{name} watches counter {}, which nothing changes.",
                        trigger.source
                    ),
                    false,
                ),
                _ => {}
            }
            if let Some(test) = &trigger.only_if
                && test.kind == TestKind::Counter
            {
                let other = (test.against != test.index).then_some(test.against);
                for counter in [Some(test.index), other].into_iter().flatten() {
                    if counter > 0 && !counted(counter) {
                        note(
                            key,
                            format!(
                                "{name} checks counter {counter}, which nothing changes, so its gate never moves."
                            ),
                            false,
                        );
                    }
                }
            }
            if trigger.action == TriggerAction::Signal
                && !m
                    .triggers
                    .iter()
                    .any(|t| t.when == TriggerWhen::Signal && t.source == trigger.group)
            {
                note(
                    key,
                    format!(
                        "{name} sends signal {}, which nothing receives.",
                        trigger.group
                    ),
                    false,
                );
            }
            if trigger.member != 0
                && trigger.member == trigger.group
                && trigger.action.targets_group()
            {
                note(
                    key,
                    format!("{name} switches off its own group {}.", trigger.member),
                    false,
                );
            }
        }
        for (index, zone) in m.zones.iter().enumerate() {
            if zone.kind == ZoneKind::Portal && !portal_exit_clear(m, zone.exit) {
                note(
                    Some(Key::Zone(index)),
                    format!(
                        "Portal {}: its exit is inside cover or too near the edge.",
                        index + 1
                    ),
                    true,
                );
            }
        }
        if m.spawn_regions.is_empty() {
            note(None, "Add a spawn region so tanks can enter.".into(), true);
        }
        if !problems.iter().any(|p| p.blocking)
            && let Err(error) = self.validate()
        {
            problems.push(Problem {
                key: None,
                text: sentence(&error),
                blocking: true,
            });
        }
        problems.sort_by_key(|p| !p.blocking);
        problems
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
        let mut keys = keys.to_vec();
        keys.sort_unstable();
        keys.dedup();
        // Reverse order keeps all indices valid while removing a batch.
        for key in keys.into_iter().rev() {
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

fn sentence(text: &str) -> String {
    let mut out = text.trim().to_owned();
    if let Some(first) = out.get(..1) {
        out.replace_range(..1, &first.to_uppercase());
    }
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}
fn seconds(value: f32) -> String {
    if value.fract().abs() < 0.01 {
        format!("{value:.0} s")
    } else {
        format!("{value:.1} s")
    }
}
fn boss_name(id: &str) -> &'static str {
    tankyard_core::BossKind::from_id(id).map_or("unknown boss", tankyard_core::BossKind::name)
}
/// A trigger in one plain sentence, for the inspector and logic list.
/// A valid map id made from a display name: "Switch Yard 2" → "switch_yard_2".
pub fn map_id(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            id.push(c);
        } else if !id.is_empty() && !id.ends_with('_') {
            id.push('_');
        }
    }
    id.truncate(48);
    let id = id.trim_end_matches('_');
    if id.is_empty() {
        "my_arena".to_owned()
    } else {
        id.to_owned()
    }
}

pub fn describe_trigger(t: &MapTrigger) -> String {
    let who = match t.who {
        TriggerWho::Anyone => "a tank",
        TriggerWho::Players => "a player",
        TriggerWho::Bots => "a bot",
    };
    let team = if t.team > 0 {
        format!(" from team {}", t.team)
    } else {
        String::new()
    };
    let mut text = match t.when {
        TriggerWhen::Every if t.offset > 0. => format!(
            "Every {}, first at {}",
            seconds(t.seconds),
            seconds(t.offset + t.seconds)
        ),
        TriggerWhen::Every => format!("Every {}", seconds(t.seconds)),
        TriggerWhen::Enter => format!("When {who}{team} enters"),
        TriggerWhen::Exit => format!("When {who}{team} leaves"),
        TriggerWhen::Start if t.seconds > 0. => format!("{} into the round", seconds(t.seconds)),
        TriggerWhen::Start => "When the round starts".into(),
        TriggerWhen::Signal => format!("On signal {}", t.source),
        TriggerWhen::Counter => format!("When counter {} reaches {}", t.source, t.threshold),
        TriggerWhen::Occupied if t.threshold > 1 => format!(
            "Every {} while {} tanks{team} are inside",
            seconds(t.seconds),
            t.threshold
        ),
        TriggerWhen::Occupied => {
            format!("Every {} while {who}{team} is inside", seconds(t.seconds))
        }
        TriggerWhen::Cleared => format!("When the last {}{team} leaves", &who[2..]),
        TriggerWhen::Destroyed => format!("When {who}{team} is destroyed here"),
    };
    if let Some(test) = &t.only_if {
        text += &format!(", if {}", describe_test(test));
    }
    if t.chance < 100 {
        text += &format!(", {}% of the time", t.chance);
    }
    text += ": ";
    text += &match t.action {
        TriggerAction::Toggle => format!("toggle group {}", t.group),
        TriggerAction::Show => format!("show group {}", t.group),
        TriggerAction::Hide => format!("hide group {}", t.group),
        TriggerAction::Announce => format!("announce \u{201c}{}\u{201d}", t.text.trim()),
        TriggerAction::Boss => format!("bring the {} here", boss_name(&t.boss)),
        TriggerAction::Signal => format!("send signal {}", t.group),
        TriggerAction::Count if t.amount < 0 => {
            format!("take {} from counter {}", t.amount.unsigned_abs(), t.group)
        }
        TriggerAction::Count => format!("add {} to counter {}", t.amount, t.group),
        TriggerAction::Shapes => format!("spawn {} shapes here", t.amount),
        TriggerAction::Set => format!("set counter {} to {}", t.group, t.amount),
        TriggerAction::Random => format!(
            "send one of signals {} to {}",
            t.group,
            i32::from(t.group) + t.amount - 1
        ),
        TriggerAction::Heal => format!("repair {}% of health here", t.amount),
        TriggerAction::Hurt => format!("deal {}% of health here", t.amount),
        TriggerAction::Award => format!("award {} points here", t.amount),
        TriggerAction::Calculate => describe_math(t),
        TriggerAction::Script => format!("raise script event \u{201c}{}\u{201d}", t.text.trim()),
    };
    if t.action.targets_group() && t.duration > 0. {
        text += &format!(" for {}", seconds(t.duration));
    }
    if t.delay > 0. {
        text += &format!(", {} later", seconds(t.delay));
    }
    text.push('.');
    if t.when.watches_area() && t.when != TriggerWhen::Occupied && t.seconds > 0. {
        text += &format!(" Rests {} after acting.", seconds(t.seconds));
    }
    if t.once {
        text += " Once per round.";
    }
    if t.member > 0 {
        text += &format!(" Off while group {} is hidden.", t.member);
    }
    text
}
/// A gate in words: "counter 1 is at least 3".
pub fn describe_test(test: &TriggerTest) -> String {
    match test.kind {
        TestKind::Counter if test.against > 0 => format!(
            "counter {} {} counter {}",
            test.index,
            test.compare.words(),
            test.against
        ),
        TestKind::Counter => format!(
            "counter {} {} {}",
            test.index,
            test.compare.words(),
            test.value
        ),
        TestKind::Shown => format!("group {} is shown", test.index),
        TestKind::Hidden => format!("group {} is hidden", test.index),
    }
}
/// A gate in a few characters: "#1≥3", "G4 on".
fn test_tag(test: &TriggerTest) -> String {
    match test.kind {
        TestKind::Counter if test.against > 0 => {
            format!("#{}{}#{}", test.index, test.compare.symbol(), test.against)
        }
        TestKind::Counter => format!("#{}{}{}", test.index, test.compare.symbol(), test.value),
        TestKind::Shown => format!("G{} on", test.index),
        TestKind::Hidden => format!("G{} off", test.index),
    }
}
/// What Calculate does, in words.
fn describe_math(t: &MapTrigger) -> String {
    let (c, x) = (
        t.group,
        if t.operand > 0 {
            format!("counter {}", t.operand)
        } else {
            t.amount.to_string()
        },
    );
    match t.math {
        MathOp::Add => format!("add {x} to counter {c}"),
        MathOp::Subtract => format!("take {x} from counter {c}"),
        MathOp::Multiply => format!("multiply counter {c} by {x}"),
        MathOp::Divide => format!("divide counter {c} by {x}, rounding down"),
        MathOp::Remainder => format!("keep counter {c}'s remainder after dividing by {x}"),
        MathOp::Lowest => format!("lower counter {c} to {x} if it is above"),
        MathOp::Highest => format!("raise counter {c} to {x} if it is below"),
        MathOp::Copy => format!("set counter {c} to {x}"),
        MathOp::Random => format!("set counter {c} to a random number from 0 below {x}"),
        MathOp::Flip => format!("flip counter {c} between 0 and 1"),
    }
}
/// A trigger in a few characters, for its badge on the canvas.
pub fn trigger_tag(t: &MapTrigger) -> String {
    let when = match t.when {
        TriggerWhen::Every => seconds(t.seconds).replace(' ', ""),
        TriggerWhen::Enter => "Enter".into(),
        TriggerWhen::Exit => "Exit".into(),
        TriggerWhen::Start => "Start".into(),
        TriggerWhen::Signal => format!("Signal {}", t.source),
        TriggerWhen::Counter => format!("#{} = {}", t.source, t.threshold),
        TriggerWhen::Occupied => format!("Inside {}", seconds(t.seconds).replace(' ', "")),
        TriggerWhen::Cleared => "Empty".into(),
        TriggerWhen::Destroyed => "Destroy".into(),
    };
    let action = match t.action {
        TriggerAction::Toggle => format!("Toggle {}", t.group),
        TriggerAction::Show => format!("Show {}", t.group),
        TriggerAction::Hide => format!("Hide {}", t.group),
        TriggerAction::Announce => "Say".into(),
        TriggerAction::Boss => "Boss".into(),
        TriggerAction::Signal => format!("Send {}", t.group),
        TriggerAction::Count => format!("#{} {:+}", t.group, t.amount),
        TriggerAction::Shapes => format!("{} shapes", t.amount),
        TriggerAction::Set => format!("#{} = {}", t.group, t.amount),
        TriggerAction::Random => format!(
            "Send {}\u{2013}{}",
            t.group,
            i32::from(t.group) + t.amount - 1
        ),
        TriggerAction::Heal => format!("+{}%", t.amount),
        TriggerAction::Hurt => format!("\u{2212}{}%", t.amount),
        TriggerAction::Award => format!("+{} pts", t.amount),
        TriggerAction::Calculate => {
            let x = if t.operand > 0 {
                format!("#{}", t.operand)
            } else {
                t.amount.to_string()
            };
            match t.math {
                MathOp::Add => format!("#{} += {x}", t.group),
                MathOp::Subtract => format!("#{} \u{2212}= {x}", t.group),
                MathOp::Multiply => format!("#{} \u{d7}= {x}", t.group),
                MathOp::Divide => format!("#{} \u{f7}= {x}", t.group),
                MathOp::Remainder => format!("#{} %= {x}", t.group),
                MathOp::Lowest => format!("#{} min {x}", t.group),
                MathOp::Highest => format!("#{} max {x}", t.group),
                MathOp::Copy => format!("#{} = {x}", t.group),
                MathOp::Random => format!("#{} = rnd {x}", t.group),
                MathOp::Flip => format!("#{} flip", t.group),
            }
        }
        TriggerAction::Script => format!("Script {}", t.text.trim()),
    };
    match &t.only_if {
        Some(test) => format!("{when} if {} \u{2192} {action}", test_tag(test)),
        None => format!("{when} \u{2192} {action}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_ids_follow_names_within_the_id_rules() {
        assert_eq!(map_id("Switch Yard 2"), "switch_yard_2");
        assert_eq!(map_id("  Ève's -- Arena!  "), "ve_s_arena");
        assert_eq!(map_id("???"), "my_arena");
        assert_eq!(map_id(&"a".repeat(80)).len(), 48);
        assert_eq!(map_id(&format!("{}_b", "a".repeat(47))), "a".repeat(47));
    }
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
    fn block(rect: [f32; 4]) -> Object {
        Object::Block(MapBlock {
            rect,
            style: BlockStyle::Slate,
            color: None,
            group: 0,
        })
    }
    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }
    #[test]
    fn turning_keeps_world_proportions_and_returns_after_four_turns() {
        let mut doc = Document::default();
        let a = doc.insert(block([0.2, 0.2, 0.1, 0.05])).unwrap();
        let b = doc.insert(block([0.4, 0.2, 0.05, 0.05])).unwrap();
        let before = [doc.rect(a).unwrap(), doc.rect(b).unwrap()];
        doc.transform(&[a, b], Transform::TurnRight);
        let turned = doc.rect(a).unwrap();
        let (w, h) = (doc.map.width, doc.map.height);
        assert!(
            (turned[2] * w - 0.05 * h).abs() < 0.01,
            "width took the height"
        );
        assert!((turned[3] * h - 0.1 * w).abs() < 0.01);
        for _ in 0..3 {
            doc.transform(&[a, b], Transform::TurnRight);
        }
        assert!(close(doc.rect(a).unwrap(), before[0]));
        assert!(close(doc.rect(b).unwrap(), before[1]));
    }
    #[test]
    fn flips_mirror_a_selection_and_its_directions() {
        let mut doc = Document::default();
        let a = doc.insert(block([0.1, 0.4, 0.1, 0.1])).unwrap();
        let pad = doc
            .insert(Object::Zone(MapZone {
                kind: ZoneKind::Conveyor,
                rect: [0.5, 0.4, 0.1, 0.1],
                strength: 1.,
                group: 0,
                angle: 0.,
                exit: [0., 0.],
            }))
            .unwrap();
        doc.transform(&[a, pad], Transform::FlipX);
        assert!(close(doc.rect(a).unwrap(), [0.5, 0.4, 0.1, 0.1]));
        assert!(close(doc.rect(pad).unwrap(), [0.1, 0.4, 0.1, 0.1]));
        let Some(Object::Zone(zone)) = doc.object(pad) else {
            panic!()
        };
        assert_eq!(zone.angle, 180.);
    }
    #[test]
    fn nudging_stops_the_whole_selection_at_the_edge() {
        let mut doc = Document::default();
        let a = doc.insert(block([0.05, 0.5, 0.1, 0.1])).unwrap();
        let b = doc.insert(block([0.25, 0.5, 0.1, 0.1])).unwrap();
        assert!(doc.nudge(&[a, b], -0.1, 0.));
        assert!(!doc.nudge(&[a, b], -0.1, 0.), "already at the edge");
        assert!(
            close(doc.rect(b).unwrap(), [0.2, 0.5, 0.1, 0.1]),
            "spacing kept"
        );
    }
    #[test]
    fn ordering_returns_the_moved_keys() {
        let mut doc = Document::default();
        for i in 0..4 {
            doc.insert(block([0.1 * i as f32, 0.1, 0.05, 0.05]));
        }
        let moved = doc.order(&[Key::Block(0), Key::Block(2)], true);
        assert_eq!(moved, vec![Key::Block(2), Key::Block(3)]);
        assert!(close(
            doc.rect(Key::Block(3)).unwrap(),
            [0.2, 0.1, 0.05, 0.05]
        ));
        let moved = doc.order(&[Key::Block(3)], false);
        assert_eq!(moved, vec![Key::Block(0)]);
        assert!(close(
            doc.rect(Key::Block(0)).unwrap(),
            [0.2, 0.1, 0.05, 0.05]
        ));
    }
    #[test]
    fn placing_keeps_layout_and_stays_inside() {
        let mut doc = Document::default();
        let objects = [block([0.1, 0.1, 0.1, 0.1]), block([0.3, 0.2, 0.1, 0.1])];
        let keys = doc.place(&objects, [0.95, 0.5]);
        assert_eq!(keys.len(), 2);
        let b = doc.bounds(&keys).unwrap();
        assert!(close(b, [0.7, 0.5, 0.3, 0.2]));
    }
    #[test]
    fn free_groups_skip_members_targets_and_hidden_groups() {
        let mut doc = Document::default();
        doc.map.hidden_groups = vec![1];
        let mut b = block([0.1, 0.1, 0.1, 0.1]);
        b.set_group(2);
        doc.insert(b);
        doc.insert(Object::Trigger(MapTrigger {
            group: 3,
            ..Default::default()
        }));
        assert_eq!(doc.next_free_group(), Some(4));
    }
    #[test]
    fn checks_point_at_the_trigger_at_fault() {
        let mut doc = Document::default();
        doc.insert(Object::Trigger(MapTrigger {
            action: TriggerAction::Announce,
            ..Default::default()
        }));
        doc.insert(Object::Trigger(MapTrigger {
            when: TriggerWhen::Signal,
            source: 4,
            group: 9,
            ..Default::default()
        }));
        let problems = doc.problems();
        assert_eq!(problems[0].key, Some(Key::Trigger(0)));
        assert!(problems[0].blocking);
        assert!(problems[0].text.contains("write the announcement"));
        assert!(
            problems
                .iter()
                .any(|p| !p.blocking && p.text.contains("signal 4"))
        );
        assert!(
            problems
                .iter()
                .any(|p| p.text.contains("group 9, which is empty"))
        );
    }
    #[test]
    fn trigger_sentences_read_naturally() {
        let t = MapTrigger {
            when: TriggerWhen::Enter,
            who: TriggerWho::Players,
            team: 2,
            action: TriggerAction::Show,
            group: 4,
            duration: 5.,
            delay: 1.5,
            chance: 50,
            seconds: 0.,
            once: true,
            ..Default::default()
        };
        assert_eq!(
            describe_trigger(&t),
            "When a player from team 2 enters, 50% of the time: show group 4 for 5 s, \
             1.5 s later. Once per round."
        );
        assert_eq!(trigger_tag(&t), "Enter \u{2192} Show 4");
    }
}

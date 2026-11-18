//! The map editor's portable document and operations, independent of the UI.
use serde::{Deserialize, Serialize};
use tankyard_content::*;
use tankyard_mods::{GameRules, PackFiles};

/// Largest editor project, as written or read: room for a map at every
/// limit, in readable JSON.
pub const DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

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
        if bytes.len() > DOCUMENT_BYTES {
            return Err(format!(
                "Editor document exceeds {} MiB.",
                DOCUMENT_BYTES >> 20
            ));
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
            || doc.map.walls.len() + doc.map.blocks.len() > tankyard_content::MAP_BLOCKS
            || doc.map.zones.len() > tankyard_content::MAP_ZONES
            || doc.map.decor.len() > tankyard_content::MAP_DECOR
            || doc.map.triggers.len() > tankyard_content::MAP_TRIGGERS
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
                Object::Zone(zone) if zone.kind.aims() => {
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
    /// Space centers evenly between the outermost centers; the end objects
    /// stay put. Sorting does not change drawing order or trigger order.
    pub fn distribute(&mut self, keys: &[Key], axis: usize) {
        if axis > 1 {
            return;
        }
        let mut items: Vec<_> = keys
            .iter()
            .filter_map(|&key| self.rect(key).map(|r| (key, r)))
            .collect();
        items.sort_by_key(|(key, _)| *key);
        items.dedup_by_key(|(key, _)| *key);
        items.sort_by(|(ka, a), (kb, b)| {
            (a[axis] + a[axis + 2] * 0.5)
                .total_cmp(&(b[axis] + b[axis + 2] * 0.5))
                .then(ka.cmp(kb))
        });
        if items.len() < 3 {
            return;
        }
        let first = items[0].1[axis] + items[0].1[axis + 2] * 0.5;
        let last = items.last().unwrap().1;
        let span = last[axis] + last[axis + 2] * 0.5 - first;
        let count = items.len() - 1;
        for (index, (key, mut r)) in items.into_iter().enumerate() {
            r[axis] = first + span * index as f32 / count as f32 - r[axis + 2] * 0.5;
            if let Some(mut object) = self.object(key) {
                object.set_rect(r);
                self.replace(key, object);
            }
        }
    }
    /// Match the widest/tallest selected rectangle, keeping centers wherever
    /// the arena bounds allow. Point markers keep their fixed handles.
    pub fn match_size(&mut self, keys: &[Key], axis: usize) {
        if axis > 1 {
            return;
        }
        let size = keys
            .iter()
            .filter_map(|&key| self.object(key))
            .filter(|o| !o.is_point())
            .map(|o| o.rect()[axis + 2])
            .fold(0f32, f32::max);
        if size <= 0. {
            return;
        }
        self.edit_each(keys, |object, mut r| {
            if object.is_point() {
                return;
            }
            let center = r[axis] + r[axis + 2] * 0.5;
            r[axis + 2] = size;
            r[axis] = center - size * 0.5;
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
            if trigger.action.targets_group()
                || (trigger.action == TriggerAction::Reset && trigger.group > 0)
            {
                mask |= MapDef::group_bit(trigger.group);
            }
            if trigger.when.watches_group() {
                mask |= MapDef::group_bit(trigger.source);
            }
            for test in trigger.conditions() {
                if !test.kind.compares() {
                    mask |= MapDef::group_bit(test.index);
                }
            }
        }
        mask
    }
    /// The lowest group nothing uses yet.
    pub fn next_free_group(&self) -> Option<u8> {
        let used = self.used_groups();
        (1..=MAP_GROUPS).find(|&group| MapDef::group_bit(group) & used == 0)
    }
    /// The counters anything refers to, in order: triggers that change,
    /// watch or check them, bars, board cells, and text fields in labels,
    /// announcements and widgets.
    pub fn used_counters(&self) -> Vec<u8> {
        let mut used = [false; MAP_COUNTERS as usize + 1];
        let mut named = Vec::new();
        for decor in &self.map.decor {
            tankyard_content::counters_named(&decor.text, &mut named);
        }
        for t in &self.map.triggers {
            let mut mark = |counter: u8| used[usize::from(counter)] = true;
            if t.action.targets_counter() {
                mark(t.group);
                mark(t.target_offset);
                if t.action == TriggerAction::Calculate {
                    mark(t.operand);
                    mark(t.operand_offset);
                }
            }
            if matches!(t.when, TriggerWhen::Counter | TriggerWhen::Changed) {
                mark(t.source);
            }
            for test in t.conditions() {
                if test.kind == TestKind::Counter {
                    mark(test.index);
                }
                if test.kind.compares() {
                    mark(test.against);
                }
            }
            if t.action == TriggerAction::Announce {
                tankyard_content::counters_named(&t.text, &mut named);
            }
            if let Some(ui) = &t.ui {
                mark(ui.bar);
                tankyard_content::counters_named(&ui.widget.title, &mut named);
                tankyard_content::counters_named(&ui.widget.text, &mut named);
                for cell in ui.widget.board.iter().flat_map(|board| &board.cells) {
                    mark(cell.counter);
                    tankyard_content::counters_named(&cell.text, &mut named);
                }
            }
        }
        for counter in named {
            used[usize::from(counter)] = true;
        }
        // Slot 0 is no counter.
        (1..=MAP_COUNTERS)
            .filter(|&counter| used[usize::from(counter)])
            .collect()
    }
    /// Which signal channels triggers send or listen on, by number.
    pub fn used_channels(&self) -> [bool; MAP_CHANNELS as usize + 1] {
        let mut used = [false; MAP_CHANNELS as usize + 1];
        for t in &self.map.triggers {
            if t.when == TriggerWhen::Signal {
                used[usize::from(t.source)] = true;
            }
            for channel in t.sends() {
                used[usize::from(channel)] = true;
            }
            used[usize::from(t.otherwise)] = true;
        }
        used
    }
    /// The lowest signal channel no trigger sends or listens on.
    pub fn next_free_channel(&self) -> Option<u8> {
        let used = self.used_channels();
        (1..=MAP_CHANNELS).find(|&channel| !used[usize::from(channel)])
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
            if trigger.action == TriggerAction::Shove
                && members(trigger.group)
                && m.body(trigger.group).is_none()
            {
                note(
                    key,
                    format!(
                        "{name} shoves group {}, which isn't pushable. Turn on Pushable in the group's Body settings.",
                        trigger.group
                    ),
                    false,
                );
            }
            if trigger.action == TriggerAction::Path
                && m.paths
                    .get(usize::try_from(trigger.amount - 1).unwrap_or(usize::MAX))
                    .is_none_or(|path| path.points.is_empty())
            {
                note(
                    key,
                    format!(
                        "{name} plays path {}, which has no points yet. Add them in the Map tab, under Paths.",
                        trigger.amount
                    ),
                    false,
                );
            }
            let senders = |channel: u8| {
                m.triggers.iter().any(|t| {
                    t.sends().any(|sent| sent == channel)
                        || (t.otherwise == channel && t.only_if.is_some())
                })
            };
            let counted = |counter: u8| {
                m.triggers
                    .iter()
                    .any(|t| t.action.targets_counter() && t.group == counter)
            };
            // Groups that can change during a round: what triggers show, hide,
            // toggle or animate. Scripts can too, so these stay notes.
            let switched = |group: u8| {
                m.triggers
                    .iter()
                    .any(|t| t.action.switches_group() && t.group == group)
            };
            let animated = |group: u8| {
                m.triggers.iter().any(|t| {
                    (t.action.animates() || t.action == TriggerAction::Path) && t.group == group
                })
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
                TriggerWhen::Counter | TriggerWhen::Changed if !counted(trigger.source) => note(
                    key,
                    format!(
                        "{name} watches counter {}, which nothing changes.",
                        trigger.source
                    ),
                    false,
                ),
                TriggerWhen::Shown | TriggerWhen::Hidden if !switched(trigger.source) => note(
                    key,
                    format!(
                        "{name} watches group {}, which nothing shows or hides.",
                        trigger.source
                    ),
                    false,
                ),
                TriggerWhen::Arrived if !animated(trigger.source) => note(
                    key,
                    format!(
                        "{name} waits for group {} to arrive, but nothing moves, turns, fades or sends it along a path.",
                        trigger.source
                    ),
                    false,
                ),
                _ => {}
            }
            let mut quiet = Vec::new();
            for test in trigger.conditions() {
                match test.kind {
                    TestKind::Counter => quiet.extend(
                        [test.index, test.against]
                            .into_iter()
                            .filter(|&c| c > 0 && !counted(c))
                            .map(|c| format!("counter {c}, which nothing changes")),
                    ),
                    TestKind::Inside | TestKind::Time if test.against > 0 => {
                        if !counted(test.against) {
                            quiet.push(format!("counter {}, which nothing changes", test.against));
                        }
                    }
                    TestKind::Shown | TestKind::Hidden if !switched(test.index) => quiet.push(
                        format!("group {}, which nothing shows or hides", test.index),
                    ),
                    _ => {}
                }
            }
            quiet.dedup();
            for what in quiet {
                note(
                    key,
                    format!("{name} checks {what}, so that condition never moves."),
                    false,
                );
            }
            let receives = |channel: u8| {
                m.triggers
                    .iter()
                    .any(|t| t.when == TriggerWhen::Signal && t.source == channel)
            };
            let mut unheard: Vec<u8> = trigger
                .sends()
                .filter(|&channel| !receives(channel))
                .collect();
            if trigger.otherwise > 0 && trigger.only_if.is_some() && !receives(trigger.otherwise) {
                unheard.push(trigger.otherwise);
            }
            unheard.sort_unstable();
            unheard.dedup();
            match unheard.as_slice() {
                [] => {}
                [one] => note(
                    key,
                    format!("{name} sends signal {one}, which nothing receives."),
                    false,
                ),
                many => note(
                    key,
                    format!(
                        "{name} sends signals {}, which nothing receives.",
                        many.iter()
                            .map(u8::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    false,
                ),
            }
            // Timers have no tank behind them.
            let to_cause = trigger.who == TriggerWho::Cause
                || trigger.ui.as_ref().is_some_and(|ui| {
                    ui.to == UiAudience::Cause
                        || (trigger.action == TriggerAction::Ui && ui.pin == UiPin::Cause)
                });
            if to_cause && matches!(trigger.when, TriggerWhen::Start | TriggerWhen::Every) {
                note(
                    key,
                    format!(
                        "{name} reaches the tank that set it off, but a timer has no tank behind it, so it reaches nobody."
                    ),
                    false,
                );
            }
            if let Some(ui) = trigger
                .ui
                .as_ref()
                .filter(|_| trigger.action == TriggerAction::Ui && trigger.duration <= 0.)
            {
                let words = [&ui.widget.text, &ui.widget.title];
                if ui.timer {
                    note(
                        key,
                        format!(
                            "{name}'s bar counts down the time left, but the widget stays until hidden, so the bar never shows. Set Hide after."
                        ),
                        false,
                    );
                } else if words.iter().any(|text| text.contains("{left}")) {
                    note(
                        key,
                        format!(
                            "{name} shows {{left}}, but the widget stays until hidden, so it's blank. Set Hide after."
                        ),
                        false,
                    );
                }
            }
            if trigger.action == TriggerAction::Reset
                && trigger.group > 0
                && !m.triggers.iter().any(|t| t.member == trigger.group)
            {
                note(
                    key,
                    format!(
                        "{name} resets group {}, which has no triggers.",
                        trigger.group
                    ),
                    false,
                );
            }
            if trigger.member != 0
                && trigger.member == trigger.group
                && trigger.action.switches_group()
            {
                note(
                    key,
                    format!("{name} switches off its own group {}.", trigger.member),
                    false,
                );
            }
        }
        if let Some(cycle) = signal_loop(&m.triggers) {
            let names: Vec<String> = cycle.iter().map(|i| (i + 1).to_string()).collect();
            note(
                Some(Key::Trigger(cycle[0])),
                format!(
                    "Triggers {} signal each other with no delay, gate or chance, so a round cuts the chain short each time it starts.",
                    names.join(", ")
                ),
                false,
            );
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
            Object::Wall(o) if m.walls.len() + m.blocks.len() < tankyard_content::MAP_BLOCKS => {
                m.walls.push(o);
                Key::Wall(m.walls.len() - 1)
            }
            Object::Block(o) if m.walls.len() + m.blocks.len() < tankyard_content::MAP_BLOCKS => {
                m.blocks.push(o);
                Key::Block(m.blocks.len() - 1)
            }
            Object::Zone(o) if m.zones.len() < tankyard_content::MAP_ZONES => {
                m.zones.push(o);
                Key::Zone(m.zones.len() - 1)
            }
            Object::Decor(o)
                if m.decor.len() < tankyard_content::MAP_DECOR
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
            Object::Trigger(o) if m.triggers.len() < tankyard_content::MAP_TRIGGERS => {
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

/// How one trigger reaches another, for the canvas arrows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// A signal it sends wakes the other.
    Signal,
    /// Its otherwise signal, sent while its gate is shut, wakes the other.
    Otherwise,
    /// It changes a counter the other watches or checks.
    Counter,
    /// It switches a group the other watches or checks.
    Group,
    /// It resets the other.
    Reset,
}

/// Whether, and how, `from` acting can affect `to`.
pub fn link(from: &MapTrigger, to: &MapTrigger) -> Option<Link> {
    // 0 names no counter or group: a Calculate that picks its counter by
    // another's value can't be followed.
    let reads_counter = |counter: u8| {
        counter > 0
            && ((matches!(to.when, TriggerWhen::Counter | TriggerWhen::Changed)
                && to.source == counter)
                || to.conditions().any(|test| {
                    (test.kind == TestKind::Counter && test.index == counter)
                        || (test.kind.compares() && test.against == counter)
                }))
    };
    let reads_group = |group: u8| {
        group > 0
            && ((matches!(to.when, TriggerWhen::Shown | TriggerWhen::Hidden) && to.source == group)
                || to
                    .conditions()
                    .any(|test| !test.kind.compares() && test.index == group))
    };
    let awaits = |group: u8| group > 0 && to.when == TriggerWhen::Arrived && to.source == group;
    let listens = |channel: u8| to.when == TriggerWhen::Signal && to.source == channel;
    if from.sends().any(&listens) {
        Some(Link::Signal)
    } else if from.action.targets_counter() && reads_counter(from.group) {
        Some(Link::Counter)
    } else if (from.action.switches_group() && reads_group(from.group))
        || (from.action.animates() && awaits(from.group))
    {
        Some(Link::Group)
    } else if from.action == TriggerAction::Reset && (from.group == 0 || to.member == from.group) {
        Some(Link::Reset)
    } else if from.otherwise > 0 && from.only_if.is_some() && listens(from.otherwise) {
        Some(Link::Otherwise)
    } else {
        None
    }
}

/// Which triggers each signal, counter and group reaches, so following
/// links never searches every trigger.
pub struct LinkIndex {
    /// By channel: triggers waiting for its signal.
    listeners: Vec<Vec<usize>>,
    /// By counter: triggers that watch or check it.
    counters: Vec<Vec<usize>>,
    /// By group: triggers that watch, check or wait for it.
    groups: Vec<Vec<usize>>,
    /// By group: the triggers in it, which resetting it resets.
    members: Vec<Vec<usize>>,
}
impl LinkIndex {
    pub fn new(triggers: &[MapTrigger]) -> Self {
        let mut index = Self {
            listeners: vec![Vec::new(); usize::from(MAP_CHANNELS) + 1],
            counters: vec![Vec::new(); usize::from(MAP_COUNTERS) + 1],
            groups: vec![Vec::new(); usize::from(MAP_GROUPS) + 1],
            members: vec![Vec::new(); usize::from(MAP_GROUPS) + 1],
        };
        // Slot 0 is no channel, counter or group.
        fn add(lists: &mut [Vec<usize>], slot: u8, trigger: usize) {
            if slot > 0
                && let Some(list) = lists.get_mut(usize::from(slot))
                && list.last() != Some(&trigger)
            {
                list.push(trigger);
            }
        }
        for (at, t) in triggers.iter().enumerate() {
            match t.when {
                TriggerWhen::Signal => add(&mut index.listeners, t.source, at),
                TriggerWhen::Counter | TriggerWhen::Changed => {
                    add(&mut index.counters, t.source, at)
                }
                TriggerWhen::Shown | TriggerWhen::Hidden | TriggerWhen::Arrived => {
                    add(&mut index.groups, t.source, at)
                }
                _ => {}
            }
            for test in t.conditions() {
                if test.kind == TestKind::Counter {
                    add(&mut index.counters, test.index, at);
                }
                if test.kind.compares() {
                    add(&mut index.counters, test.against, at);
                } else {
                    add(&mut index.groups, test.index, at);
                }
            }
            add(&mut index.members, t.member, at);
        }
        index
    }
    /// The triggers waiting for a signal on `channel`.
    pub fn listening(&self, channel: u8) -> &[usize] {
        self.listeners
            .get(usize::from(channel))
            .map_or(&[], Vec::as_slice)
    }
    /// What trigger `at` acting can affect and how, each once, into `out`.
    pub fn affected(&self, triggers: &[MapTrigger], at: usize, out: &mut Vec<(usize, Link)>) {
        out.clear();
        let Some(from) = triggers.get(at) else {
            return;
        };
        let mut reach: Vec<usize> = Vec::new();
        for channel in from.sends() {
            reach.extend_from_slice(self.listening(channel));
        }
        if from.otherwise > 0 && from.only_if.is_some() {
            reach.extend_from_slice(self.listening(from.otherwise));
        }
        let slot = usize::from(from.group);
        if from.action.targets_counter()
            && let Some(list) = self.counters.get(slot)
        {
            reach.extend_from_slice(list);
        }
        if (from.action.switches_group() || from.action.animates())
            && let Some(list) = self.groups.get(slot)
        {
            reach.extend_from_slice(list);
        }
        if from.action == TriggerAction::Reset {
            if from.group == 0 {
                reach.extend(0..triggers.len());
            } else if let Some(list) = self.members.get(slot) {
                reach.extend_from_slice(list);
            }
        }
        reach.sort_unstable();
        reach.dedup();
        out.extend(
            reach
                .into_iter()
                .filter(|&other| other != at)
                .filter_map(|other| Some((other, link(from, triggers.get(other)?)?))),
        );
    }
}

/// Triggers that signal one another in a ring with nothing to stop them, by
/// index, when there are any: no delay, gate or chance, and not once per round.
fn signal_loop(triggers: &[MapTrigger]) -> Option<Vec<usize>> {
    let free = |t: &MapTrigger| t.delay == 0. && t.chance == 100 && !t.once && t.only_if.is_none();
    let links = LinkIndex::new(triggers);
    let next: Vec<Vec<usize>> = triggers
        .iter()
        .map(|t| {
            if !free(t) {
                return Vec::new();
            }
            let mut to: Vec<usize> = t
                .sends()
                .flat_map(|channel| links.listening(channel).iter().copied())
                .filter(|&other| free(&triggers[other]))
                .collect();
            to.sort_unstable();
            to.dedup();
            to
        })
        .collect();
    // Depth-first search with its own stack, so long chains can't overflow
    // the thread's. 0 unvisited, 1 on the path, 2 finished.
    let mut state = vec![0u8; triggers.len()];
    for start in 0..triggers.len() {
        if state[start] != 0 {
            continue;
        }
        // Each step on the path, and the next of its links to follow.
        let mut path: Vec<(usize, usize)> = vec![(start, 0)];
        state[start] = 1;
        while let Some(top) = path.last_mut() {
            let (at, edge) = *top;
            if let Some(&to) = next[at].get(edge) {
                top.1 += 1;
                match state[to] {
                    0 => {
                        state[to] = 1;
                        path.push((to, 0));
                    }
                    1 => {
                        let from = path.iter().position(|&(i, _)| i == to).unwrap_or(0);
                        let mut ring: Vec<usize> = path[from..].iter().map(|&(i, _)| i).collect();
                        ring.sort_unstable();
                        return Some(ring);
                    }
                    _ => {}
                }
            } else {
                state[at] = 2;
                path.pop();
            }
        }
    }
    None
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
/// A colour as `#rrggbb`.
pub fn color_hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}
/// An animation's pace in words: "twice a second", "every 2 s".
fn pace(cycles: f32) -> String {
    match cycles {
        c if (c - 1.).abs() < 0.01 => "once a second".into(),
        c if (c - 2.).abs() < 0.01 => "twice a second".into(),
        c if c >= 1. => format!("{} times a second", trim_number(c)),
        c => format!("every {}", seconds(1. / c.max(0.01))),
    }
}
fn trim_number(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
/// The tanks a tank action reaches, for its sentence.
fn who_here(t: &MapTrigger) -> &'static str {
    match t.who {
        TriggerWho::Anyone => "the tanks",
        TriggerWho::Players => "the players",
        TriggerWho::Bots => "the bots",
        TriggerWho::Cause => "the tank that set it off",
    }
}
fn team_here(t: &MapTrigger) -> String {
    if t.team > 0 {
        format!(" from team {}", t.team)
    } else {
        String::new()
    }
}
/// The boss a Defeated trigger waits for, or `None` for any boss.
pub fn defeated_boss(t: &MapTrigger) -> Option<&'static str> {
    usize::from(t.source)
        .checked_sub(1)
        .and_then(|i| BOSS_IDS.get(i).copied())
}
fn boss_name(id: &str) -> &'static str {
    tankyard_core::BossKind::from_id(id).map_or("unknown boss", tankyard_core::BossKind::name)
}
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

/// A Show UI trigger's action, for its sentence.
fn describe_ui(t: &MapTrigger) -> String {
    let Some(ui) = t.ui.as_ref() else {
        return format!("show UI slot {} (not designed yet)", t.group);
    };
    let w = &ui.widget;
    let what = match (
        &w.board,
        [w.title.trim(), w.text.trim()]
            .into_iter()
            .find(|t| !t.is_empty()),
    ) {
        (Some(board), Some(title)) => format!(
            "board \u{201c}{title}\u{201d} ({}\u{d7}{}, {} cells)",
            board.columns,
            board.rows,
            board.cells.len()
        ),
        (Some(board), None) => format!(
            "a {}\u{d7}{} board of {} cells",
            board.columns,
            board.rows,
            board.cells.len()
        ),
        (None, Some(text)) => format!("\u{201c}{text}\u{201d}"),
        (None, None) => "a bar".to_owned(),
    };
    let to = match ui.to {
        UiAudience::Everyone => "everyone",
        UiAudience::Inside => "the players here",
        UiAudience::Cause => "the player that set it off",
    };
    let mut text = format!("show {what} in UI slot {} to {to}", t.group);
    text += match ui.pin {
        UiPin::Screen => "",
        UiPin::Area => " over this area",
        UiPin::Cause => " over the tank that set it off",
    };
    if !w.choices.is_empty() {
        let picks: Vec<String> = w
            .choices
            .iter()
            .enumerate()
            .map(|(index, choice)| match ui.channels.get(index).copied() {
                Some(channel) if channel > 0 => format!("{choice} \u{2192} {channel}"),
                _ => choice.clone(),
            })
            .collect();
        text += &format!(", asking {}", picks.join(", "));
    }
    if t.duration > 0. {
        text += &format!(" for {}", seconds(t.duration));
    }
    text
}

/// A trigger in one plain sentence, for the inspector and logic list.
pub fn describe_trigger(t: &MapTrigger) -> String {
    let who = match t.who {
        TriggerWho::Anyone | TriggerWho::Cause => "a tank",
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
        TriggerWhen::Changed => format!("When counter {} changes", t.source),
        TriggerWhen::Shown => format!("When group {} is shown", t.source),
        TriggerWhen::Hidden => format!("When group {} is hidden", t.source),
        TriggerWhen::Arrived => format!("When group {} finishes a motion", t.source),
        TriggerWhen::Touch => format!("When group {}'s blocks reach here", t.source),
        TriggerWhen::Release => format!("When group {}'s blocks leave here", t.source),
        TriggerWhen::Defeated => match defeated_boss(t) {
            Some(id) => format!("When the {} is defeated", boss_name(id)),
            None => "When a boss is defeated".into(),
        },
    };
    if let Some(gate) = describe_gate(t) {
        text += ", ";
        text += &gate;
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
        TriggerAction::Sequence => format!(
            "send signals {} to {} in turn",
            t.group,
            i32::from(t.group) + t.amount - 1
        ),
        TriggerAction::Reset if t.group == 0 => "start every trigger afresh".into(),
        TriggerAction::Reset => format!("start group {}'s triggers afresh", t.group),
        TriggerAction::Move => format!("move group {} {}", t.group, describe_travel(t.travel)),
        TriggerAction::Place if t.travel == [0., 0.] => {
            format!("move group {} back to where the map puts it", t.group)
        }
        TriggerAction::Place => format!(
            "move group {} to {} from its place",
            t.group,
            describe_travel(t.travel)
        ),
        TriggerAction::Turn => format!("turn group {}'s decor {}\u{b0}", t.group, t.amount),
        TriggerAction::Fade => format!("fade group {}'s decor to {}%", t.group, t.amount),
        TriggerAction::Tint if t.amount == 0 => format!("clear group {}'s tint", t.group),
        TriggerAction::Tint => format!(
            "tint group {}'s decor {} at {}%",
            t.group,
            color_hex(t.color),
            t.amount
        ),
        TriggerAction::Scale => format!("scale group {}'s decor to {}%", t.group, t.amount),
        TriggerAction::Animate if t.amount == 0 => format!(
            "settle group {}'s {}",
            t.group,
            t.style.name().to_lowercase()
        ),
        TriggerAction::Animate => format!(
            "{} group {}'s decor by {}{}, {}",
            t.style.name().to_lowercase(),
            t.group,
            t.amount,
            t.style.unit(),
            pace(t.animate_speed())
        ),
        TriggerAction::Stop => format!("stop group {} where it is", t.group),
        TriggerAction::Shake => format!(
            "shake the view of {}{} here at {}% for {}",
            who_here(t),
            team_here(t),
            t.amount,
            seconds(t.tween)
        ),
        TriggerAction::Sound => format!(
            "play the {} sound here at {}% volume",
            t.sound.name().to_lowercase(),
            t.amount
        ),
        TriggerAction::Shove => format!(
            "shove body {} {} at {} units a second",
            t.group,
            heading(t.angle),
            t.amount
        ),
        TriggerAction::Fling => format!(
            "fling {}{} here {} at {} units a second",
            who_here(t),
            team_here(t),
            heading(t.angle),
            t.amount
        ),
        TriggerAction::Blast if t.amount < 0 => format!(
            "pull {}{} here toward the middle at {} units a second",
            who_here(t),
            team_here(t),
            t.amount.unsigned_abs()
        ),
        TriggerAction::Blast => format!(
            "throw {}{} here outward at {} units a second",
            who_here(t),
            team_here(t),
            t.amount
        ),
        TriggerAction::Zoom => format!(
            "zoom the view of {}{} here to {}% for {}",
            who_here(t),
            team_here(t),
            t.amount,
            seconds(t.tween)
        ),
        TriggerAction::Light if t.amount == 0 => "light the arena fully".into(),
        TriggerAction::Light => format!(
            "darken the arena to {}%, tinted {}",
            t.amount,
            color_hex(t.color)
        ),
        TriggerAction::Path => format!("send group {} along path {}", t.group, t.amount),
        TriggerAction::Ui => describe_ui(t),
        TriggerAction::HideUi => {
            let slot = if t.group == 0 {
                "every UI slot".to_owned()
            } else {
                format!("UI slot {}", t.group)
            };
            match t.ui.as_ref().map_or(UiAudience::Everyone, |ui| ui.to) {
                UiAudience::Everyone => format!("hide {slot}"),
                UiAudience::Inside => format!("hide {slot} from the players here"),
                UiAudience::Cause => format!("hide {slot} from the player that set it off"),
            }
        }
    };
    if t.action.animates() {
        text += &if t.tween > 0. {
            format!(
                " over {} ({}, {})",
                seconds(t.tween),
                t.easing.curve.name(),
                t.easing.mode.name().to_lowercase()
            )
        } else {
            " at once".into()
        };
    }
    if (t.action.targets_group() || t.action == TriggerAction::Light) && t.duration > 0. {
        text += &if t.action.animates() {
            format!(", returning after {}", seconds(t.duration))
        } else {
            format!(" for {}", seconds(t.duration))
        };
    }
    if t.delay > 0. {
        text += &format!(", {} later", seconds(t.delay));
    }
    text.push('.');
    if t.otherwise > 0 && t.only_if.is_some() {
        text += &format!(" Otherwise sends signal {}.", t.otherwise);
    }
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
/// [`describe_trigger`], with the groups it mentions called by their names:
/// "hide group 4 (Vault gate)", and "Crate's blocks" for "group 3's blocks".
/// Quoted announcement and script text is left as written.
pub fn describe_trigger_named(t: &MapTrigger, names: &[GroupName]) -> String {
    name_groups(&describe_trigger(t), names)
}
fn name_groups(text: &str, names: &[GroupName]) -> String {
    if names.is_empty() {
        return text.to_owned();
    }
    let named = |group: &str| {
        let group: u8 = group.parse().ok()?;
        names
            .iter()
            .find(|n| n.group == group && !n.name.trim().is_empty())
            .map(|n| n.name.trim())
    };
    let mut out = String::with_capacity(text.len() + 32);
    let mut quoted = false;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let word_start = !out.ends_with(|c: char| c.is_alphanumeric());
        let prefix = ["group ", "body "]
            .into_iter()
            .find(|p| !quoted && word_start && rest.starts_with(p));
        if let Some(prefix) = prefix {
            let after = &rest[prefix.len()..];
            let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            if let Some(name) = (digits > 0).then(|| named(&after[..digits])).flatten() {
                let number = &after[..digits];
                let tail = &after[digits..];
                if tail.starts_with("'s ") {
                    out.push_str(name);
                } else {
                    out.push_str(&format!("{prefix}{number} ({name})"));
                }
                rest = tail;
                continue;
            }
        }
        quoted = match c {
            '\u{201c}' => true,
            '\u{201d}' => false,
            _ => quoted,
        };
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}
/// A move in words: "12% right and 5% up", as fractions of the arena.
pub fn describe_travel([x, y]: [f32; 2]) -> String {
    let part = |value: f32, positive: &str, negative: &str| {
        let percent = (value.abs() * 1000.).round() / 10.;
        (percent > 0.).then(|| {
            format!(
                "{percent}% {}",
                if value > 0. { positive } else { negative }
            )
        })
    };
    match (part(x, "right", "left"), part(y, "down", "up")) {
        (Some(x), Some(y)) => format!("{x} and {y}"),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => "nowhere".into(),
    }
}
/// A condition in words: "counter 1 is at least 3".
pub fn describe_test(test: &TriggerTest) -> String {
    let other = if test.against > 0 {
        format!("counter {}", test.against)
    } else {
        test.value.to_string()
    };
    let words = test.compare.words();
    match test.kind {
        TestKind::Counter => format!("counter {} {words} {other}", test.index),
        TestKind::Shown => format!("group {} is shown", test.index),
        TestKind::Hidden => format!("group {} is hidden", test.index),
        TestKind::Inside => format!("the tank count inside {words} {other}"),
        TestKind::Time if test.against > 0 => format!("the round time {words} {other}"),
        TestKind::Time => format!("the round time {words} {other} s"),
    }
}
/// The gate in words, starting "if" or "unless", when there is one.
pub fn describe_gate(t: &MapTrigger) -> Option<String> {
    let tests: Vec<String> = t.conditions().map(describe_test).collect();
    let list = |joint: &str| match tests.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} {joint} {last}", rest.join(", ")),
    };
    Some(match (t.gate, tests.len()) {
        (_, 0) => return None,
        (GateMode::NotAll | GateMode::NoneOf, 1) => format!("unless {}", tests[0]),
        (GateMode::All | GateMode::Any | GateMode::One, 1) => format!("if {}", tests[0]),
        (GateMode::All, _) => format!("if {}", list("and")),
        (GateMode::Any, _) => format!("if {}", list("or")),
        (GateMode::NotAll, _) => format!("unless {}", list("and")),
        (GateMode::NoneOf, _) => format!("unless {}", list("or")),
        (GateMode::One, _) => format!("if just one of {} holds", list("or")),
    })
}
/// A condition in a few characters: "#1≥3", "G4 on".
fn test_tag(test: &TriggerTest) -> String {
    let other = if test.against > 0 {
        format!("#{}", test.against)
    } else {
        test.value.to_string()
    };
    let symbol = test.compare.symbol();
    match test.kind {
        TestKind::Counter => format!("#{}{symbol}{other}", test.index),
        TestKind::Shown => format!("G{} on", test.index),
        TestKind::Hidden => format!("G{} off", test.index),
        TestKind::Inside => format!("in{symbol}{other}"),
        TestKind::Time if test.against > 0 => format!("t{symbol}{other}"),
        TestKind::Time => format!("t{symbol}{other}s"),
    }
}
/// The gate in a few characters: "#1≥3", "AND(#1≥3, G4 on)", or with
/// `compact`, "AND of 2".
fn gate_tag(t: &MapTrigger, compact: bool) -> Option<String> {
    let tests: Vec<String> = t.conditions().map(test_tag).collect();
    Some(match (t.gate, tests.len()) {
        (_, 0) => return None,
        (GateMode::NotAll | GateMode::NoneOf, 1) => format!("not {}", tests[0]),
        (_, 1) => tests[0].clone(),
        (gate, n) if compact => format!("{} of {n}", gate.short()),
        (gate, _) => format!("{}({})", gate.short(), tests.join(", ")),
    })
}
/// A counter Calculate reads or writes: `base`, or with `by` set, the
/// counter that many places past it, by counter `by`'s value.
fn counter_name(base: u8, by: u8) -> String {
    match (base, by) {
        (_, 0) => format!("counter {base}"),
        (0, _) => format!("counter #{by}"),
        _ => format!("counter {base}+#{by}"),
    }
}
/// The same, short, for badges.
fn counter_badge(base: u8, by: u8) -> String {
    match (base, by) {
        (_, 0) => format!("#{base}"),
        (0, _) => format!("#(#{by})"),
        _ => format!("#({base}+#{by})"),
    }
}
/// What Calculate does, in words.
fn describe_math(t: &MapTrigger) -> String {
    let c = counter_name(t.group, t.target_offset);
    let x = if t.operand > 0 || t.operand_offset > 0 {
        counter_name(t.operand, t.operand_offset)
    } else {
        t.amount.to_string()
    };
    match t.math {
        MathOp::Add => format!("add {x} to {c}"),
        MathOp::Subtract => format!("take {x} from {c}"),
        MathOp::Multiply => format!("multiply {c} by {x}"),
        MathOp::Divide => format!("divide {c} by {x}, rounding down"),
        MathOp::Remainder => format!("keep {c}'s remainder after dividing by {x}"),
        MathOp::Lowest => format!("lower {c} to {x} if it is above"),
        MathOp::Highest => format!("raise {c} to {x} if it is below"),
        MathOp::Copy => format!("set {c} to {x}"),
        MathOp::Random => format!("set {c} to a random number from 0 below {x}"),
        MathOp::Flip => format!("flip {c} between 0 and 1"),
        MathOp::And => format!("keep only the bits of {c} that {x} has too"),
        MathOp::Or => format!("turn on {x}'s bits in {c}"),
        MathOp::Xor => format!("flip {x}'s bits in {c}"),
        MathOp::ShiftLeft => format!("shift {c} left by {x} bits"),
        MathOp::ShiftRight => format!("shift {c} right by {x} bits, rounding down"),
        MathOp::Power => format!("raise {c} to the power of {x}"),
    }
}
/// A direction in words: "right", "up and left", or degrees between.
pub fn heading(angle: f32) -> String {
    const NAMES: [&str; 8] = [
        "right",
        "down and right",
        "down",
        "down and left",
        "left",
        "up and left",
        "up",
        "up and right",
    ];
    let degrees = angle.rem_euclid(360.);
    let step = (degrees / 45.).round();
    if (degrees - step * 45.).abs() < 0.5 {
        format!("toward {}", NAMES[step as usize % 8])
    } else {
        format!("toward {}\u{b0}", degrees.round())
    }
}
/// A direction as one arrow, for badges.
fn arrow(angle: f32) -> &'static str {
    const ARROWS: [&str; 8] = [
        "\u{2192}", "\u{2198}", "\u{2193}", "\u{2199}", "\u{2190}", "\u{2196}", "\u{2191}",
        "\u{2197}",
    ];
    ARROWS[(angle.rem_euclid(360.) / 45.).round() as usize % 8]
}
/// A trigger in a few characters, for lists.
pub fn trigger_tag(t: &MapTrigger) -> String {
    tag(t, false)
}
/// A trigger in fewer characters, for its badge on the canvas: a gate of
/// several conditions shows only how many and how they combine.
pub fn trigger_badge_tag(t: &MapTrigger) -> String {
    tag(t, true)
}
fn tag(t: &MapTrigger, compact: bool) -> String {
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
        TriggerWhen::Changed => format!("#{} changes", t.source),
        TriggerWhen::Shown => format!("G{} shown", t.source),
        TriggerWhen::Hidden => format!("G{} hidden", t.source),
        TriggerWhen::Defeated => "Boss down".into(),
        TriggerWhen::Arrived => format!("G{} arrives", t.source),
        TriggerWhen::Touch => format!("G{} touches", t.source),
        TriggerWhen::Release => format!("G{} leaves", t.source),
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
            let c = counter_badge(t.group, t.target_offset);
            let x = if t.operand > 0 || t.operand_offset > 0 {
                counter_badge(t.operand, t.operand_offset)
            } else {
                t.amount.to_string()
            };
            match t.math {
                MathOp::Add => format!("{c} += {x}"),
                MathOp::Subtract => format!("{c} \u{2212}= {x}"),
                MathOp::Multiply => format!("{c} \u{d7}= {x}"),
                MathOp::Divide => format!("{c} \u{f7}= {x}"),
                MathOp::Remainder => format!("{c} %= {x}"),
                MathOp::Lowest => format!("{c} min {x}"),
                MathOp::Highest => format!("{c} max {x}"),
                MathOp::Copy => format!("{c} = {x}"),
                MathOp::Random => format!("{c} = rnd {x}"),
                MathOp::Flip => format!("{c} flip"),
                MathOp::And => format!("{c} &= {x}"),
                MathOp::Or => format!("{c} |= {x}"),
                MathOp::Xor => format!("{c} ^= {x}"),
                MathOp::ShiftLeft => format!("{c} <<= {x}"),
                MathOp::ShiftRight => format!("{c} >>= {x}"),
                MathOp::Power => format!("{c} pow {x}"),
            }
        }
        TriggerAction::Script => format!("Script {}", t.text.trim()),
        TriggerAction::Sequence => format!(
            "Seq {}\u{2013}{}",
            t.group,
            i32::from(t.group) + t.amount - 1
        ),
        TriggerAction::Move => format!("Move {}", t.group),
        TriggerAction::Place => format!("Place {}", t.group),
        TriggerAction::Turn => format!("Turn {} {}\u{b0}", t.group, t.amount),
        TriggerAction::Fade => format!("Fade {} {}%", t.group, t.amount),
        TriggerAction::Reset if t.group == 0 => "Reset all".into(),
        TriggerAction::Reset => format!("Reset G{}", t.group),
        TriggerAction::Tint => format!("Tint {} {}%", t.group, t.amount),
        TriggerAction::Scale => format!("Scale {} {}%", t.group, t.amount),
        TriggerAction::Animate if t.amount == 0 => format!("Settle {}", t.group),
        TriggerAction::Animate => format!("{} {} {}", t.style.name(), t.group, t.amount),
        TriggerAction::Stop => format!("Stop {}", t.group),
        TriggerAction::Shake => format!("Shake {}%", t.amount),
        TriggerAction::Sound => t.sound.name().into(),
        TriggerAction::Shove => format!("Shove {} {}", t.group, arrow(t.angle)),
        TriggerAction::Fling => format!("Fling {}", arrow(t.angle)),
        TriggerAction::Blast if t.amount < 0 => "Pull in".into(),
        TriggerAction::Blast => "Blast".into(),
        TriggerAction::Zoom => format!("Zoom {}%", t.amount),
        TriggerAction::Light => format!("Dark {}%", t.amount),
        TriggerAction::Path => format!("Path {} on {}", t.amount, t.group),
        TriggerAction::Ui => match t.ui.as_ref() {
            Some(ui) if ui.widget.board.is_some() => format!("Board {}", t.group),
            Some(ui) if !ui.widget.choices.is_empty() => {
                format!("Ask {} ({})", t.group, ui.widget.choices.len())
            }
            _ => format!("UI {}", t.group),
        },
        TriggerAction::HideUi if t.group == 0 => "Hide all UI".into(),
        TriggerAction::HideUi => format!("Hide UI {}", t.group),
    };
    let mut tag = match gate_tag(t, compact) {
        Some(gate) => format!("{when} if {gate} \u{2192} {action}"),
        None => format!("{when} \u{2192} {action}"),
    };
    if t.otherwise > 0 && t.only_if.is_some() {
        tag += &format!(" else Send {}", t.otherwise);
    }
    tag
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spacing_keeps_the_end_centers_and_drawing_order_with_unequal_sizes() {
        let mut doc = Document::default();
        doc.map.walls = vec![
            [0.1, 0.2, 0.1, 0.1],
            [0.2, 0.3, 0.2, 0.1],
            [0.7, 0.4, 0.2, 0.1],
        ];
        let keys = [Key::Wall(2), Key::Wall(0), Key::Wall(1), Key::Wall(1)];
        let ends = (doc.map.walls[0], doc.map.walls[2]);
        doc.distribute(&keys, 0);
        assert!(close(doc.map.walls[0], ends.0) && close(doc.map.walls[2], ends.1));
        let centers: Vec<_> = doc.map.walls.iter().map(|r| r[0] + r[2] * 0.5).collect();
        assert!((centers[1] - (centers[0] + centers[2]) * 0.5).abs() < 1e-5);
        assert_eq!(doc.map.walls[1][2..], [0.2, 0.1]);
        assert_eq!(doc.map.walls[1][1], 0.3);
    }
    #[test]
    fn matching_sizes_respects_the_arena_and_leaves_point_markers_alone() {
        let mut doc = Document::default();
        doc.map.walls = vec![[0.01, 0.3, 0.1, 0.1], [0.5, 0.4, 0.3, 0.2]];
        doc.map.objective_positions = vec![[0.9, 0.9]];
        let keys = [Key::Wall(0), Key::Wall(1), Key::Objective(0)];
        doc.match_size(&keys, 0);
        assert_eq!(doc.map.walls[0], [0., 0.3, 0.3, 0.1]);
        assert!((doc.map.walls[1][0] - 0.5).abs() < 1e-5);
        assert_eq!(doc.map.objective_positions, [[0.9, 0.9]]);
        let before = doc.clone();
        doc.distribute(&keys, 5);
        doc.match_size(&keys, 5);
        assert_eq!(doc, before);
    }
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
    fn free_channels_skip_listeners_senders_and_otherwise() {
        let mut doc = Document::default();
        doc.insert(Object::Trigger(MapTrigger {
            when: TriggerWhen::Signal,
            source: 1,
            action: TriggerAction::Hide,
            group: 2,
            otherwise: 3,
            ..Default::default()
        }));
        assert_eq!(doc.next_free_channel(), Some(2));
        doc.insert(Object::Trigger(MapTrigger {
            action: TriggerAction::Sequence,
            group: 2,
            amount: 3,
            ..Default::default()
        }));
        assert_eq!(doc.next_free_channel(), Some(5));
        doc.insert(Object::Trigger(MapTrigger {
            action: TriggerAction::Sequence,
            group: 1,
            amount: i32::from(MAP_GROUPS),
            ..Default::default()
        }));
        assert_eq!(doc.next_free_channel(), Some(33), "past the groups");
        doc.insert(Object::Trigger(MapTrigger {
            action: TriggerAction::Ui,
            group: 1,
            ui: Some(Box::new(MapUi {
                widget: HudWidget {
                    board: Some(HudBoard {
                        cells: vec![HudCell {
                            kind: HudCellKind::Button,
                            channel: 33,
                            ..HudCell::default()
                        }],
                        ..HudBoard::default()
                    }),
                    ..MapUi::starter().widget
                },
                ..MapUi::starter()
            })),
            ..Default::default()
        }));
        assert_eq!(doc.next_free_channel(), Some(34), "board buttons send");
        doc.insert(Object::Trigger(MapTrigger {
            action: TriggerAction::Sequence,
            group: 1,
            amount: i32::from(MAP_CHANNELS),
            ..Default::default()
        }));
        assert_eq!(
            doc.next_free_channel(),
            None,
            "never reuse an occupied channel"
        );
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
    fn counter_at_least(index: u8, value: i32) -> TriggerTest {
        TriggerTest {
            index,
            value,
            ..Default::default()
        }
    }
    #[test]
    fn gates_read_as_logic_in_sentences_and_badges() {
        let mut t = MapTrigger {
            when: TriggerWhen::Signal,
            action: TriggerAction::Signal,
            source: 1,
            group: 2,
            otherwise: 3,
            gate: GateMode::Any,
            ..Default::default()
        };
        t.set_conditions(vec![
            counter_at_least(1, 1),
            TriggerTest {
                kind: TestKind::Hidden,
                index: 4,
                ..Default::default()
            },
        ]);
        assert_eq!(
            describe_trigger(&t),
            "On signal 1, if counter 1 is at least 1 or group 4 is hidden: send signal 2. \
             Otherwise sends signal 3."
        );
        assert_eq!(
            trigger_tag(&t),
            "Signal 1 if OR(#1\u{2265}1, G4 off) \u{2192} Send 2 else Send 3"
        );
        t.gate = GateMode::NoneOf;
        t.set_conditions(vec![TriggerTest {
            kind: TestKind::Time,
            compare: Compare::Below,
            value: 30,
            ..Default::default()
        }]);
        assert!(describe_trigger(&t).contains("unless the round time is below 30 s"));
        let watcher = MapTrigger {
            when: TriggerWhen::Defeated,
            source: 2,
            action: TriggerAction::Sequence,
            group: 4,
            amount: 3,
            ..Default::default()
        };
        assert_eq!(
            describe_trigger(&watcher),
            "When the Prism is defeated: send signals 4 to 6 in turn."
        );
    }
    #[test]
    fn links_follow_signals_counters_groups_and_resets() {
        let sender = MapTrigger {
            action: TriggerAction::Sequence,
            group: 2,
            amount: 3,
            ..Default::default()
        };
        let listener = MapTrigger {
            when: TriggerWhen::Signal,
            source: 4,
            ..Default::default()
        };
        assert_eq!(link(&sender, &listener), Some(Link::Signal));
        let counting = MapTrigger {
            action: TriggerAction::Calculate,
            group: 5,
            ..Default::default()
        };
        let mut gated = MapTrigger::default();
        gated.set_conditions(vec![TriggerTest {
            against: 5,
            kind: TestKind::Inside,
            ..Default::default()
        }]);
        assert_eq!(link(&counting, &gated), Some(Link::Counter));
        let hiding = MapTrigger {
            action: TriggerAction::Hide,
            group: 6,
            ..Default::default()
        };
        let alarm = MapTrigger {
            when: TriggerWhen::Hidden,
            source: 6,
            ..Default::default()
        };
        assert_eq!(link(&hiding, &alarm), Some(Link::Group));
        let reset = MapTrigger {
            action: TriggerAction::Reset,
            group: 7,
            ..Default::default()
        };
        let member = MapTrigger {
            member: 7,
            ..Default::default()
        };
        assert_eq!(link(&reset, &member), Some(Link::Reset));
        assert_eq!(link(&member, &reset), None);
    }
    #[test]
    fn checks_find_unheard_channels_quiet_groups_and_loops() {
        let mut doc = Document::default();
        // 1 and 2 signal each other with nothing in between.
        for (source, group) in [(1, 2), (2, 1)] {
            doc.insert(Object::Trigger(MapTrigger {
                when: TriggerWhen::Signal,
                action: TriggerAction::Signal,
                source,
                group,
                ..Default::default()
            }));
        }
        doc.insert(Object::Trigger(MapTrigger {
            action: TriggerAction::Sequence,
            group: 1,
            amount: 3,
            ..Default::default()
        }));
        doc.insert(Object::Trigger(MapTrigger {
            when: TriggerWhen::Shown,
            source: 9,
            action: TriggerAction::Announce,
            text: "Open".into(),
            ..Default::default()
        }));
        let problems = doc.problems();
        let says = |text: &str| problems.iter().any(|p| p.text.contains(text));
        assert!(says("Triggers 1, 2 signal each other"), "{problems:#?}");
        assert!(
            says("sends signal 3, which nothing receives"),
            "{problems:#?}"
        );
        assert!(says("watches group 9, which nothing shows or hides"));
        // A delay breaks the loop.
        if let Some(Object::Trigger(mut first)) = doc.object(Key::Trigger(0)) {
            first.delay = 1.;
            doc.replace(Key::Trigger(0), Object::Trigger(first));
        }
        assert!(!doc.problems().iter().any(|p| p.text.contains("each other")));
    }

    #[test]
    fn checks_find_widgets_that_cannot_show_what_they_ask_for() {
        let mut doc = Document::default();
        // A timer reaches the tank that set it off: nobody.
        doc.insert(Object::Trigger(MapTrigger {
            when: TriggerWhen::Every,
            seconds: 5.,
            action: TriggerAction::Award,
            amount: 10,
            who: TriggerWho::Cause,
            ..Default::default()
        }));
        // Time left on a widget that stays.
        doc.insert(Object::Trigger(MapTrigger {
            when: TriggerWhen::Enter,
            action: TriggerAction::Ui,
            group: 1,
            ui: Some(Box::new(MapUi {
                widget: HudWidget {
                    text: "Gone in {left}".into(),
                    ..HudWidget::default()
                },
                to: UiAudience::Cause,
                ..MapUi::default()
            })),
            ..Default::default()
        }));
        let problems = doc.problems();
        let says = |text: &str| problems.iter().any(|p| p.text.contains(text));
        assert!(
            says("Trigger 1 reaches the tank that set it off, but a timer"),
            "{problems:#?}"
        );
        assert!(!says("Trigger 2 reaches"), "an area has a tank behind it");
        assert!(says("Trigger 2 shows {left}"), "{problems:#?}");
        if let Some(Object::Trigger(mut shown)) = doc.object(Key::Trigger(1)) {
            shown.duration = 5.;
            doc.replace(Key::Trigger(1), Object::Trigger(shown));
        }
        assert!(!doc.problems().iter().any(|p| p.text.contains("{left}")));
    }

    #[test]
    fn summaries_call_groups_by_name_but_leave_quotes_alone() {
        let names = [
            GroupName {
                group: 3,
                name: "Crate".into(),
            },
            GroupName {
                group: 4,
                name: "Vault gate".into(),
            },
        ];
        let plate = MapTrigger {
            when: TriggerWhen::Touch,
            action: TriggerAction::Hide,
            source: 3,
            group: 4,
            ..Default::default()
        };
        assert_eq!(
            describe_trigger_named(&plate, &names),
            "When Crate's blocks reach here: hide group 4 (Vault gate)."
        );
        // Group 34 isn't group 3, and quoted text is the author's own.
        let announce = MapTrigger {
            action: TriggerAction::Announce,
            text: "group 3 is open".into(),
            member: 34,
            ..Default::default()
        };
        let text = describe_trigger_named(&announce, &names);
        assert!(text.contains("\u{201c}group 3 is open\u{201d}"), "{text}");
        assert_eq!(
            describe_trigger_named(&announce, &[]),
            describe_trigger(&announce)
        );
        let flash = MapTrigger {
            action: TriggerAction::Light,
            tween: 0.05,
            duration: 0.15,
            ..Default::default()
        };
        assert!(describe_trigger(&flash).contains("returning after"));
    }

    /// Triggers wired to each other every which way, small numbers so
    /// they meet often.
    fn tangle(count: usize, mut seed: u64) -> Vec<MapTrigger> {
        let mut roll = |below: usize| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as usize % below
        };
        fn test(roll: &mut impl FnMut(usize) -> usize) -> TriggerTest {
            TriggerTest {
                kind: TestKind::ALL[roll(TestKind::ALL.len())],
                index: roll(5) as u8,
                against: roll(4) as u8,
                ..TriggerTest::default()
            }
        }
        (0..count)
            .map(|_| {
                let only_if = (roll(3) == 0).then(|| test(&mut roll));
                let also_if = if only_if.is_some() && roll(2) == 0 {
                    vec![test(&mut roll)]
                } else {
                    Vec::new()
                };
                MapTrigger {
                    when: TriggerWhen::ALL[roll(TriggerWhen::ALL.len())],
                    source: roll(5) as u8,
                    action: TriggerAction::ALL[roll(TriggerAction::ALL.len())],
                    group: roll(5) as u8,
                    amount: roll(4) as i32,
                    member: roll(4) as u8,
                    otherwise: roll(4) as u8,
                    only_if,
                    also_if,
                    ..Default::default()
                }
            })
            .collect()
    }

    #[test]
    fn the_link_index_finds_exactly_the_links_a_full_search_finds() {
        for seed in 1..40 {
            let triggers = tangle(60, seed);
            let index = LinkIndex::new(&triggers);
            let mut found = Vec::new();
            for (at, from) in triggers.iter().enumerate() {
                index.affected(&triggers, at, &mut found);
                let every: Vec<(usize, Link)> = triggers
                    .iter()
                    .enumerate()
                    .filter(|&(other, _)| other != at)
                    .filter_map(|(other, to)| Some((other, link(from, to)?)))
                    .collect();
                assert_eq!(found, every, "seed {seed}, trigger {at}: {from:?}");
            }
        }
    }

    #[test]
    fn loops_are_found_in_long_chains_without_deep_recursion() {
        let relay = |listen: usize, send: usize| MapTrigger {
            when: TriggerWhen::Signal,
            source: (listen % 200 + 1) as u8,
            action: TriggerAction::Signal,
            group: (send % 200 + 1) as u8,
            ..Default::default()
        };
        // 200 channels in a line: each relay passes its signal on to the
        // next, and the last one stops.
        let mut chain: Vec<MapTrigger> = (0..199).map(|i| relay(i, i + 1)).collect();
        assert_eq!(signal_loop(&chain), None);
        chain.push(relay(199, 0));
        let ring = signal_loop(&chain).expect("the last one closes the ring");
        assert_eq!(ring.len(), 200);
        // A gate on any link stops it counting as a loop.
        chain[57].only_if = Some(TriggerTest::default());
        assert_eq!(signal_loop(&chain), None);
        // Thousands of listeners on one channel are no trouble.
        let mut crowd: Vec<MapTrigger> = (0..4000).map(|i| relay(i % 3, 3)).collect();
        assert_eq!(signal_loop(&crowd), None);
        crowd.push(relay(3, 0));
        assert!(signal_loop(&crowd).is_some());
    }
}

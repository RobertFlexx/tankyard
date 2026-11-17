//! The map a hosted game plays on: one of the arenas, a map saved in the
//! editor, or a map from a pack in the library.
use super::*;
use std::path::PathBuf;

/// The picker's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapSource {
    #[default]
    Arenas,
    Saved,
    Packs,
}

/// A map saved in the editor, hosted from its project file.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedMap {
    pub path: PathBuf,
    pub id: String,
    pub name: String,
    pub size: (f32, f32),
}

impl SavedMap {
    /// Pack the project's latest save into `folder`, replacing any older
    /// copy, and return the pack's path.
    fn pack_into(&self, folder: &std::path::Path) -> Result<PathBuf, String> {
        let doc = crate::editor::read_document(&self.path)
            .map_err(|error| format!("Cannot read {}: {error}", self.name))?;
        if doc.map.id != self.id || !tankyard_mods::identifier(&doc.map.id) {
            return Err(format!(
                "{} was saved under another map id since you chose it; choose it again.",
                self.name
            ));
        }
        let bytes = doc
            .pack()
            .and_then(|pack| pack.to_zip())
            .map_err(|error| format!("{} cannot be hosted yet: {error}", self.name))?;
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        let path = folder.join(format!("{}.zip", self.id));
        // Write beside it first, so a server never reads half a pack.
        let partial = folder.join(format!("{}.zip.partial", self.id));
        std::fs::write(&partial, &bytes)
            .and_then(|()| std::fs::rename(&partial, &path))
            .map_err(|e| format!("Cannot save {} for hosting: {e}", self.name))?;
        Ok(path)
    }
}

/// A saved project: a map ready to host, or its file name and why not.
pub(super) type Project = Result<SavedMap, (String, String)>;

/// Whether a pack's maps can be hosted now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PackState {
    /// Enabled, so its maps are loaded.
    Enabled,
    /// Clean or accepted, waiting to be enabled.
    Ready,
    /// Changed or never reviewed: the player reviews it in Mods first.
    Review,
    /// It cannot load.
    Broken,
}

/// One map in a library pack.
pub(super) struct PackMap {
    pack: String,
    pack_name: String,
    map: String,
    name: String,
    size: (f32, f32),
    state: PackState,
}

/// What a row on the page picks.
#[derive(Clone, Copy)]
enum Pick {
    Arena(usize),
    Saved(usize),
    Pack(usize),
}

struct Row {
    title: String,
    detail: String,
    selected: bool,
    enabled: bool,
    pick: Pick,
}

/// Projects in the editor's folder, by name, those that cannot be hosted
/// last.
fn scan_projects() -> Vec<Project> {
    let Ok(root) = crate::editor::projects_dir() else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    paths.truncate(128);
    let mut projects: Vec<Project> = paths
        .into_iter()
        .map(|path| {
            let file = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            crate::editor::read_document(&path)
                .and_then(|doc| {
                    doc.validate()?;
                    Ok(SavedMap {
                        id: doc.map.id.clone(),
                        name: doc.map.name.clone(),
                        size: (doc.map.width, doc.map.height),
                        path,
                    })
                })
                .map_err(|error| (file, error))
        })
        .collect();
    projects.sort_by_key(|project| match project {
        Ok(saved) => (0, saved.name.to_lowercase()),
        Err((file, _)) => (1, file.to_lowercase()),
    });
    projects
}

/// Every map in the library's packs, with whether it can be hosted now.
fn scan_packs(settings: &Settings) -> Vec<PackMap> {
    let mut maps = Vec::new();
    for entry in crate::mod_library::discover() {
        let Some(manifest) = &entry.manifest else {
            continue;
        };
        if entry.staff.is_some() || entry.editor_only() {
            continue;
        }
        let state = if entry.error.is_some() {
            PackState::Broken
        } else if !entry.approved(settings) {
            PackState::Review
        } else if entry.enabled(settings) {
            PackState::Enabled
        } else {
            PackState::Ready
        };
        maps.extend(manifest.maps.iter().map(|map| PackMap {
            pack: manifest.id.clone(),
            pack_name: entry.name().to_owned(),
            map: map.id.clone(),
            name: map.name.clone(),
            size: (map.width, map.height),
            state,
        }));
    }
    maps
}

fn size_text((width, height): (f32, f32)) -> String {
    format!("{width:.0} × {height:.0}")
}

/// A row of two lines: the title, and a detail line under it.
fn map_row(rect: Rect, row: &Row, s: f32) -> bool {
    let clicked = button(rect, "", row.selected, row.enabled, s);
    let room = rect.w - 28. * s;
    label(
        &fitted_label(&row.title, 17. * s, room),
        rect.x + 14. * s,
        rect.y + 25. * s,
        17. * s,
        if row.enabled {
            text_color()
        } else {
            muted_color()
        },
    );
    label(
        &fitted_label(&row.detail, 12.5 * s, room),
        rect.x + 14. * s,
        rect.y + 45. * s,
        12.5 * s,
        muted_color(),
    );
    clicked
}

impl Menu {
    /// The settings and packs a hosted game loads: the chosen saved map is
    /// packed fresh from its project and loads last.
    pub fn host_setup(
        &self,
        mut mods: Vec<PathBuf>,
    ) -> Result<(WorldConfig, Vec<PathBuf>), String> {
        let mut config = self.config();
        if let Some(saved) = &self.host_map {
            // An exported copy of the same map would clash with it.
            mods.retain(|path| crate::mod_library::inspect(path).id() != Some(saved.id.as_str()));
            if mods.len() >= tankyard_mods::MAX_MODS {
                return Err(format!(
                    "Hosting {} needs a free pack slot: disable one of your 16 packs in Mods.",
                    saved.name
                ));
            }
            mods.push(saved.pack_into(&crate::hosting::server_dir().join("maps"))?);
            config.map = saved.id.clone();
        }
        Ok((config, mods))
    }

    /// The hosted map, where it comes from and the arena size, for the Host
    /// page.
    pub(super) fn host_map_line(&self) -> String {
        let size = size_text((self.width, self.height));
        match &self.host_map {
            Some(saved) => format!("{}  ·  saved map  ·  {size}", saved.name),
            None => self.maps.get(self.map).map_or(size.clone(), |map| {
                let source = if self.builtin_maps.contains(&map.id) {
                    "arena"
                } else {
                    "pack map"
                };
                format!("{}  ·  {source}  ·  {size}", map.name)
            }),
        }
    }

    /// Host a custom map at the size it was made for. Adaptive borders would
    /// squeeze its layout, so they go off.
    fn use_map_size(&mut self, (width, height): (f32, f32)) {
        if width.is_finite() && height.is_finite() {
            self.width = width.clamp(900., 16000.).round();
            self.height = height.clamp(700., 16000.).round();
        }
        self.dynamic_arena = false;
    }

    /// Choose `index` in the arena list.
    fn pick_arena(&mut self, index: usize) {
        self.map = index;
        self.map_page = index / 8;
        self.host_map = None;
    }

    /// Select the pack map chosen before its pack had loaded, once the
    /// enabled packs' maps include it.
    pub(super) fn resolve_host_pick(&mut self) {
        let Some(pick) = &self.host_pick else {
            return;
        };
        if let Some(index) = self.maps.iter().position(|map| &map.id == pick) {
            self.host_pick = None;
            self.pick_arena(index);
            let map = &self.maps[index];
            let (name, size) = (map.name.clone(), (map.width, map.height));
            self.use_map_size(size);
            self.info = format!("Hosting {name} at its own size, {}.", size_text(size));
        }
    }

    fn poll_host_chooser(&mut self, settings: &mut Settings) {
        let Some(result) = self
            .host_chooser
            .as_ref()
            .and_then(|chooser| chooser.poll())
        else {
            return;
        };
        self.host_chooser = None;
        match result {
            Ok(Some(path)) => {
                let imported = crate::mod_library::import(&path);
                let found = imported.as_ref().ok().and_then(|entry| {
                    let manifest = entry.manifest.as_ref()?;
                    Some((
                        manifest.id.clone(),
                        manifest.maps.first().map(|m| m.id.clone()),
                    ))
                });
                self.import_result(settings, imported);
                self.pack_maps = None;
                match found {
                    Some((pack, Some(map))) if settings.enabled_mods.contains(&pack) => {
                        self.host_pick = Some(map);
                    }
                    Some((_, None)) if self.error.is_empty() => {
                        self.info.push_str(" It has no maps to host.");
                    }
                    _ => {}
                }
            }
            Ok(None) => {}
            Err(error) => self.error = error,
        }
    }

    pub(super) fn host_maps(&mut self, settings: &mut Settings, s: f32) -> Action {
        self.resolve_host_pick();
        self.poll_host_chooser(settings);
        let x = 40. * s;
        let y = 166. * s;
        let w = screen_width() - 80. * s;
        label("Choose a map", x, y, 25. * s, text_color());
        let tab_w = 128. * s;
        for (i, (title, tab)) in [
            ("Arenas", MapSource::Arenas),
            ("My maps", MapSource::Saved),
            ("Map packs", MapSource::Packs),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(
                    x + w - (3 - i) as f32 * (tab_w + 6. * s) + 6. * s,
                    y - 22. * s,
                    tab_w,
                    30. * s,
                ),
                title,
                self.host_source == tab,
                true,
                s * 0.75,
            ) && self.host_source != tab
            {
                self.host_source = tab;
                self.host_map_page = 0;
                // Each visit reads the folders again.
                self.saved_maps = None;
                self.pack_maps = None;
            }
        }
        label(
            match self.host_source {
                MapSource::Arenas => {
                    "Built-in arenas and maps from enabled packs, at the arena size set in Configure arena."
                }
                MapSource::Saved => {
                    "Maps you saved in the map editor. Hosting packs the latest save, at the map's own size."
                }
                MapSource::Packs => {
                    "Maps in your packs. Choosing one enables its pack; a pack that needs a review opens in Mods first."
                }
            },
            x,
            y + 26. * s,
            14. * s,
            muted_color(),
        );
        let rows = self.host_map_rows(settings);
        let top = y + 48. * s;
        let row_h = 58. * s;
        let gap = 8. * s;
        let columns = if w > 860. * s { 2 } else { 1 };
        let column_w = (w - gap * (columns - 1) as f32) / columns as f32;
        let fit = ((screen_height() - 190. * s - top) / (row_h + gap))
            .floor()
            .clamp(2., 8.) as usize;
        let per_page = fit * columns;
        let pages = rows.len().div_ceil(per_page).max(1);
        self.host_map_page = self.host_map_page.min(pages - 1);
        let mut chosen = None;
        for (slot, row) in rows
            .iter()
            .skip(self.host_map_page * per_page)
            .take(per_page)
            .enumerate()
        {
            let rect = Rect::new(
                x + (slot % columns) as f32 * (column_w + gap),
                top + (slot / columns) as f32 * (row_h + gap),
                column_w,
                row_h,
            );
            if map_row(rect, row, s) {
                chosen = Some(row.pick);
            }
        }
        if rows.is_empty() {
            wrap(
                match self.host_source {
                    MapSource::Arenas => "No arenas are loaded. Check the packs on the Mods page.",
                    MapSource::Saved => {
                        "No saved maps yet. Build one in the map editor and press Save; it shows up here."
                    }
                    MapSource::Packs => {
                        "No packs with maps yet. Browse for a .zip pack or the mod.json inside a pack folder, or drop packs into the mods folder and press Refresh."
                    }
                },
                x,
                top + 22. * s,
                w.min(720. * s),
                17. * s,
                text_color(),
            );
        }
        let below =
            top + fit.min(rows.len().div_ceil(columns).max(1)) as f32 * (row_h + gap) + 6. * s;
        if pages > 1 {
            let page_x = x + w - 172. * s;
            if small_button(Rect::new(page_x, below, 50. * s, 30. * s), "‹", s) {
                self.host_map_page = self.host_map_page.saturating_sub(1);
            }
            center_label(
                &format!("{} / {pages}", self.host_map_page + 1),
                page_x + 86. * s,
                below + 20. * s,
                14. * s,
                muted_color(),
            );
            if small_button(
                Rect::new(page_x + 122. * s, below, 50. * s, 30. * s),
                "›",
                s,
            ) {
                self.host_map_page = (self.host_map_page + 1).min(pages - 1);
            }
        }
        if let Some(action) = self.host_map_tools(below, s) {
            return action;
        }
        match chosen {
            Some(pick) => self.choose_host_map(settings, pick),
            None => Action::None,
        }
    }

    fn host_map_rows(&mut self, settings: &Settings) -> Vec<Row> {
        match self.host_source {
            MapSource::Arenas => self
                .maps
                .iter()
                .enumerate()
                .map(|(index, map)| Row {
                    title: map.name.clone(),
                    detail: if self.builtin_maps.contains(&map.id) {
                        format!("Arena  ·  made for {}", size_text((map.width, map.height)))
                    } else {
                        format!(
                            "From a pack  ·  made for {}",
                            size_text((map.width, map.height))
                        )
                    },
                    selected: self.host_map.is_none() && self.map == index,
                    enabled: true,
                    pick: Pick::Arena(index),
                })
                .collect(),
            MapSource::Saved => {
                let projects = self.saved_maps.get_or_insert_with(scan_projects);
                projects
                    .iter()
                    .enumerate()
                    .map(|(index, project)| match project {
                        Ok(saved) => Row {
                            title: saved.name.clone(),
                            detail: format!("{}  ·  {}", saved.id, size_text(saved.size)),
                            selected: self
                                .host_map
                                .as_ref()
                                .is_some_and(|chosen| chosen.path == saved.path),
                            enabled: true,
                            pick: Pick::Saved(index),
                        },
                        Err((file, error)) => Row {
                            title: file.clone(),
                            detail: format!("Cannot host yet: {error}"),
                            selected: false,
                            enabled: false,
                            pick: Pick::Saved(index),
                        },
                    })
                    .collect()
            }
            MapSource::Packs => {
                let chosen = self
                    .host_map
                    .is_none()
                    .then(|| self.maps.get(self.map).map(|map| map.id.clone()))
                    .flatten();
                let packs = self.pack_maps.get_or_insert_with(|| scan_packs(settings));
                packs
                    .iter()
                    .enumerate()
                    .map(|(index, map)| Row {
                        title: map.name.clone(),
                        detail: format!(
                            "{}  ·  {}  ·  {}",
                            map.pack_name,
                            size_text(map.size),
                            match map.state {
                                PackState::Enabled => "enabled",
                                PackState::Ready => "choose to enable",
                                PackState::Review => "needs a review in Mods",
                                PackState::Broken => "cannot load; see Mods",
                            }
                        ),
                        selected: map.state == PackState::Enabled
                            && chosen.as_deref() == Some(map.map.as_str()),
                        enabled: map.state != PackState::Broken,
                        pick: Pick::Pack(index),
                    })
                    .collect()
            }
        }
    }

    /// The buttons under the list: folders, the file chooser and Refresh.
    fn host_map_tools(&mut self, y: f32, s: f32) -> Option<Action> {
        let x = 40. * s;
        let button_w = 200. * s;
        let at = |i: usize| Rect::new(x + i as f32 * (button_w + 8. * s), y, button_w, 30. * s);
        match self.host_source {
            MapSource::Arenas => {
                if small_button(at(0), "Configure arena", s) {
                    return Some(Action::Page(Page::Modes));
                }
            }
            MapSource::Saved => {
                if small_button(at(0), "Open map editor", s) {
                    return Some(Action::OpenEditor);
                }
                if small_button(at(1), "Open maps folder", s) {
                    match crate::editor::projects_dir()
                        .and_then(|folder| crate::file_dialog::reveal(&folder).map(|()| folder))
                    {
                        Ok(folder) => self.info = format!("Opened {}.", folder.display()),
                        Err(error) => self.error = error,
                    }
                }
                if small_button(at(2), "Refresh", s) {
                    self.saved_maps = None;
                }
            }
            MapSource::Packs => {
                let choosing = self.host_chooser.is_some();
                if button(
                    at(0),
                    if choosing {
                        "Choosing…"
                    } else {
                        "Browse for a pack…"
                    },
                    false,
                    !choosing,
                    s * 0.75,
                ) {
                    self.host_chooser = Some(crate::file_dialog::choose_mod_pack());
                    self.error.clear();
                    self.info = "Pick a .zip pack, or the mod.json inside a pack folder.".into();
                }
                if small_button(at(1), "Open mods folder", s) {
                    let folder = crate::mod_library::library_dir();
                    match crate::file_dialog::reveal(&folder) {
                        Ok(()) => {
                            self.info = format!(
                                "Opened {}. Drop pack folders or .zip files there, then press Refresh.",
                                folder.display()
                            )
                        }
                        Err(error) => self.error = error,
                    }
                }
                if small_button(at(2), "Refresh", s) {
                    self.pack_maps = None;
                    self.mods_scanned = false;
                    self.catalog_stale = true;
                }
            }
        }
        None
    }

    fn choose_host_map(&mut self, settings: &mut Settings, pick: Pick) -> Action {
        self.error.clear();
        match pick {
            Pick::Arena(index) => {
                let Some(name) = self.maps.get(index).map(|map| map.name.clone()) else {
                    return Action::None;
                };
                self.pick_arena(index);
                self.info = format!("Hosting {name}.");
                Action::Back
            }
            Pick::Saved(index) => {
                let Some(Ok(saved)) = self.saved_maps.as_ref().and_then(|maps| maps.get(index))
                else {
                    return Action::None;
                };
                let saved = saved.clone();
                self.use_map_size(saved.size);
                self.info = format!(
                    "Hosting {} at its own size, {}.",
                    saved.name,
                    size_text(saved.size)
                );
                self.host_pick = None;
                self.host_map = Some(saved);
                Action::Back
            }
            Pick::Pack(index) => {
                let Some(map) = self.pack_maps.as_ref().and_then(|maps| maps.get(index)) else {
                    return Action::None;
                };
                let (pack, pack_name, id, name, state) = (
                    map.pack.clone(),
                    map.pack_name.clone(),
                    map.map.clone(),
                    map.name.clone(),
                    map.state,
                );
                match state {
                    PackState::Broken => Action::None,
                    PackState::Review => {
                        self.mod_focus = Some(pack);
                        self.mods_scanned = false;
                        self.info =
                            format!("Review {pack_name} and enable it, then choose {name} again.");
                        Action::Page(Page::Mods)
                    }
                    PackState::Ready if settings.enabled_mods.len() >= tankyard_mods::MAX_MODS => {
                        self.error =
                            "At most 16 packs can be enabled at once. Disable one in Mods first."
                                .into();
                        Action::None
                    }
                    PackState::Ready | PackState::Enabled => {
                        if state == PackState::Ready {
                            settings.enabled_mods.push(pack);
                            if let Err(error) = settings.save() {
                                log::warn!("Cannot save the enabled packs: {error}");
                            }
                            self.pack_maps = None;
                        }
                        // The enabled packs' maps reload before the next
                        // frame; select it once they include it.
                        self.host_pick = Some(id);
                        self.resolve_host_pick();
                        if self.host_pick.is_some() {
                            self.info = format!("Enabled {pack_name} for {name}.");
                        }
                        Action::Back
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_map_packs_into_a_pack_the_host_loads() {
        let root = std::env::temp_dir().join(format!("tankyard-host-map-{}", std::process::id()));
        let maps = root.join("maps");
        std::fs::create_dir_all(&root).unwrap();
        let mut map = tankyard_content::builtin_maps().remove(0);
        map.id = "test_yard".into();
        map.name = "Test Yard".into();
        let doc = tankyard_client::editor_document::Document::from_map(map);
        let path = root.join("test_yard.json");
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        let saved = SavedMap {
            path,
            id: "test_yard".into(),
            name: "Test Yard".into(),
            size: (3600., 2600.),
        };
        let pack = saved.pack_into(&maps).unwrap();
        let loaded = crate::mod_library::load(std::slice::from_ref(&pack)).unwrap();
        assert!(loaded.bundle.maps.iter().any(|map| map.id == "test_yard"));
        // Packing again replaces the copy and leaves nothing half written.
        saved.pack_into(&maps).unwrap();
        let files: Vec<_> = std::fs::read_dir(&maps)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(files, ["test_yard.zip"]);
        // A project saved under another id since it was chosen is refused.
        let renamed = SavedMap {
            id: "other_yard".into(),
            ..saved
        };
        assert!(renamed.pack_into(&maps).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}

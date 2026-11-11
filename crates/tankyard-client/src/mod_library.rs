//! The player's mod library: discovery, import, static review and approval.
//! Packs that need review only load after the player confirms the exact
//! contents they reviewed; a changed pack must be reviewed again.
//!
//! Enabled packs are remembered by ID, not by path, so they stay enabled
//! when the game moves, updates or starts from another folder. A pack in the
//! library overrides a bundled pack with the same ID.
use crate::settings::Settings;
use directories::ProjectDirs;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};
use tankyard_client::staff_pack::{self, Identity, Verified};
use tankyard_mods::{
    LoadedMods, Manifest, PackFiles, TextureData,
    project::Project,
    scan::{ScanReport, Verdict, scan_pack},
};

const MAX_LIBRARY: usize = 64;

#[derive(Clone)]
pub struct Entry {
    pub path: String,
    pub manifest: Option<Manifest>,
    pub report: ScanReport,
    pub digest: String,
    /// The pre-0.12 checksum, for carrying old approvals over.
    pub legacy_digest: String,
    pub archive: bool,
    pub error: Option<String>,
    /// Storefront details from `tankyard.toml`.
    pub project: Option<Project>,
    /// Why `tankyard.toml` or its icon could not be read.
    pub project_error: Option<String>,
    pub icon: Option<TextureData>,
    /// A signed staff credential, which unlocks tools instead of adding content.
    pub staff: Option<Verified>,
    /// Bytes across every file in the pack.
    pub size: u64,
    pub files: usize,
}

impl Entry {
    pub fn id(&self) -> Option<&str> {
        self.manifest.as_ref().map(|m| m.id.as_str())
    }
    pub fn name(&self) -> &str {
        self.project
            .as_ref()
            .and_then(|p| p.details.name.as_deref())
            .or(self.manifest.as_ref().map(|m| m.name.as_str()))
            .unwrap_or_else(|| file_name(&self.path))
    }
    pub fn version(&self) -> Option<&str> {
        self.project
            .as_ref()
            .and_then(|p| p.details.version.as_deref())
            .or(self.manifest.as_ref().map(|m| m.version.as_str()))
    }
    pub fn creator(&self) -> Option<&str> {
        self.project
            .as_ref()
            .and_then(|p| p.creator.name.as_deref())
            .or(self.manifest.as_ref().and_then(|m| m.author.as_deref()))
    }
    pub fn description(&self) -> Option<&str> {
        self.project
            .as_ref()
            .and_then(|p| p.details.description.as_deref())
            .or(self
                .manifest
                .as_ref()
                .and_then(|m| m.description.as_deref()))
    }
    pub fn scripted(&self) -> bool {
        self.manifest
            .as_ref()
            .is_some_and(|m| !m.scripts.is_empty())
    }
    pub fn verdict(&self) -> Verdict {
        self.report.verdict()
    }
    /// A pack of workshop plugins and nothing else.
    pub fn editor_only(&self) -> bool {
        self.manifest.as_ref().is_some_and(Manifest::editor_only)
    }
    pub fn approved(&self, settings: &Settings) -> bool {
        !self.report.requires_confirmation() || settings.approved_mods.contains(&self.digest)
    }
    pub fn enabled(&self, settings: &Settings) -> bool {
        self.id()
            .is_some_and(|id| settings.enabled_mods.iter().any(|e| e == id))
    }
    /// Packs the player added, which the game may delete; bundled packs ship
    /// with the game and stay.
    pub fn in_library(&self) -> bool {
        let library = library_dir();
        let library = library.canonicalize().unwrap_or(library);
        Path::new(&self.path)
            .canonicalize()
            .is_ok_and(|path| path.starts_with(&library))
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A pack's folder or archive name, which is its ID when the game stored it.
fn stem(path: &str) -> &str {
    let name = file_name(path.trim_end_matches(['/', '\\']));
    name.strip_suffix(".zip").unwrap_or(name)
}

/// Where imported packs live, beside the player's settings.
pub fn library_dir() -> PathBuf {
    ProjectDirs::from("org", "Tank Yard", "Tank Yard")
        .map(|p| p.data_dir().join("mods"))
        .unwrap_or_else(|| PathBuf::from("mods"))
}

/// A cheap fingerprint of a pack's files from their sizes and modification
/// times, so unchanged packs are not read, scanned or decoded again.
fn stamp(path: &Path) -> Option<u64> {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut mix = |bytes: &[u8]| {
        for b in bytes {
            hash = (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    let mut pending = vec![(path.to_path_buf(), 0)];
    let mut seen = 0;
    while let Some((current, depth)) = pending.pop() {
        let metadata = std::fs::metadata(&current).ok()?;
        mix(current.to_string_lossy().as_bytes());
        mix(&metadata.len().to_le_bytes());
        if let Ok(modified) = metadata.modified()
            && let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH)
        {
            mix(&since.as_nanos().to_le_bytes());
        }
        if metadata.is_dir() && depth < 8 {
            for child in std::fs::read_dir(&current).ok()?.filter_map(Result::ok) {
                seen += 1;
                if seen > 1024 {
                    return None;
                }
                pending.push((child.path(), depth + 1));
            }
        }
    }
    Some(hash)
}

/// Each pack's path and file stamp, identifying one exact set of packs.
type LoadKey = Vec<(PathBuf, u64)>;

thread_local! {
    static INSPECTED: RefCell<HashMap<PathBuf, (u64, Rc<Entry>)>> = RefCell::default();
    static LOADED: RefCell<Option<(LoadKey, LoadedMods)>> = const { RefCell::new(None) };
}

/// Read and review one pack without running any of it. Results are reused
/// until the pack's files change.
pub fn inspect(path: &Path) -> Entry {
    let Some(stamp) = stamp(path) else {
        return read_entry(path);
    };
    if let Some(entry) = INSPECTED.with(|cache| {
        cache
            .borrow()
            .get(path)
            .filter(|(known, _)| *known == stamp)
            .map(|(_, entry)| entry.clone())
    }) {
        return (*entry).clone();
    }
    let entry = read_entry(path);
    INSPECTED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > MAX_LIBRARY * 2 {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), (stamp, Rc::new(entry.clone())));
    });
    entry
}

fn read_entry(path: &Path) -> Entry {
    let display = path.to_string_lossy().into_owned();
    match tankyard_mods::read_pack(path) {
        Ok((manifest, pack)) => {
            // A staff pack someone changed, or signed with another key, is
            // shown as broken and never asks for a password.
            let (staff, staff_error) = match manifest.staff.is_some() {
                true => match staff_pack::verify(&manifest, &pack) {
                    Ok(verified) => (Some(verified), None),
                    Err(error) => (None, Some(error)),
                },
                false => (None, None),
            };
            let (project, mut project_error) = match Project::read(&pack) {
                Some(Ok(project)) => (Some(project), None),
                Some(Err(error)) => (None, Some(error)),
                None => (None, None),
            };
            let icon = match project.as_ref().and_then(|p| p.icon(&pack)) {
                Some(Ok(icon)) => Some(icon),
                Some(Err(error)) => {
                    project_error = Some(format!("icon: {error}"));
                    None
                }
                None => None,
            };
            Entry {
                path: display,
                report: scan_pack(&pack),
                digest: pack.content_digest(),
                legacy_digest: pack.legacy_content_digest(),
                archive: pack.archive,
                size: pack.files().map(|(_, data)| data.len() as u64).sum(),
                files: pack.len(),
                manifest: Some(manifest),
                error: staff_error,
                project,
                project_error,
                icon,
                staff,
            }
        }
        Err(error) => {
            // A pack that cannot be parsed still gets a review of its files.
            let report = PackFiles::read(path)
                .map(|pack| scan_pack(&pack))
                .unwrap_or_default();
            Entry {
                path: display,
                manifest: None,
                report,
                digest: String::new(),
                legacy_digest: String::new(),
                archive: path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("zip")),
                error: Some(error),
                project: None,
                project_error: None,
                icon: None,
                staff: None,
                size: 0,
                files: 0,
            }
        }
    }
}

/// Folders the game ships its own packs in: beside the executable, inside a
/// macOS app bundle, and the working directory.
fn bundled_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir.join("mods"));
        dirs.push(exe_dir.join("../Resources/mods"));
    }
    dirs.push(PathBuf::from("mods"));
    dirs
}

/// Every pack in the library and in the game's bundled `mods` folders. When
/// two share an ID, the library's copy wins.
pub fn discover() -> Vec<Entry> {
    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for directory in std::iter::once(library_dir()).chain(bundled_dirs()) {
        // The same folder can be reached two ways, e.g. cwd beside the exe.
        let Ok(real) = std::fs::canonicalize(&directory) else {
            continue;
        };
        if !seen.insert(real.clone()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&real) else {
            continue;
        };
        let mut found: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
                    && ((path.is_dir() && path.join("mod.json").is_file())
                        || path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip")))
            })
            .collect();
        found.sort();
        paths.extend(found);
    }
    paths.truncate(MAX_LIBRARY);
    let mut ids = std::collections::HashSet::new();
    paths
        .iter()
        .map(|path| inspect(path))
        .filter(|entry| entry.id().is_none_or(|id| ids.insert(id.to_owned())))
        .collect()
}

/// Approvals made before 0.12 were bound to a weak checksum. Each one that
/// still matches an installed pack moves to that pack's SHA-256 digest, once;
/// the rest are dropped.
fn carry_over_approvals(settings: &mut Settings) {
    let current = |digest: &String| digest.starts_with(PackFiles::SHA256_PREFIX);
    if settings.approved_mods.iter().all(current) {
        return;
    }
    let (kept, legacy): (Vec<_>, Vec<_>) = std::mem::take(&mut settings.approved_mods)
        .into_iter()
        .partition(current);
    settings.approved_mods = kept;
    for entry in discover() {
        if !entry.digest.is_empty()
            && legacy.contains(&entry.legacy_digest)
            && !settings.approved_mods.contains(&entry.digest)
        {
            settings.approved_mods.push(entry.digest);
        }
    }
}

/// Turn enabled entries saved as paths by older versions into pack IDs, and
/// carry old approvals over to current digests.
pub fn migrate(settings: &mut Settings) {
    carry_over_approvals(settings);
    if settings
        .enabled_mods
        .iter()
        .all(|entry| tankyard_mods::identifier(entry))
    {
        return;
    }
    let mut migrated = Vec::new();
    for entry in std::mem::take(&mut settings.enabled_mods) {
        let id = if tankyard_mods::identifier(&entry) {
            Some(entry)
        } else {
            inspect(Path::new(&entry))
                .id()
                .map(str::to_owned)
                .or_else(|| {
                    // A pack that moved: its folder or archive name is its ID.
                    let stem = stem(&entry);
                    tankyard_mods::identifier(stem).then(|| stem.to_owned())
                })
        };
        if let Some(id) = id
            && !migrated.contains(&id)
        {
            migrated.push(id);
        }
    }
    settings.enabled_mods = migrated;
}

/// Enabled packs sorted into those that may load now and those held back.
#[derive(Default)]
pub struct Ready {
    /// In the order they were enabled.
    pub paths: Vec<PathBuf>,
    /// Why each held pack is waiting.
    pub held: Vec<String>,
    /// Enabled IDs with no installed pack that can load, which only
    /// disabling them clears.
    pub missing: Vec<String>,
}

/// Enabled packs that may load now. Packs whose reviewed contents changed,
/// were never confirmed, are missing or are broken are held back with a reason.
pub fn ready_paths(settings: &Settings) -> Ready {
    ready_in(settings, discover)
}

fn ready_in(settings: &Settings, discover: impl FnOnce() -> Vec<Entry>) -> Ready {
    let mut ready = Ready::default();
    if let Some(paths) = &settings.session_mods {
        // Packs named on the command line, for this run only.
        for path in paths {
            let entry = inspect(path);
            if entry.staff.is_some() || entry.editor_only() {
                continue;
            }
            if let Some(error) = &entry.error {
                ready.held.push(format!("{}: {error}", entry.name()));
            } else if !settings.session_mods_trusted && !entry.approved(settings) {
                ready.held.push(format!(
                    "{} needs review; pass --accept-mod-risk to load it anyway.",
                    entry.name()
                ));
            } else {
                ready.paths.push(path.clone());
            }
        }
        return ready;
    }
    if settings.enabled_mods.is_empty() {
        return ready;
    }
    let library = discover();
    for id in &settings.enabled_mods {
        let Some(entry) = library.iter().find(|entry| entry.id() == Some(id)) else {
            // A broken pack has no readable ID, but the game stores packs
            // under their ID, so its file name still says which it is.
            let broken = library
                .iter()
                .any(|entry| entry.id().is_none() && stem(&entry.path) == id);
            ready.held.push(if broken {
                format!("{id} is enabled but cannot load; see Mods.")
            } else {
                format!("{id} is enabled but no longer installed.")
            });
            ready.missing.push(id.clone());
            continue;
        };
        if entry.staff.is_some() || entry.editor_only() {
            // Staff packs unlock tools and workshop plugins add editor
            // tools; neither loads into a match.
            continue;
        }
        if !entry.approved(settings) {
            ready.held.push(format!(
                "{} changed or was never reviewed; confirm it in Mods before playing with it.",
                entry.name()
            ));
        } else {
            ready.paths.push(PathBuf::from(&entry.path));
        }
    }
    ready
}

/// Enabled, reviewed packs with workshop plugins, and why others wait.
pub fn editor_paths(settings: &Settings) -> Ready {
    let mut ready = Ready::default();
    let entries: Vec<Entry> = match &settings.session_mods {
        Some(paths) => paths.iter().map(|path| inspect(path)).collect(),
        None => discover()
            .into_iter()
            .filter(|entry| entry.enabled(settings))
            .collect(),
    };
    for entry in entries {
        if entry
            .manifest
            .as_ref()
            .is_none_or(|manifest| manifest.editor.is_empty())
        {
            continue;
        }
        let trusted = settings.session_mods.is_some() && settings.session_mods_trusted;
        if trusted || entry.approved(settings) {
            ready.paths.push(PathBuf::from(&entry.path));
        } else {
            ready.held.push(format!(
                "{} changed or was never reviewed; confirm it in Mods first.",
                entry.name()
            ));
        }
    }
    ready
}

/// Enabled staff packs whose signatures checked out, one per identity, by
/// pack name.
pub fn staff_packs(settings: &Settings) -> Vec<(String, Verified)> {
    let mut packs: Vec<(String, Verified)> = Vec::new();
    for entry in discover() {
        if let Some(verified) = &entry.staff
            && entry.enabled(settings)
            && !packs
                .iter()
                .any(|(_, known)| known.identity == verified.identity)
        {
            packs.push((entry.name().to_owned(), verified.clone()));
        }
    }
    packs
}

/// The window title for the staff packs a player has enabled, matching the
/// editions these packs replace.
pub fn staff_title(packs: &[(String, Verified)]) -> &'static str {
    let has = |identity| packs.iter().any(|(_, v)| v.identity == identity);
    match (has(Identity::Creator), has(Identity::CentralOwner)) {
        (true, true) => "Tank Yard · Creator + Central Owner",
        (true, false) => "Tank Yard · Creator Edition",
        (false, true) => "Tank Yard · Central Owner Edition",
        (false, false) => "Tank Yard",
    }
}

/// Load packs, reusing the last result while the same unchanged packs are
/// requested again (menus, offline games and hosting all ask for them).
pub fn load(paths: &[PathBuf]) -> Result<LoadedMods, String> {
    let key: Option<LoadKey> = paths
        .iter()
        .map(|path| stamp(path).map(|stamp| (path.clone(), stamp)))
        .collect();
    if let Some(key) = &key
        && let Some(loaded) = LOADED.with(|cache| {
            cache
                .borrow()
                .as_ref()
                .filter(|(known, _)| known == key)
                .map(|(_, loaded)| loaded.clone())
        })
    {
        return Ok(loaded);
    }
    let loaded = tankyard_lua::load(paths)?;
    if let Some(key) = key {
        LOADED.with(|cache| *cache.borrow_mut() = Some((key, loaded.clone())));
    }
    Ok(loaded)
}

/// Copy a pack into the library: a `.zip` archive, a pack folder, or a
/// folder's `mod.json`. It replaces an imported pack with the same ID.
pub fn import(source: &Path) -> Result<Entry, String> {
    let source = if source
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("mod.json"))
    {
        source.parent().unwrap_or(source)
    } else {
        source
    };
    let metadata = std::fs::metadata(source).map_err(|e| format!("{}: {e}", source.display()))?;
    if metadata.is_dir() {
        let pack = PackFiles::read(source)?;
        return store_folder(&pack);
    }
    if !source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return Err("Choose a .zip archive, a pack folder or its mod.json.".into());
    }
    let pack = PackFiles::read(source)?;
    let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
    store(&pack, &bytes)
}

/// Import an archive delivered as bytes, such as a file dropped on the window.
pub fn import_bytes(bytes: &[u8]) -> Result<Entry, String> {
    let pack = PackFiles::from_zip(bytes)?;
    store(&pack, bytes)
}

/// Remove an earlier import with the same ID so only one copy remains.
fn clear_existing(directory: &Path, id: &str) -> Result<(), String> {
    let folder = directory.join(id);
    if folder.is_dir() {
        std::fs::remove_dir_all(&folder).map_err(|e| e.to_string())?;
    }
    let archive = directory.join(format!("{id}.zip"));
    if archive.is_file() {
        std::fs::remove_file(&archive).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn store(pack: &PackFiles, bytes: &[u8]) -> Result<Entry, String> {
    let manifest = tankyard_mods::parse_manifest(pack)?;
    let directory = library_dir();
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let destination = directory.join(format!("{}.zip", manifest.id));
    let temporary = directory.join(format!(".{}.zip.partial", manifest.id));
    std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    if let Err(error) = clear_existing(&directory, &manifest.id) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    std::fs::rename(&temporary, &destination).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        e.to_string()
    })?;
    Ok(inspect(&destination))
}

fn store_folder(pack: &PackFiles) -> Result<Entry, String> {
    let manifest = tankyard_mods::parse_manifest(pack)?;
    let directory = library_dir();
    let destination = directory.join(&manifest.id);
    if pack.origin.canonicalize().ok() == destination.canonicalize().ok() {
        // Already in the library.
        return Ok(inspect(&destination));
    }
    let temporary = directory.join(format!(".{}.partial", manifest.id));
    let _ = std::fs::remove_dir_all(&temporary);
    let written = (|| {
        for (name, data) in pack.files() {
            // Names were validated as relative paths inside the pack.
            let path = temporary.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, data)?;
        }
        Ok::<_, std::io::Error>(())
    })();
    if let Err(error) = written.map_err(|e| e.to_string()).and_then(|()| {
        clear_existing(&directory, &manifest.id)?;
        std::fs::rename(&temporary, &destination).map_err(|e| e.to_string())
    }) {
        let _ = std::fs::remove_dir_all(&temporary);
        return Err(error);
    }
    Ok(inspect(&destination))
}

/// Delete an imported pack from the library. Bundled packs cannot be removed.
pub fn remove(entry: &Entry) -> Result<(), String> {
    if !entry.in_library() {
        return Err(format!(
            "{} ships with the game and cannot be removed.",
            entry.name()
        ));
    }
    let path = Path::new(&entry.path);
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    INSPECTED.with(|cache| cache.borrow_mut().remove(path));
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_change_when_a_pack_file_changes() {
        let root = std::env::temp_dir().join(format!("tankyard-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(root.join("mod.json"), "{}").unwrap();
        std::fs::write(root.join("lib/a.lua"), "return 1").unwrap();
        let before = stamp(&root).unwrap();
        assert_eq!(stamp(&root), Some(before));
        std::fs::write(root.join("lib/a.lua"), "return 12").unwrap();
        assert_ne!(stamp(&root), Some(before));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn enabled_packs_without_a_loadable_copy_can_be_disabled() {
        let pack = |path: &str, id: Option<&str>| Entry {
            path: path.into(),
            manifest: id.map(|id| {
                serde_json::from_value(serde_json::json!({
                    "api_version": 1, "id": id, "name": id, "version": "1.0.0"
                }))
                .unwrap()
            }),
            report: ScanReport::default(),
            digest: String::new(),
            legacy_digest: String::new(),
            archive: false,
            error: id.is_none().then(|| "mod.json: missing field".to_owned()),
            project: None,
            project_error: None,
            icon: None,
            staff: None,
            size: 0,
            files: 0,
        };
        let settings = Settings {
            enabled_mods: vec!["vampire".into(), "gone".into(), "broken".into()],
            ..Settings::default()
        };
        let ready = ready_in(&settings, || {
            vec![
                pack("/library/vampire", Some("vampire")),
                pack("/library/broken.zip", None),
            ]
        });
        assert_eq!(ready.paths, [PathBuf::from("/library/vampire")]);
        assert_eq!(ready.missing, ["gone", "broken"]);
        assert_eq!(
            ready.held,
            [
                "gone is enabled but no longer installed.",
                "broken is enabled but cannot load; see Mods.",
            ]
        );
    }

    #[test]
    fn saved_paths_become_pack_ids() {
        let mut settings = Settings {
            enabled_mods: vec![
                "vampire".into(),
                "/somewhere/old/fast_arena".into(),
                "C:\\packs\\bounty_hunt.zip".into(),
                "vampire".into(),
                "/nowhere/Not An Id".into(),
            ],
            ..Settings::default()
        };
        migrate(&mut settings);
        assert_eq!(
            settings.enabled_mods,
            ["vampire", "fast_arena", "bounty_hunt"]
        );
    }
}

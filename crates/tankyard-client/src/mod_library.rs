//! The player's mod library: discovery, `.zip` import, static review and
//! approval. Packs that need review only load after the player confirms the
//! exact contents they reviewed; a changed pack must be reviewed again.
use crate::settings::Settings;
use directories::ProjectDirs;
use std::path::{Path, PathBuf};
use tankyard_mods::{
    Manifest, PackFiles, TextureData,
    project::Project,
    scan::{ScanReport, Verdict, scan_pack},
};

const MAX_LIBRARY: usize = 64;

pub struct Entry {
    pub path: String,
    pub manifest: Option<Manifest>,
    pub report: ScanReport,
    pub digest: String,
    pub archive: bool,
    pub error: Option<String>,
    /// Storefront details from `tankyard.toml`.
    pub project: Option<Project>,
    /// Why `tankyard.toml` or its icon could not be read.
    pub project_error: Option<String>,
    pub icon: Option<TextureData>,
    /// Bytes across every file in the pack.
    pub size: u64,
    pub files: usize,
}

impl Entry {
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
    pub fn approved(&self, settings: &Settings) -> bool {
        !self.report.requires_confirmation() || settings.approved_mods.contains(&self.digest)
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Where imported packs live, beside the player's settings.
pub fn library_dir() -> PathBuf {
    ProjectDirs::from("org", "Tank Yard", "Tank Yard")
        .map(|p| p.data_dir().join("mods"))
        .unwrap_or_else(|| PathBuf::from("mods"))
}

/// Read and review one pack without running any of it.
pub fn inspect(path: &Path) -> Entry {
    let display = path.to_string_lossy().into_owned();
    match tankyard_mods::read_pack(path) {
        Ok((manifest, pack)) => {
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
                archive: pack.archive,
                size: pack.files().map(|(_, data)| data.len() as u64).sum(),
                files: pack.len(),
                manifest: Some(manifest),
                error: None,
                project,
                project_error,
                icon,
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
                archive: path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("zip")),
                error: Some(error),
                project: None,
                project_error: None,
                icon: None,
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

/// Every pack in the library and in the game's bundled `mods` folders.
pub fn discover() -> Vec<Entry> {
    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for directory in std::iter::once(library_dir()).chain(bundled_dirs()) {
        // The same folder can be reached two ways, e.g. cwd beside the exe.
        if !std::fs::canonicalize(&directory).is_ok_and(|real| seen.insert(real)) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut found: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                (path.is_dir() && path.join("mod.json").is_file())
                    || path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
            })
            .collect();
        found.sort();
        paths.extend(found);
    }
    paths.dedup();
    paths.truncate(MAX_LIBRARY);
    paths.iter().map(|path| inspect(path)).collect()
}

/// Copy a reviewed archive into the library as `<id>.zip`.
pub fn import(source: &Path) -> Result<Entry, String> {
    if !source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return Err("Mod packs are imported as .zip archives.".into());
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

fn store(pack: &PackFiles, bytes: &[u8]) -> Result<Entry, String> {
    let manifest = tankyard_mods::parse_manifest(pack)?;
    let directory = library_dir();
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let destination = directory.join(format!("{}.zip", manifest.id));
    let temporary = directory.join(format!(".{}.zip.partial", manifest.id));
    std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, &destination).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        e.to_string()
    })?;
    Ok(inspect(&destination))
}

/// Enabled packs that may load now. Packs whose reviewed contents changed
/// or were never confirmed are held back with a reason.
pub fn ready_paths(settings: &Settings) -> (Vec<PathBuf>, Vec<String>) {
    let mut ready = Vec::new();
    let mut held = Vec::new();
    for path in &settings.enabled_mods {
        let entry = inspect(Path::new(path));
        if let Some(error) = &entry.error {
            held.push(format!("{}: {error}", entry.name()));
        } else if !entry.approved(settings) {
            held.push(format!(
                "{} changed or was never reviewed; confirm it in Mods before playing with it.",
                entry.name()
            ));
        } else {
            ready.push(PathBuf::from(path));
        }
    }
    (ready, held)
}

//! Bounded mod pack sources. A pack is a directory or a `.zip` archive; both
//! become the same in-memory file map after names, sizes, entry kinds and
//! totals are validated. Nothing is extracted to disk and nothing is executed.
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const MAX_PACK_FILES: usize = 256;
pub const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;
/// Compressed archive size accepted for import.
pub const MAX_ARCHIVE_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 1024;
const MAX_DEPTH: usize = 8;
const MAX_NAME_BYTES: usize = 200;

#[derive(Clone, Debug, Default)]
pub struct PackFiles {
    /// Where the pack was read from; used for display and approval records.
    pub origin: PathBuf,
    pub archive: bool,
    files: BTreeMap<String, Vec<u8>>,
}

impl PackFiles {
    /// Read a directory or a `.zip` file into memory.
    pub fn read(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if metadata.is_dir() {
            return Self::read_dir(path);
        }
        if metadata.len() > MAX_ARCHIVE_BYTES as u64 {
            return Err("mod archive exceeds 8 MiB".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .take(MAX_ARCHIVE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let mut pack = Self::from_zip(&bytes)?;
        pack.origin = path.to_path_buf();
        Ok(pack)
    }

    /// Validate an archive held in memory, for example a file dropped onto the
    /// window. Encrypted entries, links, unsafe names and oversized or
    /// misreported contents are rejected before any byte is used.
    pub fn from_zip(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_ARCHIVE_BYTES {
            return Err("mod archive exceeds 8 MiB".into());
        }
        if !bytes.starts_with(b"PK\x03\x04") && !bytes.starts_with(b"PK\x05\x06") {
            return Err("mod packs must be .zip archives or folders".into());
        }
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("invalid zip archive: {e}"))?;
        if archive.len() > MAX_ENTRIES {
            return Err("mod archive has too many entries".into());
        }
        let mut pack = Self {
            archive: true,
            ..Default::default()
        };
        let mut total = 0_usize;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| format!("unreadable or encrypted zip entry: {e}"))?;
            let raw = entry
                .name()
                .map_err(|e| format!("unreadable zip entry name: {e}"))?
                .into_owned();
            if entry.is_symlink() {
                return Err(format!("{raw}: links are not allowed in mod packs"));
            }
            if entry.is_dir() || junk(&raw) {
                continue;
            }
            // Reject what extractors might "repair": absolute names, drive
            // prefixes and parent references are never legitimate here.
            let absolute = raw.starts_with(['/', '\\']) || raw.as_bytes().get(1) == Some(&b':');
            let parent = raw.split(['/', '\\']).any(|part| part == "..");
            let Some(path) = entry.enclosed_name().filter(|_| !absolute && !parent) else {
                return Err(format!("{raw}: unsafe path in archive"));
            };
            let name = normalize(&path, &raw)?;
            if entry.size() > MAX_FILE_BYTES as u64 {
                return Err(format!("{name}: file exceeds 2 MiB"));
            }
            let mut data = Vec::with_capacity(entry.size().min(MAX_FILE_BYTES as u64) as usize);
            (&mut entry)
                .take(MAX_FILE_BYTES as u64 + 1)
                .read_to_end(&mut data)
                .map_err(|e| format!("{name}: {e}"))?;
            if data.len() > MAX_FILE_BYTES || data.len() as u64 != entry.size() {
                return Err(format!("{name}: archive size header does not match"));
            }
            total += data.len();
            if total > MAX_TOTAL_BYTES {
                return Err("mod pack contents exceed 8 MiB".into());
            }
            pack.insert(name, data)?;
        }
        pack.strip_wrapper_folder();
        Ok(pack)
    }

    fn read_dir(root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("{}: {e}", root.display()))?;
        let mut pack = Self {
            origin: root.clone(),
            ..Default::default()
        };
        let mut total = 0;
        let mut pending = vec![(root.clone(), 0_usize)];
        let mut entries = 0_usize;
        while let Some((directory, depth)) = pending.pop() {
            let mut children: Vec<_> = std::fs::read_dir(&directory)
                .map_err(|e| format!("{}: {e}", directory.display()))?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect();
            children.sort();
            for path in children {
                entries += 1;
                if entries > MAX_ENTRIES {
                    return Err("mod folder has too many entries".into());
                }
                let relative = path
                    .strip_prefix(&root)
                    .map_err(|_| "mod file escaped its folder".to_string())?
                    .to_path_buf();
                let raw = relative.to_string_lossy().replace('\\', "/");
                if junk(&raw) {
                    continue;
                }
                // Links may point anywhere; only files physically inside the
                // pack folder are read.
                let real = path.canonicalize().map_err(|e| format!("{raw}: {e}"))?;
                if !real.starts_with(&root) {
                    return Err(format!("{raw}: symlink cannot leave the mod directory"));
                }
                let metadata = std::fs::metadata(&real).map_err(|e| format!("{raw}: {e}"))?;
                if metadata.is_dir() {
                    if depth + 1 >= MAX_DEPTH {
                        return Err(format!("{raw}: folders are nested too deeply"));
                    }
                    pending.push((path, depth + 1));
                    continue;
                }
                let name = normalize(&relative, &raw)?;
                let data = crate::read_bounded_limit(&real, MAX_FILE_BYTES)?;
                total += data.len();
                if total > MAX_TOTAL_BYTES {
                    return Err("mod folder contents exceed 8 MiB".into());
                }
                pack.insert(name, data)?;
            }
        }
        Ok(pack)
    }

    /// Construct a pack directly, mainly for tests and tools.
    pub fn from_files(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Result<Self, String> {
        let mut pack = Self::default();
        let mut total = 0;
        for (name, data) in files {
            let name = normalize(Path::new(&name), &name)?;
            total += data.len();
            if data.len() > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES {
                return Err("mod pack contents exceed size limits".into());
            }
            pack.insert(name, data)?;
        }
        Ok(pack)
    }

    fn insert(&mut self, name: String, data: Vec<u8>) -> Result<(), String> {
        if self.files.len() >= MAX_PACK_FILES {
            return Err("mod pack has more than 256 files".into());
        }
        // Windows and macOS treat these as the same file.
        let folded = name.to_lowercase();
        if self
            .files
            .keys()
            .any(|existing| existing.to_lowercase() == folded)
        {
            return Err(format!("{name}: duplicate file name"));
        }
        self.files.insert(name, data);
        Ok(())
    }

    /// Many archivers wrap a folder around the pack. Accept exactly one such
    /// wrapper when it holds the manifest.
    fn strip_wrapper_folder(&mut self) {
        if self.files.contains_key("mod.json") {
            return;
        }
        let tops: HashSet<_> = self
            .files
            .keys()
            .filter_map(|name| name.split_once('/').map(|(top, _)| top.to_owned()))
            .collect();
        let all_nested = self.files.keys().all(|name| name.contains('/'));
        if tops.len() != 1 || !all_nested {
            return;
        }
        let top = tops.into_iter().next().unwrap();
        if !self.files.contains_key(&format!("{top}/mod.json")) {
            return;
        }
        let prefix = format!("{top}/");
        self.files = std::mem::take(&mut self.files)
            .into_iter()
            .map(|(name, data)| (name[prefix.len()..].to_owned(), data))
            .collect();
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(Vec::as_slice)
    }
    pub fn contains(&self, name: &str) -> bool {
        self.files.contains_key(name)
    }
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_slice()))
    }
    pub fn len(&self) -> usize {
        self.files.len()
    }
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
    /// Marks digests from `content_digest`.
    pub const SHA256_PREFIX: &'static str = "sha256:";
    /// SHA-256 over every file's name and bytes. Approval of a scanned pack
    /// is bound to this value; any later change requires a new review.
    pub fn content_digest(&self) -> String {
        let mut context = ring::digest::Context::new(&ring::digest::SHA256);
        for (name, data) in &self.files {
            // Lengths keep name and content boundaries unambiguous.
            context.update(&(name.len() as u64).to_le_bytes());
            context.update(name.as_bytes());
            context.update(&(data.len() as u64).to_le_bytes());
            context.update(data);
        }
        let mut digest = String::from(Self::SHA256_PREFIX);
        for byte in context.finish().as_ref() {
            digest.push_str(&format!("{byte:02x}"));
        }
        digest
    }
    /// SHA-256 over every file except `skip`, in the same form as
    /// `content_digest`, for signatures stored inside the pack they cover.
    pub fn digest_without(&self, skip: &str) -> [u8; 32] {
        let mut context = ring::digest::Context::new(&ring::digest::SHA256);
        for (name, data) in self.files.iter().filter(|(name, _)| name.as_str() != skip) {
            context.update(&(name.len() as u64).to_le_bytes());
            context.update(name.as_bytes());
            context.update(&(data.len() as u64).to_le_bytes());
            context.update(data);
        }
        context
            .finish()
            .as_ref()
            .try_into()
            .expect("SHA-256 digests are 32 bytes")
    }
    /// The checksum approvals were bound to before 0.12. Unlike SHA-256, two
    /// packs can be made to share it, so it only carries old approvals over.
    pub fn legacy_content_digest(&self) -> String {
        let mut state = 0xcbf2_9ce4_8422_2325_u64;
        let mut second = 0x9e37_79b9_7f4a_7c15_u64;
        for (name, data) in &self.files {
            for &byte in name.as_bytes().iter().chain([0_u8].iter()).chain(data) {
                state = (state ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
                second = (second ^ u64::from(byte))
                    .rotate_left(5)
                    .wrapping_mul(0x5851_f42d_4c95_7f2d);
            }
        }
        format!("{state:016x}{second:016x}")
    }
}

/// Operating-system metadata that archivers add on their own.
fn junk(name: &str) -> bool {
    let name = name.trim_start_matches("./");
    name.starts_with("__MACOSX/")
        || name.starts_with(".git/")
        || name.contains("/.git/")
        || name.split('/').any(|part| {
            matches!(
                part.to_ascii_lowercase().as_str(),
                ".ds_store" | "thumbs.db" | "desktop.ini"
            ) || part.starts_with("._")
        })
}

fn normalize(path: &Path, raw: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| format!("{raw}: file names must be UTF-8"))?;
                if part.is_empty()
                    || part
                        .chars()
                        .any(|c| c.is_control() || matches!(c, '\\' | ':'))
                    || part.ends_with('.')
                    || part.ends_with(' ')
                    || windows_reserved(part)
                {
                    return Err(format!("{raw}: unsafe file name"));
                }
                parts.push(part);
            }
            Component::CurDir => {}
            _ => return Err(format!("{raw}: unsafe path")),
        }
    }
    if parts.is_empty() || parts.len() > MAX_DEPTH {
        return Err(format!("{raw}: unsafe path depth"));
    }
    let name = parts.join("/");
    if name.len() > MAX_NAME_BYTES {
        return Err(format!("{raw}: file name is too long"));
    }
    Ok(name)
}

fn windows_reserved(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit())
}

#[cfg(test)]
pub(crate) fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }
    cursor.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_digests_change_with_any_byte_and_keep_file_boundaries() {
        let pack = |files: &[(&str, &str)]| {
            PackFiles::from_files(
                files
                    .iter()
                    .map(|(name, data)| ((*name).to_owned(), data.as_bytes().to_vec())),
            )
            .unwrap()
        };
        let base = pack(&[("a.lua", "print(1)"), ("b.lua", "print(2)")]);
        let digest = base.content_digest();
        assert!(digest.starts_with(PackFiles::SHA256_PREFIX));
        assert_eq!(digest.len(), PackFiles::SHA256_PREFIX.len() + 64);
        assert_eq!(digest, base.clone().content_digest());
        for other in [
            pack(&[("a.lua", "print(2)"), ("b.lua", "print(1)")]),
            pack(&[("a.lua", "print(1)"), ("b.lua", "print(3)")]),
            pack(&[("a.lu", "aprint(1)"), ("b.lua", "print(2)")]),
        ] {
            assert_ne!(other.content_digest(), digest);
        }
        // Old approvals are recognized by the unchanged legacy checksum.
        assert_eq!(base.legacy_content_digest().len(), 32);
        assert!(
            !base
                .legacy_content_digest()
                .starts_with(PackFiles::SHA256_PREFIX)
        );
    }
    #[test]
    fn archives_strip_one_wrapper_folder_and_ignore_os_metadata() {
        let bytes = zip_bytes(&[
            ("pack/mod.json", b"{}"),
            ("pack/scripts/main.lua", b"print('hi')"),
            ("__MACOSX/pack/._mod.json", b"\x00\x05\x16\x07binary"),
            ("pack/.DS_Store", b"\x00\x00\x00\x01Bud1"),
        ]);
        let pack = PackFiles::from_zip(&bytes).unwrap();
        assert!(pack.contains("mod.json"));
        assert!(pack.contains("scripts/main.lua"));
        assert_eq!(pack.len(), 2);
        assert_eq!(
            pack.content_digest(),
            PackFiles::from_zip(&bytes).unwrap().content_digest()
        );
    }

    #[test]
    fn traversal_reserved_names_duplicates_and_non_archives_are_rejected() {
        for name in [
            "../escape.lua",
            "/abs.lua",
            "a/../../b.lua",
            "con.lua",
            "x:y.lua",
            "C:/x.lua",
        ] {
            let bytes = zip_bytes(&[("mod.json", b"{}"), (name, b"x")]);
            assert!(PackFiles::from_zip(&bytes).is_err(), "{name}");
        }
        // Windows PowerShell archives use backslash separators.
        let bytes = zip_bytes(&[("mod.json", b"{}"), ("scripts\\main.lua", b"x")]);
        assert!(
            PackFiles::from_zip(&bytes)
                .unwrap()
                .contains("scripts/main.lua")
        );
        let bytes = zip_bytes(&[("mod.json", b"{}"), ("A.lua", b"1"), ("a.lua", b"2")]);
        assert!(
            PackFiles::from_zip(&bytes)
                .unwrap_err()
                .contains("duplicate")
        );
        assert!(PackFiles::from_zip(b"not a zip").is_err());
    }

    #[test]
    fn decompression_is_bounded_per_file_and_in_total() {
        let large = vec![b'a'; MAX_FILE_BYTES + 1];
        let bytes = zip_bytes(&[("mod.json", b"{}"), ("big.txt", &large)]);
        assert!(bytes.len() < 64 * 1024, "highly compressible bomb");
        assert!(PackFiles::from_zip(&bytes).unwrap_err().contains("2 MiB"));
        let chunk = vec![b'b'; MAX_FILE_BYTES - 16];
        let names: Vec<String> = (0..5).map(|n| format!("part{n}.txt")).collect();
        let mut files: Vec<(&str, &[u8])> = vec![("mod.json", b"{}")];
        for name in &names {
            files.push((name, &chunk));
        }
        let bytes = zip_bytes(&files);
        assert!(PackFiles::from_zip(&bytes).unwrap_err().contains("8 MiB"));
    }
}

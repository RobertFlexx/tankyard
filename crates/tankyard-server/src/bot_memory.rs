//! Keeps what bots learn between restarts: which tanks earn the most, which
//! beat which, and the networks they pool. One memory serves every arena.
use std::path::Path;
use std::sync::Mutex;
pub use tankyard_ai::SharedKnowledge;
use tankyard_ai::{Knowledge, lock_knowledge};
use tankyard_content::Content;

/// How often a running server writes what bots learned.
pub const SAVE_SECONDS: u64 = 300;

/// The memory saved at `path`, or a fresh one when there is none yet or it
/// can't be used. A damaged file is reported and left for the next save to
/// replace.
pub fn load(path: &Path, content: &Content) -> SharedKnowledge {
    if path.as_os_str().is_empty() {
        return Knowledge::default().shared();
    }
    let read = || -> std::io::Result<String> {
        // Preserve the normal missing-file path, then use the shared bounded
        // reader, which rejects FIFOs before an open can stall startup.
        std::fs::metadata(path)?;
        let bytes = tankyard_mods::read_bounded_limit(path, tankyard_ai::KNOWLEDGE_FILE_LIMIT)
            .map_err(std::io::Error::other)?;
        String::from_utf8(bytes).map_err(std::io::Error::other)
    };
    match read() {
        Ok(text) => match Knowledge::from_json(&text, content) {
            Ok(knowledge) => {
                log::info!(
                    "Bots remember {} tries of their tanks from {}.",
                    knowledge.lives(),
                    path.display()
                );
                knowledge.shared()
            }
            Err(error) => {
                log::warn!("Bots start afresh: {} ({error}).", path.display());
                Knowledge::default().shared()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Knowledge::default().shared(),
        Err(error) => {
            log::warn!(
                "Bots start afresh: can't read {} ({error}).",
                path.display()
            );
            Knowledge::default().shared()
        }
    }
}

/// Saves happen one at a time, so an older snapshot can never land on top
/// of a newer one when a game and its hosted server both save.
static SAVING: Mutex<()> = Mutex::new(());

/// Writes the memory if bots learned anything since it was last written.
/// Returns whether it wrote.
pub fn save(path: &Path, memory: &SharedKnowledge) -> std::io::Result<bool> {
    if path.as_os_str().is_empty() {
        return Ok(false);
    }
    let _saving = SAVING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (text, changes) = {
        let knowledge = lock_knowledge(memory);
        if knowledge.unsaved() == 0 {
            return Ok(false);
        }
        (knowledge.to_json(), knowledge.unsaved())
    };
    crate::storage::atomic_write(path, text.as_bytes())?;
    lock_knowledge(memory).mark_saved(changes);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::{Mode, World, WorldConfig};

    #[cfg(unix)]
    #[test]
    fn special_memory_files_do_not_stall_startup() {
        use std::{sync::mpsc, time::Duration};
        let root = std::env::temp_dir().join(format!(
            "tankyard-memory-pipe-{}-{}",
            std::process::id(),
            renet_netcode::generate_random_bytes::<8>()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("memory.json");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let memory = load(&path, &Content::builtin());
            tx.send(lock_knowledge(&memory).lives()).unwrap();
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2))
                .expect("memory load must not wait for a writer"),
            0
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn memories_round_trip_and_damaged_files_start_fresh() {
        let dir = std::env::temp_dir().join(format!(
            "tankyard-bot-memory-{}",
            renet_netcode::generate_random_bytes::<8>()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
        let path = dir.join("bot-memory.json");
        let content = Content::builtin();
        let memory = load(&path, &content);
        assert_eq!(lock_knowledge(&memory).lives(), 0);
        assert!(!save(&path, &memory).unwrap(), "nothing learned yet");
        // Play a short life so there is something to keep.
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Ffa,
                ..Default::default()
            },
            content.clone(),
        );
        world.add_player("Bot", true);
        lock_knowledge(&memory).observe(&world);
        world.tick += 3600;
        world.tanks[0].score += 500;
        lock_knowledge(&memory).observe(&world);
        world.tanks[0].deaths += 1;
        world.tick += 1;
        lock_knowledge(&memory).observe(&world);
        assert!(save(&path, &memory).unwrap());
        assert!(!save(&path, &memory).unwrap(), "already saved");
        let again = load(&path, &content);
        assert_eq!(lock_knowledge(&again).lives(), 1);
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(lock_knowledge(&load(&path, &content)).lives(), 0);
        assert_eq!(lock_knowledge(&load(Path::new(""), &content)).lives(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}

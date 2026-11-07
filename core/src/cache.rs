use crate::memory_cards::SaveInfo;
use crate::native_patches::Group;
use std::{
    collections::HashMap,
    fs,
    hash::Hash,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, OnceLock},
    time::UNIX_EPOCH,
};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FileStamp {
    pub size: u64,
    pub modified: u128,
}

impl FileStamp {
    pub fn of(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
        Some(Self {
            size: meta.len(),
            modified,
        })
    }

    pub fn tag(&self) -> String {
        format!("\"{:x}-{:x}\"", self.size, self.modified)
    }
}

pub struct Memo<K, S, V> {
    map: Mutex<HashMap<K, (S, V)>>,
    limit: usize,
}

impl<K: Eq + Hash, S: PartialEq, V: Clone> Memo<K, S, V> {
    pub fn new(limit: usize) -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            limit,
        }
    }

    pub fn get(&self, key: &K, stamp: &S) -> Option<V> {
        let map = self.map.lock().ok()?;
        map.get(key)
            .filter(|(known, _)| known == stamp)
            .map(|(_, value)| value.clone())
    }

    pub fn put(&self, key: K, stamp: S, value: V) {
        if let Ok(mut map) = self.map.lock() {
            if map.len() >= self.limit {
                map.clear();
            }
            map.insert(key, (stamp, value));
        }
    }

    pub fn clear(&self) {
        if let Ok(mut map) = self.map.lock() {
            map.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.map.lock().map(|map| map.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone)]
pub struct CardSnapshot {
    pub saves: Vec<SaveInfo>,
    pub free: Option<u64>,
    pub revision: String,
}

pub type DirEntryStamp = Vec<(String, bool, u64, u128)>;

pub static BIOS_HASHES: LazyLock<Memo<PathBuf, FileStamp, String>> =
    LazyLock::new(|| Memo::new(128));
pub static CARD_FILES: LazyLock<Memo<PathBuf, FileStamp, CardSnapshot>> =
    LazyLock::new(|| Memo::new(64));
pub static DIR_ENTRIES: LazyLock<Memo<PathBuf, DirEntryStamp, (u64, Vec<String>)>> =
    LazyLock::new(|| Memo::new(2048));
pub static SAVE_FOLDERS: LazyLock<Memo<PathBuf, (u64, usize, Option<FileStamp>), SaveInfo>> =
    LazyLock::new(|| Memo::new(2048));
pub static ZIP_INDEX: LazyLock<Memo<PathBuf, FileStamp, Arc<Vec<String>>>> =
    LazyLock::new(|| Memo::new(8));
pub static ZIP_TEXT: LazyLock<Memo<(PathBuf, String), FileStamp, Arc<String>>> =
    LazyLock::new(|| Memo::new(64));
pub static LOCAL_GROUPS: LazyLock<Memo<(PathBuf, String), FileStamp, Vec<Group>>> =
    LazyLock::new(|| Memo::new(256));
pub static EXE_VERSIONS: LazyLock<
    Memo<PathBuf, (Option<FileStamp>, Option<FileStamp>), String>,
> = LazyLock::new(|| Memo::new(16));
pub static USER_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn bios_sha1(path: &Path) -> Option<String> {
    let stamp = FileStamp::of(path);
    let key = path.to_path_buf();
    if let Some(stamp) = &stamp {
        if let Some(hash) = BIOS_HASHES.get(&key, stamp) {
            return Some(hash);
        }
    }
    let bytes = fs::read(path).ok()?;
    let hash = crate::pcsx2::compute_sha1(&bytes);
    if let Some(stamp) = stamp {
        BIOS_HASHES.put(key, stamp, hash.clone());
    }
    Some(hash)
}

pub fn clear_all() {
    BIOS_HASHES.clear();
    CARD_FILES.clear();
    DIR_ENTRIES.clear();
    SAVE_FOLDERS.clear();
    ZIP_INDEX.clear();
    ZIP_TEXT.clear();
    LOCAL_GROUPS.clear();
    EXE_VERSIONS.clear();
}

pub fn warmup(db_path: &Path) {
    let Ok(conn) = rusqlite::Connection::open(db_path) else {
        return;
    };
    let _ = crate::game_metadata::find_compatibility("", None);
    let Ok(installation) = crate::pcsx2::current_installation(&conn) else {
        return;
    };
    let _ = crate::pcsx2::installation_id(&conn);
    let active = crate::pcsx2::active_bios(&conn).ok().flatten();
    let _ = crate::pcsx2::scan_bios_directory(Path::new(&installation.bios_dir), active.as_deref());
    crate::native_patches::warm_index(&conn);
    warm_cards(&conn, db_path);
}

pub fn warm_cards(conn: &rusqlite::Connection, db_path: &Path) {
    let Ok(summary) = crate::memory_cards::list_for(conn, db_path, None, true) else {
        return;
    };
    for card in summary.cards.iter().filter(|card| !card.slots.is_empty()) {
        let _ = crate::memory_cards::list_for(conn, db_path, Some(&card.name), false);
    }
}

pub fn prefetch_game(db_path: &Path, game_id: &str, crc: &str) {
    let Ok(conn) = rusqlite::Connection::open(db_path) else {
        return;
    };
    let _ = crate::resource_settings::state(
        &conn,
        &crate::resource_settings::GameQuery {
            game_id: game_id.to_string(),
            crc: crc.to_string(),
        },
    );
    warm_cards(&conn, db_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memo_returns_value_only_for_matching_stamp() {
        let memo: Memo<String, u32, String> = Memo::new(4);
        memo.put("a".into(), 1, "one".into());
        assert_eq!(memo.get(&"a".to_string(), &1), Some("one".into()));
        assert_eq!(memo.get(&"a".to_string(), &2), None);
    }

    #[test]
    fn memo_evicts_when_full_and_clears() {
        let memo: Memo<u32, u32, u32> = Memo::new(2);
        memo.put(1, 1, 1);
        memo.put(2, 1, 2);
        memo.put(3, 1, 3);
        assert_eq!(memo.len(), 1);
        memo.clear();
        assert!(memo.is_empty());
    }

    #[test]
    fn file_stamp_changes_when_file_changes() {
        let dir = std::env::temp_dir().join(format!("rm-cache-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.bin");
        fs::write(&file, b"one").unwrap();
        let first = FileStamp::of(&file).unwrap();
        fs::write(&file, b"three").unwrap();
        let second = FileStamp::of(&file).unwrap();
        assert_ne!(first, second);
        assert!(FileStamp::of(&dir.join("missing")).is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bios_hash_is_reused_until_file_changes() {
        let dir = std::env::temp_dir().join(format!("rm-bios-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("b.bin");
        fs::write(&file, b"abc").unwrap();
        let first = bios_sha1(&file).unwrap();
        assert_eq!(first, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(bios_sha1(&file).unwrap(), first);
        fs::write(&file, b"abcd").unwrap();
        assert_ne!(bios_sha1(&file).unwrap(), first);
        fs::remove_dir_all(dir).unwrap();
    }
}

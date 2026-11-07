use crate::pcsx2;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant, UNIX_EPOCH},
};

pub static EDIT_LOCK: Mutex<()> = Mutex::new(());
const MAX_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FOLDER_BYTES: u64 = 1024 * 1024 * 1024;
pub fn digest(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(data))
}
const MAGIC: &[u8] = b"Sony PS2 Memory Card Format ";
pub type McResult<T> = Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn name(value: &str) -> McResult<&str> {
    if value.is_empty()
        || value.len() > 100
        || value.starts_with('.')
        || value.ends_with(['.', ' '])
        || value
            .chars()
            .any(|c| c.is_control() || r#"<>:"/\|?*"#.contains(c))
    {
        return Err("Nome de arquivo inválido".into());
    }
    let stem = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
    {
        return Err("Nome reservado pelo Windows".into());
    }
    Ok(value)
}
pub fn plain_meta(m: &fs::Metadata) -> McResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("Links e pontos de redirecionamento não são aceitos".into());
        }
    }
    if m.file_type().is_symlink() {
        return Err("Links não são aceitos".into());
    }
    Ok(())
}
pub fn plain(path: &Path) -> McResult<()> {
    let m = fs::symlink_metadata(path).map_err(err)?;
    plain_meta(&m)
}
fn read_checked_file(path: &Path, len: u64) -> McResult<Vec<u8>> {
    if len > MAX_BYTES {
        return Err("Arquivo acima do limite de 128 MB".into());
    }
    let mut data = Vec::with_capacity(len as usize);
    fs::File::open(path)
        .map_err(err)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(err)?;
    if data.len() as u64 > MAX_BYTES {
        return Err("Arquivo acima do limite".into());
    }
    Ok(data)
}
pub fn read_bounded(path: &Path) -> McResult<Vec<u8>> {
    let m = fs::symlink_metadata(path).map_err(err)?;
    plain_meta(&m)?;
    read_checked_file(path, m.len())
}
fn write_new_file(target: &Path, data: &[u8], sync: bool) -> McResult<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(err)?;
    f.write_all(data).map_err(err)?;
    if sync {
        f.sync_all().map_err(err)?;
    }
    Ok(())
}
pub fn copy_tree(source: &Path, target: &Path) -> McResult<()> {
    let m = fs::symlink_metadata(source).map_err(err)?;
    plain_meta(&m)?;
    if m.is_file() {
        let data = read_checked_file(source, m.len())?;
        write_new_file(target, &data, true)?;
    } else {
        fs::create_dir(target).map_err(err)?;
        let mut total = 0u64;
        for entry in walkdir::WalkDir::new(source)
            .follow_links(false)
            .min_depth(1)
        {
            let e = entry.map_err(err)?;
            let em = e.metadata().map_err(err)?;
            plain_meta(&em)?;
            let relative = e.path().strip_prefix(source).map_err(err)?;
            let dst = target.join(relative);
            if em.is_dir() {
                fs::create_dir(&dst).map_err(err)?;
            } else {
                total += em.len();
                if total > MAX_FOLDER_BYTES {
                    return Err("Cartão acima do limite de 1 GB".into());
                }
                let data = read_checked_file(e.path(), em.len())?;
                write_new_file(&dst, &data, false)?;
            }
        }
    }
    Ok(())
}
pub fn publish_new(source: &Path, target: &Path) -> McResult<()> {
    if target.exists() {
        return Err("O destino já existe".into());
    }
    let sibling = target.with_file_name(format!(".rommanager-new-{}", stamp()));
    let result = (|| {
        copy_tree(source, &sibling)?;
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn MoveFileW(existing: *const u16, new: *const u16) -> i32;
            }
            let from: Vec<u16> = sibling.as_os_str().encode_wide().chain([0]).collect();
            let to: Vec<u16> = target.as_os_str().encode_wide().chain([0]).collect();
            if unsafe { MoveFileW(from.as_ptr(), to.as_ptr()) } == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(windows))]
        {
            if target.exists() {
                return Err("O destino já existe".into());
            }
            fs::rename(&sibling, target).map_err(err)?;
        }
        Ok(())
    })();
    if result.is_err() && sibling.exists() {
        if sibling.is_dir() {
            let _ = fs::remove_dir_all(&sibling);
        } else {
            let _ = fs::remove_file(&sibling);
        }
    }
    result
}
fn subdir_entries(sub: &Path) -> McResult<(u64, Vec<String>)> {
    let mut stamp = Vec::new();
    let mut raw_entries = Vec::new();
    for entry in walkdir::WalkDir::new(sub).min_depth(1).follow_links(false) {
        let entry = entry.map_err(err)?;
        let m = entry.metadata().map_err(err)?;
        plain_meta(&m)?;
        let rel = entry
            .path()
            .strip_prefix(sub)
            .map_err(err)?
            .to_string_lossy()
            .into_owned();
        let modified = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        stamp.push((rel.clone(), m.is_dir(), m.len(), modified));
        raw_entries.push((rel, entry.into_path(), m.is_dir(), m.len()));
    }
    stamp.sort_by(|a, b| a.0.cmp(&b.0));
    let key = sub.to_path_buf();
    if let Some(cached) = crate::cache::DIR_ENTRIES.get(&key, &stamp) {
        return Ok(cached);
    }
    let mut total = 0u64;
    let mut lines = Vec::with_capacity(raw_entries.len());
    for (rel, full_path, is_dir, len) in raw_entries {
        if is_dir {
            lines.push(format!("{rel}:dir"));
        } else {
            total += len;
            if total > MAX_FOLDER_BYTES {
                return Err("Cartão acima do limite".into());
            }
            let b = read_checked_file(&full_path, len)?;
            lines.push(format!("{rel}:{}", digest(&b)));
        }
    }
    let value = (total, lines);
    crate::cache::DIR_ENTRIES.put(key, stamp, value.clone());
    Ok(value)
}
pub fn fingerprint(path: &Path) -> McResult<String> {
    if path.is_file() {
        let stamp = crate::cache::FileStamp::of(path);
        let key = path.to_path_buf();
        if let Some(stamp) = &stamp {
            if let Some(snapshot) = crate::cache::CARD_FILES.get(&key, stamp) {
                return Ok(snapshot.revision);
            }
        }
        return Ok(digest(&read_bounded(path)?));
    }
    plain(path)?;
    let mut entries = Vec::new();
    let mut total = 0u64;
    for item in fs::read_dir(path).map_err(err)? {
        let item = item.map_err(err)?;
        let m = item.metadata().map_err(err)?;
        plain_meta(&m)?;
        let item_name = item.file_name().to_string_lossy().into_owned();
        if item_name.starts_with('.') {
            continue;
        }
        let item_path = item.path();
        if m.is_dir() {
            entries.push(format!("{item_name}:dir"));
            let (sub_total, sub_lines) = subdir_entries(&item_path)?;
            total += sub_total;
            if total > MAX_FOLDER_BYTES {
                return Err("Cartão acima do limite".into());
            }
            for line in sub_lines {
                entries.push(format!(
                    "{}",
                    Path::new(&item_name).join(line).to_string_lossy()
                ));
            }
        } else if m.is_file() {
            total += m.len();
            if total > MAX_FOLDER_BYTES {
                return Err("Cartão acima do limite".into());
            }
            let b = read_checked_file(&item_path, m.len())?;
            entries.push(format!("{item_name}:{}", digest(&b)));
        }
    }
    entries.sort();
    Ok(digest(entries.join("\n").as_bytes()))
}
pub fn data_dir(conn: &Connection) -> McResult<PathBuf> {
    Ok(PathBuf::from(pcsx2::current_installation(conn)?.data_dir))
}

pub fn ini_value(content: &str, section: &str, key: &str) -> Option<String> {
    let mut current = "";
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            current = &line[1..line.len() - 1];
        } else if current.eq_ignore_ascii_case(section) {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    return Some(v.trim().trim_matches('"').to_string());
                }
            }
        }
    }
    None
}
pub fn cards_dir(conn: &Connection) -> McResult<PathBuf> {
    let data = data_dir(conn)?;
    let ini = fs::read_to_string(data.join("inis/PCSX2.ini")).unwrap_or_default();
    let p = ini_value(&ini, "Folders", "MemoryCards")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("memcards"));
    Ok(if p.is_absolute() { p } else { data.join(p) })
}
pub fn writable() -> McResult<()> {
    if pcsx2::is_pcsx2_running() {
        Err("Feche o PCSX2 antes de alterar cartões ou recursos.".into())
    } else {
        Ok(())
    }
}
pub fn store(db: &Path) -> PathBuf {
    db.parent().unwrap_or(Path::new(".")).join("a4")
}
pub fn stamp() -> String {
    format!(
        "{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    )
}
pub fn tool_path(db: &Path) -> PathBuf {
    store(db).join("tools/myMCpp.exe")
}
pub fn run_tool(
    db: &Path,
    card: &Path,
    command: &str,
    args: &[String],
    cwd: &Path,
) -> McResult<()> {
    let tool = tool_path(db);
    if !tool.is_file() {
        return Err("Prepare a ferramenta de cartões primeiro.".into());
    }
    let mut cmd = Command::new(tool);
    cmd.arg(card)
        .arg(command)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(err)?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(err)? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "A ferramenta rejeitou a operação {}. O cartão original foi preservado.",
                    command
                ))
            };
        }
        if start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Tempo limite da ferramenta; original preservado.".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
pub fn save_serial(name: &str) -> Option<String> {
    static SERIAL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let regex = SERIAL.get_or_init(|| regex::Regex::new(r"(?i)([A-Z]{4})[-_]?([0-9]{5})").unwrap());
    let c = regex.captures(name)?;
    Some(format!("{}-{}", c[1].to_ascii_uppercase(), &c[2]))
}
pub fn icon_title(bytes: &[u8]) -> Option<String> {
    if !bytes.starts_with(b"PS2D") {
        return None;
    }
    let title = bytes.get(192..260)?;
    let end = title.iter().position(|b| *b == 0).unwrap_or(title.len());
    let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(&title[..end]);
    let title = decoded
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    if title.is_empty() { None } else { Some(title) }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SaveInfo {
    pub name: String,
    pub title: String,
    pub serial: Option<String>,
    pub size: u64,
    pub files: usize,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CardInfo {
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub revision: String,
    pub saves: Vec<SaveInfo>,
    pub save_count: Option<usize>,
    pub free_bytes: Option<u64>,
    pub error: Option<String>,
    pub slots: Vec<u8>,
}
#[derive(Serialize)]
pub struct CardState {
    pub directory: String,
    pub cards: Vec<CardInfo>,
    pub running: bool,
    pub tool_ready: bool,
    pub installation_id: String,
}
fn u16le(b: &[u8], p: usize) -> McResult<u16> {
    Ok(u16::from_le_bytes(
        b.get(p..p + 2)
            .ok_or("Cartão truncado")?
            .try_into()
            .unwrap(),
    ))
}
fn u32le(b: &[u8], p: usize) -> McResult<u32> {
    Ok(u32::from_le_bytes(
        b.get(p..p + 4)
            .ok_or("Cartão truncado")?
            .try_into()
            .unwrap(),
    ))
}
pub struct CardReader {
    raw: Vec<u8>,
    alloc: usize,
    count: usize,
    root: usize,
}
#[derive(Clone)]
struct Entry {
    name: String,
    mode: u16,
    length: usize,
    cluster: usize,
}
impl CardReader {
    pub fn open(bytes: &[u8]) -> McResult<Self> {
        if !bytes.starts_with(MAGIC) || bytes.len() < 512 {
            return Err("Cartão não formatado, corrompido ou formato não suportado".into());
        }
        if u16le(bytes, 40)? != 512 || u16le(bytes, 42)? != 2 {
            return Err("Geometria de cartão não suportada".into());
        }
        let clusters = u32le(bytes, 48)? as usize;
        if clusters == 0 || clusters > 131072 {
            return Err("Capacidade inválida".into());
        }
        let raw = if bytes.len() == clusters * 1024 {
            bytes.to_vec()
        } else if bytes.len() == clusters * 1056 {
            bytes
                .chunks_exact(528)
                .flat_map(|page| page[..512].iter().copied())
                .collect()
        } else {
            return Err("Tamanho do cartão não corresponde ao cabeçalho".into());
        };
        let alloc = u32le(&raw, 52)? as usize;
        let count = u32le(&raw, 56)? as usize;
        let root = u32le(&raw, 60)? as usize;
        if alloc + count > clusters || root >= count {
            return Err("Cabeçalho de cartão inválido".into());
        }
        Ok(Self {
            raw,
            alloc,
            count,
            root,
        })
    }
    fn cluster(&self, n: usize) -> McResult<&[u8]> {
        self.raw
            .get(n * 1024..(n + 1) * 1024)
            .ok_or_else(|| "Cluster inválido".into())
    }
    fn fat(&self, n: usize) -> McResult<u32> {
        if n >= self.count {
            return Err("Índice FAT inválido".into());
        }
        let indirect = u32le(&self.raw, 80 + (n / 65536) * 4)? as usize;
        let fat = u32le(self.cluster(indirect)?, ((n / 256) % 256) * 4)? as usize;
        u32le(self.cluster(fat)?, (n % 256) * 4)
    }
    fn chain(&self, n: usize) -> McResult<Vec<u8>> {
        let mut n = n;
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        loop {
            if !seen.insert(n) || n >= self.count {
                return Err("Cadeia FAT corrompida".into());
            }
            result.extend_from_slice(self.cluster(n + self.alloc)?);
            let next = self.fat(n)?;
            if next == 0xffffffff {
                break;
            }
            if next & 0x80000000 == 0 {
                return Err("Cluster livre em cadeia ativa".into());
            }
            n = (next & 0x7fffffff) as usize;
        }
        Ok(result)
    }
    fn entries(&self, n: usize, limit: usize) -> McResult<Vec<Entry>> {
        let b = self.chain(n)?;
        if limit > b.len() / 512 {
            return Err("Diretório truncado".into());
        }
        b.chunks_exact(512)
            .take(limit)
            .filter_map(|e| {
                let mode = u16le(e, 0).ok()?;
                if mode & 0x8000 == 0 {
                    return None;
                }
                let name = String::from_utf8_lossy(e.get(64..96)?)
                    .trim_end_matches('\0')
                    .to_string();
                Some(Ok(Entry {
                    name,
                    mode,
                    length: u32le(e, 4).unwrap() as usize,
                    cluster: u32le(e, 16).unwrap() as usize,
                }))
            })
            .collect()
    }
    pub fn saves(&self) -> McResult<Vec<SaveInfo>> {
        let root = self.chain(self.root)?;
        let count = u32le(&root, 4)? as usize;
        let entries = self.entries(self.root, count)?;
        let mut saves = Vec::new();
        for e in entries {
            if e.name == "." || e.name == ".." || e.mode & 0x20 == 0 {
                continue;
            }
            name(&e.name)?;
            let files = self.entries(e.cluster, e.length)?;
            let mut size = 0;
            let mut count = 0;
            for f in &files {
                if f.mode & 0x20 == 0 {
                    size += f.length as u64;
                    count += 1;
                }
            }
            saves.push(SaveInfo {
                title: files
                    .iter()
                    .find(|f| f.name.eq_ignore_ascii_case("icon.sys"))
                    .and_then(|f| self.chain(f.cluster).ok())
                    .and_then(|b| icon_title(&b))
                    .unwrap_or_default(),
                serial: save_serial(&e.name),
                name: e.name,
                size,
                files: count,
            });
        }
        Ok(saves)
    }
    pub fn free(&self) -> McResult<u64> {
        let mut free = 0;
        for n in 0..self.count {
            if self.fat(n)? & 0x80000000 == 0 {
                free += 1024;
            }
        }
        Ok(free)
    }
}
fn file_snapshot(path: &Path) -> McResult<crate::cache::CardSnapshot> {
    let stamp = crate::cache::FileStamp::of(path);
    let key = path.to_path_buf();
    if let Some(stamp) = &stamp {
        if let Some(snapshot) = crate::cache::CARD_FILES.get(&key, stamp) {
            return Ok(snapshot);
        }
    }
    let bytes = read_bounded(path)?;
    let revision = digest(&bytes);
    let reader = CardReader::open(&bytes)?;
    let snapshot = crate::cache::CardSnapshot {
        saves: reader.saves()?,
        free: Some(reader.free()?),
        revision,
    };
    if let Some(stamp) = stamp {
        crate::cache::CARD_FILES.put(key, stamp, snapshot.clone());
    }
    Ok(snapshot)
}
pub fn inspect(path: &Path) -> McResult<(Vec<SaveInfo>, Option<u64>)> {
    if path.is_file() {
        let snapshot = file_snapshot(path)?;
        return Ok((snapshot.saves, snapshot.free));
    }
    plain(path)?;
    if !path.join("_pcsx2_superblock").exists() {
        return Err("Pasta sem metadados de cartão PCSX2".into());
    }
    let mut saves = Vec::new();
    for e in fs::read_dir(path).map_err(err)? {
        let e = e.map_err(err)?;
        let em = e.metadata().map_err(err)?;
        plain_meta(&em)?;
        let folder_name = e.file_name().to_string_lossy().into_owned();
        if em.is_dir() && !folder_name.starts_with('_') && !folder_name.starts_with('.') {
            let sub_path = e.path();
            let mut size = 0;
            let mut files = 0;
            for f in walkdir::WalkDir::new(&sub_path)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| !entry.file_name().to_string_lossy().starts_with("_pcsx2_"))
            {
                let f = f.map_err(err)?;
                let fm = f.metadata().map_err(err)?;
                plain_meta(&fm)?;
                if fm.is_file() {
                    size += fm.len();
                    files += 1;
                }
            }
            let icon_path = sub_path.join("icon.sys");
            let stamp = (size, files, crate::cache::FileStamp::of(&icon_path));
            if let Some(cached) = crate::cache::SAVE_FOLDERS.get(&sub_path, &stamp) {
                saves.push(cached);
                continue;
            }
            let info = SaveInfo {
                title: read_bounded(&icon_path)
                    .ok()
                    .and_then(|b| icon_title(&b))
                    .unwrap_or_default(),
                serial: save_serial(&folder_name),
                name: folder_name,
                size,
                files,
            };
            crate::cache::SAVE_FOLDERS.put(sub_path, stamp, info.clone());
            saves.push(info);
        }
    }
    Ok((saves, None))
}
pub fn list(conn: &Connection, db: &Path) -> McResult<CardState> {
    list_for(conn, db, None, false)
}
pub fn list_for(
    conn: &Connection,
    db: &Path,
    selected: Option<&str>,
    summary: bool,
) -> McResult<CardState> {
    if let Some(selected) = selected {
        name(selected)?;
    }
    let installation = pcsx2::installation_id(conn)?;
    let dir = cards_dir(conn)?;
    let ini = fs::read_to_string(data_dir(conn)?.join("inis/PCSX2.ini")).unwrap_or_default();
    let mut cards = Vec::new();
    let mut titles: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    if dir.exists() {
        plain(&dir)?;
        for e in fs::read_dir(&dir).map_err(err)? {
            let e = e.map_err(err)?;
            let n = e.file_name().to_string_lossy().into_owned();
            if selected.is_some_and(|selected| selected != n) {
                continue;
            }
            if n.starts_with('.') {
                continue;
            }
            if !e.path().is_dir() && !n.to_lowercase().ends_with(".ps2") {
                continue;
            }
            let mut revision = String::new();
            let analysis = if summary {
                Ok((Vec::new(), None))
            } else if e.path().is_file() {
                match file_snapshot(&e.path()) {
                    Ok(snapshot) => {
                        revision = snapshot.revision.clone();
                        Ok((snapshot.saves, snapshot.free))
                    }
                    Err(error) => {
                        revision = fingerprint(&e.path()).unwrap_or_default();
                        Err(error)
                    }
                }
            } else {
                let res = inspect(&e.path());
                revision = fingerprint(&e.path()).unwrap_or_default();
                res
            };
            let (mut saves, free_bytes, error) = match analysis {
                Ok((s, f)) => (s, f, None),
                Err(e) => (vec![], None, Some(e)),
            };
            for save in &mut saves {
                if let Some(serial) = &save.serial {
                    let title = titles
                        .entry(serial.clone())
                        .or_insert_with(|| {
                            conn.query_row(
                                "SELECT title FROM games WHERE serial=?1 ORDER BY title LIMIT 1",
                                [serial],
                                |r| r.get::<_, String>(0),
                            )
                            .ok()
                            .or_else(|| crate::game_metadata::title_for_serial(serial))
                        })
                        .clone();
                    if let Some(title) = title {
                        save.title = title;
                    }
                }
                if save.title.trim().is_empty() {
                    save.title = "Jogo não identificado".into();
                }
            }
            let slots = (1..=2)
                .filter(|s| {
                    ini_value(&ini, "MemoryCards", &format!("Slot{}_Filename", s)).as_deref()
                        == Some(&n)
                })
                .collect();
            let save_count = if summary { None } else { Some(saves.len()) };
            cards.push(CardInfo {
                name: n,
                kind: if e.path().is_dir() { "folder" } else { "file" }.into(),
                size: e.metadata().map_err(err)?.len(),
                revision,
                saves,
                save_count,
                free_bytes,
                error,
                slots,
            });
        }
    }
    cards.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(CardState {
        directory: dir.to_string_lossy().into(),
        cards,
        running: pcsx2::is_pcsx2_running(),
        tool_ready: tool_path(db).is_file(),
        installation_id: installation,
    })
}
pub fn card_path(conn: &Connection, n: &str) -> McResult<PathBuf> {
    let dir = cards_dir(conn)?;
    plain(&dir)?;
    let p = dir.join(name(n)?);
    plain(&p)?;
    Ok(p)
}
#[derive(Deserialize)]
pub struct CardAction {
    pub action: String,
    #[serde(default)]
    pub card: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub target_revision: String,
    #[serde(default)]
    pub revision: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub save: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub installation_id: String,
}
#[derive(Serialize)]
pub struct ActionResult {
    pub message: String,
    pub path: String,
    pub backup: Option<String>,
}
pub fn backup(db: &Path, src: &Path) -> McResult<PathBuf> {
    let base = store(db).join("backups").join(stamp());
    fs::create_dir_all(&base).map_err(err)?;
    let dest = base.join(src.file_name().ok_or("Arquivo inválido")?);
    copy_tree(src, &dest)?;
    if fingerprint(src)? != fingerprint(&dest)? {
        return Err("Backup divergente".into());
    }
    Ok(dest)
}
fn check_revision(path: &Path, r: &str) -> McResult<()> {
    if r.is_empty() || fingerprint(path)? != r {
        Err("O cartão mudou. Atualize a lista antes de continuar.".into())
    } else {
        Ok(())
    }
}
fn check_installation(conn: &Connection, id: &str) -> McResult<()> {
    if pcsx2::installation_id(conn)? != id {
        Err("O perfil ativo mudou. Atualize a página.".into())
    } else {
        Ok(())
    }
}
fn cleanup_stale_siblings(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".rommanager-new-") || name.starts_with(".rommanager-save-new-") {
            let p = entry.path();
            if p.is_dir() {
                let _ = fs::remove_dir_all(p);
            } else {
                let _ = fs::remove_file(p);
            }
        }
    }
}
pub fn action(conn: &Connection, db: &Path, p: CardAction) -> McResult<ActionResult> {
    let _guard = EDIT_LOCK.lock().map_err(err)?;
    writable()?;
    check_installation(conn, &p.installation_id)?;
    let dir = cards_dir(conn)?;
    fs::create_dir_all(&dir).map_err(err)?;
    plain(&dir)?;
    cleanup_stale_siblings(&dir);
    let temp = store(db).join("staging").join(stamp());
    fs::create_dir_all(&temp).map_err(err)?;
    let run = (|| {
        if p.action == "create" {
            let target = dir.join(format!("{}.ps2", name(p.target.trim_end_matches(".ps2"))?));
            if target.exists() {
                return Err("Já existe um cartão com esse nome".into());
            }
            let stage = temp.join("new.ps2");
            run_tool(db, &stage, "format", &[], &temp)?;
            inspect(&stage)?;
            writable()?;
            check_installation(conn, &p.installation_id)?;
            publish_new(&stage, &target)?;
            return Ok(ActionResult {
                message: "Cartão de 8 MB criado".into(),
                path: target.to_string_lossy().into(),
                backup: None,
            });
        }
        let card = card_path(conn, &p.card)?;
        check_revision(&card, &p.revision)?;
        if matches!(p.action.as_str(), "delete-save" | "move-save") {
            return crate::save_actions::edit(conn, db, &p, &card, &temp);
        }
        match p.action.as_str() {
            "convert" => {
                let target = dir.join(name(&p.target)?);
                if target.exists() {
                    return Err("O destino já existe".into());
                }
                if card.is_dir() {
                    if !p.target.to_lowercase().ends_with(".ps2") {
                        return Err("O cartão de arquivo deve terminar em .ps2".into());
                    }
                    let staged = temp.join("converted.ps2");
                    run_tool(db, &staged, "format", &[], &temp)?;
                    for save in inspect(&card)?.0 {
                        let psu = temp.join(format!("{}.psu", save.name));
                        fs::write(
                            &psu,
                            crate::save_formats::folder_to_psu(&card.join(&save.name))?,
                        )
                        .map_err(err)?;
                        run_tool(
                            db,
                            &staged,
                            "import",
                            &[psu.to_string_lossy().into()],
                            &temp,
                        )?;
                    }
                    let output = inspect(&staged)?.0;
                    let input = inspect(&card)?.0;
                    if output.len() != input.len()
                        || input
                            .iter()
                            .any(|s| !output.iter().any(|v| v.name == s.name && v.size == s.size))
                    {
                        return Err("A conversão não preservou todos os saves".into());
                    }
                    check_revision(&card, &p.revision)?;
                    check_installation(conn, &p.installation_id)?;
                    writable()?;
                    publish_new(&staged, &target)?;
                } else {
                    let staged = temp.join("converted-folder");
                    fs::create_dir(&staged).map_err(err)?;
                    let reader = CardReader::open(&read_bounded(&card)?)?;
                    fs::write(staged.join("_pcsx2_superblock"), &reader.raw[..8192])
                        .map_err(err)?;
                    for save in reader.saves()? {
                        run_tool(db, &card, "export", &[save.name.clone()], &temp)?;
                        crate::save_formats::psu_to_folder(
                            &read_bounded(&temp.join(format!("{}.psu", save.name)))?,
                            &staged,
                        )?;
                    }
                    let output = inspect(&staged)?.0;
                    let input = reader.saves()?;
                    if output.len() != input.len() {
                        return Err("Conversão incompleta".into());
                    }
                    check_revision(&card, &p.revision)?;
                    check_installation(conn, &p.installation_id)?;
                    writable()?;
                    publish_new(&staged, &target)?;
                }
                Ok(ActionResult {
                    message: "Conversão concluída; cartão original preservado".into(),
                    path: target.to_string_lossy().into(),
                    backup: None,
                })
            }
            "backup" => {
                let b = backup(db, &card)?;
                Ok(ActionResult {
                    message: "Backup verificado".into(),
                    path: b.to_string_lossy().into(),
                    backup: None,
                })
            }
            "duplicate" => {
                let target = dir.join(name(&p.target)?);
                let staged = temp.join("duplicate");
                copy_tree(&card, &staged)?;
                check_revision(&card, &p.revision)?;
                check_installation(conn, &p.installation_id)?;
                writable()?;
                publish_new(&staged, &target)?;
                Ok(ActionResult {
                    message: "Cartão duplicado".into(),
                    path: target.to_string_lossy().into(),
                    backup: None,
                })
            }
            "remove" => {
                let b = backup(db, &card)?;
                check_revision(&card, &p.revision)?;
                check_installation(conn, &p.installation_id)?;
                writable()?;
                if card.is_dir() {
                    fs::remove_dir_all(&card).map_err(err)?;
                } else {
                    fs::remove_file(&card).map_err(err)?;
                }
                Ok(ActionResult {
                    message: "Cartão removido; backup disponível".into(),
                    path: b.to_string_lossy().into(),
                    backup: Some(b.to_string_lossy().into()),
                })
            }
            "restore" => {
                let source = PathBuf::from(pcsx2::sanitize_path(&p.source));
                let backups = store(db).join("backups").canonicalize().map_err(err)?;
                if !source.canonicalize().map_err(err)?.starts_with(&backups) {
                    return Err("Escolha um backup criado pelo RomManager".into());
                }
                inspect(&source)?;
                let staged = temp.join("restore");
                copy_tree(&source, &staged)?;
                replace(conn, db, &card, &staged, &p)
            }
            "export" => {
                name(&p.save)?;
                let saves = inspect(&card)?.0;
                if !saves.iter().any(|s| s.name == p.save) {
                    return Err("Save não encontrado".into());
                }
                let source = temp.join(format!("{}.psu", p.save));
                if card.is_dir() {
                    fs::write(
                        &source,
                        crate::save_formats::folder_to_psu(&card.join(&p.save))?,
                    )
                    .map_err(err)?;
                } else {
                    run_tool(db, &card, "export", &[p.save.clone()], &temp)?;
                }
                read_bounded(&source)?;
                let out = store(db).join("exports").join(stamp());
                fs::create_dir_all(&out).map_err(err)?;
                let target = out.join(source.file_name().unwrap());
                copy_tree(&source, &target)?;
                Ok(ActionResult {
                    message: "Save exportado em PSU".into(),
                    path: target.to_string_lossy().into(),
                    backup: None,
                })
            }
            "import" | "preview" => {
                let source = PathBuf::from(pcsx2::sanitize_path(&p.source));
                let ext = source
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !["psu", "max"].contains(&ext.as_str()) {
                    return Err(
                        "Importe um save PSU ou MAX. Savestates não são saves de cartão.".into(),
                    );
                }
                read_bounded(&source)?;
                let scratch = temp.join("preview.ps2");
                run_tool(db, &scratch, "format", &[], &temp)?;
                run_tool(
                    db,
                    &scratch,
                    "import",
                    &[source.to_string_lossy().into()],
                    &temp,
                )?;
                let incoming = inspect(&scratch)?.0;
                if incoming.len() != 1 {
                    return Err("Save vazio ou pacote inválido".into());
                }
                let save = &incoming[0].name;
                name(save)?;
                if p.action == "preview" {
                    return Ok(ActionResult {
                        message: format!(
                            "{} · {} bytes · {} arquivos",
                            save, incoming[0].size, incoming[0].files
                        ),
                        path: save.clone(),
                        backup: None,
                    });
                }
                let collision = inspect(&card)?.0.iter().any(|s| s.name == *save);
                if collision && !p.overwrite {
                    return Err(format!(
                        "Já existe o save {}. Confirme a substituição após revisar.",
                        save
                    ));
                }
                if card.is_dir() {
                    let staged = temp.join("folder");
                    copy_tree(&card, &staged)?;
                    if collision {
                        fs::remove_dir_all(staged.join(save)).map_err(err)?;
                    }
                    run_tool(db, &scratch, "export", &[save.clone()], &temp)?;
                    let psu = read_bounded(&temp.join(format!("{}.psu", save)))?;
                    crate::save_formats::psu_to_folder(&psu, &staged)?;
                    inspect(&staged)?;
                    return replace(conn, db, &card, &staged, &p);
                }
                let staged = temp.join("edited.ps2");
                copy_tree(&card, &staged)?;
                if collision {
                    delete_save_file(&staged, save)?;
                }
                run_tool(
                    db,
                    &staged,
                    "import",
                    &[source.to_string_lossy().into()],
                    &temp,
                )?;
                if !inspect(&staged)?.0.iter().any(|s| s.name == *save) {
                    return Err("A importação não produziu o save esperado".into());
                }
                replace(conn, db, &card, &staged, &p)
            }
            _ => Err("Operação desconhecida".into()),
        }
    })();
    let _ = fs::remove_dir_all(&temp);
    run
}
fn replace(
    conn: &Connection,
    db: &Path,
    card: &Path,
    stage: &Path,
    p: &CardAction,
) -> McResult<ActionResult> {
    check_installation(conn, &p.installation_id)?;
    writable()?;
    check_revision(card, &p.revision)?;
    let b = backup(db, card)?;
    let sibling = card.with_file_name(format!(".rommanager-{}", stamp()));
    copy_tree(stage, &sibling)?;
    let old = card.with_file_name(format!(".rommanager-old-{}", stamp()));
    check_revision(card, &p.revision)?;
    fs::rename(card, &old).map_err(err)?;
    if let Err(e) = fs::rename(&sibling, card) {
        let _ = fs::rename(&old, card);
        return Err(err(e));
    }
    if old.is_dir() {
        let _ = fs::remove_dir_all(old);
    } else {
        let _ = fs::remove_file(old);
    }
    Ok(ActionResult {
        message: "Cartão atualizado; backup verificado".into(),
        path: card.to_string_lossy().into(),
        backup: Some(b.to_string_lossy().into()),
    })
}

static TOOL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub async fn prepare_tool(db: &Path) -> McResult<()> {
    let _tool_lock = TOOL_LOCK.lock().await;
    if tool_path(db).is_file() {
        return Ok(());
    }
    let response=reqwest::Client::builder().timeout(Duration::from_secs(90)).user_agent("RomManager").build().map_err(err)?
        .get("https://github.com/PCSX2/myMCpp/releases/download/v0.2.0/myMCpp-v0.2.0-windows-x64-clang.zip").send().await.map_err(err)?.error_for_status().map_err(err)?;
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(err)?;
        if bytes.len() + chunk.len() > 40 * 1024 * 1024 {
            return Err("Pacote da ferramenta acima do limite".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if digest(&bytes) != "1ee8df77aca5f39dcb88cd215fbd622b397037460a5c34e45cfa58825f948189" {
        return Err("O pacote da ferramenta não corresponde à versão verificada".into());
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(err)?;
    let dest = store(db).join(format!("tool-install-{}", stamp()));
    fs::create_dir_all(&dest).map_err(err)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(err)?;
        let relative = file
            .enclosed_name()
            .ok_or("Caminho inválido no pacote")?
            .to_path_buf();
        if file
            .unix_mode()
            .map(|m| m & 0o170000 == 0o120000)
            .unwrap_or(false)
        {
            return Err("Link no pacote".into());
        }
        let path = dest.join(relative);
        if file.is_dir() {
            fs::create_dir_all(&path).map_err(err)?;
        } else {
            if file.size() > 80 * 1024 * 1024 {
                return Err("Arquivo acima do limite".into());
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(err)?;
            }
            let mut f = fs::File::create(&path).map_err(err)?;
            std::io::copy(&mut file, &mut f).map_err(err)?;
        }
    }
    if !dest.join("myMCpp.exe").exists() {
        return Err("Executável ausente no pacote".into());
    }
    let _edit = EDIT_LOCK.lock().map_err(err)?;
    let target = store(db).join("tools");
    if target.exists() {
        fs::remove_dir_all(&dest).map_err(err)?;
    } else {
        fs::rename(dest, target).map_err(err)?;
    }
    Ok(())
}

pub async fn remove_tool(db: &Path) -> McResult<()> {
    let _tool = TOOL_LOCK.lock().await;
    let _edit = EDIT_LOCK
        .try_lock()
        .map_err(|_| "Aguarde a operação de cartões terminar.")?;
    writable()?;
    let folder = store(db).join("tools");
    if folder.exists() {
        plain(&folder)?;
        for item in walkdir::WalkDir::new(&folder).follow_links(false) {
            plain(item.map_err(err)?.path())?;
        }
        fs::remove_dir_all(folder).map_err(err)?;
    }
    Ok(())
}

#[path = "card_delete.rs"]
mod deletion;
pub fn delete_save_file(path: &Path, save: &str) -> McResult<()> {
    deletion::delete(path, save)
}

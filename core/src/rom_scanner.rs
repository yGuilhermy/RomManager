use chrono::Utc;
use regex::Regex;
use regex::bytes::Regex as BytesRegex;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledGame {
    pub id: String,
    pub title: String,
    pub filename: String,
    pub file_path: String,
    pub file_size: u64,
    pub format: String,
    pub cover_url: Option<String>,
    pub region: Option<String>,
    pub serial: Option<String>,
    pub date_added: String,
    #[serde(default)]
    pub is_missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheckResult {
    pub id: String,
    pub title: String,
    pub file_path: String,
    pub exists: bool,
    pub actual_size: u64,
    pub expected_size: u64,
    pub size_matches: bool,
    pub valid_format: bool,
    pub format: String,
    pub serial: Option<String>,
    pub healthy: bool,
    pub message: String,
}

pub fn is_safe_filename(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or("").to_uppercase();
    let reserved = matches!(
        base.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    !reserved
        && name.len() <= 240
        && !name.is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c.is_control() || "<>:\\|?*/\"".contains(c))
        && !name.ends_with([' ', '.'])
}

pub fn safe_filename(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\\|?*/\"".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.');
    if clean.is_empty() {
        "game".to_string()
    } else if is_safe_filename(clean) {
        clean.to_string()
    } else {
        format!("_{}", clean.chars().take(100).collect::<String>())
    }
}

pub fn is_valid_rom_extension(ext: &str) -> bool {
    let lower = ext.to_lowercase();
    matches!(lower.as_str(), "chd" | "iso" | "cso" | "bin" | "gz")
}

pub fn is_valid_cover_extension(ext: &str) -> bool {
    let lower = ext.to_lowercase();
    matches!(lower.as_str(), "jpg" | "jpeg" | "png" | "webp")
}

pub fn extract_region_from_name(name: &str) -> Option<String> {
    if name.contains("(USA)") || name.contains("(US)") {
        Some("USA".to_string())
    } else if name.contains("(Europe)")
        || name.contains("(EU)")
        || name.contains("(En,")
        || name.contains("(En)")
    {
        Some("Europe".to_string())
    } else if name.contains("(Japan)") || name.contains("(JP)") {
        Some("Japan".to_string())
    } else if name.contains("PT-BR") || name.contains("Portuguese") {
        Some("PT-BR".to_string())
    } else if name.contains("(Asia)") {
        Some("Asia".to_string())
    } else if name.contains("(Australia)") {
        Some("Australia".to_string())
    } else {
        None
    }
}

pub fn clean_title_from_filename(filename: &str) -> String {
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);
    let re = Regex::new(r"\s*\([^)]*\)").unwrap();
    let cleaned = re.replace_all(stem, "");
    cleaned.trim().to_string()
}

pub fn resolve_roms_directory(path: &Path) -> PathBuf {
    if path.exists() && path.is_dir() {
        return path.to_path_buf();
    }

    let path_str = path.to_string_lossy();
    if path_str.ends_with(" games") {
        let alt = PathBuf::from(path_str.trim_end_matches(" games"));
        if alt.exists() && alt.is_dir() {
            return alt;
        }
    } else if path_str.ends_with("games") {
        let alt = PathBuf::from(
            path_str
                .trim_end_matches("games")
                .trim_end_matches('\\')
                .trim_end_matches('/'),
        );
        if alt.exists() && alt.is_dir() {
            return alt;
        }
    } else {
        let alt = PathBuf::from(format!("{} games", path_str));
        if alt.exists() && alt.is_dir() {
            return alt;
        }
    }

    path.to_path_buf()
}

pub fn count_roms_in_directory(path: &Path) -> usize {
    if !path.exists() || !path.is_dir() {
        return 0;
    }
    WalkDir::new(path)
        .max_depth(2)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry.path().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(is_valid_rom_extension)
                    .unwrap_or(false)
        })
        .count()
}

pub fn count_covers_in_directory(path: &Path) -> usize {
    if !path.exists() || !path.is_dir() {
        return 0;
    }
    WalkDir::new(path)
        .max_depth(2)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry.path().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(is_valid_cover_extension)
                    .unwrap_or(false)
        })
        .count()
}

pub fn extract_serial_from_file(path: &Path) -> Option<String> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if ext.eq_ignore_ascii_case("chd") {
            if let Some(s) = crate::chd::extract_chd_serial(path) {
                return Some(s);
            }
        }
    }

    if let Ok(mut f) = File::open(path) {
        let mut buf = vec![0u8; 8 * 1024 * 1024];
        if let Ok(bytes_read) = f.read(&mut buf) {
            let slice = &buf[..bytes_read];
            let re = BytesRegex::new(r"([A-Z]{4}_[0-9]{3}\.[0-9]{2})").ok()?;
            if let Some(m) = re.captures(slice) {
                if let Some(matched) = m.get(1) {
                    if let Ok(s) = std::str::from_utf8(matched.as_bytes()) {
                        let normalized = s.replace('_', "-").replace('.', "");
                        return Some(normalized);
                    }
                }
            }
        }
    }

    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let re_fn = Regex::new(r"([A-Z]{4}[-_][0-9]{3,5})").ok()?;
    if let Some(caps) = re_fn.captures(filename) {
        if let Some(matched) = caps.get(1) {
            let normalized = matched.as_str().replace('_', "-");
            return Some(normalized);
        }
    }

    None
}

pub fn find_local_cover(
    covers_dir: &Path,
    serial: Option<&str>,
    clean_title: &str,
    raw_stem: &str,
) -> Option<String> {
    if !covers_dir.exists() || !covers_dir.is_dir() {
        return None;
    }

    let entries: Vec<String> = WalkDir::new(covers_dir)
        .max_depth(1)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.path().is_file()
                && e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(is_valid_cover_extension)
                    .unwrap_or(false)
        })
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .collect();

    let clean_title_lower = clean_title.to_lowercase();
    let raw_stem_lower = raw_stem.to_lowercase();
    let serial_lower = serial.map(|s| s.to_lowercase());

    if let Some(ref s_low) = serial_lower {
        for ext in &[".jpg", ".png", ".jpeg", ".webp", "_cov.jpg", "_cov.png"] {
            let candidate = format!("{}{}", s_low, ext);
            if let Some(found) = entries.iter().find(|e| e.to_lowercase() == candidate) {
                return Some(found.clone());
            }
        }
    }

    for ext in &[".jpg", ".png", ".jpeg", ".webp"] {
        let candidate = format!("{}{}", clean_title_lower, ext);
        if let Some(found) = entries.iter().find(|e| e.to_lowercase() == candidate) {
            return Some(found.clone());
        }
    }

    for ext in &[".jpg", ".png", ".jpeg", ".webp"] {
        let candidate = format!("{}{}", raw_stem_lower, ext);
        if let Some(found) = entries.iter().find(|e| e.to_lowercase() == candidate) {
            return Some(found.clone());
        }
    }

    for e in &entries {
        let e_low = e.to_lowercase();
        let stem = Path::new(&e_low)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if !clean_title_lower.is_empty()
            && (stem == clean_title_lower || clean_title_lower.contains(stem))
        {
            return Some(e.clone());
        }
    }

    None
}

pub async fn download_cover_from_ps2_covers(
    client: &reqwest::Client,
    serial: &str,
    covers_dir: &Path,
) -> Option<String> {
    let url = format!(
        "https://raw.githubusercontent.com/xlenore/ps2-covers/main/covers/default/{}.jpg",
        serial
    );
    if let Ok(res) = client.get(&url).send().await {
        if res.status().is_success() {
            if let Ok(bytes) = res.bytes().await {
                let filename = format!("{}.jpg", serial);
                let dest = covers_dir.join(&filename);
                if fs::write(&dest, bytes).is_ok() {
                    return Some(filename);
                }
            }
        }
    }
    None
}

pub async fn download_cover_from_libretro(
    client: &reqwest::Client,
    clean_title: &str,
    covers_dir: &Path,
) -> Option<String> {
    let encoded = urlencoding::encode(clean_title);
    let url = format!(
        "https://cdn.jsdelivr.net/gh/libretro-thumbnails/Sony_-_PlayStation_2@master/Named_Boxarts/{}.png",
        encoded
    );
    if let Ok(res) = client.get(&url).send().await {
        if res.status().is_success() {
            if let Ok(bytes) = res.bytes().await {
                let filename = format!("{}.png", clean_title);
                let dest = covers_dir.join(&filename);
                if fs::write(&dest, bytes).is_ok() {
                    return Some(filename);
                }
            }
        }
    }
    None
}

pub fn normalized_path_key(path: &Path) -> String {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let raw = resolved.to_string_lossy().replace('\\', "/");
    let unprefixed = if raw
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("//?/UNC/"))
    {
        format!("//{}", &raw[8..])
    } else {
        raw.strip_prefix("//?/").unwrap_or(&raw).to_string()
    };
    let windows = cfg!(windows)
        || unprefixed.as_bytes().get(1) == Some(&b':')
        || unprefixed.starts_with("//");
    let key = unprefixed.trim_end_matches('/');
    if windows {
        key.to_lowercase()
    } else {
        key.to_string()
    }
}

fn belongs_to_root(path: &Path, root: &Path) -> bool {
    let path = normalized_path_key(path);
    let root = normalized_path_key(root);
    path.strip_prefix(&root)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

pub fn normalize_library(conn: &mut Connection) -> Result<usize, String> {
    let records = {
        let mut stmt = conn.prepare("SELECT id,file_path,date_added,cover_url FROM games ORDER BY date_added ASC,id ASC").map_err(|e|e.to_string())?;
        stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?
    };
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DROP INDEX IF EXISTS games_path_key_unique", [])
        .map_err(|e| e.to_string())?;
    let mut survivors = std::collections::HashMap::<String, String>::new();
    let mut removed = 0;
    for (id, path, _date, cover) in records {
        let key = normalized_path_key(Path::new(&path));
        if let Some(survivor) = survivors.get(&key) {
            tx.execute("INSERT INTO game_progress(game_id,status,notes,launches,last_played) SELECT ?1,status,notes,launches,last_played FROM game_progress WHERE game_id=?2 ON CONFLICT(game_id) DO UPDATE SET status=CASE WHEN game_progress.status='completed' OR excluded.status='completed' THEN 'completed' WHEN game_progress.status='planned' THEN excluded.status ELSE game_progress.status END, notes=CASE WHEN game_progress.notes='' THEN excluded.notes WHEN excluded.notes='' OR game_progress.notes=excluded.notes THEN game_progress.notes ELSE game_progress.notes || char(10) || excluded.notes END, launches=MIN(game_progress.launches+excluded.launches,4294967295), last_played=CASE WHEN game_progress.last_played IS NULL OR julianday(excluded.last_played)>julianday(game_progress.last_played) THEN excluded.last_played ELSE game_progress.last_played END",params![survivor,id]).map_err(|e|e.to_string())?;
            let merged_notes: Option<String> = {
                use rusqlite::OptionalExtension;
                tx.query_row(
                    "SELECT notes FROM game_progress WHERE game_id=?1",
                    params![survivor],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
            };
            if let Some(notes) = merged_notes.filter(|notes| notes.chars().count() > 2000) {
                let title: String = tx
                    .query_row(
                        "SELECT title FROM games WHERE id=?1",
                        params![survivor],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let mut chunk = String::new();
                for ch in notes.chars() {
                    if chunk.len() + ch.len_utf8() > 4000 {
                        crate::database::add_history(&tx, &title, "merge_notes", Some(&chunk))
                            .map_err(|e| e.to_string())?;
                        chunk.clear();
                    }
                    chunk.push(ch);
                }
                if !chunk.is_empty() {
                    crate::database::add_history(&tx, &title, "merge_notes", Some(&chunk))
                        .map_err(|e| e.to_string())?;
                }
                tx.execute(
                    "UPDATE game_progress SET notes=?1 WHERE game_id=?2",
                    params![notes.chars().take(2000).collect::<String>(), survivor],
                )
                .map_err(|e| e.to_string())?;
            }
            tx.execute("DELETE FROM game_progress WHERE game_id=?1", params![id])
                .map_err(|e| e.to_string())?;
            tx.execute("UPDATE favorites SET id=?1 WHERE id=?2 AND NOT EXISTS(SELECT 1 FROM favorites WHERE id=?1)",params![survivor,id]).map_err(|e|e.to_string())?;
            tx.execute("DELETE FROM favorites WHERE id=?1", params![id])
                .map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE games SET cover_url=COALESCE(NULLIF(cover_url,''),?1) WHERE id=?2",
                params![cover, survivor],
            )
            .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM game_files WHERE game_id=?1", params![id])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM game_variants WHERE game_id=?1", params![id])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM games WHERE id=?1", params![id])
                .map_err(|e| e.to_string())?;
            removed += 1;
        } else {
            tx.execute("UPDATE games SET path_key=?1 WHERE id=?2", params![key, id])
                .map_err(|e| e.to_string())?;
            survivors.insert(key, id);
        }
    }
    tx.execute(
        "CREATE UNIQUE INDEX games_path_key_unique ON games(path_key)",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(removed)
}

pub(crate) static SCAN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn scan_roms_directory(
    conn: &mut Connection,
    roms_dir_raw: &Path,
    covers_dir: Option<&Path>,
) -> Result<Vec<InstalledGame>, String> {
    let _scan_guard = SCAN_LOCK.lock().await;
    normalize_library(conn)?;
    let roms_dir = resolve_roms_directory(roms_dir_raw);
    let roms_dir = fs::canonicalize(&roms_dir).map_err(|e| e.to_string())?;
    if !roms_dir.exists() || !roms_dir.is_dir() {
        return Err("O diretório de ROMs não existe ou é inválido.".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let mut found_games = Vec::new();
    let mut found_ids = std::collections::HashSet::new();
    let auto_covers =
        crate::database::get_config(conn, "auto_download_covers").as_deref() != Some("false");

    for entry in WalkDir::new(&roms_dir)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".rommanager-downloads")
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if is_valid_rom_extension(ext) {
                    let filename = path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("")
                        .to_string();
                    let raw_stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&filename)
                        .to_string();
                    let file_path = path.to_string_lossy().to_string();
                    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
                    let file_size = metadata.len();
                    let format = ext.to_lowercase();
                    let title = clean_title_from_filename(&filename);
                    let region = extract_region_from_name(&filename);
                    let serial = extract_serial_from_file(path);

                    let path_key = normalized_path_key(path);
                    let existing: Option<(String, Option<String>, String)> = {
                        use rusqlite::OptionalExtension;
                        conn.query_row(
                            "SELECT id,cover_url,date_added FROM games WHERE path_key=?1",
                            params![path_key],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                        )
                        .optional()
                        .map_err(|e| e.to_string())?
                    };
                    let (id, mut cover_url, date_added) = existing
                        .unwrap_or_else(|| (path_key.clone(), None, Utc::now().to_rfc3339()));
                    found_ids.insert(id.clone());

                    if let Some(cdir) = covers_dir {
                        if let Some(local_file) =
                            find_local_cover(cdir, serial.as_deref(), &title, &raw_stem)
                        {
                            cover_url = Some(format!(
                                "http://127.0.0.1:3000/api/covers/serve?file={}",
                                urlencoding::encode(&local_file)
                            ));
                        } else if auto_covers {
                            if let Some(ref s) = serial {
                                if let Some(downloaded) =
                                    download_cover_from_ps2_covers(&client, s, cdir).await
                                {
                                    cover_url = Some(format!(
                                        "http://127.0.0.1:3000/api/covers/serve?file={}",
                                        urlencoding::encode(&downloaded)
                                    ));
                                }
                            }
                            if cover_url.is_none() {
                                if let Some(downloaded) =
                                    download_cover_from_libretro(&client, &title, cdir).await
                                {
                                    cover_url = Some(format!(
                                        "http://127.0.0.1:3000/api/covers/serve?file={}",
                                        urlencoding::encode(&downloaded)
                                    ));
                                }
                            }
                        }
                    }

                    if cover_url.is_none() {
                        cover_url = Some(crate::api_roms::build_cover_url(&title));
                    }

                    conn.execute(
                        "INSERT INTO games (id, title, filename, file_path, file_size, format, cover_url, region, serial, date_added, path_key)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                         ON CONFLICT(path_key) DO UPDATE SET
                            title = excluded.title,
                            filename = excluded.filename,
                            file_path = excluded.file_path,
                            file_size = excluded.file_size,
                            format = excluded.format,
                            cover_url = excluded.cover_url,
                            region = excluded.region,
                            serial = excluded.serial",
                        params![
                            id,
                            raw_stem,
                            filename,
                            file_path,
                            file_size as i64,
                            format,
                            cover_url,
                            region,
                            serial,
                            date_added,
                            path_key,
                        ],
                    ).map_err(|e| e.to_string())?;

                    conn.execute(
                        "INSERT INTO game_variants (id, game_id, title, region, serial, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                         ON CONFLICT(id) DO UPDATE SET
                            title = excluded.title,
                            region = excluded.region,
                            serial = excluded.serial",
                        params![
                            format!("var_{}", id),
                            id,
                            raw_stem,
                            region,
                            serial,
                            date_added,
                        ],
                    ).map_err(|e| e.to_string())?;

                    conn.execute(
                        "INSERT INTO game_files (id, variant_id, game_id, file_path, path_key, file_size, format, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                         ON CONFLICT(path_key) DO UPDATE SET
                            variant_id = excluded.variant_id,
                            game_id = excluded.game_id,
                            file_path = excluded.file_path,
                            file_size = excluded.file_size,
                            format = excluded.format",
                        params![
                            format!("file_{}", id),
                            format!("var_{}", id),
                            id,
                            file_path,
                            path_key,
                            file_size as i64,
                            format,
                            date_added,
                        ],
                    ).map_err(|e| e.to_string())?;

                    found_games.push(InstalledGame {
                        id,
                        title: raw_stem,
                        filename,
                        file_path,
                        file_size,
                        format,
                        cover_url,
                        region,
                        serial,
                        date_added,
                        is_missing: false,
                    });
                }
            }
        }
    }

    let current_db_ids = {
        let mut stmt = conn
            .prepare("SELECT id, file_path FROM games")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<(String, String)>, _>>()
            .map_err(|e| e.to_string())?
    };

    for (db_id, db_path) in current_db_ids {
        if belongs_to_root(Path::new(&db_path), &roms_dir) && !found_ids.contains(&db_id) {
            conn.execute("DELETE FROM game_files WHERE game_id = ?1", params![db_id])
                .map_err(|e| e.to_string())?;
            conn.execute("DELETE FROM game_variants WHERE game_id = ?1", params![db_id])
                .map_err(|e| e.to_string())?;
            conn.execute("DELETE FROM games WHERE id = ?1", params![db_id])
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(found_games)
}

pub fn list_installed_games(conn: &Connection) -> Result<Vec<InstalledGame>, String> {
    let mut stmt = conn
        .prepare("SELECT id, title, filename, file_path, file_size, format, cover_url, region, serial, date_added FROM games ORDER BY title ASC")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let file_path: String = row.get(3)?;
            let is_missing = !Path::new(&file_path).exists();
            let mut serial: Option<String> = row.get(8)?;
            let format: String = row.get(5)?;
            if serial.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) && format.eq_ignore_ascii_case("chd") && !is_missing {
                if let Some(s) = crate::chd::extract_chd_serial(Path::new(&file_path)) {
                    serial = Some(s);
                }
            }
            Ok(InstalledGame {
                id: row.get(0)?,
                title: row.get(1)?,
                filename: row.get(2)?,
                file_path,
                file_size: row.get::<_, i64>(4)? as u64,
                format,
                cover_url: row.get(6)?,
                region: row.get(7)?,
                serial,
                date_added: row.get(9)?,
                is_missing,
            })
        })
        .map_err(|e| e.to_string())?;

    let games = rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    for g in &games {
        if let Some(s) = &g.serial {
            let _ = conn.execute("UPDATE games SET serial = ?1 WHERE id = ?2 AND (serial IS NULL OR serial = '')", params![s, g.id]);
        }
    }
    Ok(games)
}

pub fn verify_game_integrity(conn: &Connection, id: &str) -> Result<IntegrityCheckResult, String> {
    let mut stmt = conn
        .prepare("SELECT id, title, filename, file_path, file_size, format, serial FROM games WHERE id = ?1")
        .map_err(|e| e.to_string())?;

    let row = stmt
        .query_row(params![id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)? as u64,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|e| format!("Jogo não encontrado: {}", e))?;

    let (id, title, _filename, file_path, expected_size, format, recorded_serial) = row;
    let path = Path::new(&file_path);
    let exists = path.exists() && path.is_file();

    if !exists {
        return Ok(IntegrityCheckResult {
            id,
            title,
            file_path,
            exists: false,
            actual_size: 0,
            expected_size,
            size_matches: false,
            valid_format: false,
            format,
            serial: recorded_serial,
            healthy: false,
            message: "O arquivo da ROM não foi localizado no caminho gravado.".to_string(),
        });
    }

    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    let actual_size = metadata.len();
    let size_matches = actual_size == expected_size || expected_size == 0;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let valid_format = is_valid_rom_extension(ext);
    let current_serial = extract_serial_from_file(path).or(recorded_serial);

    let healthy = exists && size_matches && valid_format;
    let message = if healthy {
        "Arquivo íntegro, formato válido e verificado no disco.".to_string()
    } else if !size_matches {
        "O tamanho atual do arquivo difere do tamanho registrado no catálogo.".to_string()
    } else {
        "O formato ou extensão do arquivo não é reconhecido como ROM compatível.".to_string()
    };

    Ok(IntegrityCheckResult {
        id,
        title,
        file_path,
        exists,
        actual_size,
        expected_size,
        size_matches,
        valid_format,
        format,
        serial: current_serial,
        healthy,
        message,
    })
}

pub fn delete_installed_game(
    conn: &mut Connection,
    id: &str,
    _covers_dir: Option<&Path>,
) -> Result<(), String> {
    let (path, title): (String, String) = conn
        .query_row(
            "SELECT file_path, title FROM games WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Não foi possível excluir a ROM: {e}")),
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM game_files WHERE game_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM game_variants WHERE game_id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM games WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    crate::database::add_history(
        &tx,
        &title,
        "delete",
        Some("ROM removida; capas compartilhadas preservadas"),
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

pub fn open_folder_in_explorer(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("O caminho não existe.".to_string());
    }

    if path.is_file() {
        Command::new("explorer")
            .arg(format!("/select,{}", path.to_string_lossy()))
            .spawn()
            .map_err(|e| e.to_string())?;
    } else {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

use chrono::Utc;
use rusqlite::{Connection, Result, params};
use std::collections::HashMap;
use std::path::Path;

pub fn initialize_db(db_path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(db_path)?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS game_progress (
            game_id TEXT PRIMARY KEY,
            status TEXT NOT NULL DEFAULT 'planned',
            notes TEXT NOT NULL DEFAULT '',
            launches INTEGER NOT NULL DEFAULT 0,
            last_played TEXT
        );
        CREATE TABLE IF NOT EXISTS configs (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS games (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            filename TEXT NOT NULL,
            file_path TEXT NOT NULL,
            path_key TEXT,
            file_size INTEGER NOT NULL,
            format TEXT NOT NULL,
            cover_url TEXT,
            region TEXT,
            serial TEXT,
            date_added TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS downloads (
            id TEXT PRIMARY KEY,
            game_title TEXT NOT NULL,
            url TEXT NOT NULL,
            cover_url TEXT,
            status TEXT NOT NULL,
            progress REAL NOT NULL,
            speed REAL NOT NULL,
            size INTEGER NOT NULL,
            downloaded_bytes INTEGER NOT NULL,
            error_message TEXT,
            target_path TEXT,
            connections INTEGER DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS favorites (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            cover_url TEXT,
            url TEXT NOT NULL,
            size INTEGER,
            region TEXT,
            added_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            game_name TEXT NOT NULL,
            action TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            details TEXT
        );

        CREATE TABLE IF NOT EXISTS game_metadata (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT,
            source TEXT,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS game_variants (
            id TEXT PRIMARY KEY,
            game_id TEXT NOT NULL,
            title TEXT NOT NULL,
            edition TEXT,
            region TEXT,
            serial TEXT,
            disc_number INTEGER DEFAULT 1,
            disc_total INTEGER DEFAULT 1,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS game_files (
            id TEXT PRIMARY KEY,
            variant_id TEXT NOT NULL,
            game_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            path_key TEXT NOT NULL UNIQUE,
            file_size INTEGER NOT NULL,
            format TEXT NOT NULL,
            modified_at TEXT,
            sha1 TEXT,
            crc32 TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_game_variants_game_id ON game_variants(game_id);
        CREATE INDEX IF NOT EXISTS idx_game_files_variant_id ON game_files(variant_id);
        CREATE INDEX IF NOT EXISTS idx_game_files_game_id ON game_files(game_id);

        CREATE TABLE IF NOT EXISTS jobs (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            status TEXT NOT NULL,
            progress REAL NOT NULL DEFAULT 0.0,
            current_step TEXT NOT NULL DEFAULT '',
            total_steps INTEGER NOT NULL DEFAULT 0,
            completed_steps INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            finished_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);

        CREATE TABLE IF NOT EXISTS ra_account_cache (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            username TEXT NOT NULL,
            score INTEGER NOT NULL DEFAULT 0,
            softcore_score INTEGER NOT NULL DEFAULT 0,
            avatar_url TEXT NOT NULL DEFAULT '',
            synced_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ra_game_cache (
            game_id TEXT PRIMARY KEY,
            file_hash TEXT NOT NULL DEFAULT '',
            boot_executable TEXT NOT NULL DEFAULT '',
            ra_game_id INTEGER NOT NULL DEFAULT 0,
            ra_title TEXT NOT NULL DEFAULT '',
            icon_url TEXT NOT NULL DEFAULT '',
            recognition_status TEXT NOT NULL DEFAULT 'pending',
            recognition_message TEXT NOT NULL DEFAULT '',
            total_achievements INTEGER NOT NULL DEFAULT 0,
            unlocked_casual INTEGER NOT NULL DEFAULT 0,
            unlocked_hardcore INTEGER NOT NULL DEFAULT 0,
            points_total INTEGER NOT NULL DEFAULT 0,
            points_earned_casual INTEGER NOT NULL DEFAULT 0,
            points_earned_hardcore INTEGER NOT NULL DEFAULT 0,
            achievements_json TEXT NOT NULL DEFAULT '[]',
            file_size INTEGER NOT NULL DEFAULT 0,
            file_mtime INTEGER NOT NULL DEFAULT 0,
            synced_at TEXT NOT NULL
        );",
    )?;

    let _ = conn.execute(
        "ALTER TABLE downloads ADD COLUMN connections INTEGER DEFAULT 1",
        [],
    );
    let _ = conn.execute("ALTER TABLE ra_game_cache ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE ra_game_cache ADD COLUMN file_mtime INTEGER NOT NULL DEFAULT 0", []);

    seed_default_configs(&conn)?;
    migrate_pcsx2_profiles(&mut conn)?;
    let has_path_key: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('games') WHERE name='path_key')",
        [],
        |r| r.get(0),
    )?;
    if !has_path_key {
        conn.execute("ALTER TABLE games ADD COLUMN path_key TEXT", [])?;
    }
    crate::rom_scanner::normalize_library(&mut conn)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e))))?;
    sync_game_variants_and_files(&mut conn)?;

    Ok(conn)
}

pub fn sync_game_variants_and_files(conn: &mut Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO game_variants (id, game_id, title, region, serial, created_at)
         SELECT 'var_' || id, id, title, region, serial, date_added
         FROM games
         WHERE NOT EXISTS (SELECT 1 FROM game_variants WHERE game_variants.game_id = games.id)",
        [],
    )?;

    conn.execute(
        "INSERT INTO game_files (id, variant_id, game_id, file_path, path_key, file_size, format, created_at)
         SELECT 'file_' || games.id, COALESCE((SELECT id FROM game_variants WHERE game_variants.game_id = games.id LIMIT 1), 'var_' || games.id), games.id, games.file_path, games.path_key, games.file_size, games.format, games.date_added
         FROM games
         WHERE games.path_key IS NOT NULL AND NOT EXISTS (SELECT 1 FROM game_files WHERE game_files.path_key = games.path_key)",
        [],
    )?;

    Ok(())
}

fn seed_default_configs(conn: &Connection) -> Result<()> {
    let defaults = [
        ("roms_dir", ""),
        ("roms_directory", ""),
        ("pcsx2_path", "C:\\Program Files\\PCSX2\\pcsx2-qt.exe"),
        (
            "pcsx2_executable_path",
            "C:\\Program Files\\PCSX2\\pcsx2-qt.exe",
        ),
        ("covers_dir", ""),
        ("covers_directory", ""),
        ("auto_download_covers", "true"),
        ("dark_mode", "true"),
        ("max_concurrent_downloads", "2"),
        ("pcsx2_data_dir", ""),
        ("pcsx2_bios_dir", ""),
    ];

    for (k, v) in defaults.iter() {
        conn.execute(
            "INSERT INTO configs (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value WHERE value = ''",
            params![k, v],
        )?;
    }

    Ok(())
}

fn migrate_pcsx2_profiles(conn: &mut Connection) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pcsx2_profiles')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let tx = conn.transaction()?;
    let profiles = {
        let mut stmt=tx.prepare("SELECT id,name,executable_path,data_dir,bios_path,renderer,aspect_ratio,fullscreen,is_active,created_at,updated_at FROM pcsx2_profiles")?;
        let rows=stmt.query_map([],|r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"executable_path":r.get::<_,String>(2)?,"data_dir":r.get::<_,String>(3)?,"bios_path":r.get::<_,Option<String>>(4)?,"renderer":r.get::<_,String>(5)?,"aspect_ratio":r.get::<_,String>(6)?,"fullscreen":r.get::<_,i64>(7)?,"is_active":r.get::<_,i64>(8)?,"created_at":r.get::<_,String>(9)?,"updated_at":r.get::<_,String>(10)?})))?;
        rows.collect::<Result<Vec<_>>>()?
    };
    tx.execute(
        "INSERT OR IGNORE INTO configs(key,value) VALUES('legacy_pcsx2_profiles_backup',?1)",
        [serde_json::to_string(&profiles).unwrap()],
    )?;
    if let Some(active) = profiles.iter().find(|p| p["is_active"] == 1) {
        let exe = active["executable_path"].as_str().unwrap_or_default();
        let configured = get_config(&tx, "pcsx2_path").unwrap_or_default();
        if configured.is_empty() {
            set_config(&tx, "pcsx2_path", exe)?;
        }
        if configured.is_empty() || configured.eq_ignore_ascii_case(exe) {
            let data = active["data_dir"].as_str().unwrap_or_default();
            if !data.is_empty() && get_config(&tx, "pcsx2_data_dir").is_none() {
                set_config(&tx, "pcsx2_data_dir", data)?;
            }
        }
    }
    tx.execute_batch("DROP TABLE pcsx2_profiles;")?;
    tx.commit()
}

pub fn get_config(conn: &Connection, key: &str) -> Option<String> {
    let val: Option<String> = conn
        .query_row(
            "SELECT value FROM configs WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .ok();

    if let Some(v) = val {
        if !v.is_empty() {
            return Some(v);
        }
    }

    let alias = match key {
        "roms_dir" => Some("roms_directory"),
        "roms_directory" => Some("roms_dir"),
        "pcsx2_path" => Some("pcsx2_executable_path"),
        "pcsx2_executable_path" => Some("pcsx2_path"),
        "covers_dir" => Some("covers_directory"),
        "covers_directory" => Some("covers_dir"),
        _ => None,
    };

    if let Some(a) = alias {
        return conn
            .query_row(
                "SELECT value FROM configs WHERE key = ?1",
                params![a],
                |row| row.get(0),
            )
            .ok();
    }

    None
}

pub fn set_config(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO configs (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;

    let alias = match key {
        "roms_dir" => Some("roms_directory"),
        "roms_directory" => Some("roms_dir"),
        "pcsx2_path" => Some("pcsx2_executable_path"),
        "pcsx2_executable_path" => Some("pcsx2_path"),
        "covers_dir" => Some("covers_directory"),
        "covers_directory" => Some("covers_dir"),
        _ => None,
    };

    if let Some(a) = alias {
        conn.execute(
            "INSERT INTO configs (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![a, value],
        )?;
    }

    Ok(())
}

pub fn get_all_configs(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM configs")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut map = HashMap::new();
    for r in rows.flatten() {
        if r.0 == "legacy_pcsx2_profiles_backup" {
            continue;
        }
        map.insert(r.0, r.1);
    }
    Ok(map)
}

pub fn add_history(
    conn: &Connection,
    game_name: &str,
    action: &str,
    details: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO history (game_name, action, timestamp, details) VALUES (?1, ?2, ?3, ?4)",
        params![game_name, action, now, details],
    )?;
    Ok(())
}

pub fn get_game_metadata(conn: &Connection, id: &str) -> Option<(String, String)> {
    conn.query_row(
        "SELECT description, source FROM game_metadata WHERE id = ?1",
        params![id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .ok()
}

pub fn save_game_metadata(
    conn: &Connection,
    id: &str,
    title: &str,
    description: &str,
    source: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO game_metadata (id, title, description, source, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET description = excluded.description, source = excluded.source, updated_at = excluded.updated_at",
        params![id, title, description, source, now],
    )?;
    Ok(())
}

use rom_core::database;
use std::fs::{self, File};
use std::path::Path;

#[tokio::test]
async fn legacy_database_migrates_preserving_ids_and_populating_variants() {
    let test_dir = Path::new("target/test_collection_legacy");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("create test dir");
    let db_path = test_dir.join("legacy.db");

    {
        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE games (
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
            CREATE TABLE game_progress (
                game_id TEXT PRIMARY KEY,
                status TEXT NOT NULL DEFAULT 'planned',
                notes TEXT NOT NULL DEFAULT '',
                launches INTEGER NOT NULL DEFAULT 0,
                last_played TEXT
            );
            CREATE TABLE favorites (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                cover_url TEXT,
                url TEXT NOT NULL,
                size INTEGER,
                region TEXT,
                added_at TEXT NOT NULL
            );
            CREATE TABLE configs (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE downloads (
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
                target_path TEXT
            );
            CREATE TABLE history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                game_name TEXT NOT NULL,
                action TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                details TEXT
            );
            CREATE TABLE game_metadata (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT,
                source TEXT,
                updated_at TEXT NOT NULL
            );

            INSERT INTO games (id, title, filename, file_path, path_key, file_size, format, cover_url, region, serial, date_added)
            VALUES ('game_abc_123', 'Shadow of the Colossus', 'Shadow.iso', 'C:/Games/Shadow.iso', 'c:/games/shadow.iso', 1048576, 'iso', NULL, 'USA', 'SCUS-97472', '2026-01-01T00:00:00Z');

            INSERT INTO game_progress (game_id, status, notes, launches, last_played)
            VALUES ('game_abc_123', 'playing', 'Boss 4 defeated', 5, '2026-01-02T12:00:00Z');

            INSERT INTO favorites (id, title, cover_url, url, size, region, added_at)
            VALUES ('game_abc_123', 'Shadow of the Colossus', NULL, 'http://example.com/shadow.iso', 1048576, 'USA', '2026-01-01T00:00:00Z');",
        ).expect("seed legacy tables");
    }

    let conn = database::initialize_db(&db_path).expect("initialize_db should migrate legacy db");

    let count_variants: i64 = conn
        .query_row("SELECT COUNT(*) FROM game_variants", [], |r| r.get(0))
        .expect("query game_variants");
    assert_eq!(count_variants, 1);

    let count_files: i64 = conn
        .query_row("SELECT COUNT(*) FROM game_files", [], |r| r.get(0))
        .expect("query game_files");
    assert_eq!(count_files, 1);

    let variant_game_id: String = conn
        .query_row("SELECT game_id FROM game_variants WHERE game_id = 'game_abc_123'", [], |r| r.get(0))
        .expect("find variant");
    assert_eq!(variant_game_id, "game_abc_123");

    let progress_status: String = conn
        .query_row("SELECT status FROM game_progress WHERE game_id = 'game_abc_123'", [], |r| r.get(0))
        .expect("find progress");
    assert_eq!(progress_status, "playing");

    let fav_title: String = conn
        .query_row("SELECT title FROM favorites WHERE id = 'game_abc_123'", [], |r| r.get(0))
        .expect("find favorite");
    assert_eq!(fav_title, "Shadow of the Colossus");
}

#[tokio::test]
async fn distinct_mods_sharing_same_serial_do_not_merge() {
    let test_dir = Path::new("target/test_collection_mods");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("create test dir");
    let db_path = test_dir.join("mods.db");
    let roms_dir = test_dir.join("roms");
    fs::create_dir_all(&roms_dir).expect("create roms dir");

    let rom1 = roms_dir.join("Dragon Ball Z Budokai Tenkaichi 3.iso");
    let rom2 = roms_dir.join("Dragon Ball Z Budokai Tenkaichi 3 (Mod AF Version).iso");

    File::create(&rom1).expect("create rom1");
    File::create(&rom2).expect("create rom2");

    let mut conn = database::initialize_db(&db_path).expect("init db");

    let scanned1 = rom_core::rom_scanner::scan_roms_directory(&mut conn, &roms_dir, None)
        .await
        .expect("first scan");
    assert_eq!(scanned1.len(), 2);

    let games_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
        .expect("count games");
    assert_eq!(games_count, 2);

    let variants_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM game_variants", [], |r| r.get(0))
        .expect("count variants");
    assert_eq!(variants_count, 2);

    let files_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM game_files", [], |r| r.get(0))
        .expect("count files");
    assert_eq!(files_count, 2);

    let scanned2 = rom_core::rom_scanner::scan_roms_directory(&mut conn, &roms_dir, None)
        .await
        .expect("second scan");
    assert_eq!(scanned2.len(), 2);

    let games_count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))
        .expect("count games after second scan");
    assert_eq!(games_count_after, 2);
}

#[tokio::test]
async fn removed_game_file_on_disk_cleans_up_variants_and_files() {
    let test_dir = Path::new("target/test_collection_cleanup");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("create test dir");
    let db_path = test_dir.join("cleanup.db");
    let roms_dir = test_dir.join("roms");
    fs::create_dir_all(&roms_dir).expect("create roms dir");

    let rom1 = roms_dir.join("Game A.iso");
    let rom2 = roms_dir.join("Game B.iso");

    File::create(&rom1).expect("create rom1");
    File::create(&rom2).expect("create rom2");

    let mut conn = database::initialize_db(&db_path).expect("init db");

    let scanned1 = rom_core::rom_scanner::scan_roms_directory(&mut conn, &roms_dir, None)
        .await
        .expect("first scan");
    assert_eq!(scanned1.len(), 2);

    let v1: i64 = conn.query_row("SELECT COUNT(*) FROM game_variants", [], |r| r.get(0)).unwrap();
    let f1: i64 = conn.query_row("SELECT COUNT(*) FROM game_files", [], |r| r.get(0)).unwrap();
    let g1: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0)).unwrap();
    assert_eq!(v1, 2);
    assert_eq!(f1, 2);
    assert_eq!(g1, 2);

    fs::remove_file(&rom2).expect("remove rom2");

    let scanned2 = rom_core::rom_scanner::scan_roms_directory(&mut conn, &roms_dir, None)
        .await
        .expect("second scan");
    assert_eq!(scanned2.len(), 1);

    let v2: i64 = conn.query_row("SELECT COUNT(*) FROM game_variants", [], |r| r.get(0)).unwrap();
    let f2: i64 = conn.query_row("SELECT COUNT(*) FROM game_files", [], |r| r.get(0)).unwrap();
    let g2: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0)).unwrap();
    assert_eq!(v2, 1);
    assert_eq!(f2, 1);
    assert_eq!(g2, 1);

    let remaining_game_id = &scanned2[0].id;
    let del_res = rom_core::rom_scanner::delete_installed_game(&mut conn, remaining_game_id, None);
    assert!(del_res.is_ok());

    let v3: i64 = conn.query_row("SELECT COUNT(*) FROM game_variants", [], |r| r.get(0)).unwrap();
    let f3: i64 = conn.query_row("SELECT COUNT(*) FROM game_files", [], |r| r.get(0)).unwrap();
    let g3: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0)).unwrap();
    assert_eq!(v3, 0);
    assert_eq!(f3, 0);
    assert_eq!(g3, 0);
}

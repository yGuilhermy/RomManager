use std::fs;
use std::path::Path;

#[test]
fn test_catalog_query() {
    let res = rom_core::api_roms::query_catalog(Some("God of War"), None, None, None, None, 1, 10);
    assert!(res.total > 0);
    assert!(res.items.iter().any(|i| i.title.contains("God of War")));

    let res_usa =
        rom_core::api_roms::query_catalog(Some("God of War"), Some("USA"), None, None, None, 1, 10);
    assert!(res_usa.total > 0);
    assert!(
        res_usa
            .items
            .iter()
            .all(|i| i.region.eq_ignore_ascii_case("USA"))
    );
}

#[tokio::test]
async fn test_rom_scanning_and_db() {
    let test_dir = Path::new("target/test_rom_scan");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).unwrap();

    let db_path = test_dir.join("test.db");
    let roms_dir = test_dir.join("Roms");
    let covers_dir = test_dir.join("covers");
    fs::create_dir_all(&roms_dir).unwrap();
    fs::create_dir_all(&covers_dir).unwrap();

    fs::write(
        roms_dir.join("God of War II (USA).chd"),
        "dummy chd content",
    )
    .unwrap();
    fs::write(
        roms_dir.join("Gregory Horror Show (Europe).iso"),
        "dummy iso content",
    )
    .unwrap();
    fs::write(roms_dir.join("random_text.txt"), "ignore this").unwrap();

    let mut conn = rom_core::database::initialize_db(&db_path).unwrap();

    let scanned = rom_core::rom_scanner::scan_roms_directory(&mut conn, &roms_dir, None)
        .await
        .unwrap();
    assert_eq!(scanned.len(), 2);

    let listed = rom_core::rom_scanner::list_installed_games(&conn).unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().any(|g| g.title.contains("God of War II")));
    assert!(
        listed
            .iter()
            .any(|g| g.title.contains("Gregory Horror Show"))
    );

    drop(conn);
    if test_dir.exists() {
        fs::remove_dir_all(test_dir).unwrap();
    }
}

#[test]
fn download_payload_alias_and_extreme_pagination() {
    let dto: rom_core::StartDownloadPayload =
        serde_json::from_str(r#"{"title":"Game","url":"https://example.invalid/game.chd"}"#)
            .unwrap();
    assert_eq!(dto.title, "Game");
    let legacy: rom_core::StartDownloadPayload =
        serde_json::from_str(r#"{"game_title":"Game","url":"https://example.invalid/game.chd"}"#)
            .unwrap();
    assert_eq!(legacy.title, "Game");
    let page =
        rom_core::api_roms::query_catalog(None, None, None, None, None, usize::MAX, usize::MAX);
    assert!(page.items.is_empty());
    assert!(page.per_page <= 100);
}

#[tokio::test]
async fn scanning_one_root_preserves_other_roots_and_delete_failure_keeps_record() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "scan-roots-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("AB1234567890123456CD.iso"), b"ROM").unwrap();
    fs::write(first.join("CB1234567890123456AD.iso"), b"ROM").unwrap();
    fs::write(second.join("Other.iso"), b"ROM").unwrap();
    let mut conn = rom_core::database::initialize_db(&root.join("test.db")).unwrap();
    let games = rom_core::rom_scanner::scan_roms_directory(&mut conn, &first, None)
        .await
        .unwrap();
    assert_eq!(games.len(), 2);
    assert_ne!(games[0].id, games[1].id);
    rom_core::rom_scanner::scan_roms_directory(&mut conn, &second, None)
        .await
        .unwrap();
    assert_eq!(
        rom_core::rom_scanner::list_installed_games(&conn)
            .unwrap()
            .len(),
        3
    );
    fs::remove_file(&games[0].file_path).unwrap();
    fs::create_dir(&games[0].file_path).unwrap();
    assert!(rom_core::rom_scanner::delete_installed_game(&mut conn, &games[0].id, None).is_err());
    assert_eq!(
        rom_core::rom_scanner::list_installed_games(&conn)
            .unwrap()
            .len(),
        3
    );
    drop(conn);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn windows_path_identity_ignores_extended_prefix_case_and_separators() {
    use rom_core::rom_scanner::normalized_path_key;
    assert_eq!(
        normalized_path_key(Path::new(r"D:\ROMs\Game.iso")),
        normalized_path_key(Path::new(r"\\?\d:\roms\game.iso"))
    );
    assert_eq!(
        normalized_path_key(Path::new(r"D:/ROMs/Game.iso")),
        normalized_path_key(Path::new(r"D:\ROMs\Game.iso"))
    );
    assert_eq!(
        normalized_path_key(Path::new(r"\\server\share\Game.iso")),
        normalized_path_key(Path::new(r"\\?\UNC\SERVER\SHARE\game.iso"))
    );
    assert_ne!(
        normalized_path_key(Path::new(r"D:\ROMs\Game (MOD).iso")),
        normalized_path_key(Path::new(r"D:\ROMs\Game (USA).iso"))
    );
}

#[tokio::test]
async fn legacy_windows_duplicates_are_merged_and_repeated_scans_are_idempotent() {
    use rusqlite::{Connection, params};
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "scan-duplicates-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
    let roms = root.join("ROMs");
    fs::create_dir_all(&roms).unwrap();
    fs::write(roms.join("Game.iso"), b"ROM").unwrap();
    let canonical = fs::canonicalize(roms.join("Game.iso")).unwrap();
    let extended = canonical.to_string_lossy().to_string();
    let regular = if cfg!(windows) {
        extended.strip_prefix(r"\\?\").unwrap().to_string()
    } else {
        roms.join(".")
            .join("Game.iso")
            .to_string_lossy()
            .to_string()
    };
    let db = root.join("test.db");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE games(id TEXT PRIMARY KEY,title TEXT NOT NULL,filename TEXT NOT NULL,file_path TEXT NOT NULL,file_size INTEGER NOT NULL,format TEXT NOT NULL,cover_url TEXT,region TEXT,serial TEXT,date_added TEXT NOT NULL); CREATE TABLE game_progress(game_id TEXT PRIMARY KEY,status TEXT NOT NULL,notes TEXT NOT NULL,launches INTEGER NOT NULL,last_played TEXT); CREATE TABLE favorites(id TEXT PRIMARY KEY,title TEXT NOT NULL,cover_url TEXT,url TEXT NOT NULL,size INTEGER,region TEXT,added_at TEXT NOT NULL);").unwrap();
    for (id, path, date, cover) in [
        ("legacy", &regular, "2025-01-01T00:00:00Z", None),
        (
            "extended",
            &extended,
            "2026-01-01T00:00:00Z",
            Some("https://example.invalid/cover.png"),
        ),
    ] {
        conn.execute(
            "INSERT INTO games VALUES(?1,'Game','Game.iso',?2,3,'iso',?3,'USA','SLUS-00001',?4)",
            params![id, path, cover, date],
        )
        .unwrap();
    }
    conn.execute("INSERT INTO game_progress VALUES('legacy','planned','First note',2,'2026-01-01T00:00:00Z')",[]).unwrap();
    conn.execute("INSERT INTO game_progress VALUES('extended','playing','Second note',3,'2026-02-01T00:00:00Z')",[]).unwrap();
    conn.execute(
        "INSERT INTO favorites VALUES('extended','Game',NULL,'',NULL,NULL,'2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    drop(conn);
    let mut conn = rom_core::database::initialize_db(&db).unwrap();
    assert_eq!(
        rom_core::rom_scanner::list_installed_games(&conn)
            .unwrap()
            .len(),
        1
    );
    let progress: (String, String, i64) = conn
        .query_row(
            "SELECT game_id,notes,launches FROM game_progress",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(progress.0, "legacy");
    assert!(progress.1.contains("First note") && progress.1.contains("Second note"));
    assert_eq!(progress.2, 5);
    assert_eq!(
        conn.query_row("SELECT id FROM favorites", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "legacy"
    );
    for _ in 0..3 {
        let scanned = rom_core::rom_scanner::scan_roms_directory(&mut conn, &roms, None)
            .await
            .unwrap();
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].id, "legacy");
        assert_eq!(scanned[0].date_added, "2025-01-01T00:00:00Z");
        assert_eq!(
            rom_core::rom_scanner::list_installed_games(&conn)
                .unwrap()
                .len(),
            1
        );
    }
    let key = rom_core::rom_scanner::normalized_path_key(&canonical);
    assert!(conn.execute("INSERT INTO games(id,title,filename,file_path,file_size,format,date_added,path_key) VALUES('another','Game','Game.iso',?1,3,'iso','2026-01-01',?2)",params![regular,key]).is_err());
    assert_eq!(
        conn.query_row(
            "SELECT launches FROM game_progress WHERE game_id='legacy'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        5
    );
    drop(conn);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn merging_long_notes_keeps_a_valid_profile_and_archives_the_complete_notes() {
    use rusqlite::params;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "long-notes-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
    fs::create_dir_all(&root).unwrap();
    let mut conn = rom_core::database::initialize_db(&root.join("test.db")).unwrap();
    for (id, path) in [
        ("old", r"D:\ROMs\Game.iso"),
        ("new", r"\\?\D:\ROMs\Game.iso"),
    ] {
        conn.execute("INSERT INTO games(id,title,filename,file_path,file_size,format,date_added) VALUES(?1,'Game','Game.iso',?2,1,'iso',?3)",params![id,path,if id=="old"{"2025-01-01"}else{"2026-01-01"}]).unwrap();
        conn.execute(
            "INSERT INTO game_progress VALUES(?1,'playing',?2,1,NULL)",
            params![id, id.repeat(600)],
        )
        .unwrap();
    }
    assert_eq!(
        rom_core::rom_scanner::normalize_library(&mut conn).unwrap(),
        1
    );
    let notes: String = conn
        .query_row("SELECT notes FROM game_progress", [], |r| r.get(0))
        .unwrap();
    assert_eq!(notes.chars().count(), 2000);
    let archived: String = conn
        .query_row(
            "SELECT details FROM history WHERE action='merge_notes'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(archived.contains(&"old".repeat(600)));
    assert!(archived.contains(&"new".repeat(600)));
    drop(conn);
    fs::remove_dir_all(root).unwrap();
}

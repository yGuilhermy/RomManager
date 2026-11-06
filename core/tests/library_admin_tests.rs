use rom_core::database::{add_history, initialize_db};
use rom_core::rom_scanner::{delete_installed_game, list_installed_games, scan_roms_directory, verify_game_integrity};
use rusqlite::params;
use std::fs;
use std::path::Path;

#[tokio::test]
async fn multidisc_and_variants_coexist_without_collision() {
    let test_dir = Path::new("target/test_lib_multidisc");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Failed to create test dir");
    let db_path = test_dir.join("library.db");
    let mut conn = initialize_db(&db_path).expect("Failed to initialize database");
    let roms_dir = test_dir.join("roms");
    fs::create_dir_all(&roms_dir).expect("Failed to create roms directory");

    let disc1 = roms_dir.join("Final Fantasy X (Disc 1).chd");
    let disc2 = roms_dir.join("Final Fantasy X (Disc 2).chd");
    fs::write(&disc1, b"CHD_DISC_1_DATA").expect("Write disc 1");
    fs::write(&disc2, b"CHD_DISC_2_DATA").expect("Write disc 2");

    let scanned = scan_roms_directory(&mut conn, &roms_dir, None).await.expect("Scan failed");
    assert_eq!(scanned.len(), 2);

    let installed = list_installed_games(&conn).expect("List installed failed");
    assert_eq!(installed.len(), 2);

    let titles: Vec<String> = installed.iter().map(|g| g.title.clone()).collect();
    assert!(titles.contains(&"Final Fantasy X (Disc 1)".to_string()));
    assert!(titles.contains(&"Final Fantasy X (Disc 2)".to_string()));

    let rescan = scan_roms_directory(&mut conn, &roms_dir, None).await.expect("Rescan failed");
    assert_eq!(rescan.len(), 2);
    let after_rescan = list_installed_games(&conn).expect("List installed after rescan failed");
    assert_eq!(after_rescan.len(), 2);
}

#[tokio::test]
async fn missing_rom_file_is_detected_and_integrity_reports_unhealthy() {
    let test_dir = Path::new("target/test_lib_missing");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Failed to create test dir");
    let db_path = test_dir.join("missing.db");
    let mut conn = initialize_db(&db_path).expect("Failed to initialize database");
    let roms_dir = test_dir.join("roms");
    fs::create_dir_all(&roms_dir).expect("Failed to create roms directory");

    let rom_path = roms_dir.join("Gran Turismo 4 (USA).iso");
    fs::write(&rom_path, b"VALID_ISO_DATA_HEADER").expect("Write rom");

    scan_roms_directory(&mut conn, &roms_dir, None).await.expect("Scan failed");

    let list_before = list_installed_games(&conn).expect("List failed");
    assert_eq!(list_before.len(), 1);
    assert!(!list_before[0].is_missing);

    let integrity_before = verify_game_integrity(&conn, &list_before[0].id).expect("Integrity check failed");
    assert!(integrity_before.exists);
    assert!(integrity_before.healthy);

    fs::remove_file(&rom_path).expect("Remove file");

    let list_after = list_installed_games(&conn).expect("List after delete failed");
    assert_eq!(list_after.len(), 1);
    assert!(list_after[0].is_missing);

    let integrity_after = verify_game_integrity(&conn, &list_after[0].id).expect("Integrity check 2 failed");
    assert!(!integrity_after.exists);
    assert!(!integrity_after.healthy);
}

#[tokio::test]
async fn favorites_and_history_preserved_across_library_administration() {
    let test_dir = Path::new("target/test_lib_favorites");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Failed to create test dir");
    let db_path = test_dir.join("fav.db");
    let mut conn = initialize_db(&db_path).expect("Failed to initialize database");
    let roms_dir = test_dir.join("roms");
    fs::create_dir_all(&roms_dir).expect("Failed to create roms directory");

    let rom_path = roms_dir.join("Tekken 5 (USA).chd");
    fs::write(&rom_path, b"MOCK_TEKKEN_DATA").expect("Write rom");

    let scanned = scan_roms_directory(&mut conn, &roms_dir, None).await.expect("Scan failed");
    assert_eq!(scanned.len(), 1);
    let game_id = &scanned[0].id;

    conn.execute(
        "INSERT INTO favorites (id, title, url, added_at) VALUES (?1, ?2, ?3, '2026-11-05T00:00:00Z')",
        params![game_id, "Tekken 5 (USA)", "https://example.com/t5"],
    ).expect("Insert favorite failed");
    add_history(&conn, "Tekken 5 (USA)", "launch", Some("Iniciado no PCSX2"))
        .expect("Add history failed");

    let fav_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM favorites WHERE id = ?1", params![game_id], |r| r.get(0))
        .expect("Query favorite failed");
    assert_eq!(fav_count, 1);

    scan_roms_directory(&mut conn, &roms_dir, None).await.expect("Rescan failed");
    let fav_count_after_scan: i64 = conn
        .query_row("SELECT COUNT(*) FROM favorites WHERE id = ?1", params![game_id], |r| r.get(0))
        .expect("Query favorite after scan failed");
    assert_eq!(fav_count_after_scan, 1);

    delete_installed_game(&mut conn, game_id, None).expect("Delete installed game failed");
    let remaining_games = list_installed_games(&conn).expect("List failed");
    assert_eq!(remaining_games.len(), 0);

    let fav_count_preserved: i64 = conn
        .query_row("SELECT COUNT(*) FROM favorites WHERE id = ?1", params![game_id], |r| r.get(0))
        .expect("Query favorite preserved failed");
    assert_eq!(fav_count_preserved, 1);
}


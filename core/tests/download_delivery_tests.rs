use rom_core::database::{add_history, initialize_db};
use rom_core::download_manager::DownloadManager;
use rom_core::rom_scanner::{list_installed_games, scan_roms_directory};
use rusqlite::params;
use std::fs;
use std::path::Path;

#[tokio::test]
async fn download_interruption_recovers_state_for_clean_retry() {
    let test_dir = Path::new("target/test_dl_interrupt");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Failed to create test dir");
    let db_path = test_dir.join("test.db");
    let conn = initialize_db(&db_path).expect("Failed to initialize database");

    conn.execute(
        "INSERT INTO downloads (id, game_title, url, status, progress, speed, size, downloaded_bytes)
         VALUES ('dl_interrupt_1', 'Shadow of the Colossus', 'https://example.com/sotc.chd', 'Downloading', 45.0, 1024.0, 10000, 4500)",
        [],
    ).expect("Failed to insert mock download");
    conn.execute(
        "INSERT INTO downloads (id, game_title, url, status, progress, speed, size, downloaded_bytes)
         VALUES ('dl_queued_1', 'Ico', 'https://example.com/ico.chd', 'Queued', 0.0, 0.0, 8000, 0)",
        [],
    ).expect("Failed to insert mock queued download");
    drop(conn);

    let mgr = DownloadManager::new(db_path.clone());
    let conn_after = rusqlite::Connection::open(&db_path).expect("Failed to open db");
    let (status1, err1): (String, Option<String>) = conn_after
        .query_row(
            "SELECT status, error_message FROM downloads WHERE id = 'dl_interrupt_1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("Query failed");
    assert_eq!(status1, "Error");
    assert!(err1.unwrap_or_default().contains("interrompido"));

    let (status2, _): (String, Option<String>) = conn_after
        .query_row(
            "SELECT status, error_message FROM downloads WHERE id = 'dl_queued_1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("Query failed");
    assert_eq!(status2, "Error");
    drop(mgr);
}

#[tokio::test]
async fn download_delivery_to_library_is_idempotent_without_duplicates() {
    let test_dir = Path::new("target/test_dl_delivery");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Failed to create test dir");
    let db_path = test_dir.join("delivery.db");
    let mut conn = initialize_db(&db_path).expect("Failed to initialize database");
    let roms_dir = test_dir.join("roms");
    fs::create_dir_all(&roms_dir).expect("Failed to create roms directory");

    let rom_file = roms_dir.join("Silent Hill 2 (USA).chd");
    fs::write(&rom_file, b"MOCK_CHD_CONTENT_FOR_SH2").expect("Failed to write mock rom");

    conn.execute(
        "INSERT INTO downloads (id, game_title, url, status, progress, speed, size, downloaded_bytes, target_path)
         VALUES ('dl_sh2', 'Silent Hill 2 (USA)', 'https://example.com/sh2.chd', 'Completed', 100.0, 0.0, 24, 24, ?1)",
        params![rom_file.to_string_lossy().to_string()],
    ).expect("Failed to insert download");
    add_history(&conn, "Silent Hill 2 (USA)", "download", Some("Download concluído com sucesso"))
        .expect("Failed to record history");

    let first_scan = scan_roms_directory(&mut conn, &roms_dir, None).await.expect("Scan 1 failed");
    assert_eq!(first_scan.len(), 1);

    let second_scan = scan_roms_directory(&mut conn, &roms_dir, None).await.expect("Scan 2 failed");
    assert_eq!(second_scan.len(), 1);

    let installed = list_installed_games(&conn).expect("Failed to list installed");
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].title, "Silent Hill 2 (USA)");

    let file_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM game_files WHERE game_id = ?1", params![installed[0].id], |r| r.get(0))
        .expect("Failed to count files");
    assert_eq!(file_rows, 1);
}

#[tokio::test]
async fn download_cancellation_and_removal_cleans_up_records() {
    let test_dir = Path::new("target/test_dl_cancel");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Failed to create test dir");
    let db_path = test_dir.join("cancel.db");
    let init_conn = initialize_db(&db_path).expect("Failed to initialize database");
    drop(init_conn);

    let mut mgr = DownloadManager::new(db_path.clone());
    let conn = rusqlite::Connection::open(&db_path).expect("Failed to open db");
    conn.execute(
        "INSERT INTO downloads (id, game_title, url, status, progress, speed, size, downloaded_bytes)
         VALUES ('dl_cancel_1', 'Okami', 'https://example.com/okami.chd', 'Queued', 0.0, 0.0, 5000, 0)",
        [],
    ).expect("Failed to insert mock queued download");
    drop(conn);

    mgr.cancel_download("dl_cancel_1").expect("Cancel failed");

    let conn_after = rusqlite::Connection::open(&db_path).expect("Failed to open db");
    let status: String = conn_after
        .query_row("SELECT status FROM downloads WHERE id = 'dl_cancel_1'", [], |r| r.get(0))
        .expect("Query failed");
    assert_eq!(status, "Cancelled");

    mgr.delete_download("dl_cancel_1").expect("Delete failed");
    let count: i64 = conn_after
        .query_row("SELECT COUNT(*) FROM downloads WHERE id = 'dl_cancel_1'", [], |r| r.get(0))
        .expect("Query failed");
    assert_eq!(count, 0);
}


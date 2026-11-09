#[cfg(windows)]
#[tokio::test]
async fn console_http_lifecycle_keeps_source_and_cache_separate() {
    use rom_core::{AppState, console::api, covers, database, download_manager, jobs};
    use std::{
        fs,
        sync::{Arc, Mutex},
    };
    let root = std::env::temp_dir().join(format!(
        "rm-console-api-{}",
        rom_core::memory_cards::stamp()
    ));
    fs::create_dir_all(root.join("usb/DVD")).unwrap();
    fs::write(root.join("usb/DVD/Fixture.iso"), b"synthetic invalid ISO").unwrap();
    let db = root.join("local.db");
    drop(database::initialize_db(&db).unwrap());
    let state = Arc::new(AppState {
        db_path: db.clone(),
        download_manager: Arc::new(Mutex::new(download_manager::DownloadManager::new(
            db.clone(),
        ))),
        logs: Arc::new(Mutex::new(vec![])),
        cover_job: Arc::new(Mutex::new(covers::CoverJob::default())),
        job_manager: Arc::new(jobs::JobManager::new(db)),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/console", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, api::router().with_state(state))
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    let volumes = client.get(format!("{base}/volumes")).send().await.unwrap();
    assert!(volumes.status().is_success());
    assert!(
        !volumes
            .json::<Vec<serde_json::Value>>()
            .await
            .unwrap()
            .is_empty()
    );
    let invalid=client.post(format!("{base}/targets")).json(&serde_json::json!({"path":root.join("usb"),"profile":"automatic_unknown_capabilities"})).send().await.unwrap();
    assert_eq!(invalid.status().as_u16(), 422);
    let target: serde_json::Value = client
        .post(format!("{base}/targets"))
        .json(&serde_json::json!({"path":root.join("usb"),"profile":"unknown"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = target["id"].as_str().unwrap();
    let endpoint = format!("{base}/targets/{id}");
    let before: serde_json::Value = client
        .get(format!("{endpoint}/inventory"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(before["snapshot"].is_null());
    let fresh: serde_json::Value = client
        .post(format!("{endpoint}/refresh"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(fresh["snapshot"]["games"].as_array().unwrap().len(), 1);
    assert_eq!(fresh["cached"], false);
    client
        .post(format!("{endpoint}/profile"))
        .json(&serde_json::json!({"profile":"legacy_fat32"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let cached: serde_json::Value = client
        .get(format!("{endpoint}/inventory"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(cached["target"]["profile"], "legacy_fat32");
    assert_eq!(
        cached["snapshot"]["scanned_at"],
        fresh["snapshot"]["scanned_at"]
    );
    fs::rename(root.join("usb"), root.join("disconnected")).unwrap();
    assert_eq!(
        client
            .post(format!("{endpoint}/refresh"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        409
    );
    let offline: serde_json::Value = client
        .get(format!("{endpoint}/inventory"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(offline["online"], false);
    assert_eq!(offline["snapshot"]["games"], fresh["snapshot"]["games"]);
    client
        .delete(&endpoint)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(
        fs::read(root.join("disconnected/DVD/Fixture.iso")).unwrap(),
        b"synthetic invalid ISO"
    );
    server.abort();
    let _ = server.await;
    fs::remove_dir_all(root).unwrap();
}

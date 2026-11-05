use chrono::Utc;
use futures_util::StreamExt;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: String,
    pub game_title: String,
    pub url: String,
    pub cover_url: Option<String>,
    pub status: String,
    pub progress: f64,
    pub speed: f64,
    pub size: u64,
    pub downloaded_bytes: u64,
    pub error_message: Option<String>,
    pub target_path: Option<String>,
    pub connections: Option<usize>,
}

#[derive(Default)]
struct ChildTasks(Vec<tokio::task::AbortHandle>);
impl Drop for ChildTasks {
    fn drop(&mut self) {
        for handle in &self.0 {
            handle.abort();
        }
    }
}

static NEXT_DOWNLOAD_SEQ: AtomicU64 = AtomicU64::new(1);

pub struct DownloadManager {
    pub db_path: PathBuf,
    pub active_tasks: HashMap<String, tokio::task::JoinHandle<()>>,
    staging_dirs: HashMap<String, PathBuf>,
    pub progress_cache: Arc<std::sync::Mutex<HashMap<String, DownloadItem>>>,
    pub queue_paused: bool,
    last_roms_dir: Option<PathBuf>,
    last_covers_dir: Option<PathBuf>,
}

impl DownloadManager {
    pub fn new(db_path: PathBuf) -> Self {
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            let _ = conn.execute("UPDATE downloads SET status = 'Error', speed = 0, error_message = 'Download interrompido pelo encerramento do aplicativo. Tente novamente.' WHERE status IN ('Downloading', 'Extracting', 'Copying', 'Queued')", []);
        }
        Self {
            db_path,
            active_tasks: HashMap::new(),
            staging_dirs: HashMap::new(),
            progress_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
            queue_paused: false,
            last_roms_dir: None,
            last_covers_dir: None,
        }
    }

    pub fn max_concurrent_downloads(&self) -> usize {
        if let Ok(conn) = rusqlite::Connection::open(&self.db_path) {
            if let Some(val) = crate::database::get_config(&conn, "max_concurrent_downloads") {
                if let Ok(parsed) = val.trim().parse::<usize>() {
                    return parsed.clamp(1, 5);
                }
            }
        }
        2
    }

    fn resolve_dirs(&self) -> (PathBuf, Option<PathBuf>) {
        if let Some(ref r) = self.last_roms_dir {
            return (r.clone(), self.last_covers_dir.clone());
        }
        let (roms_dir_str, covers_dir_str) =
            if let Ok(conn) = rusqlite::Connection::open(&self.db_path) {
                (
                    crate::database::get_config(&conn, "roms_dir").unwrap_or_default(),
                    crate::database::get_config(&conn, "covers_dir").unwrap_or_default(),
                )
            } else {
                (String::new(), String::new())
            };
        let roms_dir = if !roms_dir_str.trim().is_empty() {
            PathBuf::from(&roms_dir_str)
        } else if let Ok(profile) = std::env::var("USERPROFILE") {
            let p = PathBuf::from(profile).join("Downloads").join("PS2_ROMs");
            let _ = fs::create_dir_all(&p);
            p
        } else {
            PathBuf::from("roms")
        };
        let covers_dir = if !covers_dir_str.trim().is_empty() {
            Some(PathBuf::from(&covers_dir_str))
        } else {
            None
        };
        (roms_dir, covers_dir)
    }

    fn spawn_task(
        &mut self,
        mut item: DownloadItem,
        roms_dir: PathBuf,
        covers_dir: Option<PathBuf>,
    ) {
        item.status = "Downloading".to_string();
        let download_id = item.id.clone();
        let title = item.game_title.clone();
        let url = item.url.clone();
        let cover_url = item.cover_url.clone();
        let provided_size = if item.size > 0 { Some(item.size) } else { None };
        {
            let mut cache = self.progress_cache.lock().unwrap();
            cache.insert(download_id.clone(), item);
        }
        self.staging_dirs.insert(
            download_id.clone(),
            roms_dir.join(".rommanager-downloads").join(&download_id),
        );
        let id_clone = download_id.clone();
        let db_path_clone = self.db_path.clone();
        let cache_clone = Arc::clone(&self.progress_cache);
        let handle = tokio::spawn(async move {
            run_download_task(
                id_clone,
                db_path_clone,
                cache_clone,
                title,
                url,
                cover_url,
                provided_size,
                roms_dir,
                covers_dir,
            )
            .await;
        });
        self.active_tasks.insert(download_id, handle);
    }

    pub fn process_queue(&mut self) {
        self.active_tasks.retain(|_, task| !task.is_finished());
        self.staging_dirs.retain(|id, path| {
            self.active_tasks.contains_key(id)
                || (path.exists() && fs::remove_dir_all(path).is_err())
        });
        self.progress_cache
            .lock()
            .unwrap()
            .retain(|id, _| self.active_tasks.contains_key(id));

        if self.queue_paused {
            return;
        }

        let limit = self.max_concurrent_downloads();
        while self.active_tasks.len() < limit {
            let next_item = {
                let Ok(conn) = rusqlite::Connection::open(&self.db_path) else {
                    break;
                };
                let Ok(item) = conn.query_row(
                    "SELECT id, game_title, url, cover_url, size FROM downloads WHERE status = 'Queued' ORDER BY rowid ASC LIMIT 1",
                    [],
                    |row| {
                        Ok(DownloadItem {
                            id: row.get(0)?,
                            game_title: row.get(1)?,
                            url: row.get(2)?,
                            cover_url: row.get(3)?,
                            status: "Downloading".to_string(),
                            progress: 0.0,
                            speed: 0.0,
                            size: row.get::<_, i64>(4)? as u64,
                            downloaded_bytes: 0,
                            error_message: None,
                            target_path: None,
                            connections: Some(1),
                        })
                    },
                ) else {
                    break;
                };
                let _ = conn.execute(
                    "UPDATE downloads SET status = 'Downloading', progress = 0, speed = 0 WHERE id = ?1",
                    params![item.id],
                );
                item
            };
            let (roms_dir, covers_dir) = self.resolve_dirs();
            let _ = fs::create_dir_all(&roms_dir);
            self.spawn_task(next_item, roms_dir, covers_dir);
        }
    }

    pub fn get_downloads(&mut self) -> Result<Vec<DownloadItem>, String> {
        self.process_queue();
        let mut list = Vec::new();
        let cache = self.progress_cache.lock().map_err(|e| e.to_string())?;
        {
            let conn = rusqlite::Connection::open(&self.db_path).map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare("SELECT id, game_title, url, cover_url, status, progress, speed, size, downloaded_bytes, error_message, target_path, connections FROM downloads ORDER BY rowid DESC")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(DownloadItem {
                        id: row.get(0)?,
                        game_title: row.get(1)?,
                        url: row.get(2)?,
                        cover_url: row.get(3)?,
                        status: row.get(4)?,
                        progress: row.get(5)?,
                        speed: row.get(6)?,
                        size: row.get::<_, i64>(7)? as u64,
                        downloaded_bytes: row.get::<_, i64>(8)? as u64,
                        error_message: row.get(9)?,
                        target_path: row.get(10)?,
                        connections: row.get::<_, Option<usize>>(11).unwrap_or(Some(1)),
                    })
                })
                .map_err(|e| e.to_string())?;

            for row in rows {
                let r = row.map_err(|e| e.to_string())?;
                if let Some(active) = cache.get(&r.id) {
                    list.push(active.clone());
                } else {
                    list.push(r);
                }
            }
        }
        Ok(list)
    }

    pub fn start_download(
        &mut self,
        title: String,
        url: String,
        cover_url: Option<String>,
        provided_size: Option<u64>,
        roms_dir: PathBuf,
        covers_dir: Option<PathBuf>,
    ) -> Result<String, String> {
        let parsed = reqwest::Url::parse(&url).map_err(|_| "URL de download inválida")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Protocolo não suportado".into());
        }
        if title.trim().is_empty() {
            return Err("Título obrigatório".into());
        }
        self.last_roms_dir = Some(roms_dir.clone());
        self.last_covers_dir = covers_dir.clone();
        if self.get_downloads()?.iter().any(|d| {
            d.url == url
                && matches!(
                    d.status.as_str(),
                    "Downloading" | "Extracting" | "Copying" | "Queued"
                )
        }) {
            return Err("Este download já está ativo".into());
        }
        if !roms_dir.exists() {
            fs::create_dir_all(&roms_dir).map_err(|e| e.to_string())?;
        }

        let seq = NEXT_DOWNLOAD_SEQ.fetch_add(1, Ordering::Relaxed);
        let download_id = format!(
            "{:x}-{:x}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0),
            seq
        );
        let can_start_now =
            !self.queue_paused && self.active_tasks.len() < self.max_concurrent_downloads();
        let initial_status = if can_start_now {
            "Downloading".to_string()
        } else {
            "Queued".to_string()
        };

        let item = DownloadItem {
            id: download_id.clone(),
            game_title: title,
            url,
            cover_url,
            status: initial_status,
            progress: 0.0,
            speed: 0.0,
            size: provided_size.unwrap_or(0),
            downloaded_bytes: 0,
            error_message: None,
            target_path: None,
            connections: Some(1),
        };

        {
            let conn = rusqlite::Connection::open(&self.db_path).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO downloads (id, game_title, url, cover_url, status, progress, speed, size, downloaded_bytes, error_message, target_path, connections)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    item.id,
                    item.game_title,
                    item.url,
                    item.cover_url,
                    item.status,
                    item.progress,
                    item.speed,
                    item.size as i64,
                    item.downloaded_bytes as i64,
                    item.error_message,
                    item.target_path,
                    item.connections,
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        if can_start_now {
            self.spawn_task(item, roms_dir, covers_dir);
        }

        Ok(download_id)
    }

    pub fn cancel_download(&mut self, id: &str) -> Result<(), String> {
        if self
            .progress_cache
            .lock()
            .unwrap()
            .get(id)
            .map(|d| matches!(d.status.as_str(), "Extracting" | "Copying"))
            .unwrap_or(false)
        {
            return Err("Aguarde a extração terminar antes de remover este download".into());
        }
        if let Some(handle) = self.active_tasks.remove(id) {
            handle.abort();
        }

        {
            let mut cache = self.progress_cache.lock().unwrap();
            if let Some(item) = cache.get_mut(id) {
                item.status = "Cancelled".to_string();
                item.speed = 0.0;
            }
        }

        let conn = rusqlite::Connection::open(&self.db_path).map_err(|e| e.to_string())?;
        conn.execute("UPDATE downloads SET status = 'Cancelled', speed = 0 WHERE id = ?1 AND status IN ('Downloading', 'Queued')", params![id]).map_err(|e| e.to_string())?;
        drop(conn);
        self.process_queue();
        Ok(())
    }

    pub fn pause_queue(&mut self) {
        self.queue_paused = true;
    }

    pub fn resume_queue(&mut self) {
        self.queue_paused = false;
        self.process_queue();
    }

    pub fn cancel_queued_downloads(&mut self) -> Result<usize, String> {
        let conn = rusqlite::Connection::open(&self.db_path).map_err(|e| e.to_string())?;
        let count = conn
            .execute(
                "UPDATE downloads SET status = 'Cancelled', speed = 0 WHERE status = 'Queued'",
                [],
            )
            .map_err(|e| e.to_string())?;
        Ok(count)
    }

    pub fn delete_download(&mut self, id: &str) -> Result<(), String> {
        self.cancel_download(id)?;
        {
            let mut cache = self.progress_cache.lock().unwrap();
            cache.remove(id);
        }

        let conn = rusqlite::Connection::open(&self.db_path).map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM downloads WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn retry_download(
        &mut self,
        id: &str,
        roms_dir: PathBuf,
        covers_dir: Option<PathBuf>,
    ) -> Result<(), String> {
        let (title, url, cover_url, size) = {
            if let Ok(conn) = rusqlite::Connection::open(&self.db_path) {
                conn.query_row(
                    "SELECT game_title, url, cover_url, size FROM downloads WHERE id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, i64>(3)? as u64,
                        ))
                    },
                )
                .map_err(|e| e.to_string())?
            } else {
                return Err("Falha ao abrir banco de dados".to_string());
            }
        };

        self.delete_download(id)?;
        let _ = self.start_download(title, url, cover_url, Some(size), roms_dir, covers_dir)?;
        Ok(())
    }
}

async fn run_download_task(
    id: String,
    db_path: PathBuf,
    cache: Arc<std::sync::Mutex<HashMap<String, DownloadItem>>>,
    title: String,
    url: String,
    cover_url: Option<String>,
    provided_size: Option<u64>,
    roms_dir: PathBuf,
    covers_dir: Option<PathBuf>,
) {
    let mut children = ChildTasks::default();
    update_both_status(
        &db_path,
        &cache,
        &id,
        "Downloading",
        0.0,
        0.0,
        provided_size.unwrap_or(0),
        0,
        None,
        None,
        Some(1),
    );

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(60))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            update_both_status(
                &db_path,
                &cache,
                &id,
                "Error",
                0.0,
                0.0,
                0,
                0,
                Some(&e.to_string()),
                None,
                None,
            );
            return;
        }
    };

    let mut probe_req = client.get(&url).header("Range", "bytes=0-0");
    if url.contains("cdromance") {
        probe_req = probe_req.header("Referer", "https://cdromance.org/");
    }

    let probe_resp = probe_req.send().await;
    let mut response_filename = None;
    let mut resolved_url = url.clone();
    let mut is_range_supported = false;
    let mut total_size: u64 = provided_size.unwrap_or(0);

    if let Ok(resp) = probe_resp {
        resolved_url = resp.url().to_string();
        response_filename = resp
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .and_then(disposition_filename);
        if resp.status().as_u16() == 206 {
            is_range_supported = true;
            if let Some(cr) = resp.headers().get("content-range") {
                if let Ok(cr_str) = cr.to_str() {
                    if let Some(slash_pos) = cr_str.rfind('/') {
                        if let Ok(parsed_len) = cr_str[slash_pos + 1..].trim().parse::<u64>() {
                            total_size = parsed_len;
                        }
                    }
                }
            }
        } else if resp.status().is_success() {
            if let Some(len) = resp.content_length() {
                total_size = len;
            }
        }
    }

    if total_size == 0 {
        if let Ok(head_resp) = client.head(&url).send().await {
            if let Some(len) = head_resp.content_length() {
                total_size = len;
            }
        }
    }

    let inferred_name = response_filename
        .unwrap_or_else(|| derive_filename_from_url_or_title(&resolved_url, &title));
    let final_dest = roms_dir.join(&inferred_name);
    let work_dir = roms_dir.join(".rommanager-downloads").join(&id);
    if let Err(e) = fs::create_dir_all(&work_dir) {
        update_both_status(
            &db_path,
            &cache,
            &id,
            "Error",
            0.0,
            0.0,
            total_size,
            0,
            Some(&e.to_string()),
            None,
            Some(1),
        );
        return;
    }
    if final_dest.exists() {
        update_both_status(
            &db_path,
            &cache,
            &id,
            "Error",
            0.0,
            0.0,
            total_size,
            0,
            Some("O arquivo de destino já existe; não foi sobrescrito"),
            None,
            Some(1),
        );
        return;
    }
    let part_dest = work_dir.join(format!("{}.part", inferred_name));

    let can_use_multi = is_range_supported && total_size >= 10_000_000;

    let download_success = if can_use_multi {
        let num_workers: usize = if total_size >= 500_000_000 { 6 } else { 4 };
        update_both_status(
            &db_path,
            &cache,
            &id,
            "Downloading",
            0.0,
            0.0,
            total_size,
            0,
            None,
            None,
            Some(num_workers),
        );

        let chunk_size = total_size / (num_workers as u64);
        let mut ranges = Vec::new();
        for i in 0..num_workers {
            let start = (i as u64) * chunk_size;
            let end = if i == num_workers - 1 {
                total_size - 1
            } else {
                ((i + 1) as u64) * chunk_size - 1
            };
            ranges.push((start, end));
        }

        let downloaded_atomic = Arc::new(AtomicU64::new(0));
        let is_finished = Arc::new(AtomicBool::new(false));

        let monitor_id = id.clone();
        let monitor_db = db_path.clone();
        let monitor_cache = Arc::clone(&cache);
        let monitor_downloaded = Arc::clone(&downloaded_atomic);
        let monitor_finished = Arc::clone(&is_finished);

        let monitor_handle = tokio::spawn(async move {
            let mut last_instant = Instant::now();
            let mut last_bytes = 0u64;
            let mut db_tick = 0u32;
            while !monitor_finished.load(Ordering::Relaxed) {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                let current_bytes = monitor_downloaded.load(Ordering::Relaxed);
                let elapsed = last_instant.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    (current_bytes.saturating_sub(last_bytes) as f64) / elapsed
                } else {
                    0.0
                };
                last_bytes = current_bytes;
                last_instant = Instant::now();

                let progress = if total_size > 0 {
                    ((current_bytes as f64 / total_size as f64) * 100.0).min(99.9)
                } else {
                    0.0
                };

                {
                    let mut c = monitor_cache.lock().unwrap();
                    if let Some(item) = c.get_mut(&monitor_id) {
                        if item.status == "Cancelled" {
                            break;
                        }
                        item.progress = progress;
                        item.speed = speed;
                        item.downloaded_bytes = current_bytes;
                        item.size = total_size;
                        item.connections = Some(num_workers);
                        item.status = "Downloading".to_string();
                    }
                }

                db_tick += 1;
                if db_tick % 4 == 0 {
                    update_db_status(
                        &monitor_db,
                        &monitor_id,
                        "Downloading",
                        progress,
                        speed,
                        total_size,
                        current_bytes,
                        None,
                        None,
                        Some(num_workers),
                    );
                }
            }
        });

        children.0.push(monitor_handle.abort_handle());
        let mut worker_handles = Vec::new();
        for (i, (start, end)) in ranges.into_iter().enumerate() {
            let worker_url = url.clone();
            let worker_client = client.clone();
            let chunk_path = work_dir.join(format!("{}.part.{}", inferred_name, i));
            let worker_downloaded = Arc::clone(&downloaded_atomic);

            let handle = tokio::spawn(async move {
                download_chunk_range(
                    worker_client,
                    worker_url,
                    start,
                    end,
                    chunk_path,
                    worker_downloaded,
                )
                .await
            });
            children.0.push(handle.abort_handle());
            worker_handles.push(handle);
        }

        let mut worker_error = None;
        for h in worker_handles {
            match h.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    worker_error = Some(e);
                }
                Err(e) => {
                    worker_error = Some(e.to_string());
                }
            }
        }

        is_finished.store(true, Ordering::Relaxed);
        let _ = monitor_handle.await;

        if let Some(err) = worker_error {
            for i in 0..num_workers {
                let p = work_dir.join(format!("{}.part.{}", inferred_name, i));
                let _ = fs::remove_file(p);
            }
            update_both_status(
                &db_path,
                &cache,
                &id,
                "Error",
                0.0,
                0.0,
                total_size,
                downloaded_atomic.load(Ordering::Relaxed),
                Some(&err),
                None,
                Some(num_workers),
            );
            false
        } else {
            let merge_res = tokio::task::spawn_blocking({
                let work_dir = work_dir.clone();
                let inferred_name = inferred_name.clone();
                let part_dest = part_dest.clone();
                move || -> Result<(), std::io::Error> {
                    let mut out = File::create(&part_dest)?;
                    for i in 0..num_workers {
                        let chunk_path = work_dir.join(format!("{}.part.{}", inferred_name, i));
                        let mut chunk_file = File::open(&chunk_path)?;
                        std::io::copy(&mut chunk_file, &mut out)?;
                        drop(chunk_file);
                        let _ = fs::remove_file(&chunk_path);
                    }
                    out.flush()?;
                    Ok(())
                }
            })
            .await;

            match merge_res {
                Ok(Ok(())) => true,
                Ok(Err(e)) => {
                    let _ = fs::remove_file(&part_dest);
                    update_both_status(
                        &db_path,
                        &cache,
                        &id,
                        "Error",
                        0.0,
                        0.0,
                        total_size,
                        total_size,
                        Some(&format!("Falha ao unir arquivos: {}", e)),
                        None,
                        Some(num_workers),
                    );
                    false
                }
                Err(e) => {
                    let _ = fs::remove_file(&part_dest);
                    update_both_status(
                        &db_path,
                        &cache,
                        &id,
                        "Error",
                        0.0,
                        0.0,
                        total_size,
                        total_size,
                        Some(&format!("Erro interno: {}", e)),
                        None,
                        Some(num_workers),
                    );
                    false
                }
            }
        }
    } else {
        update_both_status(
            &db_path,
            &cache,
            &id,
            "Downloading",
            0.0,
            0.0,
            total_size,
            0,
            None,
            None,
            Some(1),
        );

        let mut req = client.get(&url);
        if url.contains("cdromance") {
            req = req.header("Referer", "https://cdromance.org/");
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                update_both_status(
                    &db_path,
                    &cache,
                    &id,
                    "Error",
                    0.0,
                    0.0,
                    total_size,
                    0,
                    Some(&e.to_string()),
                    None,
                    Some(1),
                );
                return;
            }
        };

        if !resp.status().is_success() {
            let err = format!("Servidor retornou status HTTP: {}", resp.status());
            update_both_status(
                &db_path,
                &cache,
                &id,
                "Error",
                0.0,
                0.0,
                total_size,
                0,
                Some(&err),
                None,
                Some(1),
            );
            return;
        }

        let file = File::create(&part_dest);
        let mut file = match file {
            Ok(f) => f,
            Err(e) => {
                update_both_status(
                    &db_path,
                    &cache,
                    &id,
                    "Error",
                    0.0,
                    0.0,
                    total_size,
                    0,
                    Some(&e.to_string()),
                    None,
                    Some(1),
                );
                return;
            }
        };

        let mut stream = resp.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut last_update = Instant::now();
        let mut last_downloaded: u64 = 0;
        let mut failed = false;

        while let Some(chunk_res) = stream.next().await {
            let chunk = match chunk_res {
                Ok(c) => c,
                Err(e) => {
                    let _ = fs::remove_file(&part_dest);
                    update_both_status(
                        &db_path,
                        &cache,
                        &id,
                        "Error",
                        0.0,
                        0.0,
                        total_size,
                        downloaded,
                        Some(&e.to_string()),
                        None,
                        Some(1),
                    );
                    failed = true;
                    break;
                }
            };

            if let Err(e) = file.write_all(&chunk) {
                let _ = fs::remove_file(&part_dest);
                update_both_status(
                    &db_path,
                    &cache,
                    &id,
                    "Error",
                    0.0,
                    0.0,
                    total_size,
                    downloaded,
                    Some(&e.to_string()),
                    None,
                    Some(1),
                );
                failed = true;
                break;
            }

            downloaded += chunk.len() as u64;

            if last_update.elapsed().as_millis() >= 250 {
                let progress = if total_size > 0 {
                    ((downloaded as f64 / total_size as f64) * 100.0).min(99.9)
                } else {
                    0.0
                };

                let delta_time = last_update.elapsed().as_secs_f64();
                let delta_bytes = downloaded.saturating_sub(last_downloaded);
                let speed = if delta_time > 0.0 {
                    (delta_bytes as f64) / delta_time
                } else {
                    0.0
                };

                update_both_status(
                    &db_path,
                    &cache,
                    &id,
                    "Downloading",
                    progress,
                    speed,
                    total_size,
                    downloaded,
                    None,
                    None,
                    Some(1),
                );

                last_update = Instant::now();
                last_downloaded = downloaded;
            }
        }

        if let Err(e) = file.flush() {
            update_both_status(
                &db_path,
                &cache,
                &id,
                "Error",
                0.0,
                0.0,
                total_size,
                downloaded,
                Some(&e.to_string()),
                None,
                Some(1),
            );
            failed = true;
        }
        drop(file);
        if !failed && total_size > 0 && downloaded != total_size {
            update_both_status(
                &db_path,
                &cache,
                &id,
                "Error",
                0.0,
                0.0,
                total_size,
                downloaded,
                Some("Tamanho recebido diferente do esperado"),
                None,
                Some(1),
            );
            failed = true;
        }
        !failed
    };

    if !download_success {
        return;
    }

    let is_archive = inferred_name.ends_with(".7z")
        || inferred_name.ends_with(".zip")
        || inferred_name.ends_with(".rar");
    let mut final_installed_path = final_dest.clone();

    if is_archive {
        update_both_status(
            &db_path,
            &cache,
            &id,
            "Extracting",
            100.0,
            0.0,
            total_size,
            total_size,
            None,
            None,
            Some(1),
        );
        if cache
            .lock()
            .unwrap()
            .get(&id)
            .map(|d| d.status == "Cancelled")
            .unwrap_or(true)
        {
            return;
        }
        let extraction_dir = work_dir.join("extracted");
        let install_dir = roms_dir.join(crate::rom_scanner::safe_filename(&title));
        if install_dir.exists() {
            update_both_status(
                &db_path,
                &cache,
                &id,
                "Error",
                100.0,
                0.0,
                total_size,
                total_size,
                Some("A pasta de instalação já existe"),
                None,
                Some(1),
            );
            return;
        }
        let archive = part_dest.clone();
        let dest = extraction_dir.clone();
        let format = inferred_name.clone();
        let extracted =
            tokio::task::spawn_blocking(move || extract_archive(&archive, &dest, &format)).await;
        let result = match extracted {
            Ok(Ok(path)) => path
                .strip_prefix(&extraction_dir)
                .map(|p| p.to_path_buf())
                .map_err(|e| e.to_string())
                .and_then(|relative| {
                    fs::rename(&extraction_dir, &install_dir)
                        .map(|_| install_dir.join(relative))
                        .map_err(|e| e.to_string())
                }),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(e.to_string()),
        };
        match result {
            Ok(path) => final_installed_path = path,
            Err(e) => {
                update_both_status(
                    &db_path,
                    &cache,
                    &id,
                    "Error",
                    100.0,
                    0.0,
                    total_size,
                    total_size,
                    Some(&e),
                    None,
                    Some(1),
                );
                return;
            }
        }
    } else {
        update_both_status(
            &db_path,
            &cache,
            &id,
            "Copying",
            100.0,
            0.0,
            total_size,
            total_size,
            None,
            None,
            Some(1),
        );
        if cache
            .lock()
            .unwrap()
            .get(&id)
            .map(|d| d.status == "Cancelled")
            .unwrap_or(true)
        {
            return;
        }
        let _scan_guard = crate::rom_scanner::SCAN_LOCK.lock().await;
        let source = part_dest.clone();
        let destination = final_dest.clone();
        let result =
            tokio::task::spawn_blocking(move || copy_without_overwrite(&source, &destination))
                .await;
        if let Err(error) = result.map_err(|e| e.to_string()).and_then(|r| r) {
            update_both_status(
                &db_path,
                &cache,
                &id,
                "Error",
                100.0,
                0.0,
                total_size,
                total_size,
                Some(&error),
                None,
                Some(1),
            );
            return;
        }
    }
    let _ = fs::remove_dir_all(&work_dir);
    let scan_covers = covers_dir.clone();

    if let Some(curl) = cover_url {
        let ctarget_dir = covers_dir.unwrap_or_else(|| roms_dir.join("covers"));
        let _ = fs::create_dir_all(&ctarget_dir);
        let cover_dest =
            ctarget_dir.join(format!("{}.png", crate::rom_scanner::safe_filename(&title)));
        let _ = download_cover_art(&client, &curl, &cover_dest).await;
    }

    let installed_str = final_installed_path.to_string_lossy().to_string();
    update_both_status(
        &db_path,
        &cache,
        &id,
        "Completed",
        100.0,
        0.0,
        total_size,
        total_size,
        None,
        Some(&installed_str),
        Some(1),
    );

    if let Ok(mut conn) = rusqlite::Connection::open(&db_path) {
        let _ =
            crate::rom_scanner::scan_roms_directory(&mut conn, &roms_dir, scan_covers.as_deref())
                .await;
        let _ = crate::database::add_history(
            &conn,
            &title,
            "download",
            Some("Download concluído com sucesso"),
        );
    }
}

async fn download_chunk_range(
    client: reqwest::Client,
    url: String,
    start: u64,
    end: u64,
    chunk_path: PathBuf,
    downloaded_atomic: Arc<AtomicU64>,
) -> Result<(), String> {
    let expected_bytes = end - start + 1;
    let mut attempts = 0;

    while attempts < 3 {
        attempts += 1;
        let existing_len = fs::metadata(&chunk_path).map(|m| m.len()).unwrap_or(0);
        if existing_len == expected_bytes {
            return Ok(());
        }

        if existing_len > expected_bytes {
            return Err("Bloco parcial maior que o esperado".into());
        }
        let cur_start = start + existing_len;
        let mut req = client
            .get(&url)
            .header("Range", format!("bytes={}-{}", cur_start, end));
        if url.contains("cdromance") {
            req = req.header("Referer", "https://cdromance.org/");
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                continue;
            }
        };

        if resp.status().as_u16() != 206 {
            return Err("O servidor ignorou o intervalo solicitado; tente novamente".into());
        }
        let expected_range = format!("bytes {}-{}/", cur_start, end);
        if !resp
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.starts_with(&expected_range))
            .unwrap_or(false)
        {
            return Err("Content-Range não corresponde ao bloco solicitado".into());
        }

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&chunk_path);

        let mut file = match file {
            Ok(f) => f,
            Err(e) => return Err(e.to_string()),
        };

        let mut stream = resp.bytes_stream();
        let mut failed = false;

        while let Some(chunk_res) = stream.next().await {
            match chunk_res {
                Ok(chunk) => {
                    if file.metadata().map_err(|e| e.to_string())?.len() + chunk.len() as u64
                        > expected_bytes
                    {
                        return Err("Servidor enviou mais bytes que o intervalo solicitado".into());
                    }
                    if let Err(e) = file.write_all(&chunk) {
                        return Err(e.to_string());
                    }
                    downloaded_atomic.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                }
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }

        let _ = file.flush();
        drop(file);

        if !failed {
            let final_len = fs::metadata(&chunk_path).map(|m| m.len()).unwrap_or(0);
            if final_len == expected_bytes {
                return Ok(());
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    Err(format!("Falha ao baixar bloco {}-{}", start, end))
}

fn copy_without_overwrite(source: &Path, destination: &Path) -> Result<(), String> {
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|e| e.to_string())?;
    let result = std::io::copy(&mut input, &mut output).and_then(|_| output.sync_all());
    drop(output);
    if let Err(error) = result {
        let _ = fs::remove_file(destination);
        return Err(error.to_string());
    }
    Ok(())
}

fn disposition_filename(header: &str) -> Option<String> {
    let fields: Vec<&str> = header.split(';').map(str::trim).collect();
    let value = fields
        .iter()
        .find_map(|p| {
            p.strip_prefix("filename*=")
                .and_then(|v| v.split_once("''").map(|(_, name)| name))
        })
        .or_else(|| {
            fields
                .iter()
                .find_map(|p| p.strip_prefix("filename=").map(|v| v.trim_matches('"')))
        })?;
    let decoded = urlencoding::decode(value).ok()?;
    let extension = Path::new(decoded.as_ref())
        .extension()?
        .to_str()?
        .to_lowercase();
    if !matches!(
        extension.as_str(),
        "chd" | "iso" | "cso" | "bin" | "gz" | "zip" | "7z" | "rar"
    ) {
        return None;
    }
    Some(crate::rom_scanner::safe_filename(&decoded))
}

fn derive_filename_from_url_or_title(url: &str, title: &str) -> String {
    if let Ok(parsed) = reqwest::Url::parse(url) {
        if let Some(last) = parsed.path_segments().and_then(|mut p| p.next_back()) {
            if let Ok(decoded) = urlencoding::decode(last) {
                let lower = decoded.to_lowercase();
                if [".chd", ".iso", ".cso", ".bin", ".gz", ".zip", ".7z", ".rar"]
                    .iter()
                    .any(|ext| lower.ends_with(ext))
                {
                    return crate::rom_scanner::safe_filename(&decoded);
                }
            }
        }
    }
    format!("{}.chd", crate::rom_scanner::safe_filename(title))
}

fn archive_entry_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    let normalized = name.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        || normalized
            .split('/')
            .any(|part| !crate::rom_scanner::is_safe_filename(part))
    {
        return Err("Caminho inseguro no arquivo compactado".into());
    }
    Ok(root.join(path))
}

fn extract_archive(
    archive_path: &Path,
    out_dir: &Path,
    original_name: &str,
) -> Result<PathBuf, String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let format = original_name.to_lowercase();
    let mut budget = 64u64 * 1024 * 1024 * 1024;
    if format.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(File::open(archive_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let target = archive_entry_path(out_dir, entry.name().trim_end_matches('/'))?;
            if entry
                .unix_mode()
                .map(|mode| mode & 0o170000 == 0o120000)
                .unwrap_or(false)
            {
                return Err("Links não são permitidos no arquivo".into());
            }
            budget = budget
                .checked_sub(entry.size())
                .ok_or("Arquivo descompactado excede 64 GiB")?;
            if entry.is_dir() {
                fs::create_dir_all(target).map_err(|e| e.to_string())?;
            } else {
                fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                let mut out = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(target)
                    .map_err(|e| e.to_string())?;
                let expected = entry.size();
                let copied = std::io::copy(
                    &mut std::io::Read::take(&mut entry, expected.saturating_add(1)),
                    &mut out,
                )
                .map_err(|e| e.to_string())?;
                if copied != expected {
                    return Err("Tamanho da entrada ZIP não corresponde ao declarado".into());
                }
            }
        }
    } else if format.ends_with(".7z") {
        sevenz_rust::decompress_file_with_extract_fn(archive_path, out_dir, |entry, reader, _| {
            if entry.is_directory() && matches!(entry.name(), "" | "." | "./") {
                return Ok(true);
            }
            let target = archive_entry_path(out_dir, entry.name().trim_end_matches('/'))
                .map_err(sevenz_rust::Error::other)?;
            budget = budget
                .checked_sub(entry.size())
                .ok_or_else(|| sevenz_rust::Error::other("Arquivo descompactado excede 64 GiB"))?;
            if entry.is_directory() {
                fs::create_dir_all(target).map_err(sevenz_rust::Error::io)?;
            } else {
                fs::create_dir_all(target.parent().unwrap()).map_err(sevenz_rust::Error::io)?;
                let mut out = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(target)
                    .map_err(sevenz_rust::Error::io)?;
                let copied = std::io::copy(
                    &mut std::io::Read::take(reader, entry.size().saturating_add(1)),
                    &mut out,
                )
                .map_err(sevenz_rust::Error::io)?;
                if copied != entry.size() {
                    return Err(sevenz_rust::Error::other(
                        "Tamanho da entrada 7z não corresponde ao declarado",
                    ));
                }
            }
            Ok(true)
        })
        .map_err(|e| e.to_string())?;
    } else if format.ends_with(".rar") {
        let mut archive = unrar::Archive::new(archive_path)
            .open_for_processing()
            .map_err(|e| e.to_string())?;
        while let Some(entry) = archive.read_header().map_err(|e| e.to_string())? {
            let name = entry.entry().filename.to_string_lossy();
            let target = archive_entry_path(out_dir, name.trim_end_matches(['/', '\\']))?;
            if entry.entry().file_attr & 0o170000 == 0o120000
                || entry.entry().file_attr & 0x400 != 0
            {
                return Err("Links não são permitidos no arquivo RAR".into());
            }
            budget = budget
                .checked_sub(entry.entry().unpacked_size)
                .ok_or("Arquivo descompactado excede 64 GiB")?;
            if entry.entry().is_directory() {
                fs::create_dir_all(&target).map_err(|e| e.to_string())?;
                archive = entry.skip().map_err(|e| e.to_string())?;
            } else {
                fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                if target.exists() {
                    return Err("Entrada duplicada no arquivo RAR".into());
                }
                archive = entry.extract_to(&target).map_err(|e| e.to_string())?;
                let meta = fs::symlink_metadata(&target).map_err(|e| e.to_string())?;
                if !meta.is_file()
                    || !fs::canonicalize(&target)
                        .map_err(|e| e.to_string())?
                        .starts_with(fs::canonicalize(out_dir).map_err(|e| e.to_string())?)
                {
                    return Err("Entrada RAR não é um arquivo regular no destino permitido".into());
                }
            }
        }
    } else {
        return Err("Formato não suportado. Use CHD, ISO, ZIP, 7z ou RAR".into());
    }
    walkdir::WalkDir::new(out_dir)
        .into_iter()
        .filter_map(Result::ok)
        .find(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(crate::rom_scanner::is_valid_rom_extension)
                    .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .ok_or_else(|| "Nenhuma ROM encontrada no arquivo".into())
}

async fn download_cover_art(
    client: &reqwest::Client,
    cover_url: &str,
    dest_path: &Path,
) -> Result<(), String> {
    if dest_path.exists() {
        return Ok(());
    }
    let resp = client
        .get(cover_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        fs::write(dest_path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn update_both_status(
    db_path: &Path,
    cache: &Arc<std::sync::Mutex<HashMap<String, DownloadItem>>>,
    id: &str,
    status: &str,
    progress: f64,
    speed: f64,
    size: u64,
    downloaded_bytes: u64,
    error_message: Option<&str>,
    target_path: Option<&str>,
    connections: Option<usize>,
) {
    {
        let mut c = cache.lock().unwrap();
        if let Some(item) = c.get_mut(id) {
            if item.status == "Cancelled" {
                return;
            }
            item.status = status.to_string();
            item.progress = progress;
            item.speed = speed;
            item.size = size;
            item.downloaded_bytes = downloaded_bytes;
            item.error_message = error_message.map(|s| s.to_string());
            if let Some(t) = target_path {
                item.target_path = Some(t.to_string());
            }
            if let Some(conn) = connections {
                item.connections = Some(conn);
            }
        }
    }

    update_db_status(
        db_path,
        id,
        status,
        progress,
        speed,
        size,
        downloaded_bytes,
        error_message,
        target_path,
        connections,
    );
}

fn update_db_status(
    db_path: &Path,
    id: &str,
    status: &str,
    progress: f64,
    speed: f64,
    size: u64,
    downloaded_bytes: u64,
    error_message: Option<&str>,
    target_path: Option<&str>,
    connections: Option<usize>,
) {
    if let Ok(conn) = rusqlite::Connection::open(db_path) {
        let _ = conn.execute(
            "UPDATE downloads SET
                status = ?1,
                progress = ?2,
                speed = ?3,
                size = ?4,
                downloaded_bytes = ?5,
                error_message = ?6,
                target_path = COALESCE(?7, target_path),
                connections = COALESCE(?8, connections)
             WHERE id = ?9 AND status != 'Cancelled'",
            params![
                status,
                progress,
                speed,
                size as i64,
                downloaded_bytes as i64,
                error_message,
                target_path,
                connections.map(|c| c as i64),
                id,
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!(
                    "download-test-{}",
                    Utc::now().timestamp_nanos_opt().unwrap()
                ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn zip_part_extracts_and_rejects_traversal() {
        let fixture = Fixture::new();
        for (name, allowed) in [
            ("game.iso", true),
            ("../outside.iso", false),
            ("C:/outside.iso", false),
        ] {
            let archive = fixture.0.join(if allowed {
                "good.zip.part"
            } else {
                "bad.zip.part"
            });
            let file = File::create(&archive).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file(name, zip::write::FileOptions::default())
                .unwrap();
            writer.write_all(b"ROM").unwrap();
            writer.finish().unwrap();
            let out = fixture.0.join(if allowed { "good" } else { "bad" });
            let result = extract_archive(&archive, &out, "game.zip");
            assert_eq!(result.is_ok(), allowed);
            if let Ok(path) = result {
                assert_eq!(fs::read(path).unwrap(), b"ROM");
            }
        }
        assert!(!fixture.0.join("outside.iso").exists());
    }

    #[test]
    fn installing_a_file_never_overwrites_existing_data() {
        let fixture = Fixture::new();
        let source = fixture.0.join("download.part");
        let target = fixture.0.join("game.chd");
        fs::write(&source, b"new ROM").unwrap();
        fs::write(&target, b"existing ROM").unwrap();
        assert!(copy_without_overwrite(&source, &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"existing ROM");
        fs::remove_file(&target).unwrap();
        copy_without_overwrite(&source, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new ROM");
    }

    #[test]
    fn seven_zip_part_uses_the_original_format() {
        let fixture = Fixture::new();
        let source = fixture.0.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("game.iso"), b"ROM").unwrap();
        let archive = fixture.0.join("game.7z.part");
        sevenz_rust::compress_to_path(&source, &archive).unwrap();
        let extracted = extract_archive(&archive, &fixture.0.join("out"), "game.7z").unwrap();
        assert_eq!(fs::read(extracted).unwrap(), b"ROM");
    }

    #[test]
    fn filename_is_safe_and_disposition_preserves_archive_format() {
        assert_eq!(
            disposition_filename("attachment; filename*=UTF-8''Game%20Name.7z").as_deref(),
            Some("Game Name.7z")
        );
        assert_eq!(
            derive_filename_from_url_or_title("https://example.invalid/%2E%2E%5Cgame.iso", "Game"),
            ".._game.iso"
        );
        assert!(!crate::rom_scanner::is_safe_filename("CON.png"));
        assert!(!crate::rom_scanner::is_safe_filename("../cover.jpg"));
        assert!(archive_entry_path(Path::new("target"), "..\\outside.iso").is_err());
        assert!(archive_entry_path(Path::new("target"), "folder/game.iso").is_ok());
    }

    #[tokio::test]
    async fn range_worker_requires_partial_status_and_matching_header() {
        use axum::{Router, body::Body, response::Response, routing::get};
        let app = Router::new()
            .route("/ignored", get(|| async { "ROM!" }))
            .route(
                "/correct",
                get(|| async {
                    Response::builder()
                        .status(206)
                        .header("content-range", "bytes 0-3/4")
                        .body(Body::from("ROM!"))
                        .unwrap()
                }),
            )
            .route(
                "/wrong",
                get(|| async {
                    Response::builder()
                        .status(206)
                        .header("content-range", "bytes 1-4/5")
                        .body(Body::from("ROM!"))
                        .unwrap()
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let fixture = Fixture::new();
        for (route, valid) in [("ignored", false), ("correct", true), ("wrong", false)] {
            let result = download_chunk_range(
                reqwest::Client::new(),
                format!("http://{addr}/{route}"),
                0,
                3,
                fixture.0.join(route),
                Arc::new(AtomicU64::new(0)),
            )
            .await;
            assert_eq!(result.is_ok(), valid, "{route}");
        }
        task.abort();
    }

    #[tokio::test]
    async fn dropping_parent_cancels_child_tasks() {
        let hits = Arc::new(AtomicU64::new(0));
        let child_hits = hits.clone();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let parent = tokio::spawn(async move {
            let mut children = ChildTasks::default();
            let child = tokio::spawn(async move {
                loop {
                    child_hits.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                }
            });
            children.0.push(child.abort_handle());
            ready_tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        ready_rx.await.unwrap();
        parent.abort();
        let _ = parent.await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let stopped = hits.load(Ordering::Relaxed);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(hits.load(Ordering::Relaxed), stopped);
    }

    #[tokio::test]
    async fn queue_respects_concurrency_limit_and_pause_controls() {
        let fixture = Fixture::new();
        let db_path = fixture.0.join("queue.db");
        let roms_dir = fixture.0.join("roms");
        let conn = crate::database::initialize_db(&db_path).unwrap();
        crate::database::set_config(&conn, "max_concurrent_downloads", "1").unwrap();
        drop(conn);

        let mut mgr = DownloadManager::new(db_path);
        mgr.pause_queue();

        let id1 = mgr
            .start_download(
                "Game 1".to_string(),
                "https://127.0.0.1:9/g1.iso".to_string(),
                None,
                Some(1024),
                roms_dir.clone(),
                None,
            )
            .unwrap();
        let id2 = mgr
            .start_download(
                "Game 2".to_string(),
                "https://127.0.0.1:9/g2.iso".to_string(),
                None,
                Some(1024),
                roms_dir,
                None,
            )
            .unwrap();
        assert_ne!(id1, id2);
        let list = mgr.get_downloads().unwrap();
        assert_eq!(list.iter().filter(|d| d.status == "Queued").count(), 2);

        let cancelled = mgr.cancel_queued_downloads().unwrap();
        assert_eq!(cancelled, 2);
        let after = mgr.get_downloads().unwrap();
        assert_eq!(after.iter().filter(|d| d.status == "Cancelled").count(), 2);
    }
}

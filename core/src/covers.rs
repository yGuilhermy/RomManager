use crate::{AppState, database, rom_scanner};
use axum::{Json, extract::State, http::StatusCode};
use chrono::Utc;
use futures_util::StreamExt;
use rusqlite::params;
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Default, Serialize)]
pub struct CoverJob {
    pub running: bool,
    pub total: usize,
    pub processed: usize,
    pub updated: usize,
    pub preserved: usize,
    pub failed: usize,
    pub current: String,
    pub error: Option<String>,
}
pub async fn status(State(state): State<Arc<AppState>>) -> Json<CoverJob> {
    Json(state.cover_job.lock().unwrap().clone())
}
pub async fn start(
    State(state): State<Arc<AppState>>,
) -> Result<(StatusCode, Json<CoverJob>), (StatusCode, String)> {
    if state.cover_job.lock().unwrap().running {
        return Err((
            StatusCode::CONFLICT,
            "Já existe uma regeneração em andamento".into(),
        ));
    }
    let conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let games = rom_scanner::list_installed_games(&conn)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    if games.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Sincronize a biblioteca antes de regenerar capas".into(),
        ));
    }
    let directory = match database::get_config(&conn, "covers_dir").filter(|p| !p.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => {
            let roms = database::get_config(&conn, "roms_dir")
                .filter(|p| !p.is_empty())
                .ok_or((
                    StatusCode::BAD_REQUEST,
                    "Configure a pasta de ROMs ou capas".into(),
                ))?;
            PathBuf::from(roms).join("covers")
        }
    };
    std::fs::create_dir_all(&directory).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let directory =
        std::fs::canonicalize(directory).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    database::set_config(&conn, "covers_dir", &directory.to_string_lossy())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let initial = {
        let mut job = state.cover_job.lock().unwrap();
        if job.running {
            return Err((
                StatusCode::CONFLICT,
                "Já existe uma regeneração em andamento".into(),
            ));
        }
        *job = CoverJob {
            running: true,
            total: games.len(),
            ..Default::default()
        };
        job.clone()
    };
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                let mut job = state.cover_job.lock().unwrap();
                job.running = false;
                job.error = Some(e.to_string());
                return;
            }
        };
        for game in games {
            state.cover_job.lock().unwrap().current = game.title.clone();
            let clean = rom_scanner::clean_title_from_filename(&game.filename);
            let local = rom_scanner::find_local_cover(
                &directory,
                game.serial.as_deref(),
                &clean,
                &game.title,
            );
            let preserved = local.is_some();
            let result = match local {
                Some(name) => Ok(name),
                None => fetch_cover(&client, &directory, &game).await,
            };
            let saved = match result {
                Ok(name) => {
                    let _guard = rom_scanner::SCAN_LOCK.lock().await;
                    let revision = Utc::now().timestamp_millis();
                    let url = format!(
                        "http://127.0.0.1:3000/api/covers/serve?file={}&v={}",
                        urlencoding::encode(&name),
                        revision
                    );
                    rusqlite::Connection::open(&state.db_path)
                        .and_then(|conn| {
                            conn.execute(
                                "UPDATE games SET cover_url=?1 WHERE id=?2",
                                params![url, game.id],
                            )
                        })
                        .map(|rows| rows > 0)
                        .unwrap_or(false)
                }
                Err(_) => false,
            };
            let mut job = state.cover_job.lock().unwrap();
            job.processed += 1;
            if saved {
                if preserved {
                    job.preserved += 1;
                } else {
                    job.updated += 1;
                }
            } else {
                job.failed += 1;
            }
        }
        let mut job = state.cover_job.lock().unwrap();
        job.running = false;
        job.current.clear();
    });
    Ok((StatusCode::ACCEPTED, Json(initial)))
}

async fn fetch_cover(
    client: &reqwest::Client,
    directory: &Path,
    game: &rom_scanner::InstalledGame,
) -> Result<String, String> {
    let mut sources = Vec::new();
    if let Some(serial) = game
        .serial
        .as_deref()
        .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    {
        sources.push(format!(
            "https://raw.githubusercontent.com/xlenore/ps2-covers/main/covers/default/{serial}.jpg"
        ));
    }
    sources.push(crate::api_roms::build_cover_url(&game.title));
    let clean = rom_scanner::clean_title_from_filename(&game.filename);
    if clean != game.title {
        sources.push(crate::api_roms::build_cover_url(&clean));
    }
    for source in sources {
        if let Ok(response) = client.get(source).send().await {
            if !response.status().is_success() {
                continue;
            }
            let mut data = Vec::new();
            let mut stream = response.bytes_stream();
            let mut valid = true;
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) if data.len() + bytes.len() <= 16 * 1024 * 1024 => {
                        data.extend_from_slice(&bytes)
                    }
                    _ => {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid {
                continue;
            }
            let extension = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
                "png"
            } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
                "jpg"
            } else {
                continue;
            };
            let hash = game.id.bytes().fold(0xcbf29ce484222325u64, |h, b| {
                (h ^ (b as u64)).wrapping_mul(0x100000001b3)
            });
            let name = format!("rommanager_{hash:016x}.{extension}");
            let temp = directory.join(format!("rommanager_{hash:016x}.tmp"));
            tokio::fs::write(&temp, &data)
                .await
                .map_err(|e| e.to_string())?;
            tokio::fs::rename(temp, directory.join(&name))
                .await
                .map_err(|e| e.to_string())?;
            return Ok(name);
        }
    }
    Err("Nenhuma capa disponível nas fontes consultadas".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn regenerating_preserves_personal_covers_without_network() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "cover-test-{}",
                Utc::now().timestamp_nanos_opt().unwrap()
            ));
        std::fs::create_dir_all(root.join("covers")).unwrap();
        let db = root.join("test.db");
        let conn = database::initialize_db(&db).unwrap();
        database::set_config(&conn, "covers_dir", &root.join("covers").to_string_lossy()).unwrap();
        std::fs::write(root.join("covers/Game.png"), b"personal image").unwrap();
        conn.execute("INSERT INTO games(id,title,filename,file_path,file_size,format,date_added) VALUES('game','Game','Game.iso','Game.iso',1,'iso',?1)",params![Utc::now().to_rfc3339()]).unwrap();
        drop(conn);
        let state = Arc::new(AppState {
            db_path: db.clone(),
            download_manager: Arc::new(std::sync::Mutex::new(
                crate::download_manager::DownloadManager::new(db.clone()),
            )),
            logs: Arc::new(std::sync::Mutex::new(vec![])),
            cover_job: Arc::new(std::sync::Mutex::new(CoverJob::default())),
            job_manager: Arc::new(crate::jobs::JobManager::new(db.clone())),
        });
        let _started = start(State(state.clone())).await.unwrap();
        for _ in 0..200 {
            if !state.cover_job.lock().unwrap().running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let job = state.cover_job.lock().unwrap().clone();
        assert!(!job.running);
        assert_eq!(job.preserved, 1);
        assert_eq!(job.failed, 0);
        assert_eq!(
            std::fs::read(root.join("covers/Game.png")).unwrap(),
            b"personal image"
        );
        let conn = rusqlite::Connection::open(db).unwrap();
        let url: String = conn
            .query_row("SELECT cover_url FROM games WHERE id='game'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(url.contains("file=Game.png&v="));
        drop(conn);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }
}

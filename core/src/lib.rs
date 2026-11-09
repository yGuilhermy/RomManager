pub mod achievements;
pub mod api_roms;
pub mod cache;
pub mod catalog_refresh;
pub mod chd;
pub mod console;
pub mod covers;
pub mod database;
pub mod download_manager;
pub mod exe_runner;
pub mod game_metadata;
pub mod game_resources;
pub mod jobs;
pub mod memory_cards;
pub mod pcsx2;
mod windows_processes;
pub mod profile;
pub mod resource_settings;
pub mod native_patches;
pub mod online_http;
pub mod extras_sources;
pub mod gamefaqs;
pub mod extras_install;
mod extras_api;
pub mod rom_scanner;
pub mod save_formats;
pub mod save_actions;

use axum::{
    Json, Router,
    extract::Request,
    extract::{
        Path as AxumPath, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{delete, get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower_http::cors::{Any, CorsLayer};

pub struct AppState {
    pub db_path: PathBuf,
    pub download_manager: Arc<Mutex<download_manager::DownloadManager>>,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub cover_job: Arc<Mutex<covers::CoverJob>>,
    pub job_manager: Arc<jobs::JobManager>,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub database: String,
    pub app: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConfigItem {
    pub key: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct PathValidateQuery {
    pub path: Option<String>,
}

#[derive(Serialize)]
pub struct ValidationResponse {
    pub valid: bool,
    pub message: String,
}

#[derive(Deserialize)]
pub struct CatalogQuery {
    pub q: Option<String>,
    pub search: Option<String>,
    pub region: Option<String>,
    pub category: Option<String>,
    pub sort: Option<String>,
    pub letter: Option<String>,
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    pub format: Option<String>,
}

#[derive(Deserialize)]
pub struct TranslationsQuery {
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct GameMetadataQuery {
    pub title: Option<String>,
    pub serial: Option<String>,
}

#[derive(Deserialize)]
pub struct StartDownloadPayload {
    #[serde(alias = "game_title")]
    pub title: String,
    pub url: String,
    pub cover_url: Option<String>,
    pub size: Option<u64>,
}

#[derive(Deserialize)]
pub struct BulkDownloadPayload {
    pub items: Vec<StartDownloadPayload>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FavoriteItem {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub url: String,
    pub size: Option<u64>,
    pub region: Option<String>,
    pub added_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: i64,
    pub game_name: String,
    pub action: String,
    pub timestamp: String,
    pub details: Option<String>,
}

pub fn get_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("ROMMANAGER_DATA_DIR") {
        let dir = PathBuf::from(path);
        let _ = fs::create_dir_all(&dir);
        return dir.join("local.db");
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        let app_dir = PathBuf::from(appdata).join("RomManager");
        let _ = fs::create_dir_all(&app_dir);
        app_dir.join("local.db")
    } else {
        PathBuf::from("local.db")
    }
}

pub async fn start_api() {
    let db_path = get_db_path();
    let conn = database::initialize_db(&db_path).expect("Failed to initialize database");
    drop(conn);

    let download_mgr = Arc::new(Mutex::new(download_manager::DownloadManager::new(
        db_path.clone(),
    )));

    let queue_mgr = Arc::clone(&download_mgr);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
        loop {
            interval.tick().await;
            if let Ok(mut mgr) = queue_mgr.lock() {
                mgr.process_queue();
            }
        }
    });

    let logs = Arc::new(Mutex::new(vec![
        format!("Servidor do RomManager iniciado com sucesso na porta {}.", api_port()),
    ]));

    let job_mgr = Arc::new(jobs::JobManager::new(db_path.clone()));

    let state = Arc::new(AppState {
        db_path,
        download_manager: download_mgr,
        logs,
        cover_job: Arc::new(Mutex::new(covers::CoverJob::default())),
        job_manager: job_mgr,
    });

    let warm_db = state.db_path.clone();
    tokio::task::spawn_blocking(move || cache::warmup(&warm_db));

    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://localhost:1420"),
            HeaderValue::from_static("http://127.0.0.1:1420"),
            HeaderValue::from_static("http://tauri.localhost"),
            HeaderValue::from_static("https://tauri.localhost"),
            HeaderValue::from_static("tauri://localhost"),
        ])
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers(Any);

    let app = Router::new()
        .merge(console::api::router())
        .route("/health", get(health_check))
        .route("/api/health", get(health_check))
        .route("/config", get(get_configs).post(set_config))
        .route("/api/config", get(get_configs).post(set_config))
        .route("/api/config/validate-roms-dir", get(validate_roms_dir))
        .route("/api/config/validate-pcsx2-exe", get(validate_pcsx2_exe))
        .route("/api/config/validate-covers-dir", get(validate_covers_dir))
        .route("/api/covers/serve", get(serve_cover))
        .route("/api/catalog", get(get_catalog))
        .route("/api/catalog/status", get(extras_api::catalog_status))
        .route("/api/catalog/refresh", post(extras_api::catalog_update))
        .route("/api/extras/games", get(extras_api::games))
        .route("/api/extras/items", get(extras_api::items))
        .route("/api/extras/operations", post(extras_api::start))
        .route("/api/extras/operations/:id", get(extras_api::operation))
        .route("/api/roms/catalog", get(get_catalog))
        .route("/api/roms/translations", get(get_translations))
        .route("/api/games", get(list_games))
        .route("/api/games/scan", post(scan_games))
        .route(
            "/api/games/covers/regenerate",
            get(covers::status).post(covers::start),
        )
        .route("/api/games/run", post(run_game_direct))
        .route("/api/game/metadata", get(get_game_metadata_handler))
        .route("/api/games/:id/launch", post(launch_game))
        .route("/api/games/:id/verify", get(verify_game_handler))
        .route("/api/games/:id", delete(delete_game))
        .route("/api/games/:id/open-folder", post(open_game_folder))
        .route("/api/downloads", get(get_downloads).post(start_download))
        .route("/api/downloads/start", post(start_download))
        .route("/api/downloads/bulk", post(start_bulk_downloads))
        .route("/api/downloads/queue", get(get_queue_status))
        .route("/api/downloads/queue/pause", post(pause_download_queue))
        .route("/api/downloads/queue/resume", post(resume_download_queue))
        .route(
            "/api/downloads/queue/cancel-all",
            post(cancel_queued_downloads),
        )
        .route("/api/downloads/:id/cancel", post(cancel_download))
        .route("/api/downloads/:id/retry", post(retry_download))
        .route("/api/downloads/:id", delete(delete_download))
        .route("/api/jobs", get(list_jobs_handler).post(create_job_handler))
        .route(
            "/api/jobs/:id",
            get(get_job_handler).delete(delete_job_handler),
        )
        .route("/api/jobs/:id/cancel", post(cancel_job_handler))
        .route("/api/pcsx2/detect", get(pcsx2_detect_handler))
        .route("/api/pcsx2/diagnostic", get(pcsx2_diagnostic_handler))
        .route(
            "/api/pcsx2/resources",
            get(game_resources_state).post(game_resources_action),
        )
        .route("/api/pcsx2/memory-cards", get(memory_cards_list))
        .route("/api/pcsx2/memory-cards/action", post(memory_cards_action))
        .route(
            "/api/pcsx2/memory-cards/tool",
            get(memory_cards_tool_state)
                .post(memory_cards_tool)
                .delete(memory_cards_tool_remove),
        )
        .route(
            "/api/pcsx2/resources/game",
            get(game_resource_settings).post(toggle_game_resource),
        )
        .route("/api/cache/prefetch", post(cache_prefetch_handler))
        .route("/api/pcsx2/bios", get(pcsx2_list_bios_handler))
        .route("/api/pcsx2/bios/import", post(pcsx2_import_bios_handler))
        .route("/api/pcsx2/bios/select", post(pcsx2_select_bios_handler))
        .route(
            "/api/pcsx2/covers/export",
            post(pcsx2_export_covers_handler),
        )
        .route("/api/pcsx2/achievements/state", get(achievements_state_handler))
        .route("/api/pcsx2/achievements/login", post(achievements_login_handler))
        .route("/api/pcsx2/achievements/logout", post(achievements_logout_handler))
        .route("/api/pcsx2/achievements/game", get(achievements_game_handler))
        .route(
            "/api/pcsx2/achievements/game/sync",
            post(achievements_game_sync_handler),
        )
        .route("/api/favorites", get(list_favorites).post(add_favorite))
        .route("/api/favorites/:id", delete(delete_favorite))
        .route(
            "/api/profile",
            get(profile::get_profile).post(profile::update_profile),
        )
        .route("/api/profile/games", post(profile::update_game))
        .route("/api/profile/export", get(profile::export_profile))
        .route(
            "/api/profile/import",
            post(profile::restore_profile).layer(axum::extract::DefaultBodyLimit::max(
                profile::BACKUP_MAX_BYTES,
            )),
        )
        .route("/api/history", get(list_history))
        .route("/api/history/clear", post(clear_history))
        .route("/api/logs", get(get_logs).delete(clear_logs))
        .route("/api/logs/clear", post(clear_logs))
        .route("/api/open-folder", get(open_local_folder))
        .route("/api/open", get(open_browser_url))
        .route("/api/ws", get(ws_handler))
        .layer(cors)
        .layer(middleware::from_fn(local_request_only))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], api_port()));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

fn api_port()->u16 {std::env::var("ROMMANAGER_PORT").ok().and_then(|v|v.parse::<u16>().ok()).filter(|p|*p!=0).unwrap_or(3000)}

async fn local_request_only(req: Request, next: Next) -> axum::response::Response {
    let host = req
        .headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if host != format!("localhost:{}",api_port()) && host != format!("127.0.0.1:{}",api_port()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Some(origin) = req.headers().get("origin") {
        if !matches!(
            origin.to_str().unwrap_or(""),
            "http://localhost:1420"
                | "http://127.0.0.1:1420"
                | "http://tauri.localhost"
                | "https://tauri.localhost"
                | "tauri://localhost"
        ) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    next.run(req).await
}

async fn health_check(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let db_status = match rusqlite::Connection::open(&state.db_path) {
        Ok(c) => match c.query_row("SELECT 1", [], |_| Ok(())) {
            Ok(_) => "Connected".to_string(),
            Err(_) => "Error".to_string(),
        },
        Err(_) => "Error".to_string(),
    };

    Json(HealthResponse {
        status: "OK".to_string(),
        database: db_status,
        app: "RomManager".to_string(),
    })
}

async fn get_configs(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HashMap<String, String>>, (StatusCode, String)> {
    let conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    database::get_all_configs(&conn)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn set_config(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ConfigItem>,
) -> StatusCode {
    let _native_guard = if payload.key.starts_with("pcsx2_") {
        match a4_edit_guard() {
            Ok(g) => Some(g),
            Err((status, _)) => return status,
        }
    } else {
        None
    };
    let path = PathBuf::from(&payload.value);
    let valid = match payload.key.as_str() {
        "roms_dir" | "roms_directory" | "covers_dir" | "covers_directory" => path.is_dir(),
        "pcsx2_data_dir" => payload.value.is_empty() || path.is_dir(),
        "pcsx2_path" | "pcsx2_executable_path" => exe_runner::validate_pcsx2_executable(&path),
        "auto_download_covers" | "dark_mode" => matches!(payload.value.as_str(), "true" | "false"),
        "max_concurrent_downloads" => matches!(payload.value.trim(), "1" | "2" | "3" | "4" | "5"),
        _ => false,
    };
    if !valid {
        return StatusCode::BAD_REQUEST;
    }
    if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
        let previous_executable = database::get_config(&conn, "pcsx2_path");
        if database::set_config(&conn, &payload.key, &payload.value).is_ok() {
            if matches!(payload.key.as_str(), "pcsx2_path" | "pcsx2_executable_path")
                && !previous_executable.is_some_and(|p| p.eq_ignore_ascii_case(&payload.value))
            {
                let _ = conn.execute("DELETE FROM configs WHERE key='pcsx2_data_dir'", []);
            }
            if payload.key == "max_concurrent_downloads" {
                if let Ok(mut mgr) = state.download_manager.lock() {
                    mgr.process_queue();
                }
            }
            let mut logs = state.logs.lock().unwrap();
            logs.push(format!(
                "Configuração atualizada: {} = {}",
                payload.key, payload.value
            ));
            return StatusCode::OK;
        }
    }
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn validate_roms_dir(Query(q): Query<PathValidateQuery>) -> Json<ValidationResponse> {
    if let Some(path_str) = q.path {
        let p = PathBuf::from(&path_str);
        let resolved = rom_scanner::resolve_roms_directory(&p);
        if resolved.exists() && resolved.is_dir() {
            let count = rom_scanner::count_roms_in_directory(&resolved);
            return Json(ValidationResponse {
                valid: true,
                message: format!(
                    "Diretório validado: {} ({} jogos encontrados)",
                    resolved.to_string_lossy(),
                    count
                ),
            });
        }
    }
    Json(ValidationResponse {
        valid: false,
        message: "O caminho informado não é um diretório válido ou não foi localizado.".to_string(),
    })
}

async fn validate_pcsx2_exe(Query(q): Query<PathValidateQuery>) -> Json<ValidationResponse> {
    if let Some(path_str) = q.path {
        let p = PathBuf::from(&path_str);
        if exe_runner::validate_pcsx2_executable(&p) {
            return Json(ValidationResponse {
                valid: true,
                message: "Executável do PCSX2 validado com sucesso.".to_string(),
            });
        }
    }
    Json(ValidationResponse {
        valid: false,
        message: "Executável do PCSX2 não encontrado no caminho informado.".to_string(),
    })
}

async fn validate_covers_dir(Query(q): Query<PathValidateQuery>) -> Json<ValidationResponse> {
    if let Some(path_str) = q.path {
        let p = PathBuf::from(&path_str);
        if p.exists() && p.is_dir() {
            let count = rom_scanner::count_covers_in_directory(&p);
            return Json(ValidationResponse {
                valid: true,
                message: format!("Pasta de capas validada ({} capas detectadas).", count),
            });
        }
    }
    Json(ValidationResponse {
        valid: false,
        message: "A pasta de capas informada não existe ou é inválida.".to_string(),
    })
}

#[derive(Deserialize)]
struct CoverServeQuery {
    file: String,
}

async fn serve_cover(
    Query(q): Query<CoverServeQuery>,
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, StatusCode> {
    let covers_dir_str = {
        if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
            database::get_config(&conn, "covers_dir").unwrap_or_default()
        } else {
            String::new()
        }
    };

    if covers_dir_str.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    let root = fs::canonicalize(&covers_dir_str).map_err(|_| StatusCode::NOT_FOUND)?;
    if !rom_scanner::is_safe_filename(&q.file) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let p = fs::canonicalize(root.join(&q.file)).map_err(|_| StatusCode::NOT_FOUND)?;
    if !p.starts_with(&root)
        || !rom_scanner::is_valid_cover_extension(
            p.extension().and_then(|x| x.to_str()).unwrap_or(""),
        )
    {
        return Err(StatusCode::FORBIDDEN);
    }
    if !p.exists() || !p.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }

    let meta = fs::metadata(&p).map_err(|_| StatusCode::NOT_FOUND)?;
    if meta.len() > 32 * 1024 * 1024 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let etag = cache::FileStamp::of(&p).map(|stamp| stamp.tag());
    if let Some(tag) = &etag {
        let unchanged = headers
            .get("if-none-match")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == tag);
        if unchanged {
            return axum::response::Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header("ETag", tag.as_str())
                .header("Cache-Control", "public, max-age=86400")
                .body(axum::body::Body::empty())
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    let bytes = fs::read(&p).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let content_type = match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };

    let mut builder = axum::response::Response::builder()
        .header("Content-Type", content_type)
        .header("Cache-Control", "public, max-age=86400");
    if let Some(tag) = &etag {
        builder = builder.header("ETag", tag.as_str());
    }
    builder
        .body(axum::body::Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct PrefetchPayload {
    game_id: String,
    #[serde(default)]
    crc: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    serial: Option<String>,
}

async fn cache_prefetch_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PrefetchPayload>,
) -> StatusCode {
    let db = state.db_path.clone();
    let game_id = payload.game_id.clone();
    let crc = payload.crc.clone();
    tokio::task::spawn_blocking(move || cache::prefetch_game(&db, &game_id, &crc));
    if !payload.title.trim().is_empty() {
        let db = state.db_path.clone();
        tokio::spawn(async move {
            let _ =
                game_metadata::get_metadata_for_game(&db, &payload.title, payload.serial.as_deref())
                    .await;
        });
    }
    StatusCode::ACCEPTED
}

async fn get_catalog(State(state): State<Arc<AppState>>, Query(q): Query<CatalogQuery>) -> Json<api_roms::CatalogResponse> {
    let page = q.page.unwrap_or(1);
    let per_page = q.per_page.unwrap_or(24);
    let search_query = q.q.or(q.search);
    let snapshot=catalog_refresh::snapshot(&state.db_path);
    let res = api_roms::query_catalog_items(
        &snapshot.items,
        search_query.as_deref(),
        q.region.as_deref(),
        q.category.as_deref(),
        q.sort.as_deref(),
        q.letter.as_deref(),
        q.format.as_deref(),
        page,
        per_page,
    );
    Json(res)
}

async fn get_translations(
    Query(q): Query<TranslationsQuery>,
) -> Json<Vec<api_roms::RomCatalogItem>> {
    let query_str = q.q.unwrap_or_else(|| "portuguese".to_string());
    let res = api_roms::search_translations(&query_str)
        .await
        .unwrap_or_default();
    Json(res)
}

async fn list_games(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<rom_scanner::InstalledGame>>, (StatusCode, String)> {
    let conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    rom_scanner::list_installed_games(&conn)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn scan_games(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<rom_scanner::InstalledGame>>, (StatusCode, String)> {
    let mut conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let roms = database::get_config(&conn, "roms_dir").unwrap_or_default();
    if roms.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Configure a pasta de ROMs antes de sincronizar".into(),
        ));
    }
    let covers = database::get_config(&conn, "covers_dir")
        .filter(|c| !c.is_empty())
        .map(PathBuf::from);
    let scanned =
        rom_scanner::scan_roms_directory(&mut conn, &PathBuf::from(roms), covers.as_deref())
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    state.logs.lock().unwrap().push(format!(
        "Varredura concluída: {} jogos encontrados.",
        scanned.len()
    ));
    Ok(Json(scanned))
}

#[derive(Deserialize)]
struct DirectRunPayload {
    game_path: String,
}

async fn run_game_direct(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DirectRunPayload>,
) -> (StatusCode, String) {
    let _a4_guard = match a4_edit_guard() {
        Ok(guard) => guard,
        Err(error) => return error,
    };
    let clean_game_path = pcsx2::sanitize_path(&payload.game_path);
    if clean_game_path.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "Caminho do jogo não informado".to_string(),
        );
    }

    let (pcsx2_path_str, fullscreen, data_dir) = {
        if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
            let p_exe = database::get_config(&conn, "pcsx2_path")
                .map(|s| pcsx2::sanitize_path(&s))
                .unwrap_or_default();
            let fs_flag = false;
            (
                p_exe,
                fs_flag,
                database::get_config(&conn, "pcsx2_data_dir"),
            )
        } else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Falha no banco de dados".to_string(),
            );
        }
    };

    if pcsx2_path_str.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "Executável do PCSX2 não configurado nas Configurações ou Perfil.".to_string(),
        );
    }

    let p_exe = PathBuf::from(&pcsx2_path_str);
    let p_rom = PathBuf::from(&clean_game_path);

    if !p_rom.exists() || !p_rom.is_file() {
        return (
            StatusCode::NOT_FOUND,
            format!(
                "O arquivo da ROM não existe no disco: '{}'",
                clean_game_path
            ),
        );
    }

    if !exe_runner::validate_pcsx2_executable(&p_exe) {
        return (
            StatusCode::NOT_FOUND,
            format!(
                "O executável do PCSX2 não foi encontrado ou é inválido: '{}'",
                pcsx2_path_str
            ),
        );
    }

    match exe_runner::launch_pcsx2_with_data_dir(
        &p_exe,
        &p_rom,
        fullscreen,
        data_dir
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(std::path::Path::new),
    ) {
        Ok(_) => {
            let mut logs = state.logs.lock().unwrap();
            logs.push(format!("Jogo iniciado no emulador: {}", clean_game_path));
            if let Ok(mut conn) = rusqlite::Connection::open(&state.db_path) {
                let _ = profile::record_launch(&mut conn, &clean_game_path);
            }
            (StatusCode::OK, "Jogo iniciado com sucesso".to_string())
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn launch_game(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> (StatusCode, String) {
    let _a4_guard = match a4_edit_guard() {
        Ok(guard) => guard,
        Err(error) => return error,
    };
    let (pcsx2_path_str, file_path_str, fullscreen, data_dir) = {
        if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
            let p_exe = database::get_config(&conn, "pcsx2_path")
                .map(|s| pcsx2::sanitize_path(&s))
                .unwrap_or_default();
            let fs_flag = false;
            let rom_p = conn
                .query_row(
                    "SELECT file_path FROM games WHERE id = ?1",
                    rusqlite::params![id],
                    |row| row.get::<_, String>(0),
                )
                .map(|s| pcsx2::sanitize_path(&s))
                .unwrap_or_default();
            (
                p_exe,
                rom_p,
                fs_flag,
                database::get_config(&conn, "pcsx2_data_dir"),
            )
        } else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Falha no banco de dados".to_string(),
            );
        }
    };

    if pcsx2_path_str.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "Caminho do executável do PCSX2 não está configurado nas Configurações.".to_string(),
        );
    }

    let p_exe = PathBuf::from(&pcsx2_path_str);
    let rom_p = PathBuf::from(&file_path_str);

    if !rom_p.exists() || !rom_p.is_file() {
        return (
            StatusCode::NOT_FOUND,
            format!("O arquivo da ROM não existe no disco: '{}'", file_path_str),
        );
    }

    if !exe_runner::validate_pcsx2_executable(&p_exe) {
        return (
            StatusCode::NOT_FOUND,
            format!(
                "O executável do PCSX2 não foi encontrado ou é inválido: '{}'",
                pcsx2_path_str
            ),
        );
    }

    match exe_runner::launch_pcsx2_with_data_dir(
        &p_exe,
        &rom_p,
        fullscreen,
        data_dir
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(std::path::Path::new),
    ) {
        Ok(_) => {
            let mut logs = state.logs.lock().unwrap();
            logs.push(format!("PCSX2 iniciado para: {}", rom_p.display()));
            if let Ok(mut conn) = rusqlite::Connection::open(&state.db_path) {
                let _ = profile::record_launch(&mut conn, &file_path_str);
            }
            (StatusCode::OK, "Jogo iniciado no PCSX2.".to_string())
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn verify_game_handler(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<rom_scanner::IntegrityCheckResult>, (StatusCode, String)> {
    let conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match rom_scanner::verify_game_integrity(&conn, &id) {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((StatusCode::NOT_FOUND, e)),
    }
}

async fn delete_game(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    let covers_dir_str = {
        if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
            database::get_config(&conn, "covers_dir").unwrap_or_default()
        } else {
            String::new()
        }
    };
    let c_path = if covers_dir_str.is_empty() {
        None
    } else {
        Some(PathBuf::from(&covers_dir_str))
    };

    if let Ok(mut conn) = rusqlite::Connection::open(&state.db_path) {
        if rom_scanner::delete_installed_game(&mut conn, &id, c_path.as_deref()).is_ok() {
            let mut logs = state.logs.lock().unwrap();
            logs.push(format!(
                "Jogo e arquivos removidos do disco e da biblioteca: {}",
                id
            ));
            return StatusCode::OK;
        }
    }
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn open_game_folder(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
        if let Ok(path_str) = conn.query_row(
            "SELECT file_path FROM games WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, String>(0),
        ) {
            let p = PathBuf::from(&path_str);
            if rom_scanner::open_folder_in_explorer(&p).is_ok() {
                return StatusCode::OK;
            }
        }
    }
    StatusCode::NOT_FOUND
}

async fn get_downloads(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<download_manager::DownloadItem>>, (StatusCode, String)> {
    state
        .download_manager
        .lock()
        .unwrap()
        .get_downloads()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn start_download(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartDownloadPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (roms_dir_str, covers_dir_str) = {
        if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
            let r = database::get_config(&conn, "roms_dir").unwrap_or_default();
            let c = database::get_config(&conn, "covers_dir").unwrap_or_default();
            (r, c)
        } else {
            (String::new(), String::new())
        }
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

    let mut mgr = state.download_manager.lock().unwrap();
    match mgr.start_download(
        payload.title.clone(),
        payload.url.clone(),
        payload.cover_url.clone(),
        payload.size,
        roms_dir,
        covers_dir,
    ) {
        Ok(id) => {
            let mut logs = state.logs.lock().unwrap();
            logs.push(format!("Download iniciado: {} (ID: {})", payload.title, id));
            (
                StatusCode::OK,
                Json(serde_json::json!({ "success": true, "id": id })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        ),
    }
}

async fn start_bulk_downloads(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<BulkDownloadPayload>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (roms_dir_str, covers_dir_str) = {
        if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
            let r = database::get_config(&conn, "roms_dir").unwrap_or_default();
            let c = database::get_config(&conn, "covers_dir").unwrap_or_default();
            (r, c)
        } else {
            (String::new(), String::new())
        }
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

    let mut added = 0usize;
    let mut skipped = 0usize;
    let mut ids = Vec::new();
    {
        let mut mgr = state.download_manager.lock().unwrap();
        for item in payload.items {
            match mgr.start_download(
                item.title,
                item.url,
                item.cover_url,
                item.size,
                roms_dir.clone(),
                covers_dir.clone(),
            ) {
                Ok(id) => {
                    added += 1;
                    ids.push(id);
                }
                Err(_) => {
                    skipped += 1;
                }
            }
        }
    }
    if added > 0 {
        let mut logs = state.logs.lock().unwrap();
        logs.push(format!(
            "Download em massa iniciado: {} adicionados, {} ignorados.",
            added, skipped
        ));
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "added": added,
            "skipped": skipped,
            "ids": ids
        })),
    )
}

async fn get_queue_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mgr = state.download_manager.lock().unwrap();
    Json(serde_json::json!({
        "paused": mgr.queue_paused,
        "max_concurrent": mgr.max_concurrent_downloads()
    }))
}

async fn pause_download_queue(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mut mgr = state.download_manager.lock().unwrap();
    mgr.pause_queue();
    state
        .logs
        .lock()
        .unwrap()
        .push("Fila de downloads pausada.".to_string());
    Json(serde_json::json!({ "paused": true }))
}

async fn resume_download_queue(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mut mgr = state.download_manager.lock().unwrap();
    mgr.resume_queue();
    state
        .logs
        .lock()
        .unwrap()
        .push("Fila de downloads retomada.".to_string());
    Json(serde_json::json!({ "paused": false }))
}

async fn cancel_queued_downloads(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut mgr = state.download_manager.lock().unwrap();
    let cancelled = mgr
        .cancel_queued_downloads()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    if cancelled > 0 {
        state.logs.lock().unwrap().push(format!(
            "{} downloads pendentes na fila foram cancelados.",
            cancelled
        ));
    }
    Ok(Json(serde_json::json!({ "cancelled": cancelled })))
}

async fn cancel_download(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    let mut mgr = state.download_manager.lock().unwrap();
    if mgr.cancel_download(&id).is_ok() {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn retry_download(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    let (roms_dir_str, covers_dir_str) = {
        if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
            let r = database::get_config(&conn, "roms_dir").unwrap_or_default();
            let c = database::get_config(&conn, "covers_dir").unwrap_or_default();
            (r, c)
        } else {
            (String::new(), String::new())
        }
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

    let mut mgr = state.download_manager.lock().unwrap();
    if mgr.retry_download(&id, roms_dir, covers_dir).is_ok() {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn delete_download(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    let mut mgr = state.download_manager.lock().unwrap();
    if mgr.delete_download(&id).is_ok() {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn list_favorites(State(state): State<Arc<AppState>>) -> Json<Vec<FavoriteItem>> {
    if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
        let mut stmt = conn
            .prepare("SELECT id, title, cover_url, url, size, region, added_at FROM favorites ORDER BY added_at DESC")
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok(FavoriteItem {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    cover_url: row.get(2)?,
                    url: row.get(3)?,
                    size: row.get::<_, Option<i64>>(4)?.map(|s| s as u64),
                    region: row.get(5)?,
                    added_at: row.get(6)?,
                })
            })
            .unwrap();
        return Json(rows.flatten().collect());
    }
    Json(Vec::new())
}

async fn add_favorite(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<FavoriteItem>,
) -> StatusCode {
    if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
        let now = Utc::now().to_rfc3339();
        let _ = conn.execute(
            "INSERT INTO favorites (id, title, cover_url, url, size, region, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                cover_url = excluded.cover_url,
                url = excluded.url,
                size = excluded.size,
                region = excluded.region",
            rusqlite::params![
                payload.id,
                payload.title,
                payload.cover_url,
                payload.url,
                payload.size.map(|s| s as i64),
                payload.region,
                now,
            ],
        );
        return StatusCode::OK;
    }
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn delete_favorite(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
        let _ = conn.execute("DELETE FROM favorites WHERE id = ?1", rusqlite::params![id]);
        return StatusCode::OK;
    }
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn list_history(State(state): State<Arc<AppState>>) -> Json<Vec<HistoryItem>> {
    if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
        let mut stmt = conn
            .prepare("SELECT id, game_name, action, timestamp, details FROM history ORDER BY id DESC LIMIT 100")
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok(HistoryItem {
                    id: row.get(0)?,
                    game_name: row.get(1)?,
                    action: row.get(2)?,
                    timestamp: row.get(3)?,
                    details: row.get(4)?,
                })
            })
            .unwrap();
        return Json(rows.flatten().collect());
    }
    Json(Vec::new())
}

async fn clear_history(State(state): State<Arc<AppState>>) -> StatusCode {
    if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
        let _ = conn.execute("DELETE FROM history", []);
        return StatusCode::OK;
    }
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn get_logs(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    let logs = state.logs.lock().unwrap();
    Json(logs.clone())
}

async fn clear_logs(State(state): State<Arc<AppState>>) -> StatusCode {
    let mut logs = state.logs.lock().unwrap();
    logs.clear();
    StatusCode::OK
}

#[derive(Deserialize)]
pub struct OpenFolderQuery {
    pub path: Option<String>,
}

async fn open_local_folder(Query(q): Query<OpenFolderQuery>) -> StatusCode {
    if let Some(path_str) = q.path {
        let p = PathBuf::from(&path_str);
        if rom_scanner::open_folder_in_explorer(&p).is_ok() {
            return StatusCode::OK;
        }
    }
    StatusCode::BAD_REQUEST
}

#[derive(Deserialize)]
pub struct OpenUrlQuery {
    pub url: Option<String>,
}

async fn open_browser_url(Query(q): Query<OpenUrlQuery>) -> StatusCode {
    if let Some(url_str) = q.url {
        let Ok(url) = reqwest::Url::parse(&url_str) else {
            return StatusCode::BAD_REQUEST;
        };
        if !matches!(url.scheme(), "http" | "https") {
            return StatusCode::BAD_REQUEST;
        }
        if std::process::Command::new("explorer")
            .arg(url.as_str())
            .spawn()
            .is_ok()
        {
            return StatusCode::OK;
        }
    }
    StatusCode::BAD_REQUEST
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
    loop {
        interval.tick().await;

        let downloads = {
            let mut mgr = state.download_manager.lock().unwrap();
            match mgr.get_downloads() {
                Ok(items) => items,
                Err(_) => break,
            }
        };

        let msg = serde_json::json!({
            "type": "downloads_update",
            "data": downloads
        });

        if let Ok(json_str) = serde_json::to_string(&msg) {
            if socket.send(Message::Text(json_str)).await.is_err() {
                break;
            }
        }
    }
}

async fn get_game_metadata_handler(
    Query(query): Query<GameMetadataQuery>,
) -> Json<game_metadata::GameMetadataResponse> {
    let title = query.title.unwrap_or_default();
    let serial = query.serial.as_deref();
    let db_path = get_db_path();

    let res = game_metadata::get_metadata_for_game(&db_path, &title, serial).await;
    Json(res)
}

async fn list_jobs_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<jobs::JobRecord>>, (StatusCode, String)> {
    state
        .job_manager
        .list_jobs()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn get_job_handler(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<jobs::JobRecord>, StatusCode> {
    match state.job_manager.get_job(&id) {
        Ok(Some(job)) => Ok(Json(job)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn create_job_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<jobs::CreateJobPayload>,
) -> Result<Json<jobs::JobRecord>, (StatusCode, String)> {
    let steps = payload.total_steps.unwrap_or(10);
    state
        .job_manager
        .create_job(&payload.kind, &payload.title, steps)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn cancel_job_handler(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    match state.job_manager.request_cancel(&id) {
        Ok(true) => StatusCode::OK,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn delete_job_handler(
    AxumPath(id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    match state.job_manager.delete_job(&id) {
        Ok(true) => StatusCode::OK,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Deserialize)]
struct ImportBiosPayload {
    source_path: String,
}

#[derive(Deserialize)]
struct SelectBiosPayload {
    bios_path: String,
}

#[derive(Deserialize)]
struct ExportCoversPayload {
    game_id: Option<String>,
    overwrite: Option<bool>,
}

async fn pcsx2_detect_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<pcsx2::Pcsx2Installation>> {
    let configured_path = if let Ok(conn) = rusqlite::Connection::open(&state.db_path) {
        database::get_config(&conn, "pcsx2_path")
    } else {
        None
    };
    let data_root = state.db_path.parent();
    Json(pcsx2::detect_installations(
        configured_path.as_deref(),
        data_root,
    ))
}

async fn pcsx2_diagnostic_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<pcsx2::Pcsx2Diagnostic>, (StatusCode, String)> {
    let conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    pcsx2::run_diagnostic(&conn)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn pcsx2_list_bios_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<pcsx2::BiosInfo>>, (StatusCode, String)> {
    let conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let bios_dir =
        pcsx2::native_folder(&conn, "Bios", "bios").map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let active_bios = pcsx2::active_bios(&conn).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(pcsx2::scan_bios_directory(
        &bios_dir,
        active_bios.as_deref(),
    )))
}

async fn pcsx2_import_bios_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ImportBiosPayload>,
) -> Result<(StatusCode, Json<pcsx2::BiosImportResult>), (StatusCode, String)> {
    let _guard = a4_edit_guard()?;
    memory_cards::writable().map_err(|e| (StatusCode::CONFLICT, e))?;
    let conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let bios_dir =
        pcsx2::native_folder(&conn, "Bios", "bios").map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let clean_source = pcsx2::sanitize_path(&payload.source_path);
    let source = PathBuf::from(&clean_source);
    match pcsx2::import_bios_archive(&bios_dir, &source) {
        Ok(info) => Ok((StatusCode::CREATED, Json(info))),
        Err(e) => {
            if e.contains("duplicada") || e.contains("já estão instaladas") {
                Err((StatusCode::CONFLICT, e))
            } else {
                Err((StatusCode::BAD_REQUEST, e))
            }
        }
    }
}

async fn pcsx2_select_bios_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SelectBiosPayload>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let _guard = a4_edit_guard()?;
    let conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    pcsx2::select_bios(&conn, &payload.bios_path)
        .map(|_| (StatusCode::OK, "BIOS selecionada.".into()))
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn pcsx2_export_covers_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExportCoversPayload>,
) -> Result<Json<pcsx2::CoversExportResult>, (StatusCode, String)> {
    let conn = rusqlite::Connection::open(&state.db_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _guard = a4_edit_guard()?;
    memory_cards::writable().map_err(|e| (StatusCode::CONFLICT, e))?;
    let covers_dir = pcsx2::native_folder(&conn, "Covers", "covers")
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let overwrite = payload.overwrite.unwrap_or(false);
    pcsx2::export_covers_for_game(&conn, &covers_dir, overwrite, payload.game_id.as_deref())
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[cfg(test)]
mod request_security_tests {
    use super::*;
    #[test]
    fn installation_and_launch_guard_rejects_instead_of_waiting_for_edits() {
        let _edit = memory_cards::EDIT_LOCK.lock().unwrap();
        assert!(matches!(a4_edit_guard(), Err((StatusCode::CONFLICT, _))));
    }
    #[tokio::test]
    async fn foreign_origins_and_hosts_are_rejected() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(local_request_only));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = reqwest::Client::new();
        for (host, origin, status) in [
            ("127.0.0.1:3000", "http://tauri.localhost", 200),
            ("localhost:3000", "http://localhost:1420", 200),
            ("localhost:3000", "https://untrusted.example", 403),
            ("untrusted.example:3000", "http://tauri.localhost", 403),
        ] {
            let response = client
                .get(format!("http://{address}/"))
                .header("host", host)
                .header("origin", origin)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
        }
        task.abort();
    }
}

#[derive(Deserialize, Default)]
struct MemoryCardsQuery {
    #[serde(default)]
    summary: bool,
    card: Option<String>,
}
async fn memory_cards_list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MemoryCardsQuery>,
) -> Result<Json<memory_cards::CardState>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&state.db_path).map_err(|e| e.to_string())?;
        memory_cards::list_for(&conn, &state.db_path, query.card.as_deref(), query.summary)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(|e| (StatusCode::BAD_REQUEST, e))
}
async fn memory_cards_action(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<memory_cards::CardAction>,
) -> Result<Json<memory_cards::ActionResult>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&state.db_path).map_err(|e| e.to_string())?;
        memory_cards::action(&conn, &state.db_path, payload)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(|e| (StatusCode::CONFLICT, e))
}
async fn memory_cards_tool(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    memory_cards::prepare_tool(&state.db_path)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(serde_json::json!({"ready":true})))
}

async fn game_resources_state(
    State(state): State<Arc<AppState>>,
) -> Result<Json<game_resources::ResourceState>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&state.db_path).map_err(|e| e.to_string())?;
        game_resources::state(&conn, &state.db_path)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(|e| (StatusCode::BAD_REQUEST, e))
}
async fn game_resources_action(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<game_resources::ResourceRequest>,
) -> Result<Json<game_resources::ResourcePlan>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&state.db_path).map_err(|e| e.to_string())?;
        game_resources::action(&conn, &state.db_path, payload)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(|e| (StatusCode::CONFLICT, e))
}

fn a4_edit_guard() -> Result<std::sync::MutexGuard<'static, ()>, (StatusCode, String)> {
    memory_cards::EDIT_LOCK
        .try_lock()
        .map_err(|error| match error {
            std::sync::TryLockError::WouldBlock => (
                StatusCode::CONFLICT,
                "Uma operação de cartões ou recursos está em andamento. Aguarde e tente novamente."
                    .into(),
            ),
            std::sync::TryLockError::Poisoned(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Falha no controle de operações".into(),
            ),
        })
}

async fn memory_cards_tool_state(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ready":memory_cards::tool_path(&state.db_path).is_file(),"path":memory_cards::tool_path(&state.db_path),"version":"0.2.0"}),
    )
}
async fn memory_cards_tool_remove(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    memory_cards::remove_tool(&state.db_path)
        .await
        .map_err(|e| (StatusCode::CONFLICT, e))?;
    Ok(Json(serde_json::json!({"ready":false})))
}
async fn game_resource_settings(
    State(state): State<Arc<AppState>>,
    Query(query): Query<resource_settings::GameQuery>,
) -> Result<Json<resource_settings::GameState>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || {
        let c = rusqlite::Connection::open(&state.db_path).map_err(|e| e.to_string())?;
        resource_settings::state(&c, &query)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(|e| (StatusCode::BAD_REQUEST, e))
}
async fn toggle_game_resource(
    State(state): State<Arc<AppState>>,
    Json(req): Json<resource_settings::ToggleRequest>,
) -> Result<Json<resource_settings::GameState>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || {
        let c = rusqlite::Connection::open(&state.db_path).map_err(|e| e.to_string())?;
        resource_settings::toggle(&c, &state.db_path, &req)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(|e| (StatusCode::CONFLICT, e))
}

async fn achievements_state_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<achievements::AchievementsState>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&state.db_path).map_err(|e| e.to_string())?;
        achievements::get_state(&conn, &state.db_path)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn achievements_login_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<achievements::LoginPayload>,
) -> Result<Json<achievements::AchievementsState>, (StatusCode, String)> {
    let result = achievements::login_account(&state.db_path, payload)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    if let Some(acc) = &result.account {
        if let Ok(mut logs) = state.logs.lock() {
            logs.push(format!(
                "Conta RetroAchievements vinculada em modo consulta: {}",
                acc.username
            ));
        }
    }
    Ok(Json(result))
}

async fn achievements_logout_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<achievements::AchievementsState>, (StatusCode, String)> {
    let db_path = state.db_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
        achievements::logout_account(&conn, &db_path)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    if let Ok(mut logs) = state.logs.lock() {
        logs.push("Conta RetroAchievements desvinculada e segredo removido do cofre.".into());
    }
    Ok(Json(result))
}

async fn achievements_game_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<achievements::GameProgressQuery>,
) -> Result<Json<achievements::RaGameProgress>, (StatusCode, String)> {
    achievements::get_or_sync_game_progress(&state.db_path, &query.game_id, query.refresh)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn achievements_game_sync_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<achievements::SyncGamePayload>,
) -> Result<Json<achievements::RaGameProgress>, (StatusCode, String)> {
    achievements::get_or_sync_game_progress(&state.db_path, &payload.game_id, true)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}


use crate::{
    AppState, catalog_refresh, extras_install, extras_sources,
    jobs::JobRecord,
    memory_cards as mc,
    online_http::{self, OnlineHttp},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
type Error = (StatusCode, String);
fn bad(e: impl std::fmt::Display) -> Error {
    (StatusCode::BAD_GATEWAY, e.to_string())
}
#[derive(Deserialize)]
pub struct SearchQuery {
    pub kind: String,
    pub source: Option<String>,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub page: usize,
    pub per_page: Option<usize>,
    pub sort: Option<String>,
    pub letter: Option<String>,
    #[serde(default)]
    pub refresh: bool,
}
#[derive(Deserialize)]
pub struct ItemsQuery {
    pub kind: String,
    pub key: String,
    #[serde(default)]
    pub refresh: bool,
}
pub async fn games(
    State(s): State<Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<extras_sources::GameResults>, Error> {
    if !extras_sources::kind_valid(&q.kind) || q.q.len() > 256 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Categoria ou busca inválida".into(),
        ));
    }
    let http = OnlineHttp::new(&s.db_path, q.refresh, None).map_err(bad)?;
    let options=extras_sources::BrowseOptions{page:q.page,per_page:q.per_page.unwrap_or(24),sort:q.sort.unwrap_or_else(||"popular".into()),letter:q.letter.unwrap_or_default().trim().to_uppercase()};
    options.validate().map_err(|e|(StatusCode::BAD_REQUEST,e))?;
    if q.kind == "saves" && q.source.as_deref().unwrap_or("gamefaqs") == "gamefaqs" {
        return crate::gamefaqs::browse(&http, &q.q, &options)
            .await
            .map(Json)
            .map_err(bad);
    }
    if q.source
        .as_deref()
        .is_some_and(|source| source != "apollo" && source != "gamefaqs")
    {
        return Err((StatusCode::BAD_REQUEST, "Fonte desconhecida".into()));
    }
    extras_sources::browse(&http, &q.kind, &q.q, &options)
        .await
        .map(Json)
        .map_err(bad)
}
pub async fn items(
    State(s): State<Arc<AppState>>,
    Query(q): Query<ItemsQuery>,
) -> Result<Json<extras_sources::ItemResults>, Error> {
    let http = OnlineHttp::new(&s.db_path, q.refresh, None).map_err(bad)?;
    extras_sources::items(&http, &q.kind, &q.key)
        .await
        .map(Json)
        .map_err(bad)
}
fn task_available(s: &AppState, kind: &str) -> Result<(), Error> {
    let jobs = s.job_manager.list_jobs().map_err(bad)?;
    let active = jobs
        .iter()
        .filter(|j| ["running", "queued", "cancel_requested"].contains(&j.status.as_str()))
        .collect::<Vec<_>>();
    if active.len() >= 3 || active.iter().any(|j| j.kind == kind) {
        return Err((
            StatusCode::CONFLICT,
            "Já existe uma tarefa deste tipo em andamento. Acompanhe em Atividade.".into(),
        ));
    }
    Ok(())
}
static START: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub async fn start(
    State(s): State<Arc<AppState>>,
    Json(r): Json<extras_install::ExtraRequest>,
) -> Result<(StatusCode, Json<JobRecord>), Error> {
    extras_install::validate_request(&r).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let job = {
        let _guard = START.lock().map_err(bad)?;
        task_available(&s, "extra")?;
        s.job_manager
            .begin_task("extra", "Baixar extra de PlayStation 2", 100)
            .map_err(bad)?
    };
    let id = job.id.clone();
    let jobs = (*s.job_manager).clone();
    let db = s.db_path.clone();
    tokio::spawn(async move {
        match extras_install::run(db, jobs.clone(), id.clone(), r).await {
            Ok(result) => {
                let _ = jobs.finish_task(&id, Ok(&result.message));
            }
            Err(error) => {
                let _ = jobs.finish_task(&id, Err(&error));
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
pub async fn operation(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Error> {
    mc::name(&id).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let job = s
        .job_manager
        .get_job(&id)
        .map_err(bad)?
        .filter(|j| j.kind == "extra")
        .ok_or((StatusCode::NOT_FOUND, "Tarefa não encontrada".into()))?;
    let path = online_http::root(&s.db_path)
        .join("operations")
        .join(format!("{id}.json"));
    let result = mc::read_bounded(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<extras_install::Receipt>(&b).ok());
    Ok(Json(serde_json::json!({"job":job,"result":result})))
}
pub async fn catalog_status(
    State(s): State<Arc<AppState>>,
) -> Json<catalog_refresh::CatalogStatus> {
    Json(catalog_refresh::status(&s.db_path))
}
pub async fn catalog_update(
    State(s): State<Arc<AppState>>,
) -> Result<(StatusCode, Json<JobRecord>), Error> {
    let job = {
        let _guard = START.lock().map_err(bad)?;
        task_available(&s, "catalog-refresh")?;
        s.job_manager
            .begin_task("catalog-refresh", "Atualizar catálogo PS2", 100)
            .map_err(bad)?
    };
    let id = job.id.clone();
    let jobs = (*s.job_manager).clone();
    let db = s.db_path.clone();
    tokio::spawn(async move {
        let result = catalog_refresh::refresh(&db, jobs.clone(), id.clone()).await;
        let _ = match result {
            Ok(()) => jobs.finish_task(
                &id,
                Ok("Catálogo atualizado; biblioteca e favoritos preservados"),
            ),
            Err(error) => jobs.finish_task(&id, Err(&error)),
        };
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

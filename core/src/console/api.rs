use super::{
    devices::{self, DeviceProbe, Profile, SystemProbe},
    inventory,
};
use crate::AppState;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};
type Error = (StatusCode, String);
type StateRef = State<Arc<AppState>>;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/console/volumes", get(volumes))
        .route("/api/console/targets", get(targets).post(register))
        .route("/api/console/targets/:id", get(target).delete(forget))
        .route("/api/console/targets/:id/profile", post(profile))
        .route("/api/console/targets/:id/inventory", get(inventory))
        .route("/api/console/targets/:id/refresh", post(refresh))
}
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<Json<T>, Error> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .map_err(|e| (StatusCode::CONFLICT, e))
}
fn connection(path: &std::path::Path) -> Result<rusqlite::Connection, String> {
    let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}
async fn volumes() -> Result<Json<Vec<devices::Volume>>, Error> {
    blocking(|| SystemProbe.volumes()).await
}
async fn targets(State(s): StateRef) -> Result<Json<Vec<devices::Target>>, Error> {
    blocking(move || devices::list(&connection(&s.db_path)?)).await
}
async fn target(
    State(s): StateRef,
    Path(id): Path<String>,
) -> Result<Json<devices::Target>, Error> {
    blocking(move || devices::get(&connection(&s.db_path)?, &id)).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Register {
    path: PathBuf,
    profile: Profile,
}
async fn register(
    State(s): StateRef,
    Json(body): Json<Register>,
) -> Result<Json<devices::Target>, Error> {
    if body.path.as_os_str().len() > 32767 {
        return Err((StatusCode::BAD_REQUEST, "Caminho muito longo.".into()));
    }
    blocking(move || {
        devices::register(
            &connection(&s.db_path)?,
            &body.path,
            body.profile,
            &SystemProbe,
        )
    })
    .await
}
async fn forget(
    State(s): StateRef,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Error> {
    blocking(move || {
        devices::forget(&connection(&s.db_path)?, &id)?;
        Ok(serde_json::json!({"removed_registration":true}))
    })
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileRequest {
    profile: Profile,
}
async fn profile(
    State(s): StateRef,
    Path(id): Path<String>,
    Json(body): Json<ProfileRequest>,
) -> Result<Json<devices::Target>, Error> {
    blocking(move || devices::set_profile(&connection(&s.db_path)?, &id, body.profile)).await
}
async fn inventory(
    State(s): StateRef,
    Path(id): Path<String>,
) -> Result<Json<inventory::InventoryView>, Error> {
    blocking(move || inventory::cached(&connection(&s.db_path)?, &id, &SystemProbe)).await
}
async fn refresh(
    State(s): StateRef,
    Path(id): Path<String>,
) -> Result<Json<inventory::InventoryView>, Error> {
    blocking(move || inventory::refresh(&connection(&s.db_path)?, &id, &SystemProbe)).await
}

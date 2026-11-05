use crate::{AppState, FavoriteItem, HistoryItem, database};
use axum::{Json, extract::State, http::StatusCode};
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const BACKUP_MAX_BYTES: usize = 16 * 1024 * 1024;

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;
fn failure(error: impl ToString) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
fn invalid(message: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProfile {
    pub name: String,
    pub bio: String,
    pub color: String,
    pub created_at: String,
}
impl Default for LocalProfile {
    fn default() -> Self {
        Self {
            name: "Jogador PlayStation".into(),
            bio: String::new(),
            color: "blue".into(),
            created_at: Utc::now().to_rfc3339(),
        }
    }
}
impl LocalProfile {
    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.chars().count() > 60
            || self.name.chars().any(char::is_control)
        {
            return Err("O nome deve ter entre 1 e 60 caracteres".into());
        }
        if self.bio.chars().count() > 300 {
            return Err("A descrição deve ter até 300 caracteres".into());
        }
        if !["blue", "green", "amber", "rose", "slate"].contains(&self.color.as_str()) {
            return Err("Cor de perfil inválida".into());
        }
        if chrono::DateTime::parse_from_rfc3339(&self.created_at).is_err() {
            return Err("Data de perfil inválida".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameProgress {
    pub game_id: String,
    pub status: String,
    pub notes: String,
    #[serde(default)]
    pub launches: u32,
    pub last_played: Option<String>,
}
impl GameProgress {
    fn validate(&self) -> Result<(), String> {
        if self.game_id.is_empty() || self.game_id.len() > 4096 || self.notes.chars().count() > 2000
        {
            return Err("Identificador ou notas do jogo inválidos".into());
        }
        if !["planned", "playing", "completed", "paused"].contains(&self.status.as_str()) {
            return Err("Status de jogo inválido".into());
        }
        if self
            .last_played
            .as_ref()
            .is_some_and(|d| chrono::DateTime::parse_from_rfc3339(d).is_err())
        {
            return Err("Data de partida inválida".into());
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct ProfileState {
    pub profile: LocalProfile,
    pub games: Vec<GameProgress>,
}

fn load_profile(conn: &Connection) -> Result<LocalProfile, String> {
    match database::get_config(conn, "local_profile") {
        Some(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string()),
        None => {
            let profile = LocalProfile::default();
            save_profile(conn, &profile)?;
            Ok(profile)
        }
    }
}
fn save_profile(conn: &Connection, profile: &LocalProfile) -> Result<(), String> {
    profile.validate()?;
    let mut profile = profile.clone();
    profile.name = profile.name.trim().to_string();
    profile.bio = profile.bio.trim().to_string();
    database::set_config(
        conn,
        "local_profile",
        &serde_json::to_string(&profile).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn load_games(conn: &Connection) -> Result<Vec<GameProgress>, String> {
    let mut stmt = conn
        .prepare("SELECT game_id, status, notes, launches, last_played FROM game_progress")
        .map_err(|e| e.to_string())?;
    stmt.query_map([], |r| {
        Ok(GameProgress {
            game_id: r.get(0)?,
            status: r.get(1)?,
            notes: r.get(2)?,
            launches: r.get(3)?,
            last_played: r.get(4)?,
        })
    })
    .map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())
}

pub async fn get_profile(State(state): State<Arc<AppState>>) -> ApiResult<ProfileState> {
    let conn = Connection::open(&state.db_path).map_err(failure)?;
    Ok(Json(ProfileState {
        profile: load_profile(&conn).map_err(failure)?,
        games: load_games(&conn).map_err(failure)?,
    }))
}
pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    Json(mut profile): Json<LocalProfile>,
) -> ApiResult<LocalProfile> {
    profile.validate().map_err(|e| invalid(&e))?;
    let conn = Connection::open(&state.db_path).map_err(failure)?;
    profile.created_at = load_profile(&conn).map_err(failure)?.created_at;
    profile.name = profile.name.trim().to_string();
    profile.bio = profile.bio.trim().to_string();
    save_profile(&conn, &profile).map_err(failure)?;
    Ok(Json(profile))
}
pub async fn update_game(
    State(state): State<Arc<AppState>>,
    Json(game): Json<GameProgress>,
) -> ApiResult<GameProgress> {
    game.validate().map_err(|e| invalid(&e))?;
    let conn = Connection::open(&state.db_path).map_err(failure)?;
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM games WHERE id = ?1)",
            params![game.game_id],
            |r| r.get(0),
        )
        .map_err(failure)?;
    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            "Jogo não encontrado na biblioteca".into(),
        ));
    }
    conn.execute("INSERT INTO game_progress (game_id, status, notes, launches, last_played) VALUES (?1, ?2, ?3, 0, NULL) ON CONFLICT(game_id) DO UPDATE SET status=excluded.status, notes=excluded.notes", params![game.game_id, game.status, game.notes]).map_err(failure)?;
    let saved = load_games(&conn)
        .map_err(failure)?
        .into_iter()
        .find(|g| g.game_id == game.game_id)
        .ok_or_else(|| failure("Não foi possível ler o progresso salvo"))?;
    Ok(Json(saved))
}

pub fn record_launch(conn: &mut Connection, path: &str) -> Result<(), String> {
    let game: Option<(String, String)> = conn
        .query_row(
            "SELECT id, title FROM games WHERE file_path=?1",
            params![path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if let Some((id, title)) = game {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO game_progress(game_id,status,notes,launches,last_played) VALUES (?1,'playing','',1,?2) ON CONFLICT(game_id) DO UPDATE SET launches=launches+1,last_played=excluded.last_played,status=CASE WHEN status='planned' THEN 'playing' ELSE status END", params![id, Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
        database::add_history(&tx, &title, "launch", Some("Iniciado no PCSX2"))
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileBackup {
    pub format: String,
    pub version: u32,
    pub profile: LocalProfile,
    pub games: Vec<GameProgress>,
    pub favorites: Vec<FavoriteItem>,
    pub history: Vec<HistoryItem>,
}
impl ProfileBackup {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "rommanager-profile" || self.version != 1 {
            return Err("Formato ou versão de backup não suportados".into());
        }
        self.profile.validate()?;
        if self.games.len() > 10000 || self.favorites.len() > 10000 || self.history.len() > 20000 {
            return Err("Backup excede os limites de itens".into());
        }
        for game in &self.games {
            game.validate()?;
        }
        for favorite in &self.favorites {
            if favorite.id.is_empty() || favorite.id.len() > 4096 || favorite.title.len() > 1024 {
                return Err("Favorito inválido no backup".into());
            }
            if !favorite.url.is_empty() {
                let url =
                    reqwest::Url::parse(&favorite.url).map_err(|_| "URL de favorito inválida")?;
                if !matches!(url.scheme(), "http" | "https") {
                    return Err("Protocolo de favorito não permitido".into());
                }
            }
        }
        for item in &self.history {
            if item.game_name.len() > 1024
                || item.action.len() > 40
                || item.details.as_ref().is_some_and(|d| d.len() > 4096)
                || chrono::DateTime::parse_from_rfc3339(&item.timestamp).is_err()
            {
                return Err("Histórico inválido no backup".into());
            }
        }
        Ok(())
    }
}

pub async fn export_profile(State(state): State<Arc<AppState>>) -> ApiResult<ProfileBackup> {
    let conn = Connection::open(&state.db_path).map_err(failure)?;
    let mut favorites = conn.prepare("SELECT id,title,cover_url,url,size,region,added_at FROM favorites ORDER BY added_at DESC").map_err(failure)?;
    let favorites = favorites
        .query_map([], |r| {
            Ok(FavoriteItem {
                id: r.get(0)?,
                title: r.get(1)?,
                cover_url: r.get(2)?,
                url: r.get(3)?,
                size: r.get(4)?,
                region: r.get(5)?,
                added_at: r.get(6)?,
            })
        })
        .map_err(failure)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failure)?;
    let mut history = conn
        .prepare("SELECT id,game_name,action,timestamp,details FROM history ORDER BY id DESC")
        .map_err(failure)?;
    let history = history
        .query_map([], |r| {
            Ok(HistoryItem {
                id: r.get(0)?,
                game_name: r.get(1)?,
                action: r.get(2)?,
                timestamp: r.get(3)?,
                details: r.get(4)?,
            })
        })
        .map_err(failure)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failure)?;
    let backup = ProfileBackup {
        format: "rommanager-profile".into(),
        version: 1,
        profile: load_profile(&conn).map_err(failure)?,
        games: load_games(&conn).map_err(failure)?,
        favorites,
        history,
    };
    backup.validate().map_err(failure)?;
    if serde_json::to_vec(&backup).map_err(failure)?.len() > BACKUP_MAX_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "O perfil excede o limite de backup de 16 MiB".into(),
        ));
    }
    Ok(Json(backup))
}

pub fn import_backup(conn: &mut Connection, backup: &ProfileBackup) -> Result<(), String> {
    backup.validate()?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    save_profile(&tx, &backup.profile)?;
    for g in &backup.games {
        tx.execute("INSERT INTO game_progress(game_id,status,notes,launches,last_played) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(game_id) DO UPDATE SET status=excluded.status, notes=excluded.notes, launches=MAX(launches,excluded.launches), last_played=CASE WHEN last_played IS NULL OR excluded.last_played>last_played THEN excluded.last_played ELSE last_played END", params![g.game_id,g.status,g.notes,g.launches,g.last_played]).map_err(|e| e.to_string())?;
    }
    for f in &backup.favorites {
        tx.execute("INSERT INTO favorites(id,title,cover_url,url,size,region,added_at) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO NOTHING", params![f.id,f.title,f.cover_url,f.url,f.size,f.region,f.added_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339())]).map_err(|e| e.to_string())?;
    }
    for h in &backup.history {
        tx.execute("INSERT INTO history(game_name,action,timestamp,details) SELECT ?1,?2,?3,?4 WHERE NOT EXISTS (SELECT 1 FROM history WHERE game_name=?1 AND action=?2 AND timestamp=?3)", params![h.game_name,h.action,h.timestamp,h.details]).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
pub async fn restore_profile(
    State(state): State<Arc<AppState>>,
    Json(backup): Json<ProfileBackup>,
) -> ApiResult<ProfileState> {
    backup.validate().map_err(|e| invalid(&e))?;
    let mut conn = Connection::open(&state.db_path).map_err(failure)?;
    import_backup(&mut conn, &backup).map_err(failure)?;
    Ok(Json(ProfileState {
        profile: load_profile(&conn).map_err(failure)?,
        games: load_games(&conn).map_err(failure)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_and_backup_validation_is_strict() {
        let mut p = LocalProfile::default();
        p.name = " ".into();
        assert!(p.validate().is_err());
        p.name = "Player".into();
        p.color = "url(evil)".into();
        assert!(p.validate().is_err());
        p.color = "green".into();
        assert!(p.validate().is_ok());
        let mut backup = ProfileBackup {
            format: "rommanager-profile".into(),
            version: 1,
            profile: p,
            games: vec![],
            favorites: vec![],
            history: vec![],
        };
        assert!(backup.validate().is_ok());
        backup.version = 2;
        assert!(backup.validate().is_err());
    }
    #[test]
    fn launch_tracking_is_persistent_and_does_not_invent_playtime() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE games(id TEXT,title TEXT,file_path TEXT); CREATE TABLE game_progress(game_id TEXT PRIMARY KEY,status TEXT,notes TEXT,launches INTEGER,last_played TEXT); CREATE TABLE history(id INTEGER PRIMARY KEY,game_name TEXT,action TEXT,timestamp TEXT,details TEXT); INSERT INTO games VALUES('game','Game','rom.iso');").unwrap();
        record_launch(&mut conn, "rom.iso").unwrap();
        record_launch(&mut conn, "rom.iso").unwrap();
        let games = load_games(&conn).unwrap();
        assert_eq!(games[0].launches, 2);
        assert_eq!(games[0].status, "playing");
        assert!(games[0].last_played.is_some());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn imports_merge_without_doubling_launches_or_losing_favorites() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE configs(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE game_progress(game_id TEXT PRIMARY KEY,status TEXT,notes TEXT,launches INTEGER,last_played TEXT); CREATE TABLE favorites(id TEXT PRIMARY KEY,title TEXT,cover_url TEXT,url TEXT,size INTEGER,region TEXT,added_at TEXT); CREATE TABLE history(id INTEGER PRIMARY KEY,game_name TEXT,action TEXT,timestamp TEXT,details TEXT);").unwrap();
        let backup = ProfileBackup {
            format: "rommanager-profile".into(),
            version: 1,
            profile: LocalProfile::default(),
            games: vec![GameProgress {
                game_id: "game".into(),
                status: "playing".into(),
                notes: "note".into(),
                launches: 4,
                last_played: None,
            }],
            favorites: vec![],
            history: vec![],
        };
        import_backup(&mut conn, &backup).unwrap();
        import_backup(&mut conn, &backup).unwrap();
        assert_eq!(load_games(&conn).unwrap()[0].launches, 4);
        let mut invalid = backup;
        invalid.games[0].status = "invalid".into();
        assert!(import_backup(&mut conn, &invalid).is_err());
        assert_eq!(load_games(&conn).unwrap()[0].status, "playing");
    }
}

#[cfg(test)]
mod transaction_tests {
    use super::*;
    #[test]
    fn import_rolls_back_profile_when_a_later_insert_fails() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE configs(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE game_progress(game_id TEXT PRIMARY KEY,status TEXT,notes TEXT,launches INTEGER,last_played TEXT); CREATE TABLE favorites(id TEXT PRIMARY KEY,title TEXT,cover_url TEXT,url TEXT,size INTEGER,region TEXT,added_at TEXT); CREATE TABLE history(id INTEGER PRIMARY KEY,game_name TEXT,action TEXT,timestamp TEXT,details TEXT); CREATE TRIGGER fail_favorite BEFORE INSERT ON favorites BEGIN SELECT RAISE(FAIL,'injected failure'); END;").unwrap();
        let mut original = LocalProfile::default();
        original.name = "Original".into();
        save_profile(&conn, &original).unwrap();
        let mut changed = original.clone();
        changed.name = "Changed".into();
        let backup = ProfileBackup {
            format: "rommanager-profile".into(),
            version: 1,
            profile: changed,
            games: vec![],
            favorites: vec![FavoriteItem {
                id: "favorite".into(),
                title: "Game".into(),
                cover_url: None,
                url: "https://example.invalid/game.chd".into(),
                size: None,
                region: None,
                added_at: None,
            }],
            history: vec![],
        };
        assert!(import_backup(&mut conn, &backup).is_err());
        assert_eq!(load_profile(&conn).unwrap().name, "Original");
    }
}

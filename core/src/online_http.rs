use crate::{game_resources, jobs::JobManager, memory_cards as mc};
use futures_util::StreamExt;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

pub const PATCH_REPO: &str = "PCSX2/pcsx2_patches";
pub const CHEAT_REPO: &str = "xs1l3n7x/pcsx2_cheats_collection";
pub const SAVE_REPO: &str = "bucanero/apollo-saves";
pub const TEXTURE_REPO: &str = "Made-in-Slovakia/rac";
pub const GAMEFAQS_ARCHIVE: &str =
    "https://archive.org/download/gamefaqs_savegames/ps2_savegames.zip";
pub const PARTS: [&str; 6] = [
    "PS2-1G1R-P1-1G1R",
    "PS2-1G1R-P2-MattB258",
    "PS2-1G1R-MattB258-P3",
    "PS2-1G1R-P4-MattB258",
    "PS2-1G1R-P5-MattB258",
    "PS2-1G1R-P6-MattB258",
];
pub fn root(db: &Path) -> PathBuf {
    db.parent().unwrap_or(Path::new(".")).join("a6")
}
pub fn permitted(url: &reqwest::Url) -> bool {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    let path = url.path();
    let repos = [PATCH_REPO, CHEAT_REPO, SAVE_REPO, TEXTURE_REPO];
    match url.host_str().unwrap_or("") {
        "api.github.com" => repos.iter().any(|repo| {
            path.starts_with(&format!("/repos/{repo}/git/trees/"))
                || path.starts_with(&format!("/repos/{repo}/git/ref/heads/"))
        }),
        "raw.githubusercontent.com" => repos
            .iter()
            .any(|repo| path.starts_with(&format!("/{repo}/"))),
        host if host == "archive.org" || host.ends_with(".archive.org") => {
            path == "/download/gamefaqs_savegames/ps2_savegames.zip"
                || path
                    .strip_prefix('/')
                    .and_then(|p| p.split_once('/'))
                    .is_some_and(|(bucket, rest)| {
                        !bucket.is_empty()
                            && bucket.bytes().all(|b| b.is_ascii_digit())
                            && rest == "items/gamefaqs_savegames/ps2_savegames.zip"
                    })
                || PARTS.iter().any(|part| path == format!("/metadata/{part}"))
        }
        _ => false,
    }
}

#[derive(Clone)]
pub struct OnlineHttp {
    client: reqwest::Client,
    db: PathBuf,
    pub stale: Arc<AtomicBool>,
    force: bool,
    task: Option<(JobManager, String)>,
    #[cfg(test)]
    pub endpoint: Option<String>,
}
impl OnlineHttp {
    pub(crate) fn force_refresh(&self) -> bool {
        self.force
    }
    pub(crate) fn cache_root(&self) -> PathBuf {
        root(&self.db)
    }
    pub fn new(db: &Path, force: bool, task: Option<(JobManager, String)>) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .user_agent("RomManager/0.5 extras")
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(90))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() < 3 && permitted(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            db: db.into(),
            force,
            stale: Arc::new(AtomicBool::new(false)),
            task,
            #[cfg(test)]
            endpoint: None,
        })
    }
    pub fn check(&self) -> Result<(), String> {
        if self
            .task
            .as_ref()
            .is_some_and(|(jobs, id)| jobs.cancelled(id))
        {
            Err("Cancelamento confirmado; destino preservado".into())
        } else {
            Ok(())
        }
    }
    async fn cancellable<T>(
        &self,
        future: impl std::future::Future<Output = T>,
    ) -> Result<T, String> {
        if self.task.is_none() {
            return Ok(future.await);
        }
        tokio::pin!(future);
        let mut timer = tokio::time::interval(Duration::from_millis(200));
        loop {
            tokio::select! { result=&mut future => return Ok(result), _=timer.tick()=>self.check()?, }
        }
    }
    pub async fn get(&self, url: &str, limit: usize, cache: bool) -> Result<Vec<u8>, String> {
        self.get_checked(url, limit, cache, |_| Ok(())).await
    }
    pub async fn get_checked<F>(
        &self,
        url: &str,
        limit: usize,
        cache: bool,
        validate: F,
    ) -> Result<Vec<u8>, String>
    where
        F: Fn(&[u8]) -> Result<(), String> + Send + Sync,
    {
        self.request_checked(url, limit, cache, None, validate)
            .await
    }
    pub async fn range_checked<F>(
        &self,
        start: u64,
        end: u64,
        validate: F,
    ) -> Result<Vec<u8>, String>
    where
        F: Fn(&[u8]) -> Result<(), String> + Send + Sync,
    {
        if start > end || end >= crate::gamefaqs::ARCHIVE_SIZE || end - start >= 32 * 1024 * 1024 {
            return Err("Intervalo do arquivo GameFAQs inválido".into());
        }
        self.request_checked(
            GAMEFAQS_ARCHIVE,
            (end - start + 1) as usize,
            true,
            Some((start, end)),
            validate,
        )
        .await
    }
    async fn request_checked<F>(
        &self,
        url: &str,
        limit: usize,
        cache: bool,
        range: Option<(u64, u64)>,
        validate: F,
    ) -> Result<Vec<u8>, String>
    where
        F: Fn(&[u8]) -> Result<(), String> + Send + Sync,
    {
        self.check()?;
        let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
        if !permitted(&parsed) {
            return Err("Fonte ou endereço não permitido".into());
        }
        let cache_key = match range {
            Some((a, b)) => format!("{url}|{a}-{b}"),
            None => url.into(),
        };
        let path = root(&self.db)
            .join("http")
            .join(mc::digest(cache_key.as_bytes()));
        let cached = if cache {
            mc::read_bounded(&path)
                .ok()
                .filter(|b| b.len() <= limit && validate(b).is_ok())
        } else {
            None
        };
        let fresh = fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|d| d.as_secs() < 21600);
        if !self.force && (fresh || range.is_some()) {
            if let Some(bytes) = cached.clone() {
                return Ok(bytes);
            }
        }
        let result = async {
            let target = url.to_string();
            #[cfg(test)]
            let target = self
                .endpoint
                .as_ref()
                .map(|base| format!("{base}{}", parsed.path()))
                .unwrap_or(target);
            let mut request = self.client.get(target);
            if let Some((a, b)) = range {
                request = request.header(reqwest::header::RANGE, format!("bytes={a}-{b}"));
            }
            let response = self
                .cancellable(request.send())
                .await?
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("Fonte respondeu HTTP {}", response.status()));
            }
            if let Some((a, b)) = range {
                let expected = format!("bytes {a}-{b}/{}", crate::gamefaqs::ARCHIVE_SIZE);
                if response.status() != reqwest::StatusCode::PARTIAL_CONTENT
                    || response
                        .headers()
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok())
                        != Some(expected.as_str())
                {
                    return Err("A fonte não confirmou o intervalo do arquivo GameFAQs".into());
                }
            }
            if response.content_length().is_some_and(|n| n > limit as u64) {
                return Err("Download acima do limite".into());
            }
            let mut stream = response.bytes_stream();
            let mut bytes = Vec::new();
            while let Some(chunk) = self.cancellable(stream.next()).await? {
                self.check()?;
                let chunk = chunk.map_err(|e| e.to_string())?;
                if chunk.len() > limit.saturating_sub(bytes.len()) {
                    return Err("Download acima do limite".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            if bytes.is_empty() {
                return Err("Resposta vazia da fonte".into());
            }
            if range.is_some() && bytes.len() != limit {
                return Err("Intervalo GameFAQs incompleto".into());
            }
            validate(&bytes)?;
            Ok(bytes)
        }
        .await;
        match result {
            Ok(bytes) => {
                self.check()?;
                if cache {
                    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                    game_resources::atomic_file(&path, &bytes)?;
                }
                Ok(bytes)
            }
            Err(error) => {
                self.check()?;
                if let Some(bytes) = cached {
                    self.stale.store(true, Ordering::Relaxed);
                    Ok(bytes)
                } else {
                    Err(error)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn verified_immutable_ranges_do_not_expire_or_recontact_the_source() {
        let folder = std::env::temp_dir().join(format!("rm-immutable-{}", mc::stamp()));
        let mut http = OnlineHttp::new(&folder.join("local.db"), false, None).unwrap();
        http.endpoint = Some("http://127.0.0.1:1".into());
        let path = root(&folder.join("local.db"))
            .join("http")
            .join(mc::digest(format!("{GAMEFAQS_ARCHIVE}|0-3").as_bytes()));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"save").unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
            .unwrap();
        assert_eq!(
            http.range_checked(0, 3, |b| if b == b"save" {
                Ok(())
            } else {
                Err("bad".into())
            })
            .await
            .unwrap(),
            b"save"
        );
        assert!(!http.stale.load(Ordering::Relaxed));
        let _ = fs::remove_dir_all(folder);
    }
    #[tokio::test]
    async fn archive_ranges_require_exact_partial_responses_and_preserve_valid_cache() {
        use axum::{Router, extract::State, http::StatusCode, routing::get};
        let folder = std::env::temp_dir().join(format!("rm-range-{}", mc::stamp()));
        let response = Arc::new(std::sync::Mutex::new((
            206u16,
            format!("bytes 0-3/{}", crate::gamefaqs::ARCHIVE_SIZE),
        )));
        async fn serve(
            State(state): State<Arc<std::sync::Mutex<(u16, String)>>>,
        ) -> (StatusCode, [(String, String); 1], Vec<u8>) {
            let (status, range) = state.lock().unwrap().clone();
            (
                StatusCode::from_u16(status).unwrap(),
                [("content-range".into(), range)],
                b"save".to_vec(),
            )
        }
        let app = Router::new()
            .route("/*path", get(serve))
            .with_state(response.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut http = OnlineHttp::new(&folder.join("local.db"), true, None).unwrap();
        http.endpoint = Some(format!("http://{address}"));
        let validate = |b: &[u8]| {
            if b == b"save" {
                Ok(())
            } else {
                Err("invalid".into())
            }
        };
        assert_eq!(http.range_checked(0, 3, validate).await.unwrap(), b"save");
        *response.lock().unwrap() = (200, String::new());
        assert_eq!(http.range_checked(0, 3, validate).await.unwrap(), b"save");
        assert!(http.stale.load(Ordering::Relaxed));
        assert!(http.range_checked(4, 7, validate).await.is_err());
        *response.lock().unwrap() = (206, format!("bytes 0-3/{}", crate::gamefaqs::ARCHIVE_SIZE));
        assert!(http.range_checked(4, 7, validate).await.is_err());
        assert!(http.range_checked(9, 1, validate).await.is_err());
        assert!(permitted(
            &"https://dn720005.ca.archive.org/0/items/gamefaqs_savegames/ps2_savegames.zip"
                .parse()
                .unwrap()
        ));
        assert!(!permitted(
            &"https://archive.org/0/items/other/ps2_savegames.zip"
                .parse()
                .unwrap()
        ));
        server.abort();
        let _ = fs::remove_dir_all(folder);
    }
    #[test]
    fn urls_reject_local_servers_userinfo_unapproved_repositories_and_archive_downloads() {
        for url in [
            "http://127.0.0.1/a",
            "https://localhost/a",
            "https://api.github.com.evil.test/repos/PCSX2/pcsx2_patches/git/trees/main",
            "https://raw.githubusercontent.com/evil/repo/main/a",
            "https://archive.org/download/game.iso",
            "https://user@raw.githubusercontent.com/PCSX2/pcsx2_patches/main/patches/a",
        ] {
            assert!(!permitted(&url.parse().unwrap()), "{url}");
        }
        assert!(permitted(
            &"https://api.github.com/repos/PCSX2/pcsx2_patches/git/trees/main?recursive=1"
                .parse()
                .unwrap()
        ));
    }
    #[tokio::test]
    async fn invalid_responses_preserve_verified_cache_and_download_limits_are_enforced() {
        use axum::{Router, extract::State, http::StatusCode, routing::get};
        let folder = std::env::temp_dir().join(format!("rm-http-{}", mc::stamp()));
        fs::create_dir_all(&folder).unwrap();
        let body = Arc::new(std::sync::Mutex::new((StatusCode::OK, b"good".to_vec())));
        async fn respond(
            State(state): State<Arc<std::sync::Mutex<(StatusCode, Vec<u8>)>>>,
        ) -> (StatusCode, Vec<u8>) {
            state.lock().unwrap().clone()
        }
        let app = Router::new()
            .route("/*path", get(respond))
            .with_state(body.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut http = OnlineHttp::new(&folder.join("local.db"), false, None).unwrap();
        http.endpoint = Some(format!("http://{address}"));
        let url = "https://raw.githubusercontent.com/PCSX2/pcsx2_patches/main/test";
        let valid = |bytes: &[u8]| {
            if bytes == b"good" {
                Ok(())
            } else {
                Err("invalid payload".into())
            }
        };
        assert_eq!(
            http.get_checked(url, 100, true, valid).await.unwrap(),
            b"good"
        );
        http.force = true;
        body.lock().unwrap().1 = b"bad".to_vec();
        assert_eq!(
            http.get_checked(url, 100, true, valid).await.unwrap(),
            b"good"
        );
        assert!(http.stale.load(Ordering::Relaxed));
        assert_eq!(
            fs::read(
                root(&folder.join("local.db"))
                    .join("http")
                    .join(mc::digest(url.as_bytes()))
            )
            .unwrap(),
            b"good"
        );
        *body.lock().unwrap() = (StatusCode::OK, vec![0; 1000]);
        assert!(http.get(url, 8, false).await.is_err());
        *body.lock().unwrap() = (StatusCode::SERVICE_UNAVAILABLE, b"offline".to_vec());
        assert_eq!(
            http.get_checked(url, 100, true, valid).await.unwrap(),
            b"good"
        );
        server.abort();
        fs::remove_dir_all(folder).unwrap();
    }
    #[tokio::test]
    async fn cancelled_or_deleted_tasks_cannot_start_network_requests() {
        let folder = std::env::temp_dir().join(format!("rm-http-cancel-{}", mc::stamp()));
        fs::create_dir_all(&folder).unwrap();
        let db = folder.join("local.db");
        crate::database::initialize_db(&db).unwrap();
        let jobs = JobManager::new(db.clone());
        let task = jobs.begin_task("extra", "QA", 100).unwrap();
        jobs.request_cancel(&task.id).unwrap();
        let http = OnlineHttp::new(&db, false, Some((jobs.clone(), task.id.clone()))).unwrap();
        assert!(
            http.get(
                "https://raw.githubusercontent.com/PCSX2/pcsx2_patches/main/test",
                100,
                false
            )
            .await
            .unwrap_err()
            .contains("Cancelamento")
        );
        jobs.delete_job(&task.id).unwrap();
        assert!(http.check().is_err());
        fs::remove_dir_all(folder).unwrap();
    }
    #[tokio::test]
    async fn cancellation_interrupts_a_server_that_never_sends_headers() {
        let folder = std::env::temp_dir().join(format!("rm-http-wait-{}", mc::stamp()));
        fs::create_dir_all(&folder).unwrap();
        let db = folder.join("local.db");
        crate::database::initialize_db(&db).unwrap();
        let jobs = JobManager::new(db.clone());
        let task = jobs.begin_task("extra", "QA", 100).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = axum::Router::new().fallback(|| async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            "late response"
        });
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut http = OnlineHttp::new(&db, false, Some((jobs.clone(), task.id.clone()))).unwrap();
        http.endpoint = Some(format!("http://{addr}"));
        let request = tokio::spawn(async move {
            http.get(
                "https://raw.githubusercontent.com/PCSX2/pcsx2_patches/main/test",
                100,
                false,
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        jobs.request_cancel(&task.id).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), request)
            .await
            .unwrap()
            .unwrap();
        assert!(result.unwrap_err().contains("Cancelamento"));
        server.abort();
        fs::remove_dir_all(folder).unwrap();
    }
}

use crate::{
    api_roms::{self, RomCatalogItem},
    game_resources,
    jobs::JobManager,
    online_http::{self, OnlineHttp},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    pub schema: u32,
    pub updated_at: String,
    pub items: Vec<RomCatalogItem>,
}
#[derive(Serialize)]
pub struct CatalogStatus {
    pub schema: u32,
    pub updated_at: Option<String>,
    pub total: usize,
    pub source: &'static str,
}
static REFRESH: Mutex<()> = Mutex::new(());
static CACHED: std::sync::LazyLock<
    Mutex<
        Option<(
            std::path::PathBuf,
            Option<crate::cache::FileStamp>,
            Arc<CatalogSnapshot>,
        )>,
    >,
> = std::sync::LazyLock::new(|| Mutex::new(None));
pub fn snapshot(db: &Path) -> Arc<CatalogSnapshot> {
    let path = online_http::root(db).join("catalog.json");
    let stamp = crate::cache::FileStamp::of(&path);
    let mut cache = CACHED.lock().unwrap();
    if let Some((known, version, data)) = &*cache {
        if known == &path && version == &stamp {
            return data.clone();
        }
    }
    let data = fs::read(&path)
        .ok()
        .filter(|b| b.len() < 16 * 1024 * 1024)
        .and_then(|b| serde_json::from_slice::<CatalogSnapshot>(&b).ok())
        .filter(|s| s.schema == 1 && validate(&s.items).is_ok())
        .unwrap_or_else(|| CatalogSnapshot {
            schema: 1,
            updated_at: String::new(),
            items: api_roms::embedded_catalog().clone(),
        });
    let data = Arc::new(data);
    *cache = Some((path, stamp, data.clone()));
    data
}
pub fn status(db: &Path) -> CatalogStatus {
    let s = snapshot(db);
    CatalogStatus {
        schema: 1,
        updated_at: (!s.updated_at.is_empty()).then(|| s.updated_at.clone()),
        total: s.items.len(),
        source: "Internet Archive · seis coleções PS2 1G1R",
    }
}
pub fn validate(items: &[RomCatalogItem]) -> Result<(), String> {
    if items.is_empty() || items.len() > 30000 {
        return Err("Quantidade inválida no catálogo".into());
    }
    let mut ids = HashSet::new();
    for i in items {
        if !online_http::PARTS.contains(&i.part.as_str())
            || i.id != format!("{}:{}", i.part, i.filename)
            || !i.filename.to_lowercase().ends_with(".chd")
            || !crate::rom_scanner::is_safe_filename(&i.filename)
            || i.size == 0
            || i.title.is_empty()
            || i.title.len() > 512
            || !ids.insert(&i.id)
        {
            return Err("Registro inválido ou duplicado no catálogo".into());
        }
        let expected = format!(
            "https://archive.org/download/{}/{}",
            i.part,
            urlencoding::encode(&i.filename)
        );
        if i.url != expected {
            return Err("URL do catálogo não corresponde ao arquivo".into());
        }
    }
    Ok(())
}
pub fn part_items(part: &str, body: &[u8]) -> Result<Vec<RomCatalogItem>, String> {
    if !online_http::PARTS.contains(&part) {
        return Err("Coleção não permitida".into());
    }
    let v: serde_json::Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    let files = v["files"]
        .as_array()
        .ok_or("Metadados da coleção inválidos")?;
    let mut out = Vec::new();
    for f in files {
        let Some(name) = f["name"]
            .as_str()
            .filter(|n| n.to_lowercase().ends_with(".chd"))
        else {
            continue;
        };
        let size = f["size"]
            .as_u64()
            .or_else(|| f["size"].as_str().and_then(|s| s.parse().ok()))
            .ok_or("Tamanho inválido na fonte")?;
        let title = name[..name.len() - 4].to_string();
        let region = if title.contains("(USA") {
            "USA"
        } else if title.contains("(Europe") {
            "Europe"
        } else if title.contains("(Japan") {
            "Japan"
        } else {
            "Other"
        };
        out.push(RomCatalogItem {
            id: format!("{part}:{name}"),
            title: title.clone(),
            filename: name.into(),
            part: part.into(),
            size,
            region: region.into(),
            url: format!(
                "https://archive.org/download/{part}/{}",
                urlencoding::encode(name)
            ),
            cover_url: api_roms::build_cover_url(&title),
            format: "CHD".into(),
            source: "Catálogo Oficial (CHD)".into(),
        });
    }
    validate(&out)?;
    Ok(out)
}
pub async fn refresh(db: &Path, jobs: JobManager, id: String) -> Result<(), String> {
    let http = OnlineHttp::new(db, true, Some((jobs.clone(), id.clone())))?;
    let mut items = Vec::new();
    for (n, part) in online_http::PARTS.iter().enumerate() {
        jobs.task_progress(
            &id,
            (n as f64) * 14.0,
            &format!("Consultando coleção {} de 6", n + 1),
        )
        .map_err(|e| e.to_string())?;
        let bytes = http
            .get(
                &format!("https://archive.org/metadata/{part}"),
                16 * 1024 * 1024,
                false,
            )
            .await?;
        items.extend(part_items(part, &bytes)?);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    http.check()?;
    publish(db, items)
}
fn publish(db: &Path, items: Vec<RomCatalogItem>) -> Result<(), String> {
    validate(&items)?;
    let _guard = REFRESH.lock().map_err(|e| e.to_string())?;
    let path = online_http::root(db).join("catalog.json");
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let data = CatalogSnapshot {
        schema: 1,
        updated_at: chrono::Utc::now().to_rfc3339(),
        items,
    };
    game_resources::atomic_file(
        &path,
        &serde_json::to_vec(&data).map_err(|e| e.to_string())?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_snapshot_does_not_replace_previous_catalog_or_local_database() {
        let root =
            std::env::temp_dir().join(format!("rm-catalog-{}", crate::memory_cards::stamp()));
        fs::create_dir_all(&root).unwrap();
        let db = root.join("local.db");
        fs::write(&db, b"personal library unchanged").unwrap();
        let items = part_items(
            online_http::PARTS[0],
            br#"{"files":[{"name":"Game (USA).chd","size":"123"}]}"#,
        )
        .unwrap();
        publish(&db, items).unwrap();
        let snapshot_path = online_http::root(&db).join("catalog.json");
        let before = fs::read(&snapshot_path).unwrap();
        assert!(publish(&db, Vec::new()).is_err());
        assert_eq!(fs::read(snapshot_path).unwrap(), before);
        assert_eq!(fs::read(&db).unwrap(), b"personal library unchanged");
        assert_eq!(snapshot(&db).items.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}

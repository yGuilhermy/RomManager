use crate::{
    extras_sources::{ExtraGame, ExtraItem, GameResults, ItemResults, RemoteFile},
    memory_cards as mc,
    online_http::{GAMEFAQS_ARCHIVE, OnlineHttp},
};
use futures_util::StreamExt;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    sync::{Arc, OnceLock},
};

pub const ARCHIVE_SIZE: u64 = 1_818_163_653;
const CENTRAL_OFFSET: u64 = 1_814_977_754;
const CENTRAL_SIZE: u64 = 3_185_877;
const CENTRAL_SHA256: &str = "ece8965b60377ea59ca392530156d8f2e72b1c01f2efe633ac50dd73714c7904";
const SOURCE: &str = "GameFAQs · arquivo de 21/04/2023";
const VERSION: &str = "2023-04-21-ad38f7a3248f2a36ace7953b22807399fbceab25";
const MAX_FILE: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug)]
struct Entry {
    path: String,
    size: u32,
    compressed: u32,
    crc: u32,
    method: u16,
    offset: u32,
}
fn u16_at(b: &[u8], n: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        b.get(n..n + 2)
            .ok_or("Índice ZIP truncado")?
            .try_into()
            .unwrap(),
    ))
}
fn u32_at(b: &[u8], n: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        b.get(n..n + 4)
            .ok_or("Índice ZIP truncado")?
            .try_into()
            .unwrap(),
    ))
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() < 1024
        && !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}
fn ps2_path(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    parts.len() == 3
        && (parts[1].starts_with("playstation2-") || parts[1].starts_with("playstation-2-"))
}
fn parse_index(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i..i + 4) != Some(b"PK\x01\x02") {
            return Err("Diretório ZIP inválido".into());
        }
        let n = u16_at(bytes, i + 28)? as usize;
        let extra = u16_at(bytes, i + 30)? as usize;
        let comment = u16_at(bytes, i + 32)? as usize;
        let end = i
            .checked_add(46 + n + extra + comment)
            .filter(|&e| e <= bytes.len())
            .ok_or("Índice ZIP truncado")?;
        let raw = bytes.get(i + 46..i + 46 + n).ok_or("Nome ZIP truncado")?;
        let path = std::str::from_utf8(raw)
            .map_err(|_| "Nome ZIP não UTF-8")?
            .to_string();
        if ps2_path(&path) && !path.ends_with('/') {
            let entry = Entry {
                path,
                size: u32_at(bytes, i + 24)?,
                compressed: u32_at(bytes, i + 20)?,
                crc: u32_at(bytes, i + 16)?,
                method: u16_at(bytes, i + 10)?,
                offset: u32_at(bytes, i + 42)?,
            };
            if !safe_path(&entry.path)
                || !seen.insert(entry.path.clone())
                || u16_at(bytes, i + 8)? & 1 != 0
                || ![0, 8].contains(&entry.method)
                || entry.size as usize > MAX_FILE
                || entry.compressed as usize > MAX_FILE
                || entry.offset as u64 + 30 + entry.compressed as u64 > CENTRAL_OFFSET
            {
                return Err("Entrada ZIP insegura ou incompatível".into());
            }
            entries.push(entry);
        }
        i = end;
    }
    Ok(entries)
}
fn bundled_index() -> Result<Arc<Vec<Entry>>, String> {
    static INDEX: OnceLock<Result<Arc<Vec<Entry>>, String>> = OnceLock::new();
    INDEX
        .get_or_init(|| {
            let bytes = include_bytes!("../data/gamefaqs-central.bin");
            if mc::digest(bytes) != CENTRAL_SHA256 {
                return Err("Índice GameFAQs embarcado inválido".into());
            }
            parse_index(bytes).map(Arc::new)
        })
        .clone()
}
async fn index(http: &OnlineHttp) -> Result<Arc<Vec<Entry>>, String> {
    let bundled = bundled_index()?;
    if !http.force_refresh() {
        return Ok(bundled);
    }
    let result = http
        .range_checked(CENTRAL_OFFSET, CENTRAL_OFFSET + CENTRAL_SIZE - 1, |b| {
            if mc::digest(b) != CENTRAL_SHA256 {
                return Err(
                    "O índice GameFAQs mudou; download suspenso para preservar a integridade"
                        .into(),
                );
            }
            parse_index(b).map(|_| ())
        })
        .await;
    if result.is_err() {
        http.stale.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    http.check()?;
    Ok(bundled)
}
fn key(title: &str) -> String {
    format!("GFQ:{}", &mc::digest(title.as_bytes())[..24])
}
fn title(path: &str) -> String {
    path.split('/').next().unwrap_or(path).replace("_ ", ": ")
}
fn cover(title: &str) -> Option<String> {
    use std::sync::OnceLock;
    static COVERS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    let normalize = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    COVERS.get_or_init(|| {
        let mut covers=BTreeMap::new();
        for game in crate::game_metadata::get_compat_list() {
            if crate::extras_sources::normalized_serial(&game.serial).is_some() {
                covers.entry(normalize(&game.title)).or_insert_with(||format!("https://raw.githubusercontent.com/xlenore/ps2-covers/main/covers/default/{}.jpg",game.serial));
            }
        }
        covers
    }).get(&normalize(title)).cloned()
}
fn supported(path: &str) -> bool {
    ["max", "zip", "cbs", "xps", "sps", "xpo"].contains(&path.rsplit('.').next().unwrap_or(""))
}
fn format_name(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "max" => "Max Drive",
        "zip" => "PSV / ZIP",
        "cbs" => "CodeBreaker",
        "sps" => "SharkPort",
        "xps" => "X-Port",
        _ => "Xploder",
    }
}
fn region(path: &str) -> &'static str {
    let group = path.split('/').nth(1).unwrap_or("");
    if group.ends_with("north-america") {
        "América do Norte"
    } else if group.ends_with("europe") {
        "Europa"
    } else if group.ends_with("japan") {
        "Japão"
    } else {
        "Outra região"
    }
}
pub async fn games(http: &OnlineHttp, q: &str, page: usize) -> Result<GameResults, String> {
    browse(http,q,&crate::extras_sources::BrowseOptions{page,..Default::default()}).await
}
pub async fn browse(http:&OnlineHttp,q:&str,options:&crate::extras_sources::BrowseOptions)->Result<GameResults,String>{
    let entries = index(http).await?;
    let mut games = BTreeMap::new();
    for entry in entries.iter().filter(|e| supported(&e.path)) {
        let raw = entry.path.split('/').next().unwrap();
        let title = title(&entry.path);
            games.entry(key(raw)).or_insert(ExtraGame {
                cover_url: cover(&title),
                key: key(raw),
                title,
                serial: None,
                crcs: vec![],
            });
    }
    crate::extras_sources::browse_games(games.into_values().collect(),q,options,SOURCE.into(),VERSION.into(),http.stale.load(std::sync::atomic::Ordering::Relaxed))
}
fn check_header(header: &[u8], entry: &Entry) -> Result<(), String> {
    if header.len() != 30
        || &header[..4] != b"PK\x03\x04"
        || u16_at(header, 6)? & 1 != 0
        || u16_at(header, 8)? != entry.method
        || u32_at(header, 14)? != entry.crc
        || u32_at(header, 18)? != entry.compressed
        || u32_at(header, 22)? != entry.size
        || u16_at(header, 26)? as usize != entry.path.len()
    {
        return Err("Cabeçalho do save diverge do índice GameFAQs".into());
    }
    Ok(())
}
fn decode(entry: &Entry, header: &[u8], body: &[u8]) -> Result<Vec<u8>, String> {
    check_header(header, entry)?;
    let prefix = entry.path.len() + u16_at(header, 28)? as usize;
    if body.get(..entry.path.len()) != Some(entry.path.as_bytes())
        || body.len() != prefix + entry.compressed as usize
    {
        return Err("Conteúdo ZIP truncado ou nome divergente".into());
    }
    let compressed = &body[prefix..];
    let mut output = Vec::new();
    if entry.method == 8 {
        flate2::read::DeflateDecoder::new(compressed)
            .take(entry.size as u64 + 1)
            .read_to_end(&mut output)
            .map_err(|e| e.to_string())?;
    } else {
        output.extend_from_slice(compressed);
    }
    let mut crc = flate2::Crc::new();
    crc.update(&output);
    if output.len() != entry.size as usize || crc.sum() != entry.crc {
        return Err("Save GameFAQs corrompido: tamanho ou CRC32 divergente".into());
    }
    Ok(output)
}
async fn read_entry(http: &OnlineHttp, entry: &Entry) -> Result<Vec<u8>, String> {
    let start = entry.offset as u64;
    let header = http
        .range_checked(start, start + 29, |b| check_header(b, entry))
        .await?;
    let length = entry.path.len() as u64 + u16_at(&header, 28)? as u64 + entry.compressed as u64;
    if length == 0 || length > MAX_FILE as u64 {
        return Err("Save acima do limite".into());
    }
    let body = http
        .range_checked(start + 30, start + 29 + length, |b| {
            decode(entry, &header, b).map(|_| ())
        })
        .await?;
    decode(entry, &header, &body)
}
fn descriptions(text: &str) -> BTreeMap<String, (String, String)> {
    let mut result = BTreeMap::new();
    let mut id = String::new();
    let mut author = String::new();
    let mut lines = Vec::new();
    for line in text.lines().chain(std::iter::once("0")) {
        let line = line.trim();
        if !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()) {
            if !id.is_empty() {
                result.insert(id.clone(), (author.clone(), lines.join("\n")));
            }
            id = line.into();
            author.clear();
            lines.clear();
        } else if let Some(from) = line.strip_prefix("From ") {
            author = from
                .split(" (")
                .next()
                .unwrap_or(from)
                .chars()
                .take(100)
                .collect();
        } else if lines.len() < 30 {
            lines.push(line.to_string());
        }
    }
    result
}
type SaveDescriptions = BTreeMap<String, BTreeMap<String, (String, String)>>;
#[derive(serde::Serialize, serde::Deserialize)]
struct DescriptionCache {
    version: String,
    groups: SaveDescriptions,
}
async fn load_descriptions(
    http: &OnlineHttp,
    entries: &[Entry],
    game_key: &str,
    prefix: &str,
) -> Result<SaveDescriptions, String> {
    let path = http
        .cache_root()
        .join("gamefaqs/descriptions")
        .join(format!("{}.json", mc::digest(game_key.as_bytes())));
    let cached = mc::read_bounded(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<DescriptionCache>(&b).ok())
        .filter(|c| c.version == VERSION);
    if !http.force_refresh() {
        if let Some(c) = cached {
            return Ok(c.groups);
        }
    }
    let mut groups = cached.map(|c| c.groups).unwrap_or_default();
    let requests = entries
        .iter()
        .filter(|e| e.path.starts_with(prefix) && e.path.ends_with("/description.txt"))
        .take(16)
        .cloned()
        .collect::<Vec<_>>();
    let transport = http.clone();
    let mut pending = futures_util::stream::iter(requests)
        .map(move |entry: Entry| {
            let http = transport.clone();
            async move {
                let result = read_entry(&http, &entry).await;
                (entry, result)
            }
        })
        .buffer_unordered(3);
    let work = async {
        while let Some((entry, result)) = pending.next().await {
            if let Ok(bytes) = result {
                groups.insert(
                    entry.path.rsplit_once('/').unwrap().0.to_string(),
                    descriptions(&String::from_utf8_lossy(&bytes)),
                );
            } else {
                http.stale.store(true,std::sync::atomic::Ordering::Relaxed);
            }
            http.check()?;
        }
        Ok::<(), String>(())
    };
    if let Ok(result) = tokio::time::timeout(std::time::Duration::from_secs(8), work).await {
        result?;
    } else {
        http.stale.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    http.check()?;
    if let Ok(bytes) = serde_json::to_vec(&DescriptionCache {
        version: VERSION.into(),
        groups: groups.clone(),
    }) {
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_ok() {
                let _ = crate::game_resources::atomic_file(&path, &bytes);
            }
        }
    }
    Ok(groups)
}
pub async fn items(http: &OnlineHttp, game_key: &str) -> Result<ItemResults, String> {
    let entries = index(http).await?;
    let game = entries
        .iter()
        .find(|e| key(e.path.split('/').next().unwrap()) == game_key)
        .ok_or("Jogo ausente no arquivo GameFAQs")?;
    let raw = game.path.split('/').next().unwrap();
    let prefix = format!("{raw}/");
    let descriptions_by_group = load_descriptions(http, &entries, game_key, &prefix).await?;
    let mut items = Vec::new();
    for entry in entries
        .iter()
        .filter(|e| e.path.starts_with(&prefix) && supported(&e.path))
    {
        let (group, file) = entry.path.rsplit_once('/').unwrap();
        let number = file.split('.').next().unwrap();
        let (author, description) = descriptions_by_group
            .get(group)
            .and_then(|d| d.get(number))
            .cloned()
            .unwrap_or_default();
        let installable = entry.path.ends_with(".max") || entry.path.ends_with(".zip");
        items.push(ExtraItem{id:mc::digest(format!("{VERSION}|{}",entry.path).as_bytes()), game_key:game_key.into(),title:format!("Save #{number} · {} · {}",format_name(&entry.path),region(&entry.path)),description,kind:"saves".into(),serial:None,crc:None,source:SOURCE.into(),source_url:"https://archive.org/details/gamefaqs_savegames".into(),author:if author.is_empty(){"Colaborador GameFAQs".into()}else{author},license:"Não informada pelos autores; direitos dos respectivos colaboradores".into(),version:VERSION.into(),bytes:entry.size as u64,groups:vec![],installable,install_note:if installable {"Confira a região e a revisão descritas. Aplicação requer myMCpp; ZIP deve conter um único PSV de PS2.".into()}else{"Formato disponível para download; conversão automática ainda não suportada.".into()},files:vec![RemoteFile{path:entry.path.clone(),url:GAMEFAQS_ARCHIVE.into(),size:entry.size as u64,blob:format!("{:08x}",entry.crc)}]});
    }
    items.sort_by_key(|i| (!i.installable, i.title.clone()));
    Ok(ItemResults {
        items,
        stale: http.stale.load(std::sync::atomic::Ordering::Relaxed),
        source: SOURCE.into(),
        version: VERSION.into(),
    })
}
pub async fn download(http: &OnlineHttp, item: &ExtraItem) -> Result<(String, Vec<u8>), String> {
    let entries = index(http).await?;
    let path = &item.files.first().ok_or("Save sem arquivo")?.path;
    let entry = entries
        .iter()
        .find(|e| {
            &e.path == path
                && supported(path)
                && key(path.split('/').next().unwrap()) == item.game_key
        })
        .ok_or("Save não encontrado no índice fixado")?;
    let bytes = read_entry(http, entry).await?;
    Ok((entry.path.rsplit('/').next().unwrap().to_string(), bytes))
}

pub fn prepare_psu(bytes: &[u8], extension: &str) -> Result<Vec<u8>, String> {
    if extension != "zip" {
        return Err("Use a conversão MAX com myMCpp".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    if zip.len() != 1 {
        return Err(
            "O ZIP contém vários arquivos. Baixe e importe o save desejado separadamente.".into(),
        );
    }
    let f = zip.by_index(0).map_err(|e| e.to_string())?;
    if f.enclosed_name().is_none()
        || !f.name().to_lowercase().ends_with(".psv")
        || f.size() > MAX_FILE as u64
        || f.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
    {
        return Err("O pacote não contém um único PSV de PS2 compatível".into());
    }
    let mut raw = Vec::new();
    f.take(MAX_FILE as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    if raw.len() > MAX_FILE {
        return Err("Save expandido acima do limite".into());
    }
    crate::save_formats::psv_to_psu(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn bundled_catalog_browses_without_a_source_connection() {
        let folder = std::env::temp_dir().join(format!("rm-gfq-browse-{}", mc::stamp()));
        let mut http = OnlineHttp::new(&folder.join("local.db"), false, None).unwrap();
        http.endpoint = Some("http://127.0.0.1:1".into());
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), games(&http, "", 1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.total, 1853);
        assert!(result.games[0].title.contains("San Andreas"));
        assert_eq!(
            bundled_index()
                .unwrap()
                .iter()
                .filter(|e| supported(&e.path))
                .count(),
            8909
        );
        let _ = std::fs::remove_dir_all(folder);
    }
    #[tokio::test]
    async fn stalled_descriptions_return_the_list_and_reopening_uses_disk_cache() {
        use axum::{Router, routing::get};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let hits = Arc::new(AtomicUsize::new(0));
        let requests = hits.clone();
        let app = Router::new().route(
            "/*path",
            get(move || {
                let requests = requests.clone();
                async move {
                    requests.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    "late"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let folder = std::env::temp_dir().join(format!("rm-gfq-cache-{}", mc::stamp()));
        let db = folder.join("local.db");
        let mut http = OnlineHttp::new(&db, false, None).unwrap();
        http.endpoint = Some(format!("http://{address}"));
        let game_key = key("007_ NightFire");
        let first =
            tokio::time::timeout(std::time::Duration::from_secs(10), items(&http, &game_key))
                .await
                .unwrap()
                .unwrap();
        assert!(!first.items.is_empty());
        let count = hits.load(Ordering::Relaxed);
        assert!(count > 0 && count <= 3);
        let mut reopened = OnlineHttp::new(&db, false, None).unwrap();
        reopened.endpoint = Some(format!("http://{address}"));
        let second = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            items(&reopened, &game_key),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(second.items.len(), first.items.len());
        assert_eq!(hits.load(Ordering::Relaxed), count);
        server.abort();
        let _ = std::fs::remove_dir_all(folder);
    }
    #[test]
    fn ps2_filter_excludes_other_platforms_and_unsafe_paths() {
        assert!(ps2_path("Game/playstation2-max-drive-save-europe/12.max"));
        assert!(!ps2_path(
            "Game/playstation-3-game-save-directory-zip-europe/12.zip"
        ));
        assert!(!ps2_path("Game/xbox-save/12.zip"));
        for p in ["../a", "a/../b", "a\\b", "/a", "a//b", "a/b:c"] {
            assert!(!safe_path(p));
        }
        assert_ne!(key("God of War"), key("God of War II"));
        assert!(key("x").len() <= 32);
    }
    #[test]
    fn gamefaqs_application_requires_explicit_revision_confirmation_but_download_does_not() {
        let body = serde_json::json!({"action":"download","kind":"saves","game_key":"GFQ:test","item_id":"a".repeat(64)});
        let mut request: crate::extras_install::ExtraRequest =
            serde_json::from_value(body).unwrap();
        assert!(crate::extras_install::validate_request(&request).is_ok());
        request.action = "install".into();
        request.installation_id = "a".repeat(64);
        request.card = "test.ps2".into();
        request.card_revision = "r".into();
        assert!(
            crate::extras_install::validate_request(&request)
                .unwrap_err()
                .contains("Confirme")
        );
        request.save_confirmed = true;
        assert!(crate::extras_install::validate_request(&request).is_ok());
    }
    #[test]
    fn descriptions_keep_authors_and_individual_saves() {
        let parsed = descriptions(
            "12\r\nFrom Alice (01/01/2000; 2KB)\r\n100% complete\r\n13\r\nFrom Bob (02/01/2000; 3KB)\r\nNew game",
        );
        assert_eq!(parsed["12"], ("Alice".into(), "100% complete".into()));
        assert_eq!(parsed["13"].0, "Bob");
    }
    #[test]
    fn decoder_rejects_crc_size_and_header_mismatch() {
        let path = "Game/playstation2-max-drive-save-europe/1.max";
        let bytes = b"save";
        let mut crc = flate2::Crc::new();
        crc.update(bytes);
        let entry = Entry {
            path: path.into(),
            size: 4,
            compressed: 4,
            crc: crc.sum(),
            method: 0,
            offset: 0,
        };
        let mut header = vec![0u8; 30];
        header[..4].copy_from_slice(b"PK\x03\x04");
        header[14..18].copy_from_slice(&entry.crc.to_le_bytes());
        header[18..22].copy_from_slice(&4u32.to_le_bytes());
        header[22..26].copy_from_slice(&4u32.to_le_bytes());
        header[26..28].copy_from_slice(&(path.len() as u16).to_le_bytes());
        let mut body = path.as_bytes().to_vec();
        body.extend_from_slice(bytes);
        assert_eq!(decode(&entry, &header, &body).unwrap(), bytes);
        *body.last_mut().unwrap() = 0;
        assert!(decode(&entry, &header, &body).is_err());
        assert!(decode(&entry, &header, &[]).is_err());
        header[6] = 1;
        assert!(decode(&entry, &header, &body).is_err());
        assert!(parse_index(b"bad").is_err());
    }
}

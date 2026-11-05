use crate::database;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pcsx2CompatEntry {
    pub title: String,
    #[serde(default)]
    pub serial: String,
    #[serde(default)]
    pub crc: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_tested_version: Option<String>,
    #[serde(default)]
    pub last_tested_date: Option<String>,
    #[serde(default)]
    pub wiki_link: Option<String>,
    #[serde(default)]
    pub forum_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMetadataResponse {
    pub title: String,
    pub description: Option<String>,
    pub source: Option<String>,
    pub compatibility: Option<Pcsx2CompatEntry>,
}

static COMPAT_LIST: OnceLock<Vec<Pcsx2CompatEntry>> = OnceLock::new();

pub fn get_compat_list() -> &'static [Pcsx2CompatEntry] {
    COMPAT_LIST.get_or_init(|| {
        let raw = include_str!("../data/pcsx2_compat.json");
        serde_json::from_str(raw).unwrap_or_default()
    })
}

pub fn find_compatibility(title: &str, serial: Option<&str>) -> Option<Pcsx2CompatEntry> {
    let list = get_compat_list();

    if let Some(s) = serial {
        let clean_s = s.trim().to_uppercase().replace("-", "").replace("_", "");
        if !clean_s.is_empty() {
            for entry in list {
                let entry_s = entry
                    .serial
                    .to_uppercase()
                    .replace("-", "")
                    .replace("_", "");
                if entry_s == clean_s {
                    return Some(entry.clone());
                }
            }
        }
    }

    let norm_title = normalize_for_match(title);
    if norm_title.is_empty() {
        return None;
    }

    for entry in list {
        let norm_entry = normalize_for_match(&entry.title);
        if norm_entry == norm_title {
            return Some(entry.clone());
        }
    }

    for entry in list {
        let norm_entry = normalize_for_match(&entry.title);
        if norm_entry.contains(&norm_title) || norm_title.contains(&norm_entry) {
            return Some(entry.clone());
        }
    }

    None
}

fn normalize_for_match(s: &str) -> String {
    let re = regex::Regex::new(r"\s*\([^)]*\)|\s*\[[^\]]*\]").unwrap();
    let cleaned = re.replace_all(s, "");
    cleaned
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn clean_game_title(title: &str) -> String {
    let re = regex::Regex::new(r"\s*\([^)]*\)|\s*\[[^\]]*\]").unwrap();
    let cleaned = re.replace_all(title, "");
    cleaned.trim().to_string()
}

async fn try_fetch_wikipedia_summary(
    client: &reqwest::Client,
    lang: &str,
    page: &str,
) -> Option<String> {
    let encoded = urlencoding::encode(page);
    let url = format!(
        "https://{}.wikipedia.org/api/rest_v1/page/summary/{}",
        lang, encoded
    );
    let resp = client.get(&url).send().await.ok()?;
    if resp.status().is_success() {
        let json: serde_json::Value = resp.json().await.ok()?;
        if let Some(t) = json.get("type").and_then(|v| v.as_str()) {
            if t == "disambiguation" {
                return None;
            }
        }
        if let Some(extract) = json.get("extract").and_then(|v| v.as_str()) {
            if extract.len() > 30 {
                return Some(extract.to_string());
            }
        }
    }
    None
}

async fn try_wikipedia_search(client: &reqwest::Client, lang: &str, query: &str) -> Option<String> {
    let q = format!("{} video game", query);
    let encoded = urlencoding::encode(&q);
    let url = format!(
        "https://{}.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&utf8=&format=json",
        lang, encoded
    );
    let resp = client.get(&url).send().await.ok()?;
    if resp.status().is_success() {
        let json: serde_json::Value = resp.json().await.ok()?;
        let first_title = json
            .get("query")
            .and_then(|q| q.get("search"))
            .and_then(|s| s.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("title"))
            .and_then(|t| t.as_str())?;

        return try_fetch_wikipedia_summary(client, lang, first_title).await;
    }
    None
}

pub async fn fetch_game_description(title: &str) -> Option<(String, String)> {
    let clean_title = clean_game_title(title);
    if clean_title.is_empty() {
        return None;
    }

    let client = match reqwest::Client::builder()
        .user_agent("RomManager/1.0 (https://github.com/RomManager)")
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    let pt_candidates = vec![
        clean_title.clone(),
        format!("{} (jogo eletrônico)", clean_title),
        format!("{} (jogo)", clean_title),
    ];

    for cand in pt_candidates {
        if let Some(desc) = try_fetch_wikipedia_summary(&client, "pt", &cand).await {
            return Some((desc, "Wikipédia (PT)".to_string()));
        }
    }

    let en_candidates = vec![
        clean_title.clone(),
        format!("{} (video game)", clean_title),
        format!("{} (game)", clean_title),
    ];

    for cand in en_candidates {
        if let Some(desc) = try_fetch_wikipedia_summary(&client, "en", &cand).await {
            return Some((desc, "Wikipedia (EN)".to_string()));
        }
    }

    if let Some(desc) = try_wikipedia_search(&client, "pt", &clean_title).await {
        return Some((desc, "Wikipédia (PT)".to_string()));
    }

    if let Some(desc) = try_wikipedia_search(&client, "en", &clean_title).await {
        return Some((desc, "Wikipedia (EN)".to_string()));
    }

    None
}

pub async fn get_metadata_for_game(
    db_path: &std::path::Path,
    title: &str,
    serial: Option<&str>,
) -> GameMetadataResponse {
    let serial = serial.filter(|s| {
        regex::Regex::new(r"^[A-Za-z]{4}[-_]?[0-9]{5}$")
            .unwrap()
            .is_match(s)
    });
    let cache_id = format!("v2:{}", serial.unwrap_or(title).trim().to_uppercase());

    let cached = {
        if let Ok(conn) = rusqlite::Connection::open(db_path) {
            database::get_game_metadata(&conn, &cache_id)
        } else {
            None
        }
    };

    let (description, source) = if let Some((cached_desc, cached_src)) = cached {
        (Some(cached_desc), Some(cached_src))
    } else if let Some((desc, src)) = fetch_game_description(title).await {
        if let Ok(conn) = rusqlite::Connection::open(db_path) {
            let _ = database::save_game_metadata(&conn, &cache_id, title, &desc, &src);
        }
        (Some(desc), Some(src))
    } else {
        (None, None)
    };

    let compatibility = find_compatibility(title, serial);

    GameMetadataResponse {
        title: title.to_string(),
        description,
        source,
        compatibility,
    }
}

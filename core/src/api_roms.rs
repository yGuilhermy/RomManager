use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

static CATALOG: OnceLock<Vec<RomCatalogItem>> = OnceLock::new();
static POPULAR_IDS: OnceLock<HashMap<String, usize>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RomCatalogItem {
    pub id: String,
    pub title: String,
    pub filename: String,
    pub part: String,
    pub size: u64,
    pub region: String,
    pub url: String,
    pub cover_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogResponse {
    pub items: Vec<RomCatalogItem>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
}

fn get_catalog() -> &'static Vec<RomCatalogItem> {
    CATALOG.get_or_init(|| {
        let raw = include_str!("catalog.json");
        serde_json::from_str(raw).unwrap_or_default()
    })
}

fn get_popular_map() -> &'static HashMap<String, usize> {
    POPULAR_IDS.get_or_init(|| {
        let raw = include_str!("popular_ids.json");
        let list: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
        let mut map = HashMap::new();
        for (i, id) in list.into_iter().enumerate() {
            map.insert(id, i);
        }
        map
    })
}

pub fn query_catalog(
    query: Option<&str>,
    region: Option<&str>,
    category: Option<&str>,
    sort: Option<&str>,
    letter: Option<&str>,
    page: usize,
    per_page: usize,
) -> CatalogResponse {
    let catalog = get_catalog();
    let popular_map = get_popular_map();

    let q = query.map(|s| s.trim().to_lowercase()).unwrap_or_default();
    let reg = region.map(|s| s.trim()).unwrap_or_default();
    let cat = category
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default();
    let srt = sort
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "relevance".to_string());
    let let_filter = letter.map(|s| s.trim().to_uppercase()).unwrap_or_default();

    let is_popular_mode = srt == "popular" || cat == "popular";

    let words: Vec<&str> = q.split_whitespace().collect();

    let mut filtered: Vec<RomCatalogItem> = catalog
        .iter()
        .filter(|item| {
            let title_lower = item.title.to_lowercase();
            let file_lower = item.filename.to_lowercase();

            if !words.is_empty() {
                let matches_all_words = words
                    .iter()
                    .all(|w| title_lower.contains(w) || file_lower.contains(w));
                if !matches_all_words {
                    return false;
                }
            }

            if !let_filter.is_empty() {
                if let_filter == "#" || let_filter == "0-9" {
                    let first_char = item.title.chars().next().unwrap_or(' ');
                    if first_char.is_alphabetic() {
                        return false;
                    }
                } else if let Some(target_char) = let_filter.chars().next() {
                    let first_char = item.title.to_uppercase().chars().next().unwrap_or(' ');
                    if first_char != target_char {
                        return false;
                    }
                }
            }

            if !reg.is_empty() && !reg.eq_ignore_ascii_case("all") {
                if !item.region.eq_ignore_ascii_case(reg) {
                    return false;
                }
            }

            if !cat.is_empty() && cat != "popular" {
                let matches_cat = title_lower.contains(&cat) || file_lower.contains(&cat);
                if !matches_cat {
                    return false;
                }
            }

            true
        })
        .cloned()
        .collect();

    match srt.as_str() {
        "popular" => {
            filtered.sort_by(|a, b| {
                let rank_a = popular_map.get(&a.id).copied().unwrap_or(usize::MAX);
                let rank_b = popular_map.get(&b.id).copied().unwrap_or(usize::MAX);
                if rank_a != rank_b {
                    rank_a.cmp(&rank_b)
                } else {
                    a.title.to_lowercase().cmp(&b.title.to_lowercase())
                }
            });
        }
        "title_asc" => {
            filtered.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        "title_desc" => {
            filtered.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()));
        }
        "size_desc" => {
            filtered.sort_by(|a, b| b.size.cmp(&a.size));
        }
        "size_asc" => {
            filtered.sort_by(|a, b| a.size.cmp(&b.size));
        }
        "relevance" => {
            if is_popular_mode {
                filtered.sort_by(|a, b| {
                    let rank_a = popular_map.get(&a.id).copied().unwrap_or(usize::MAX);
                    let rank_b = popular_map.get(&b.id).copied().unwrap_or(usize::MAX);
                    if rank_a != rank_b {
                        rank_a.cmp(&rank_b)
                    } else {
                        a.title.to_lowercase().cmp(&b.title.to_lowercase())
                    }
                });
            } else if !q.is_empty() {
                filtered.sort_by(|a, b| {
                    let a_title = a.title.to_lowercase();
                    let b_title = b.title.to_lowercase();

                    let a_starts = a_title.starts_with(&q);
                    let b_starts = b_title.starts_with(&q);

                    if a_starts && !b_starts {
                        std::cmp::Ordering::Less
                    } else if !a_starts && b_starts {
                        std::cmp::Ordering::Greater
                    } else {
                        a_title.len().cmp(&b_title.len())
                    }
                });
            } else {
                filtered.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
            }
        }
        _ => {}
    }

    let total = filtered.len();
    let safe_per_page = if per_page == 0 { 24 } else { per_page.min(100) };
    let safe_page = if page == 0 { 1 } else { page };
    let total_pages = if total == 0 {
        1
    } else {
        (total + safe_per_page - 1) / safe_per_page
    };

    let start = (safe_page - 1).saturating_mul(safe_per_page);
    let items = if start >= total {
        Vec::new()
    } else {
        filtered
            .into_iter()
            .skip(start)
            .take(safe_per_page)
            .collect()
    };

    CatalogResponse {
        items,
        total,
        page: safe_page,
        per_page: safe_per_page,
        total_pages,
    }
}

pub fn build_cover_url(title: &str) -> String {
    let mut clean = title.trim().to_string();
    if clean.contains("(USA, Canada)") {
        clean = clean.replace("(USA, Canada)", "(USA)");
    }
    for ch in ['&', ':', '/', '*', '?', '"', '<', '>', '|'] {
        clean = clean.replace(ch, "_");
    }
    let encoded = urlencoding::encode(&clean);
    format!(
        "https://cdn.jsdelivr.net/gh/libretro-thumbnails/Sony_-_PlayStation_2@master/Named_Boxarts/{}.png",
        encoded
    )
}

pub async fn search_translations(query: &str) -> Result<Vec<RomCatalogItem>, String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())?;

    let q = if query.trim().is_empty() {
        "portuguese".to_string()
    } else {
        query.trim().to_string()
    };

    let search_url = format!(
        "https://cdromance.org/?s={}&platform=ps2-iso",
        urlencoding::encode(&q)
    );

    let html = client
        .get(&search_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let link_re = Regex::new(r#"<a\s+href="(https://cdromance\.org/ps2-iso/[^"/]+/)""#)
        .map_err(|e| e.to_string())?;

    let mut game_pages = Vec::new();
    for cap in link_re.captures_iter(&html) {
        if let Some(m) = cap.get(1) {
            let u = m.as_str().to_string();
            if !game_pages.contains(&u) && !u.contains("#") {
                game_pages.push(u);
            }
        }
    }

    let id_re = Regex::new(r#"data-id="(\d+)""#).map_err(|e| e.to_string())?;
    let title_re =
        Regex::new(r#"<h1[^>]*class="entry-title"[^>]*>(.*?)</h1>"#).map_err(|e| e.to_string())?;

    let mut results = Vec::new();

    for page_url in game_pages.into_iter().take(6) {
        let page_html = match client.get(&page_url).send().await {
            Ok(r) => match r.text().await {
                Ok(t) => t,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        let post_id = match id_re.captures(&page_html).and_then(|c| c.get(1)) {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };

        let page_title = title_re
            .captures(&page_html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| "PS2 Game".to_string());

        let ajax_url = "https://cdromance.org/wp-content/plugins/cdr-main/public/ajax.php";
        let params = [("post_id", &post_id)];

        let ajax_res = match client
            .post(ajax_url)
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", &page_url)
            .form(&params)
            .send()
            .await
        {
            Ok(r) => match r.text().await {
                Ok(t) => t,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        let dl_re =
            Regex::new(r#"href="(https://[^"]+download\.php[^"]+)""#).map_err(|e| e.to_string())?;
        let fname_re = Regex::new(r#"data-filename="([^"]+)""#).map_err(|e| e.to_string())?;

        let first_dl = dl_re
            .captures(&ajax_res)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());
        let filename = fname_re
            .captures(&ajax_res)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| format!("{}.7z", page_title));

        if let Some(dl_url) = first_dl {
            let cover_url = build_cover_url(&page_title);
            results.push(RomCatalogItem {
                id: format!("cdr:{}", post_id),
                title: page_title,
                filename,
                part: "CDRomance".to_string(),
                size: 0,
                region: "PT-BR / Mod".to_string(),
                url: dl_url,
                cover_url,
            });
        }
    }

    Ok(results)
}

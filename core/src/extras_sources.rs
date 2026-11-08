use crate::{
    game_metadata, game_resources as gr, memory_cards as mc, native_patches,
    online_http::{self, OnlineHttp},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn popularity_title(title: &str) -> String {
    title
        .split('(')
        .next()
        .unwrap_or(title)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn popularity_rank(title: &str) -> usize {
    static RANKS: OnceLock<BTreeMap<String, usize>> = OnceLock::new();
    let ranks = RANKS.get_or_init(|| {
        let mut ranks = BTreeMap::new();
        for game in crate::api_roms::embedded_catalog() {
            if let Some(&rank) = crate::api_roms::get_popular_map().get(&game.id) {
                ranks
                    .entry(popularity_title(&game.title))
                    .and_modify(|old: &mut usize| *old = (*old).min(rank))
                    .or_insert(rank);
            }
        }
        ranks
    });
    ranks
        .get(&popularity_title(title))
        .copied()
        .unwrap_or(usize::MAX)
}

pub(crate) fn sort_games(games: &mut [ExtraGame]) {
    games.sort_by_cached_key(|g| {
        (
            popularity_rank(&g.title),
            g.title.to_lowercase(),
            g.key.clone(),
        )
    });
}

#[cfg(test)]
mod popularity_tests {
    use super::*;

    fn game(title: &str, key: &str) -> ExtraGame {
        ExtraGame {
            cover_url: None,
            title: title.into(),
            key: key.into(),
            serial: Some(key.into()),
            crcs: vec![],
        }
    }

    #[test]
    fn popular_games_precede_alphabetical_results_without_removing_editions() {
        let mut games = vec![
            game("A new unknown game", "SLUS-00000"),
            game("God of War (Europe)", "SCES-53133"),
            game("Grand Theft Auto: San Andreas", "SLUS-20946"),
            game("God of War (USA)", "SCUS-97399"),
        ];
        sort_games(&mut games);
        assert_eq!(games.len(), 4);
        assert_eq!(games[0].key, "SLUS-20946");
        assert_eq!(games[1].key, "SCES-53133");
        assert_eq!(games[2].key, "SCUS-97399");
        assert_eq!(games[3].key, "SLUS-00000");
    }

    #[test]
    fn unknown_titles_keep_alphabetical_order_and_sequels_do_not_inherit_rank() {
        let mut games = vec![
            game("Z unknown", "Z"),
            game("A unknown", "B"),
            game("A unknown", "A"),
        ];
        sort_games(&mut games);
        assert_eq!(
            games.iter().map(|g| g.key.as_str()).collect::<Vec<_>>(),
            vec!["A", "B", "Z"]
        );
        assert_eq!(popularity_rank("God of War invented sequel"), usize::MAX);
        assert_ne!(popularity_rank("God of War"), usize::MAX);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RemoteFile {
    pub path: String,
    pub url: String,
    pub size: u64,
    pub blob: String,
}
#[derive(Clone, Serialize)]
pub struct ExtraGame {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    pub key: String,
    pub title: String,
    pub serial: Option<String>,
    pub crcs: Vec<String>,
}
#[derive(Clone, Serialize)]
pub struct ExtraItem {
    pub id: String,
    pub game_key: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub serial: Option<String>,
    pub crc: Option<String>,
    pub source: String,
    pub source_url: String,
    pub author: String,
    pub license: String,
    pub version: String,
    pub bytes: u64,
    pub groups: Vec<native_patches::Group>,
    pub installable: bool,
    pub install_note: String,
    #[serde(skip_serializing)]
    pub files: Vec<RemoteFile>,
}
#[derive(Serialize)]
pub struct GameResults {
    pub games: Vec<ExtraGame>,
    pub total: usize,
    pub page: usize,
    pub stale: bool,
    pub source: String,
    pub version: String,
}
#[derive(Serialize)]
pub struct ItemResults {
    pub items: Vec<ExtraItem>,
    pub stale: bool,
    pub source: String,
    pub version: String,
}
#[derive(Deserialize)]
struct Ref {
    object: RefObject,
}
#[derive(Deserialize)]
struct RefObject {
    sha: String,
}
#[derive(Deserialize)]
struct TreeBody {
    tree: Vec<TreeFile>,
    truncated: bool,
}
#[derive(Deserialize)]
struct TreeFile {
    path: String,
    sha: String,
    #[serde(default)]
    size: u64,
    #[serde(rename = "type")]
    kind: String,
}
struct SourceIndex {
    version: String,
    files: Vec<TreeFile>,
    games: BTreeMap<String, ExtraGame>,
    cheats: Vec<(String, String, String)>,
}
pub fn kind_valid(kind: &str) -> bool {
    ["saves", "cheats", "patches", "textures"].contains(&kind)
}
fn repo(kind: &str) -> Result<&'static str, String> {
    match kind {
        "saves" => Ok(online_http::SAVE_REPO),
        "cheats" => Ok(online_http::CHEAT_REPO),
        "patches" => Ok(online_http::PATCH_REPO),
        "textures" => Ok(online_http::TEXTURE_REPO),
        _ => Err("Categoria de extras inválida".into()),
    }
}
fn raw(repo: &str, version: &str, path: &str) -> String {
    format!(
        "https://raw.githubusercontent.com/{repo}/{version}/{}",
        path.split('/')
            .map(|s| urlencoding::encode(s).into_owned())
            .collect::<Vec<_>>()
            .join("/")
    )
}
fn document(
    files: &[TreeFile],
    repo: &str,
    version: &str,
    path: &str,
) -> Result<RemoteFile, String> {
    let file = files
        .iter()
        .find(|f| f.path == path)
        .ok_or("Documento ausente no índice da fonte")?;
    Ok(RemoteFile {
        path: file.path.clone(),
        url: raw(repo, version, path),
        size: file.size,
        blob: file.sha.clone(),
    })
}
pub fn normalized_serial(s: &str) -> Option<String> {
    let s = s
        .trim()
        .replace(['-', '_', '.', ' '], "")
        .to_ascii_uppercase();
    if s.is_ascii()
        && s.len() == 9
        && s[..4].bytes().all(|c| c.is_ascii_uppercase())
        && s[4..].bytes().all(|c| c.is_ascii_digit())
    {
        Some(format!("{}-{}", &s[..4], &s[4..]))
    } else {
        None
    }
}
pub fn parse_cheat_table(text: &str) -> Vec<(String, String, String)> {
    text.lines()
        .filter_map(|line| {
            let cells = line
                .trim()
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect::<Vec<_>>();
            if cells.len() != 3
                || !gr::crc_valid(cells[2])
                || cells[0].is_empty()
                || cells[0].len() > 512
            {
                return None;
            }
            let serial = normalized_serial(cells[1]).unwrap_or_default();
            Some((cells[0].into(), serial, cells[2].to_ascii_uppercase()))
        })
        .collect()
}
fn filename_identity(path: &str) -> Option<(String, String)> {
    let stem = path.rsplit('/').next()?.strip_suffix(".pnach")?;
    let (serial, crc) = stem.split_once('_')?;
    if !gr::serial_valid(serial) || !gr::crc_valid(crc) {
        return None;
    }
    Some((serial.into(), crc.to_ascii_uppercase()))
}
fn cheat_file(path: &str, serial: &str, crc: &str) -> bool {
    path == format!("cheats/{crc}.pnach")
        || (!serial.is_empty() && path == format!("cheats/{serial}_{crc}.pnach"))
}
fn add_game(
    games: &mut BTreeMap<String, ExtraGame>,
    key: String,
    title: String,
    serial: Option<String>,
    crc: Option<String>,
) {
    let game = games.entry(key.clone()).or_insert_with(|| ExtraGame {
        cover_url: None,
        key,
        title,
        serial,
        crcs: Vec::new(),
    });
    if let Some(crc) = crc {
        if !game.crcs.contains(&crc) {
            game.crcs.push(crc);
            game.crcs.sort();
        }
    }
}
async fn index(http: &OnlineHttp, kind: &str) -> Result<SourceIndex, String> {
    let repo = repo(kind)?;
    let branch = if kind == "saves" { "master" } else { "main" };
    let r: Ref = serde_json::from_slice(
        &http
            .get_checked(
                &format!("https://api.github.com/repos/{repo}/git/ref/heads/{branch}"),
                128 * 1024,
                true,
                |b| {
                    let r: Ref = serde_json::from_slice(b).map_err(|e| e.to_string())?;
                    if r.object.sha.len() == 40
                        && r.object.sha.bytes().all(|c| c.is_ascii_hexdigit())
                    {
                        Ok(())
                    } else {
                        Err("Revisão da fonte inválida".into())
                    }
                },
            )
            .await?,
    )
    .map_err(|e| e.to_string())?;
    let version = r.object.sha;
    if version.len() != 40 || !version.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Revisão da fonte inválida".into());
    }
    let t: TreeBody = serde_json::from_slice(
        &http
            .get_checked(
                &format!("https://api.github.com/repos/{repo}/git/trees/{version}?recursive=1"),
                8 * 1024 * 1024,
                true,
                |b| {
                    let t: TreeBody = serde_json::from_slice(b).map_err(|e| e.to_string())?;
                    if t.truncated || t.tree.is_empty() || t.tree.len() > 50000 {
                        Err("Índice da fonte incompleto".into())
                    } else {
                        Ok(())
                    }
                },
            )
            .await?,
    )
    .map_err(|e| e.to_string())?;
    if t.truncated || t.tree.len() > 50000 {
        return Err("Índice incompleto da fonte; tente atualizar depois".into());
    }
    let files = t
        .tree
        .into_iter()
        .filter(|f| f.kind == "blob")
        .collect::<Vec<_>>();
    let mut games = BTreeMap::new();
    let mut cheats = Vec::new();
    match kind {
        "saves" => {
            let text = verified(
                http,
                &document(&files, repo, &version, "PS2/games.txt")?,
                512 * 1024,
            )
            .await?;
            for line in std::str::from_utf8(&text)
                .map_err(|e| e.to_string())?
                .lines()
            {
                let Some((id, title)) = line.split_once('=') else {
                    continue;
                };
                let Some(serial) = normalized_serial(id) else {
                    continue;
                };
                if !title.is_empty() && title.len() < 512 {
                    add_game(&mut games, serial.clone(), title.into(), Some(serial), None);
                }
            }
        }
        "cheats" => {
            let text = verified(
                http,
                &document(&files, repo, &version, "README.md")?,
                1024 * 1024,
            )
            .await?;
            cheats = parse_cheat_table(std::str::from_utf8(&text).map_err(|e| e.to_string())?);
            for (title, serial, crc) in &cheats {
                if !files.iter().any(|f| cheat_file(&f.path, serial, crc)) {
                    continue;
                }
                let key = if serial.is_empty() {
                    format!("CRC:{crc}")
                } else {
                    serial.clone()
                };
                add_game(
                    &mut games,
                    key,
                    title.clone(),
                    (!serial.is_empty()).then(|| serial.clone()),
                    Some(crc.clone()),
                );
            }
        }
        _ => {
            for f in &files {
                if kind == "patches" && !f.path.starts_with("patches/") {
                    continue;
                }
                if kind == "textures" && !f.path.starts_with("PCSX2/cheats/") {
                    continue;
                }
                if let Some((serial, crc)) = filename_identity(&f.path) {
                    if kind == "textures"
                        && !files.iter().any(|v| {
                            v.path
                                .starts_with(&format!("PCSX2/textures/{serial}/replacements/"))
                        })
                    {
                        continue;
                    }
                    let title = game_metadata::title_for_serial(&serial)
                        .unwrap_or_else(|| format!("PlayStation 2 · {serial}"));
                    add_game(&mut games, serial.clone(), title, Some(serial), Some(crc));
                }
            }
        }
    }
    if games.is_empty() {
        return Err("O índice da fonte não contém jogos reconhecidos".into());
    }
    Ok(SourceIndex {
        version,
        files,
        games,
        cheats,
    })
}
pub async fn games(
    http: &OnlineHttp,
    kind: &str,
    q: &str,
    page: usize,
) -> Result<GameResults, String> {
    let idx = index(http, kind).await?;
    let q = q.trim().to_lowercase();
    if q.len() > 256 {
        return Err("Busca muito longa".into());
    }
    let mut all = idx
        .games
        .into_values()
        .filter(|g| {
            q.split_whitespace()
                .all(|s| format!("{} {}", g.title, g.key).to_lowercase().contains(s))
        })
        .collect::<Vec<_>>();
    sort_games(&mut all);
    let total = all.len();
    let page = page.max(1).min(total.div_ceil(24).max(1));
    Ok(GameResults {
        games: all.into_iter().skip((page - 1) * 24).take(24).collect(),
        total,
        page,
        stale: http.stale.load(std::sync::atomic::Ordering::Relaxed),
        source: repo(kind)?.into(),
        version: idx.version,
    })
}
pub async fn items(http: &OnlineHttp, kind: &str, key: &str) -> Result<ItemResults, String> {
    if kind == "saves" && key.starts_with("GFQ:") { return crate::gamefaqs::items(http,key).await; }
    if key.len() > 32 {
        return Err("Identificador de jogo inválido".into());
    }
    let idx = index(http, kind).await?;
    let game = idx
        .games
        .get(key)
        .ok_or("Jogo não encontrado nesta fonte")?;
    let repo = repo(kind)?;
    let version = &idx.version;
    let mut entries: Vec<(String, String, Option<String>, Vec<&TreeFile>)> = Vec::new();
    if kind == "saves" {
        let serial = game.serial.as_ref().unwrap().replace('-', "");
        let prefix = format!("PS2/{serial}/");
        let text = verified(
            http,
            &document(&idx.files, repo, version, &format!("{prefix}saves.txt"))?,
            512 * 1024,
        )
        .await?;
        for line in std::str::from_utf8(&text)
            .map_err(|e| e.to_string())?
            .lines()
        {
            let Some((name, description)) = line.split_once('=') else {
                continue;
            };
            if !crate::rom_scanner::is_safe_filename(name) || !name.ends_with(".zip") {
                continue;
            }
            if let Some(file) = idx
                .files
                .iter()
                .find(|f| f.path == format!("{prefix}{name}"))
            {
                entries.push((
                    format!("{} · {}", game.title, name.trim_end_matches(".zip")),
                    description.chars().take(2048).collect(),
                    None,
                    vec![file],
                ));
            }
        }
    } else if kind == "textures" {
        let serial = game.serial.as_ref().unwrap();
        let prefix = format!("PCSX2/textures/{serial}/replacements/");
        let files = idx
            .files
            .iter()
            .filter(|f| {
                f.path.starts_with(&prefix)
                    && (f.path.ends_with(".png") || f.path.ends_with(".dds"))
            })
            .collect::<Vec<_>>();
        for crc in &game.crcs {
            entries.push((
                format!("{} · texturas e ícones", game.title),
                "Texturas do projeto Ratchet & Clank de Made-in-Slovakia".into(),
                Some(crc.clone()),
                files.clone(),
            ));
        }
    } else {
        for file in &idx.files {
            let crc = if kind == "patches" {
                filename_identity(&file.path)
                    .filter(|(s, _)| Some(s) == game.serial.as_ref())
                    .map(|(_, c)| c)
            } else {
                idx.cheats
                    .iter()
                    .find(|(_, s, c)| {
                        (if s.is_empty() {
                            format!("CRC:{c}")
                        } else {
                            s.clone()
                        }) == game.key
                            && cheat_file(&file.path, s, c)
                    })
                    .map(|(_, _, c)| c.clone())
            };
            if let Some(crc) = crc {
                entries.push((
                    format!("{} · {}", game.title, crc),
                    String::new(),
                    Some(crc),
                    vec![file],
                ));
            }
        }
    }
    let mut items = Vec::new();
    for (title, description, crc, files) in entries {
        if files.is_empty()
            || files.len() > 5000
            || files.iter().any(|f| f.size > 128 * 1024 * 1024)
        {
            continue;
        }
        let remote = files
            .iter()
            .map(|f| RemoteFile {
                path: f.path.clone(),
                url: raw(repo, version, &f.path),
                size: f.size,
                blob: f.sha.clone(),
            })
            .collect::<Vec<_>>();
        let id = mc::digest(
            format!(
                "{kind}|{key}|{version}|{:?}|{}",
                remote.iter().map(|f| &f.path).collect::<Vec<_>>(),
                crc.as_deref().unwrap_or("")
            )
            .as_bytes(),
        );
        let (groups, install_note) = if kind == "patches" || kind == "cheats" {
            let bytes = verified(http, &remote[0], 2 * 1024 * 1024).await?;
            let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
            let groups = native_patches::parse(text, kind, &remote[0].path, false);
            let note = gr::validate_pnach(text).err().unwrap_or_else(|| {
                if groups.is_empty() || groups.iter().any(|g| g.automatic) {
                    "Códigos sem seleção individual; use a importação manual.".into()
                } else {
                    String::new()
                }
            });
            (groups, note)
        } else {
            (Vec::new(), String::new())
        };
        let installable = install_note.is_empty();
        items.push(ExtraItem {
            id,
            game_key: key.into(),
            title,
            description,
            kind: kind.into(),
            serial: game.serial.clone(),
            crc,
            source: repo.into(),
            source_url: format!("https://github.com/{repo}"),
            author: repo.split('/').next().unwrap().into(),
            license: if kind == "textures" {
                "MIT"
            } else if kind == "saves" {
                "GPL-3.0 (repositório); autoria dos saves varia"
            } else {
                "Não informada pela fonte"
            }
            .into(),
            version: version.clone(),
            bytes: files.iter().map(|f| f.size).sum(),
            groups,
            installable,
            install_note,
            files: remote,
        });
    }
    Ok(ItemResults {
        items,
        stale: http.stale.load(std::sync::atomic::Ordering::Relaxed),
        source: repo.into(),
        version: version.clone(),
    })
}
pub async fn verified(
    http: &OnlineHttp,
    file: &RemoteFile,
    limit: usize,
) -> Result<Vec<u8>, String> {
    http.get_checked(&file.url, limit, true, |bytes| {
        let mut object = format!("blob {}\0", bytes.len()).into_bytes();
        object.extend_from_slice(bytes);
        if file.blob.len() != 40
            || crate::pcsx2::compute_sha1(&object) != file.blob
            || bytes.len() as u64 != file.size
        {
            return Err("O arquivo recebido não corresponde à revisão da fonte".into());
        }
        Ok(())
    })
    .await
}

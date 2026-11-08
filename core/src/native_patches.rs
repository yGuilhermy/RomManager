use crate::cache::{FileStamp, LOCAL_GROUPS, ZIP_INDEX, ZIP_TEXT};
use crate::{game_resources as gr, memory_cards as mc, pcsx2};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};
type R<T> = Result<T, String>;

fn zip_location(installation: &pcsx2::Pcsx2Installation) -> R<PathBuf> {
    Ok(Path::new(&installation.path)
        .parent()
        .ok_or("Pasta do PCSX2 inválida")?
        .join("resources/patches.zip"))
}
fn zip_names(zip_path: &Path) -> R<Option<(Arc<Vec<String>>, FileStamp)>> {
    if !zip_path.is_file() {
        return Ok(None);
    }
    mc::plain(zip_path)?;
    let stamp = FileStamp::of(zip_path).ok_or("Pacote de patches ilegível.")?;
    if stamp.size > 64 * 1024 * 1024 {
        return Err("Pacote de patches acima do limite.".into());
    }
    let key = zip_path.to_path_buf();
    if let Some(names) = ZIP_INDEX.get(&key, &stamp) {
        return Ok(Some((names, stamp)));
    }
    let zip = zip::ZipArchive::new(fs::File::open(zip_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if zip.len() > 100000 {
        return Err("Pacote de patches com entradas demais.".into());
    }
    let names = Arc::new(zip.file_names().map(str::to_string).collect::<Vec<_>>());
    ZIP_INDEX.put(key, stamp.clone(), names.clone());
    Ok(Some((names, stamp)))
}
fn zip_text(zip_path: &Path, stamp: &FileStamp, entry: &str) -> R<Arc<String>> {
    let key = (zip_path.to_path_buf(), entry.to_string());
    if let Some(text) = ZIP_TEXT.get(&key, stamp) {
        return Ok(text);
    }
    let mut zip = zip::ZipArchive::new(fs::File::open(zip_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut file = zip.by_name(entry).map_err(|e| e.to_string())?;
    if file.size() > 2 * 1024 * 1024 {
        return Err("Patch nativo acima do limite.".into());
    }
    let mut text = String::new();
    file.by_ref()
        .take(2 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 2 * 1024 * 1024 {
        return Err("Patch nativo acima do limite.".into());
    }
    let text = Arc::new(text);
    ZIP_TEXT.put(key, stamp.clone(), text.clone());
    Ok(text)
}
pub fn warm_index(conn: &Connection) {
    if let Ok(installation) = pcsx2::current_installation(conn) {
        if let Ok(path) = zip_location(&installation) {
            let _ = zip_names(&path);
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Group {
    pub name: String,
    pub description: String,
    pub author: String,
    pub kind: String,
    pub source: String,
    pub bundled: bool,
    pub definition_hash: String,
    pub enabled: bool,
    pub selected: bool,
    pub automatic: bool,
}
pub struct Catalogue {
    pub groups: Vec<Group>,
    pub crcs: Vec<String>,
    pub crc: String,
    pub warnings: Vec<String>,
}
pub fn ini_list(text: &str, section: &str, key: &str) -> Vec<String> {
    let mut current = "";
    let mut result = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            current = &line[1..line.len() - 1];
            continue;
        }
        if current.eq_ignore_ascii_case(section) {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    let value = v.trim().trim_matches('"');
                    if !value.is_empty() && !result.iter().any(|v| v == value) {
                        result.push(value.into());
                    }
                }
            }
        }
    }
    result
}
pub fn set_list(text: &str, section: &str, key: &str, values: &[String]) -> String {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut found = false;
    let mut inserted = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if current.eq_ignore_ascii_case(section) && !inserted {
                for value in values {
                    lines.push(format!("{key} = {value}"));
                }
                inserted = true;
            }
            current = trimmed[1..trimmed.len() - 1].into();
            if current.eq_ignore_ascii_case(section) {
                found = true;
            }
        }
        if current.eq_ignore_ascii_case(section)
            && trimmed
                .split_once('=')
                .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        {
            continue;
        }
        lines.push(line.to_string());
    }
    if !found {
        if values.is_empty() {
            return text.to_string();
        }
        lines.push(format!("[{section}]"));
    }
    if !inserted {
        for value in values {
            lines.push(format!("{key} = {value}"));
        }
    }
    lines.join("\r\n")
}
pub(crate) fn parse(text: &str, kind: &str, source: &str, bundled: bool) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
            let name = &line[1..line.len() - 1];
            if name.len() > 1024 || name.chars().any(char::is_control) {
                continue;
            }
            groups.push(Group {
                name: name.into(),
                description: String::new(),
                author: String::new(),
                kind: kind.into(),
                source: source.into(),
                bundled,
                definition_hash: mc::digest(text.as_bytes()),
                enabled: false,
                selected: false,
                automatic: false,
            });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if groups.is_empty() && key.trim() == "patch" {
            groups.push(Group{name:String::new(),description:"Códigos sem grupo; seguem a opção geral de cheats ou o carregamento do arquivo local.".into(),author:String::new(),kind:kind.into(),source:source.into(),bundled,definition_hash:mc::digest(text.as_bytes()),enabled:true,selected:true,automatic:true});
        }
        if let Some(group) = groups.last_mut() {
            match key.trim() {
                "description" => group.description = value.trim().chars().take(8192).collect(),
                "comment" if group.description.is_empty() => {
                    group.description = value.trim().chars().take(8192).collect()
                }
                "author" => group.author = value.trim().chars().take(1024).collect(),
                _ => {}
            }
        }
    }
    groups
}
fn file_crc(name: &str, serial: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    let rest = upper.strip_prefix(&format!("{serial}_"))?;
    if !upper.ends_with(".PNACH") || rest.len() < 14 {
        return None;
    }
    let crc = rest.get(..8)?;
    gr::crc_valid(crc).then(|| crc.into())
}
fn disk_files(conn: &Connection, kind: &str) -> R<Vec<PathBuf>> {
    let key = if kind == "cheats" {
        "Cheats"
    } else {
        "Patches"
    };
    let folder = pcsx2::native_folder(conn, key, kind)?;
    if !folder.is_dir() {
        return Ok(vec![]);
    }
    mc::plain(&folder)?;
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())?.take(10001) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().is_file() {
            files.push(entry.path());
        }
    }
    if files.len() > 10000 {
        return Err("Pasta de recursos excede o limite de arquivos.".into());
    }
    files.sort();
    Ok(files)
}
pub fn catalogue(
    conn: &Connection,
    serial: &str,
    requested: &str,
    global: &str,
    local: &str,
    cheats_enabled: bool,
) -> R<Catalogue> {
    let installation = pcsx2::current_installation(conn)?;
    let zip_path = zip_location(&installation)?;
    let zip_index = zip_names(&zip_path)?;
    let mut crcs = BTreeSet::new();
    if let Some((names, _)) = &zip_index {
        for name in names.iter() {
            if !name.contains(['/', '\\']) {
                if let Some(crc) = file_crc(name, serial) {
                    crcs.insert(crc);
                }
            }
        }
    }
    let patches = disk_files(conn, "patches")?;
    let cheats = disk_files(conn, "cheats")?;
    for path in patches.iter().chain(&cheats) {
        if let Some(crc) = file_crc(&path.file_name().unwrap().to_string_lossy(), serial) {
            crcs.insert(crc);
        }
    }
    let crc = if !requested.is_empty() {
        if !gr::crc_valid(requested) {
            return Err("CRC inválido.".into());
        }
        requested.to_ascii_uppercase()
    } else if crcs.len() == 1 {
        crcs.first().unwrap().clone()
    } else {
        String::new()
    };
    let mut result = Catalogue {
        groups: vec![],
        crcs: crcs.into_iter().collect(),
        crc: crc.clone(),
        warnings: vec![],
    };
    if crc.is_empty() {
        return Ok(result);
    }
    let mut suppressed = false;
    for (kind, files) in [("patches", patches), ("cheats", cheats)] {
        for path in files {
            let upper = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_uppercase();
            if !upper.ends_with(".PNACH")
                || !(upper.starts_with(&format!("{serial}_{crc}")) || upper.starts_with(&crc))
            {
                continue;
            }
            let local_key = (path.clone(), kind.to_string());
            let local_stamp = FileStamp::of(&path);
            let cached = local_stamp
                .as_ref()
                .and_then(|stamp| LOCAL_GROUPS.get(&local_key, stamp));
            let groups = match cached {
                Some(groups) => groups,
                None => {
                    let bytes = mc::read_bounded(&path)?;
                    if bytes.len() > 2 * 1024 * 1024 {
                        return Err("PNACH acima do limite de 2 MB.".into());
                    }
                    let text = String::from_utf8(bytes)
                        .map_err(|_| "PNACH não está em UTF-8.".to_string())?;
                    let groups = parse(&text, kind, &path.to_string_lossy(), false);
                    if let Some(stamp) = local_stamp {
                        LOCAL_GROUPS.put(local_key, stamp, groups.clone());
                    }
                    groups
                }
            };
            suppressed |= kind == "patches" && groups.iter().any(|g| g.automatic);
            result.groups.extend(groups);
        }
    }
    if suppressed {
        result.warnings.push("Um patch local sem grupos impede o PCSX2 de carregar os patches do pacote nativo para esta revisão.".into());
    }
    if !suppressed {
        if let Some((names, stamp)) = &zip_index {
            let entry = [format!("{serial}_{crc}.pnach"), format!("{crc}.pnach")]
                .into_iter()
                .find_map(|wanted| {
                    names
                        .iter()
                        .find(|name| name.eq_ignore_ascii_case(&wanted))
                        .cloned()
                });
            if let Some(entry) = entry {
                let text = zip_text(&zip_path, stamp, &entry)?;
                result.groups.extend(parse(
                    &text,
                    "patches",
                    &format!("patches.zip/{entry}"),
                    true,
                ));
            }
        }
    }
    let mut seen = BTreeSet::new();
    result
        .groups
        .retain(|g| seen.insert((g.kind.clone(), g.name.clone())));
    apply_enabled(&mut result.groups, global, local, cheats_enabled);
    Ok(result)
}

pub fn apply_enabled(groups: &mut [Group], global: &str, local: &str, cheats_enabled: bool) {
    for group in groups {
        let section = if group.kind == "patches" {
            "Patches"
        } else {
            "Cheats"
        };
        let enabled = ini_list(local, section, "Enable");
        let disabled = ini_list(local, section, "Disable");
        let default = if group.name == "Widescreen 16:9" {
            Some("EnableWideScreenPatches")
        } else if group.name == "No-Interlacing" {
            Some("EnableNoInterlacingPatches")
        } else {
            None
        };
        let inherited = default
            .and_then(|key| {
                mc::ini_value(local, "EmuCore", key)
                    .or_else(|| mc::ini_value(global, "EmuCore", key))
            })
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        group.selected = group.automatic || ((enabled.contains(&group.name) || inherited) && !disabled.contains(&group.name));
        group.enabled = group.selected && (group.kind != "cheats" || cheats_enabled);
    }
}

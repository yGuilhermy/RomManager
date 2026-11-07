use crate::memory_cards as mc;
use chrono::{Datelike, Timelike};
use std::{collections::BTreeMap, fs, path::Path};
type R<T> = Result<T, String>;
fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}
fn u32le(b: &[u8], at: usize) -> R<usize> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or("Save truncado")?
            .try_into()
            .unwrap(),
    ) as usize)
}
fn filename(b: &[u8]) -> R<String> {
    let bytes = b.get(64..96).ok_or("Metadado truncado")?;
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let n = std::str::from_utf8(&bytes[..end]).map_err(e)?.to_string();
    mc::name(&n)?;
    Ok(n)
}
fn set_date(b: &mut [u8], at: usize, time: i64) {
    if date(b, at) == time {
        return;
    }
    if let Some(date) = chrono::DateTime::from_timestamp(time, 0) {
        b[at] = 0;
        b[at + 1] = date.second() as u8;
        b[at + 2] = date.minute() as u8;
        b[at + 3] = date.hour() as u8;
        b[at + 4] = date.day() as u8;
        b[at + 5] = date.month() as u8;
        b[at + 6..at + 8].copy_from_slice(&(date.year() as u16).to_le_bytes());
    }
}
fn date(b: &[u8], at: usize) -> i64 {
    let year = u16::from_le_bytes([b[at + 6], b[at + 7]]) as i32;
    chrono::NaiveDate::from_ymd_opt(year, b[at + 5] as u32, b[at + 4] as u32)
        .and_then(|d| d.and_hms_opt(b[at + 3] as u32, b[at + 2] as u32, b[at + 1] as u32))
        .map(|d| d.and_utc().timestamp())
        .unwrap_or(0)
}
fn entry(n: &str, mode: u16, length: usize) -> R<Vec<u8>> {
    if n.len() > 31 || !n.is_ascii() {
        return Err("Nome de save não suportado sem perda de codificação".into());
    }
    let mut b = vec![0u8; 512];
    b[..2].copy_from_slice(&mode.to_le_bytes());
    b[4..8].copy_from_slice(&(length as u32).to_le_bytes());
    b[64..64 + n.len()].copy_from_slice(n.as_bytes());
    Ok(b)
}
fn index(path: &Path) -> R<BTreeMap<String, BTreeMap<String, i64>>> {
    let p = path.join("_pcsx2_index");
    if !p.exists() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(p).map_err(e)?;
    if let Ok(v) = serde_json::from_str(&text) {
        return Ok(v);
    }
    if text.trim_start().starts_with('{') {
        let keys = regex::Regex::new(r#"([,{]\s*)([^"'\s:{},]+)\s*:"#).map_err(e)?;
        let normalized = keys.replace_all(&text, r#"${1}"${2}":"#);
        return serde_json::from_str(&normalized).map_err(e);
    }
    let mut map: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    let mut current = String::new();
    for raw in text.lines() {
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        if !raw.starts_with(' ') {
            current = raw
                .trim()
                .strip_suffix(':')
                .ok_or("Índice de pasta não suportado")?
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            map.entry(current.clone()).or_default();
        } else {
            let (key, value) = raw
                .trim()
                .split_once(':')
                .ok_or("Índice de pasta inválido")?;
            if !["order", "timeCreated", "timeModified"].contains(&key) {
                return Err("Índice contém metadados não suportados".into());
            }
            map.entry(current.clone())
                .or_default()
                .insert(key.into(), value.trim().parse().map_err(e)?);
        }
    }
    Ok(map)
}
/// A PSU contains directory entries and uncompressed, 1024-byte-aligned file payloads.
pub fn psu_to_folder(bytes: &[u8], parent: &Path) -> R<String> {
    if bytes.len() < 1536 {
        return Err("PSU truncado".into());
    }
    let root = &bytes[..512];
    let name = filename(root)?;
    let count = u32le(root, 4)?;
    if count < 2 || count > 4096 {
        return Err("Contagem de arquivos PSU inválida".into());
    }
    let target = parent.join(&name);
    if target.exists() {
        return Err("Já existe este save".into());
    }
    let mut items = Vec::new();
    let mut at = 1536;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..count - 2 {
        let header = bytes.get(at..at + 512).ok_or("PSU truncado")?.to_vec();
        at += 512;
        let n = filename(&header)?;
        if n.starts_with('_') || !seen.insert(n.to_lowercase()) {
            return Err("Nome duplicado ou reservado no PSU".into());
        }
        if header[0] & 0x20 != 0 {
            return Err("Saves com subdiretórios não suportados".into());
        }
        let length = u32le(&header, 4)?;
        let end = at.checked_add(length).ok_or("Tamanho inválido")?;
        let data = bytes
            .get(at..end)
            .ok_or("Conteúdo do PSU truncado")?
            .to_vec();
        at = at
            .checked_add(length.div_ceil(1024) * 1024)
            .ok_or("Tamanho inválido")?;
        if at > bytes.len() {
            return Err("PSU sem alinhamento completo".into());
        }
        items.push((n, header, data));
    }
    fs::create_dir(&target).map_err(e)?;
    fs::create_dir(target.join("_pcsx2_meta")).map_err(e)?;
    fs::write(target.join("_pcsx2_meta_directory"), root).map_err(e)?;
    let mut idx = serde_json::Map::new();
    idx.insert(
        "$ROOT".into(),
        serde_json::json!({"timeCreated":date(root,8),"timeModified":date(root,24)}),
    );
    for (i, (n, header, data)) in items.into_iter().enumerate() {
        fs::write(target.join(&n), data).map_err(e)?;
        fs::write(target.join("_pcsx2_meta").join(&n), &header).map_err(e)?;
        idx.insert(n,serde_json::json!({"order":i+1,"timeCreated":date(&header,8),"timeModified":date(&header,24)}));
    }
    fs::write(
        target.join("_pcsx2_index"),
        serde_json::to_vec_pretty(&idx).map_err(e)?,
    )
    .map_err(e)?;
    Ok(name)
}
pub fn folder_to_psu(path: &Path) -> R<Vec<u8>> {
    mc::plain(path)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Nome de pasta inválido")?;
    mc::name(name)?;
    let idx = index(path)?;
    let mut files = Vec::new();
    for item in fs::read_dir(path).map_err(e)? {
        let item = item.map_err(e)?;
        let n = item.file_name().to_string_lossy().into_owned();
        mc::plain(&item.path())?;
        if n.starts_with("_pcsx2_") {
            continue;
        }
        mc::name(&n)?;
        if item.path().is_dir() {
            return Err("Save com subdiretório não suportado".into());
        }
        files.push(n);
    }
    files.sort_by_key(|n| {
        idx.get(n)
            .and_then(|d| d.get("order"))
            .copied()
            .unwrap_or(i64::MAX)
    });
    let meta = path.join("_pcsx2_meta_directory");
    let mut root = if meta.exists() {
        mc::read_bounded(&meta)?
    } else {
        entry(name, 0x8427, files.len() + 2)?
    };
    if root.len() != 512 {
        return Err("Metadados de pasta inválidos".into());
    }
    root[4..8].copy_from_slice(&((files.len() + 2) as u32).to_le_bytes());
    if !meta.exists() {
        if let Some(times) = idx.get("$ROOT") {
            if let Some(t) = times.get("timeCreated") {
                set_date(&mut root, 8, *t);
            }
            if let Some(t) = times.get("timeModified") {
                set_date(&mut root, 24, *t);
            }
        }
    }
    let mut bytes = root;
    bytes.extend(entry(".", 0x8427, files.len() + 2)?);
    bytes.extend(entry("..", 0x8427, 0)?);
    for n in files {
        let data = mc::read_bounded(&path.join(&n))?;
        let meta = path.join("_pcsx2_meta").join(&n);
        let mut header = if meta.exists() {
            mc::read_bounded(&meta)?
        } else {
            entry(&n, 0x8497, data.len())?
        };
        if header.len() != 512 {
            return Err("Metadados de arquivo inválidos".into());
        }
        header[4..8].copy_from_slice(&(data.len() as u32).to_le_bytes());
        if !meta.exists() {
            if let Some(times) = idx.get(&n) {
                if let Some(t) = times.get("timeCreated") {
                    set_date(&mut header, 8, *t);
                }
                if let Some(t) = times.get("timeModified") {
                    set_date(&mut header, 24, *t);
                }
            }
        }
        bytes.extend(header);
        bytes.extend(&data);
        bytes.resize(bytes.len() + ((1024 - data.len() % 1024) % 1024), 0);
        if bytes.len() > 128 * 1024 * 1024 {
            return Err("Save acima do limite".into());
        }
    }
    Ok(bytes)
}

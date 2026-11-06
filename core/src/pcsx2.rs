use crate::database;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pcsx2Installation {
    pub path: String,
    pub version: String,
    pub is_portable: bool,
    pub is_managed: bool,
    pub data_dir: String,
    pub bios_dir: String,
    pub covers_dir: String,
    pub inis_dir: String,
    pub is_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pcsx2Profile {
    pub id: String,
    pub name: String,
    pub executable_path: String,
    pub data_dir: String,
    pub bios_path: Option<String>,
    pub renderer: String,
    pub aspect_ratio: String,
    pub fullscreen: bool,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiosInfo {
    pub filename: String,
    pub path: String,
    pub size: u64,
    pub sha1: String,
    pub region: String,
    pub version: String,
    pub model: String,
    pub is_valid: bool,
    pub is_active: bool,
    pub is_duplicate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiosImportResult {
    pub count: usize,
    pub items: Vec<BiosInfo>,
    pub message: String,
    pub filename: String,
    pub path: String,
    pub size: u64,
    pub sha1: String,
    pub region: String,
    pub version: String,
    pub model: String,
    pub is_valid: bool,
    pub is_active: bool,
    pub is_duplicate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pcsx2Diagnostic {
    pub executable_valid: bool,
    pub executable_path: String,
    pub executable_version: String,
    pub executable_type: String,
    pub is_running: bool,
    pub data_dir_valid: bool,
    pub data_dir_writable: bool,
    pub bios_dir_valid: bool,
    pub bios_count: usize,
    pub active_bios_valid: bool,
    pub covers_dir_valid: bool,
    pub roms_dir_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedRelease {
    pub version: String,
    pub release_name: String,
    pub channel: String,
    pub published_at: String,
    pub download_url: String,
    pub file_size: u64,
    pub installed: bool,
    #[serde(default)]
    pub executable_path: Option<String>,
    #[serde(default)]
    pub data_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoversExportResult {
    pub total_games: usize,
    pub exported: usize,
    pub preserved: usize,
    pub skipped_no_serial: usize,
    pub errors: usize,
    pub target_directory: String,
}

pub fn is_pcsx2_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let output = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq pcsx2*", "/NH"])
            .creation_flags(0x08000000)
            .output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
            return text.lines().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("pcsx2") && trimmed.contains(".exe")
            });
        }
    }
    false
}

pub fn sanitize_path(input: &str) -> String {
    input.trim().trim_matches('"').trim_matches('\'').trim().to_string()
}

fn extract_pe_version(bytes: &[u8]) -> Option<String> {
    let pv_tag = [
        b'P', 0, b'r', 0, b'o', 0, b'd', 0, b'u', 0, b'c', 0, b't', 0, b'V', 0, b'e', 0, b'r',
        0, b's', 0, b'i', 0, b'o', 0, b'n', 0,
    ];
    let fv_tag = [
        b'F', 0, b'i', 0, b'l', 0, b'e', 0, b'V', 0, b'e', 0, b'r', 0, b's', 0, b'i', 0, b'o',
        0, b'n', 0,
    ];

    for tag in &[&pv_tag[..], &fv_tag[..]] {
        if let Some(pos) = bytes.windows(tag.len()).position(|w| w == *tag) {
            let mut offset = pos + tag.len();
            while offset + 1 < bytes.len() && bytes[offset] == 0 && bytes[offset + 1] == 0 {
                offset += 2;
            }
            let mut chars = Vec::new();
            while offset + 1 < bytes.len() {
                let b0 = bytes[offset];
                let b1 = bytes[offset + 1];
                if b0 == 0 && b1 == 0 {
                    break;
                }
                if b1 != 0 {
                    break;
                }
                let ch = b0 as char;
                if ch.is_ascii_graphic() {
                    chars.push(ch);
                } else {
                    break;
                }
                offset += 2;
            }
            if !chars.is_empty() {
                let s: String = chars.into_iter().collect();
                if s.chars().any(|c| c.is_ascii_digit()) {
                    return Some(format!("v{}", s.trim_start_matches('v')));
                }
            }
        }
    }

    if let Some(pos) = bytes.windows(8).position(|w| w == b"VERSION_") {
        let mut offset = pos + 8;
        let mut chars = Vec::new();
        while offset < bytes.len()
            && (bytes[offset].is_ascii_digit() || bytes[offset] == b'.' || bytes[offset] == b'_')
        {
            let ch = if bytes[offset] == b'_' {
                '.'
            } else {
                bytes[offset] as char
            };
            chars.push(ch);
            offset += 1;
        }
        if !chars.is_empty() {
            let s: String = chars.into_iter().collect();
            return Some(format!("v{}", s.trim_start_matches('v')));
        }
    }

    None
}

pub fn extract_pcsx2_version(exe_path: &Path) -> String {
    if !exe_path.exists() || !exe_path.is_file() {
        return "Não encontrado".to_string();
    }
    if let Some(parent) = exe_path.parent() {
        let v_file = parent.join("version.json");
        if v_file.exists() {
            if let Ok(content) = fs::read_to_string(&v_file) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(ver) = val.get("version").and_then(|v| v.as_str()) {
                        return format!("v{}", ver.trim_start_matches('v'));
                    }
                }
            }
        }
    }
    if let Ok(bytes) = fs::read(exe_path) {
        if let Some(ver) = extract_pe_version(&bytes) {
            return format!("PCSX2 {}", ver);
        }
    }
    if let Some(stem) = exe_path.file_stem().and_then(|s| s.to_str()) {
        let lower = stem.to_lowercase();
        if let Some(idx) = lower.find("-v") {
            let part = &lower[idx + 2..];
            let v: String = part
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !v.is_empty() {
                return format!("PCSX2 v{}", v);
            }
        } else if let Some(idx) = lower.find('v') {
            let part = &lower[idx + 1..];
            let v: String = part
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if !v.is_empty() && v.contains('.') {
                return format!("PCSX2 v{}", v);
            }
        }
    }
    "Versão desconhecida".to_string()
}

pub fn inspect_installation(exe_path: &Path) -> Result<Pcsx2Installation, String> {
    if !crate::exe_runner::validate_pcsx2_executable(exe_path) {
        return Err("O executável do PCSX2 não foi encontrado ou é inválido.".to_string());
    }
    let parent_dir = exe_path.parent().unwrap_or(Path::new("."));
    let is_portable =
        parent_dir.join("portable.ini").exists() || parent_dir.join("inis").is_dir();
    let is_managed = parent_dir.join("version.json").exists()
        || parent_dir
            .to_string_lossy()
            .to_lowercase()
            .contains("managed_pcsx2");

    let data_dir = if is_portable {
        parent_dir.to_path_buf()
    } else if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let docs_pcsx2 = PathBuf::from(user_profile).join("Documents").join("PCSX2");
        if docs_pcsx2.exists() {
            docs_pcsx2
        } else {
            parent_dir.to_path_buf()
        }
    } else {
        parent_dir.to_path_buf()
    };

    let bios_dir = data_dir.join("bios");
    let covers_dir = data_dir.join("covers");
    let inis_dir = data_dir.join("inis");

    Ok(Pcsx2Installation {
        path: exe_path.to_string_lossy().to_string(),
        version: extract_pcsx2_version(exe_path),
        is_portable,
        is_managed,
        data_dir: data_dir.to_string_lossy().to_string(),
        bios_dir: bios_dir.to_string_lossy().to_string(),
        covers_dir: covers_dir.to_string_lossy().to_string(),
        inis_dir: inis_dir.to_string_lossy().to_string(),
        is_running: is_pcsx2_running(),
    })
}

pub fn detect_installations(
    configured_path: Option<&str>,
    data_root: Option<&Path>,
) -> Vec<Pcsx2Installation> {
    let mut results: Vec<Pcsx2Installation> = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();
    let running = is_pcsx2_running();

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(cfg) = configured_path.filter(|s| !s.is_empty()) {
        candidates.push(PathBuf::from(cfg));
    }
    if let Some(root) = data_root {
        candidates.push(root.join("managed_pcsx2").join("pcsx2-qt.exe"));
        let versions_dir = root.join("managed_pcsx2").join("versions");
        if versions_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&versions_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        candidates.push(p.join("pcsx2-qt.exe"));
                    }
                }
            }
        }
    }
    candidates.push(PathBuf::from("C:\\Program Files\\PCSX2\\pcsx2-qt.exe"));
    candidates.push(PathBuf::from("C:\\Program Files (x86)\\PCSX2\\pcsx2-qt.exe"));
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Programs")
                .join("PCSX2")
                .join("pcsx2-qt.exe"),
        );
    }

    for path in candidates {
        let normalized = path.to_string_lossy().to_lowercase();
        if seen_paths.contains(&normalized) {
            continue;
        }
        seen_paths.insert(normalized);
        if let Ok(mut info) = inspect_installation(&path) {
            info.is_running = running;
            if info.is_managed && results.iter().any(|existing: &Pcsx2Installation| existing.is_managed && existing.version == info.version) {
                continue;
            }
            results.push(info);
        }
    }

    results
}

pub fn compute_sha1(data: &[u8]) -> String {
    let mut state = [0x67452301u32, 0xEFCDAB89u32, 0x98BADCFEu32, 0x10325476u32, 0xC3D2E1F0u32];

    let process_block = |block: &[u8], h: &mut [u32; 5]| {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];

        for &item in &w[0..20] {
            let f = (b & c) | ((!b) & d);
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(0x5A827999)
                .wrapping_add(item);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        for &item in &w[20..40] {
            let f = b ^ c ^ d;
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(0x6ED9EBA1)
                .wrapping_add(item);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        for &item in &w[40..60] {
            let f = (b & c) | (b & d) | (c & d);
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(0x8F1BBCDC)
                .wrapping_add(item);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        for &item in &w[60..80] {
            let f = b ^ c ^ d;
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(0xCA62C1D6)
                .wrapping_add(item);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    };

    let full_blocks = data.len() / 64;
    for chunk in data[..full_blocks * 64].chunks_exact(64) {
        process_block(chunk, &mut state);
    }

    let rem = &data[full_blocks * 64..];
    let bit_len = (data.len() as u64) * 8;
    let mut tail = [0u8; 128];
    tail[..rem.len()].copy_from_slice(rem);
    tail[rem.len()] = 0x80;

    if rem.len() < 56 {
        tail[56..64].copy_from_slice(&bit_len.to_be_bytes());
        process_block(&tail[..64], &mut state);
    } else {
        tail[120..128].copy_from_slice(&bit_len.to_be_bytes());
        process_block(&tail[..64], &mut state);
        process_block(&tail[64..128], &mut state);
    }

    format!(
        "{:08x}{:08x}{:08x}{:08x}{:08x}",
        state[0], state[1], state[2], state[3], state[4]
    )
}

pub fn parse_scph_model(filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    let bytes = lower.as_bytes();
    let scph_idx = lower.find("scph")?;
    let mut i = scph_idx + 4;
    while i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'_' || bytes[i] == b' ') {
        i += 1;
    }
    let start_num = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == start_num {
        return None;
    }
    if i < bytes.len()
        && bytes[i].is_ascii_alphabetic()
        && (i + 1 == bytes.len() || !bytes[i + 1].is_ascii_alphabetic())
    {
        i += 1;
    }
    let model_code = &lower[start_num..i];
    Some(format!("SCPH-{}", model_code.to_ascii_uppercase()))
}

pub fn identify_bios(filename: &str, sha1: &str) -> (String, String, String) {
    let lower_name = filename.to_lowercase();
    let is_ps2_pattern = lower_name.starts_with("ps2-")
        && lower_name.len() >= 9
        && lower_name.as_bytes()[4].is_ascii_digit()
        && lower_name.as_bytes()[5].is_ascii_digit()
        && lower_name.as_bytes()[6].is_ascii_digit()
        && lower_name.as_bytes()[7].is_ascii_digit()
        && matches!(lower_name.as_bytes()[8], b'a' | b'e' | b'j' | b'h');

    let (model, region, version) = if is_ps2_pattern {
        let major = &lower_name[4..6];
        let minor = &lower_name[6..8];
        let reg = &lower_name[8..9];
        let suff = match reg {
            "a" => "1",
            "e" => "4",
            "j" => "0",
            "h" => "6",
            _ => "0",
        };
        let m = match (major, minor) {
            ("01", "00") => "SCPH-10000".to_string(),
            ("01", "10") => format!("SCPH-1800{}", suff),
            ("01", "20") => format!("SCPH-3000{}", suff),
            ("01", "60") => format!("SCPH-3900{}", suff),
            ("02", "00") => format!("SCPH-7000{}", suff),
            ("02", "20") => format!("SCPH-7700{}", suff),
            ("02", "30") => format!("SCPH-9000{}", suff),
            _ => format!("PS2 BIOS v{}.{}", major, minor),
        };
        let r = match reg {
            "a" => "USA / NTSC-U",
            "e" => "Europe / PAL",
            "j" => "Japan / NTSC-J",
            "h" => "Asia",
            _ => "Global / Outro",
        };
        let v = format!("v{}.{}", major, minor);
        (m, r.to_string(), v)
    } else {
        let m = if let Some(parsed) = parse_scph_model(filename) {
            parsed
        } else if sha1.eq_ignore_ascii_case("ad039130da4f9ffab03b4122d4f20ecb2ce43c08") {
            "SCPH-10000".to_string()
        } else if sha1.eq_ignore_ascii_case("44a4e50eb932906b3bc55ea1b4f420b9e4a8b79b") {
            "SCPH-39001".to_string()
        } else if sha1.eq_ignore_ascii_case("991206f65dd4b4e9fb6a4ad6f6c5b6ff8fb6f59b") {
            "SCPH-70004".to_string()
        } else if sha1.eq_ignore_ascii_case("f86e3ec019446d3215f79a9643440eecda258384") {
            "SCPH-77001".to_string()
        } else if sha1.eq_ignore_ascii_case("a531f868ad963d41f71df42f1f3a1f72782bcf2e") {
            "SCPH-90006".to_string()
        } else {
            "PlayStation 2 BIOS".to_string()
        };

        let r = if lower_name.contains("usa")
            || lower_name.contains("ntsc-u")
            || lower_name.contains("na")
            || m.ends_with('1')
            || m.ends_with("11")
            || m.ends_with("12")
        {
            "USA / NTSC-U"
        } else if lower_name.contains("eur")
            || lower_name.contains("pal")
            || m.ends_with('2')
            || m.ends_with('3')
            || m.ends_with('4')
        {
            "Europe / PAL"
        } else if lower_name.contains("jap")
            || lower_name.contains("ntsc-j")
            || m.ends_with('0')
        {
            "Japan / NTSC-J"
        } else if lower_name.contains("asia") || m.ends_with('6') || m.ends_with('7') {
            "Asia"
        } else {
            "Global / Outro"
        };

        let v = if lower_name.contains("v01.00")
            || lower_name.contains("1.00")
            || m.starts_with("SCPH-1")
        {
            "v01.00".to_string()
        } else if lower_name.contains("v01.60")
            || lower_name.contains("1.60")
            || m.starts_with("SCPH-3")
        {
            "v01.60".to_string()
        } else if lower_name.contains("v01.90")
            || lower_name.contains("1.90")
            || m.starts_with("SCPH-5")
        {
            "v01.90".to_string()
        } else if lower_name.contains("v02.00")
            || lower_name.contains("2.00")
            || m.starts_with("SCPH-700")
        {
            "v02.00".to_string()
        } else if lower_name.contains("v02.20")
            || lower_name.contains("2.20")
            || m.starts_with("SCPH-750")
            || m.starts_with("SCPH-770")
            || m.starts_with("SCPH-790")
        {
            "v02.20".to_string()
        } else if lower_name.contains("v02.30")
            || lower_name.contains("2.30")
            || m.starts_with("SCPH-9")
        {
            "v02.30".to_string()
        } else {
            "v01.xx".to_string()
        };

        (m, r.to_string(), v)
    };

    (model, region, version)
}

pub fn scan_bios_directory(bios_dir: &Path, active_bios_path: Option<&str>) -> Vec<BiosInfo> {
    let mut list = Vec::new();
    if !bios_dir.exists() || !bios_dir.is_dir() {
        return list;
    }

    let mut hashes_seen = std::collections::HashMap::new();

    if let Ok(entries) = fs::read_dir(bios_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !matches!(ext.as_str(), "bin" | "rom" | "gz" | "bios" | "") {
                continue;
            }

            if let Ok(meta) = entry.metadata() {
                let size = meta.len();
                if !(512 * 1024..=16 * 1024 * 1024).contains(&size) {
                    continue;
                }

                if let Ok(bytes) = fs::read(&path) {
                    let sha1 = compute_sha1(&bytes);
                    let filename = path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("bios.bin")
                        .to_string();
                    let (model, region, version) = identify_bios(&filename, &sha1);

                    let is_active = active_bios_path
                        .map(|ap| {
                            let ap_clean = sanitize_path(ap);
                            let ap_path = Path::new(&ap_clean);
                            if ap_path == path || ap_clean.eq_ignore_ascii_case(&path.to_string_lossy()) {
                                return true;
                            }
                            if let (Some(ap_name), Some(cur_name)) = (ap_path.file_name(), path.file_name()) {
                                if ap_name.to_string_lossy().eq_ignore_ascii_case(&cur_name.to_string_lossy()) {
                                    return true;
                                }
                            }
                            false
                        })
                        .unwrap_or(false);

                    let count = hashes_seen.entry(sha1.clone()).or_insert(0);
                    *count += 1;

                    list.push(BiosInfo {
                        filename,
                        path: path.to_string_lossy().to_string(),
                        size,
                        sha1,
                        region,
                        version,
                        model,
                        is_valid: true,
                        is_active,
                        is_duplicate: false,
                    });
                }
            }
        }
    }

    for item in &mut list {
        if let Some(&count) = hashes_seen.get(&item.sha1) {
            if count > 1 {
                item.is_duplicate = true;
            }
        }
    }

    list.sort_by(|a, b| b.is_active.cmp(&a.is_active).then_with(|| a.filename.cmp(&b.filename)));

    list
}

pub fn import_bios_archive(bios_dir: &Path, source_file: &Path) -> Result<BiosImportResult, String> {
    if !source_file.exists() || !source_file.is_file() {
        return Err("Arquivo de origem da BIOS não existe no disco.".to_string());
    }

    let ext = source_file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "zip" {
        let file = fs::File::open(source_file)
            .map_err(|e| format!("Não foi possível abrir o arquivo ZIP: {}", e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Falha ao ler conteúdo do arquivo ZIP: {}", e))?;

        if !bios_dir.exists() {
            fs::create_dir_all(bios_dir)
                .map_err(|e| format!("Falha ao criar diretório de BIOS: {}", e))?;
        }

        let mut existing_hashes: std::collections::HashSet<String> = scan_bios_directory(bios_dir, None)
            .into_iter()
            .map(|b| b.sha1.to_ascii_lowercase())
            .collect();

        let mut imported: Vec<BiosInfo> = Vec::new();
        let mut potential_indices: Vec<usize> = Vec::new();

        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                if !entry.is_file() {
                    continue;
                }
                let raw_name = entry.name();
                let file_part = raw_name.rsplit(['/', '\\']).next().unwrap_or(raw_name).to_lowercase();
                let is_valid_name = file_part.ends_with(".bin")
                    || file_part.ends_with(".rom")
                    || file_part.ends_with(".gz")
                    || file_part.ends_with(".bios");
                let size = entry.size();
                let is_valid_size = (512 * 1024..=16 * 1024 * 1024).contains(&size);

                if is_valid_name && is_valid_size {
                    potential_indices.push(i);
                }
            }
        }

        if potential_indices.is_empty() {
            return Err("O arquivo ZIP selecionado não contém nenhum arquivo de BIOS (.bin) válido de PlayStation 2.".to_string());
        }

        for i in potential_indices {
            let mut entry = match archive.by_index(i) {
                Ok(e) => e,
                Err(_) => continue,
            };

            let raw_name = entry.name().to_string();
            let file_part = raw_name.rsplit(['/', '\\']).next().unwrap_or(&raw_name).to_lowercase();
            let is_valid_name = file_part.ends_with(".bin")
                || file_part.ends_with(".rom")
                || file_part.ends_with(".gz")
                || file_part.ends_with(".bios");

            if !is_valid_name {
                continue;
            }

            let mut bytes = Vec::new();
            if entry.read_to_end(&mut bytes).is_err() {
                continue;
            }

            let size = bytes.len() as u64;
            if !(512 * 1024..=16 * 1024 * 1024).contains(&size) {
                continue;
            }

            let sha1 = compute_sha1(&bytes);
            let sha1_lower = sha1.to_ascii_lowercase();

            if existing_hashes.contains(&sha1_lower) {
                continue;
            }

            let raw_file_name = raw_name.rsplit(['/', '\\']).next().unwrap_or("bios.bin");
            let safe_name: String = raw_file_name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();

            let base_name = if safe_name.is_empty() { "bios.bin".to_string() } else { safe_name };
            let mut final_name = base_name.clone();
            let mut target_path = bios_dir.join(&final_name);
            let mut counter = 1;
            while target_path.exists() {
                let stem = Path::new(&base_name).file_stem().and_then(|s| s.to_str()).unwrap_or("bios");
                let ext_part = Path::new(&base_name).extension().and_then(|e| e.to_str()).unwrap_or("bin");
                final_name = format!("{}_{}.{}", stem, counter, ext_part);
                target_path = bios_dir.join(&final_name);
                counter += 1;
            }

            if fs::write(&target_path, &bytes).is_err() {
                continue;
            }

            existing_hashes.insert(sha1_lower);
            let (model, region, version) = identify_bios(&final_name, &sha1);

            imported.push(BiosInfo {
                filename: final_name,
                path: target_path.to_string_lossy().to_string(),
                size,
                sha1,
                region,
                version,
                model,
                is_valid: true,
                is_active: false,
                is_duplicate: false,
            });
        }

        if imported.is_empty() {
            return Err("Todas as BIOS encontradas no pacote ZIP já estão instaladas no emulador.".to_string());
        }

        let first = imported[0].clone();
        let count = imported.len();
        return Ok(BiosImportResult {
            count,
            items: imported,
            message: format!("{} arquivo(s) de BIOS importado(s) com sucesso a partir do arquivo ZIP.", count),
            filename: first.filename,
            path: first.path,
            size: first.size,
            sha1: first.sha1,
            region: first.region,
            version: first.version,
            model: first.model,
            is_valid: true,
            is_active: false,
            is_duplicate: false,
        });
    }

    let single = import_bios_file(bios_dir, source_file)?;
    Ok(BiosImportResult {
        count: 1,
        items: vec![single.clone()],
        message: format!("BIOS '{}' importada com sucesso.", single.filename),
        filename: single.filename,
        path: single.path,
        size: single.size,
        sha1: single.sha1,
        region: single.region,
        version: single.version,
        model: single.model,
        is_valid: single.is_valid,
        is_active: single.is_active,
        is_duplicate: single.is_duplicate,
    })
}

pub fn import_bios_file(bios_dir: &Path, source_file: &Path) -> Result<BiosInfo, String> {
    if !source_file.exists() || !source_file.is_file() {
        return Err("Arquivo de origem da BIOS não existe no disco.".to_string());
    }

    let ext = source_file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "zip" {
        let pkg = import_bios_archive(bios_dir, source_file)?;
        return pkg.items.into_iter().next().ok_or_else(|| {
            "Nenhuma BIOS válida foi encontrada no arquivo ZIP.".to_string()
        });
    }

    if !matches!(ext.as_str(), "bin" | "rom" | "gz" | "bios" | "") {
        return Err(
            "Extensão inválida para arquivo de BIOS. Formatos suportados: .bin, .rom, .gz, .bios, .zip"
                .to_string(),
        );
    }

    let meta = source_file
        .metadata()
        .map_err(|e| format!("Não foi possível ler os metadados do arquivo: {}", e))?;
    let size = meta.len();
    if !(512 * 1024..=16 * 1024 * 1024).contains(&size) {
        return Err(
            "O arquivo selecionado não possui o tamanho característico de uma BIOS de PlayStation 2."
                .to_string(),
        );
    }

    let bytes =
        fs::read(source_file).map_err(|e| format!("Falha ao ler arquivo da BIOS: {}", e))?;
    let sha1 = compute_sha1(&bytes);

    if !bios_dir.exists() {
        fs::create_dir_all(bios_dir)
            .map_err(|e| format!("Falha ao criar diretório de BIOS: {}", e))?;
    }

    let existing = scan_bios_directory(bios_dir, None);
    if let Some(dup) = existing.iter().find(|b| b.sha1.eq_ignore_ascii_case(&sha1)) {
        return Err(format!(
            "BIOS duplicada: o arquivo '{}' já possui este mesmo conteúdo.",
            dup.filename
        ));
    }

    let raw_name = source_file
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("bios.bin");
    let safe_name: String = raw_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    let target_path = bios_dir.join(&safe_name);
    fs::write(&target_path, &bytes)
        .map_err(|e| format!("Falha ao gravar arquivo no diretório de BIOS: {}", e))?;

    let (model, region, version) = identify_bios(&safe_name, &sha1);

    Ok(BiosInfo {
        filename: safe_name,
        path: target_path.to_string_lossy().to_string(),
        size,
        sha1,
        region,
        version,
        model,
        is_valid: true,
        is_active: false,
        is_duplicate: false,
    })
}

pub fn list_profiles(conn: &Connection) -> Result<Vec<Pcsx2Profile>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, executable_path, data_dir, bios_path, renderer, aspect_ratio, fullscreen, is_active, created_at, updated_at
             FROM pcsx2_profiles
             ORDER BY is_active DESC, name ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Pcsx2Profile {
                id: row.get(0)?,
                name: row.get(1)?,
                executable_path: row.get(2)?,
                data_dir: row.get(3)?,
                bios_path: row.get(4)?,
                renderer: row.get(5)?,
                aspect_ratio: row.get(6)?,
                fullscreen: row.get::<_, i64>(7)? != 0,
                is_active: row.get::<_, i64>(8)? != 0,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut list = Vec::new();
    for r in rows {
        if let Ok(p) = r {
            list.push(p);
        }
    }
    Ok(list)
}

pub fn get_active_profile(conn: &Connection) -> Result<Pcsx2Profile, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, executable_path, data_dir, bios_path, renderer, aspect_ratio, fullscreen, is_active, created_at, updated_at
             FROM pcsx2_profiles
             WHERE is_active = 1
             LIMIT 1",
        )
        .map_err(|e| e.to_string())?;

    stmt.query_row([], |row| {
        Ok(Pcsx2Profile {
            id: row.get(0)?,
            name: row.get(1)?,
            executable_path: row.get(2)?,
            data_dir: row.get(3)?,
            bios_path: row.get(4)?,
            renderer: row.get(5)?,
            aspect_ratio: row.get(6)?,
            fullscreen: row.get::<_, i64>(7)? != 0,
            is_active: row.get::<_, i64>(8)? != 0,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })
    .map_err(|e| format!("Nenhum perfil ativo encontrado: {}", e))
}

pub fn save_profile(conn: &mut Connection, profile: &Pcsx2Profile) -> Result<(), String> {
    if is_pcsx2_running() {
        return Err(
            "O PCSX2 está aberto. Feche o emulador antes de salvar alterações nos perfis para evitar perda de dados."
                .to_string(),
        );
    }

    if profile.name.trim().is_empty() {
        return Err("O nome do perfil não pode ser vazio.".to_string());
    }

    if profile.executable_path.trim().is_empty() {
        return Err("O caminho do executável do PCSX2 não pode ser vazio.".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    if profile.is_active {
        tx.execute("UPDATE pcsx2_profiles SET is_active = 0", [])
            .map_err(|e| e.to_string())?;
    }

    tx.execute(
        "INSERT INTO pcsx2_profiles (id, name, executable_path, data_dir, bios_path, renderer, aspect_ratio, fullscreen, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            executable_path = excluded.executable_path,
            data_dir = excluded.data_dir,
            bios_path = excluded.bios_path,
            renderer = excluded.renderer,
            aspect_ratio = excluded.aspect_ratio,
            fullscreen = excluded.fullscreen,
            is_active = excluded.is_active,
            updated_at = excluded.updated_at",
        params![
            profile.id,
            profile.name,
            profile.executable_path,
            profile.data_dir,
            profile.bios_path,
            profile.renderer,
            profile.aspect_ratio,
            if profile.fullscreen { 1 } else { 0 },
            if profile.is_active { 1 } else { 0 },
            profile.created_at,
            now,
        ],
    )
    .map_err(|e| e.to_string())?;

    if profile.is_active {
        let _ = tx.execute(
            "INSERT INTO configs (key, value) VALUES ('pcsx2_path', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![profile.executable_path],
        );
        let _ = tx.execute(
            "INSERT INTO configs (key, value) VALUES ('pcsx2_executable_path', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![profile.executable_path],
        );
        if !profile.data_dir.is_empty() {
            let _ = tx.execute(
                "INSERT INTO configs (key, value) VALUES ('pcsx2_data_dir', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![profile.data_dir],
            );
        }
    }

    tx.commit().map_err(|e| e.to_string())?;
    if profile.is_active {
        let _ = apply_profile_to_ini(profile);
    }
    Ok(())
}

fn set_ini_value(ini_text: &str, target_section: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = ini_text.lines().map(|s| s.to_string()).collect();
    let section_header = format!("[{}]", target_section);
    let mut current_section: Option<String> = None;
    let mut section_found = false;
    let mut key_updated = false;
    let mut section_header_idx: Option<usize> = None;

    for i in 0..lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let sec_name = &trimmed[1..trimmed.len() - 1];
            current_section = Some(sec_name.to_string());
            if sec_name.eq_ignore_ascii_case(target_section) {
                section_found = true;
                section_header_idx = Some(i);
            }
        } else if let Some(ref sec) = current_section {
            if sec.eq_ignore_ascii_case(target_section) {
                if let Some(eq_pos) = trimmed.find('=') {
                    let k = trimmed[..eq_pos].trim();
                    if k.eq_ignore_ascii_case(key) {
                        lines[i] = format!("{} = {}", key, value);
                        key_updated = true;
                        break;
                    }
                }
            }
        }
    }

    if !section_found {
        if !lines.is_empty() && !lines.last().unwrap().trim().is_empty() {
            lines.push("".to_string());
        }
        lines.push(section_header);
        lines.push(format!("{} = {}", key, value));
    } else if !key_updated {
        if let Some(idx) = section_header_idx {
            lines.insert(idx + 1, format!("{} = {}", key, value));
        }
    }

    lines.join("\r\n")
}

pub fn apply_profile_to_ini(profile: &Pcsx2Profile) -> Result<(), String> {
    let data_dir = if !profile.data_dir.is_empty() {
        PathBuf::from(&profile.data_dir)
    } else {
        let exe_path = PathBuf::from(&profile.executable_path);
        if let Ok(insp) = inspect_installation(&exe_path) {
            PathBuf::from(insp.data_dir)
        } else {
            exe_path.parent().unwrap_or(Path::new(".")).to_path_buf()
        }
    };

    let inis_dir = data_dir.join("inis");
    if fs::create_dir_all(&inis_dir).is_err() {
        return Err("Falha ao criar diretório inis do PCSX2.".to_string());
    }

    let ini_path = inis_dir.join("PCSX2.ini");
    let content = if ini_path.exists() {
        fs::read_to_string(&ini_path).unwrap_or_default()
    } else {
        String::new()
    };

    let is_wide = profile.aspect_ratio == "16:9" || profile.aspect_ratio.contains("16:9");
    let aspect_val = if is_wide { "16:9" } else { "4:3" };
    let wide_patches = if is_wide { "true" } else { "false" };

    let mut updated = set_ini_value(&content, "EmuCore/GS", "AspectRatio", aspect_val);
    updated = set_ini_value(&updated, "UI", "AspectRatio", aspect_val);
    updated = set_ini_value(&updated, "EmuCore", "EnableWideScreenPatches", wide_patches);
    updated = set_ini_value(&updated, "EmuCore/GS", "Renderer", &profile.renderer);

    if let Some(ref b) = profile.bios_path {
        let bios_name = Path::new(b)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(b);
        updated = set_ini_value(&updated, "Filenames", "BIOS", bios_name);
    }

    fs::write(&ini_path, updated.as_bytes()).map_err(|e| format!("Falha ao gravar PCSX2.ini: {}", e))?;
    Ok(())
}

pub fn set_active_profile(conn: &mut Connection, id: &str) -> Result<(), String> {
    if is_pcsx2_running() {
        return Err("O PCSX2 está aberto. Feche o emulador antes de alternar perfis.".to_string());
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("UPDATE pcsx2_profiles SET is_active = 0", [])
        .map_err(|e| e.to_string())?;
    let affected = tx
        .execute(
            "UPDATE pcsx2_profiles SET is_active = 1 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("Perfil não encontrado.".to_string());
    }

    let profile: Option<Pcsx2Profile> = tx
        .query_row(
            "SELECT id, name, executable_path, data_dir, bios_path, renderer, aspect_ratio, fullscreen, is_active, created_at, updated_at
             FROM pcsx2_profiles WHERE id = ?1",
            params![id],
            |r| {
                Ok(Pcsx2Profile {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    executable_path: r.get(2)?,
                    data_dir: r.get(3)?,
                    bios_path: r.get(4)?,
                    renderer: r.get(5)?,
                    aspect_ratio: r.get(6)?,
                    fullscreen: r.get::<_, i64>(7)? != 0,
                    is_active: true,
                    created_at: r.get(9)?,
                    updated_at: r.get(10)?,
                })
            },
        )
        .ok();

    if let Some(ref prof) = profile {
        let _ = tx.execute(
            "INSERT INTO configs (key, value) VALUES ('pcsx2_path', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![prof.executable_path],
        );
        let _ = tx.execute(
            "INSERT INTO configs (key, value) VALUES ('pcsx2_executable_path', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![prof.executable_path],
        );
        if !prof.data_dir.is_empty() {
            let _ = tx.execute(
                "INSERT INTO configs (key, value) VALUES ('pcsx2_data_dir', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![prof.data_dir],
            );
        }
    }

    tx.commit().map_err(|e| e.to_string())?;

    if let Some(ref prof) = profile {
        let _ = apply_profile_to_ini(prof);
    }
    Ok(())
}

pub fn delete_profile(conn: &mut Connection, id: &str) -> Result<(), String> {
    if is_pcsx2_running() {
        return Err("O PCSX2 está aberto. Feche o emulador antes de excluir perfis.".to_string());
    }

    let is_act: bool = conn
        .query_row(
            "SELECT is_active FROM pcsx2_profiles WHERE id = ?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .map(|v| v != 0)
        .unwrap_or(false);

    if is_act {
        return Err("Não é permitido excluir o perfil que está atualmente ativo.".to_string());
    }

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pcsx2_profiles", [], |r| r.get(0))
        .unwrap_or(0);
    if count <= 1 {
        return Err("Não é possível excluir o único perfil existente.".to_string());
    }

    conn.execute("DELETE FROM pcsx2_profiles WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run_diagnostic(conn: &Connection) -> Result<Pcsx2Diagnostic, String> {
    let profile = get_active_profile(conn).ok();
    let pcsx2_exe_path = profile
        .as_ref()
        .map(|p| p.executable_path.clone())
        .or_else(|| database::get_config(conn, "pcsx2_path"))
        .unwrap_or_default();

    let exe_path = PathBuf::from(&pcsx2_exe_path);
    let executable_valid = crate::exe_runner::validate_pcsx2_executable(&exe_path);
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    let (executable_version, executable_type, data_dir, bios_dir, covers_dir) = if executable_valid {
        let insp = inspect_installation(&exe_path).unwrap_or(Pcsx2Installation {
            path: pcsx2_exe_path.clone(),
            version: extract_pcsx2_version(&exe_path),
            is_portable: false,
            is_managed: false,
            data_dir: "".to_string(),
            bios_dir: "".to_string(),
            covers_dir: "".to_string(),
            inis_dir: "".to_string(),
            is_running: false,
        });
        let t = if insp.is_managed {
            "Gerenciado pelo RomManager"
        } else if insp.is_portable {
            "Portátil (Portable)"
        } else {
            "Instalação Padrão (Shared/Installer)"
        };

        let resolved_data_dir = profile
            .as_ref()
            .map(|p| p.data_dir.clone())
            .filter(|s| !s.is_empty())
            .or_else(|| database::get_config(conn, "pcsx2_data_dir").filter(|s| !s.is_empty()))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(insp.data_dir));

        let resolved_bios_dir = database::get_config(conn, "pcsx2_bios_dir")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| resolved_data_dir.join("bios"));

        let resolved_covers_dir = resolved_data_dir.join("covers");

        (
            insp.version,
            t.to_string(),
            resolved_data_dir,
            resolved_bios_dir,
            resolved_covers_dir,
        )
    } else {
        errors.push(format!(
            "Executável do PCSX2 não foi encontrado ou é inválido: '{}'",
            pcsx2_exe_path
        ));
        (
            "Desconhecido".to_string(),
            "Não encontrado".to_string(),
            PathBuf::new(),
            PathBuf::new(),
            PathBuf::new(),
        )
    };

    let is_running = is_pcsx2_running();
    if is_running {
        warnings.push(
            "O emulador PCSX2 está aberto. Edições de configuração e perfis estão bloqueadas temporariamente para evitar corrupção."
                .to_string(),
        );
    }

    let data_dir_valid = data_dir.exists() && data_dir.is_dir();
    let data_dir_writable = if data_dir_valid {
        let test_file = data_dir.join(".write_test");
        if fs::write(&test_file, b"test").is_ok() {
            let _ = fs::remove_file(test_file);
            true
        } else {
            false
        }
    } else {
        false
    };

    if executable_valid {
        if !data_dir_valid {
            warnings.push(format!(
                "Diretório de dados do PCSX2 não foi encontrado ou não é acessível: '{}'",
                data_dir.display()
            ));
        } else if !data_dir_writable {
            warnings.push(format!(
                "O diretório de dados do PCSX2 está protegido contra gravação: '{}'",
                data_dir.display()
            ));
        }
    }

    let bios_dir_valid = bios_dir.exists() && bios_dir.is_dir();
    let (bios_count, active_bios_valid) = if bios_dir_valid {
        let count = fs::read_dir(&bios_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| {
                        e.path().is_file()
                            && matches!(
                                e.path()
                                    .extension()
                                    .and_then(|ext| ext.to_str())
                                    .unwrap_or("")
                                    .to_lowercase()
                                    .as_str(),
                                "bin" | "rom" | "gz" | "bios" | ""
                            )
                    })
                    .count()
            })
            .unwrap_or(0);
        let active_valid = profile
            .as_ref()
            .and_then(|p| p.bios_path.as_deref())
            .filter(|bp| !bp.trim().is_empty())
            .map(|bp| Path::new(bp).exists())
            .unwrap_or(count > 0);
        if count == 0 {
            warnings.push(
                "Nenhuma BIOS encontrada no diretório do PCSX2. Jogos não poderão ser iniciados sem ao menos uma BIOS PS2 instalada."
                    .to_string(),
            );
        }
        (count, active_valid)
    } else {
        warnings.push(format!(
            "Diretório de BIOS do PCSX2 não existe: '{}'",
            bios_dir.display()
        ));
        (0, false)
    };

    let covers_dir_valid = covers_dir.exists() && covers_dir.is_dir();

    let roms_dir_str = database::get_config(conn, "roms_dir")
        .or_else(|| database::get_config(conn, "roms_directory"))
        .unwrap_or_default();
    let roms_dir_valid = !roms_dir_str.is_empty() && Path::new(&roms_dir_str).is_dir();
    if !roms_dir_valid {
        errors.push(format!(
            "Diretório de ROMs não configurado ou inexistente: '{}'",
            roms_dir_str
        ));
    }

    Ok(Pcsx2Diagnostic {
        executable_valid,
        executable_path: pcsx2_exe_path,
        executable_version,
        executable_type,
        is_running,
        data_dir_valid,
        data_dir_writable,
        bios_dir_valid,
        bios_count,
        active_bios_valid,
        covers_dir_valid,
        roms_dir_valid,
        warnings,
        errors,
    })
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    published_at: Option<String>,
    prerelease: bool,
    assets: Vec<GhAsset>,
}

fn find_windows_qt_asset(assets: &[GhAsset]) -> Option<&GhAsset> {
    assets.iter().find(|a| {
        let name = a.name.to_lowercase();
        name.contains("windows") && name.contains("qt") && name.ends_with(".7z") && !name.contains("symbols")
    })
}

pub async fn find_official_release(managed_dir: &Path, tag: &str) -> Result<Vec<ManagedRelease>, String> {
    if tag.is_empty() || tag.len() > 80 || !tag.bytes().all(|c| c.is_ascii_alphanumeric() || b".-_".contains(&c)) {
        return Err("Informe uma tag de versão válida, como v2.2.0.".into());
    }
    let client = reqwest::Client::builder().user_agent("RomManager").timeout(std::time::Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
    let release = client.get(format!("https://api.github.com/repos/PCSX2/pcsx2/releases/tags/{}", tag)).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|_| "Versão não encontrada no GitHub ou limite da API atingido.".to_string())?.json::<GhRelease>().await.map_err(|e| e.to_string())?;
    let asset = find_windows_qt_asset(&release.assets).ok_or("Esta versão não possui pacote Windows Qt compatível.")?;
    let version = release.tag_name.trim_start_matches('v');
    let entry = ManagedRelease {
        version: if release.prerelease { format!("{}-nightly", version) } else { version.to_string() },
        release_name: release.tag_name.clone(), channel: if release.prerelease { "nightly" } else { "stable" }.into(),
        published_at: release.published_at.clone().unwrap_or_default(), download_url: asset.browser_download_url.clone(), file_size: asset.size,
        installed: false, executable_path: None, data_dir: None,
    };
    let mut list = list_official_releases(managed_dir);
    list.retain(|r| r.version != entry.version);
    list.insert(0, entry);
    fs::create_dir_all(managed_dir).map_err(|e| e.to_string())?;
    fs::write(managed_dir.join("releases_cache_v2.json"), serde_json::to_vec(&list).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(list_official_releases(managed_dir))
}

pub async fn list_official_releases_async(managed_dir: &Path, force_refresh: bool) -> Vec<ManagedRelease> {
    let cache_file = managed_dir.join("releases_cache_v2.json");
    let mut should_fetch = true;

    if !force_refresh && cache_file.exists() {
        if let Ok(meta) = fs::metadata(&cache_file) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    if elapsed.as_secs() < 1800 {
                        should_fetch = false;
                    }
                }
            }
        }
    }

    if should_fetch {
        if let Ok(client) = reqwest::Client::builder()
            .user_agent("RomManager/0.5.1")
            .timeout(std::time::Duration::from_secs(8))
            .build()
        {
            let mut fetched_releases = Vec::new();

            if let Ok(resp) = client.get("https://api.github.com/repos/PCSX2/pcsx2/releases/latest").send().await {
                if resp.status().is_success() {
                    if let Ok(rel) = resp.json::<GhRelease>().await {
                        if let Some(asset) = find_windows_qt_asset(&rel.assets) {
                            let tag = rel.tag_name.trim_start_matches('v');
                            fetched_releases.push(ManagedRelease {
                                version: tag.to_string(),
                                release_name: format!("PCSX2 v{} (Estável Oficial)", tag),
                                channel: "stable".to_string(),
                                published_at: rel.published_at.unwrap_or_default(),
                                download_url: asset.browser_download_url.clone(),
                                file_size: asset.size,
                                installed: false,
                                executable_path: None,
                                data_dir: None,
                            });
                        }
                    }
                }
            }

            if let Ok(resp) = client.get("https://api.github.com/repos/PCSX2/pcsx2/releases?per_page=100").send().await {
                if resp.status().is_success() {
                    if let Ok(releases) = resp.json::<Vec<GhRelease>>().await {
                        for rel in releases {
                            {
                                if let Some(asset) = find_windows_qt_asset(&rel.assets) {
                                    let tag = rel.tag_name.trim_start_matches('v');
                                    fetched_releases.push(ManagedRelease {
                                        version: if rel.prerelease { format!("{}-nightly", tag) } else { tag.to_string() },
                                        release_name: format!("PCSX2 v{} Nightly (Canal Oficial)", tag),
                                        channel: if rel.prerelease { "nightly" } else { "stable" }.to_string(),
                                        published_at: rel.published_at.unwrap_or_default(),
                                        download_url: asset.browser_download_url.clone(),
                                        file_size: asset.size,
                                        installed: false,
                                        executable_path: None,
                                        data_dir: None,
                                    });

                                }
                            }
                        }
                    }
                }
            }

            if !fetched_releases.is_empty() {
                let mut seen = std::collections::HashSet::new();
                fetched_releases.retain(|r| seen.insert(r.version.clone()));
                let _ = fs::create_dir_all(managed_dir);
                let _ = fs::write(&cache_file, serde_json::to_string(&fetched_releases).unwrap_or_default());
            }
        }
    }

    list_official_releases(managed_dir)
}

pub fn list_official_releases(managed_dir: &Path) -> Vec<ManagedRelease> {
    let current_version = if managed_dir.join("version.json").exists() {
        fs::read_to_string(managed_dir.join("version.json"))
            .ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|v| {
                v.get("version")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string())
            })
    } else {
        None
    };

    let check_installed = |v: &str| -> (bool, Option<String>, Option<String>) {
        let v_clean = v.trim_start_matches('v');
        let candidates = [
            managed_dir.join("versions").join(v),
            managed_dir.join("versions").join(v_clean),
            managed_dir.join("versions").join(format!("v{}", v_clean)),
            managed_dir.join(v),
            managed_dir.join(v_clean),
        ];
        for dir in candidates {
            let exe = dir.join("pcsx2-qt.exe");
            if exe.exists() {
                return (true, Some(exe.to_string_lossy().to_string()), Some(dir.to_string_lossy().to_string()));
            }
        }
        if let Some(cur) = &current_version {
            if cur.trim_start_matches('v') == v_clean {
                let exe = managed_dir.join("pcsx2-qt.exe");
                if exe.exists() {
                    return (true, Some(exe.to_string_lossy().to_string()), Some(managed_dir.to_string_lossy().to_string()));
                }
            }
        }
        (false, None, None)
    };

    let cache_file = managed_dir.join("releases_cache_v2.json");
    let mut raw_releases: Vec<ManagedRelease> = if cache_file.exists() {
        fs::read_to_string(&cache_file)
            .ok()
            .and_then(|c| serde_json::from_str::<Vec<ManagedRelease>>(&c).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let versions_dir = managed_dir.join("versions");
    if versions_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&versions_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let exe = p.join("pcsx2-qt.exe");
                    if exe.exists() {
                        let dir_name = entry.file_name().to_string_lossy().to_string();
                        let dir_clean = dir_name.trim_start_matches('v').to_string();
                        let already_present = raw_releases.iter().any(|r| {
                            r.version == dir_name
                                || r.version.trim_start_matches('v') == dir_clean
                        });
                        if !already_present {
                            let is_nightly = dir_name.to_lowercase().contains("nightly");
                            raw_releases.push(ManagedRelease {
                                version: dir_name.clone(),
                                release_name: format!(
                                    "PCSX2 v{} (Instalação Local)",
                                    dir_clean
                                ),
                                channel: if is_nightly { "nightly" } else { "stable" }.to_string(),
                                published_at: String::new(),
                                download_url: String::new(),
                                file_size: 0,
                                installed: true,
                                executable_path: Some(exe.to_string_lossy().to_string()),
                                data_dir: Some(p.to_string_lossy().to_string()),
                            });
                        }
                    }
                }
            }
        }
    }

    for rel in &mut raw_releases {
        let (inst, exe, dir) = check_installed(&rel.version);
        if inst {
            rel.installed = true;
            rel.executable_path = exe;
            rel.data_dir = dir;
        }
    }

    raw_releases
}

pub fn install_or_update_managed_pcsx2(
    managed_dir: &Path,
    version: &str,
    mock_payload: Option<&[u8]>,
    simulate_fail: bool,
) -> Result<Pcsx2Installation, String> {
    let backup_dir = managed_dir.with_extension("backup");

    if managed_dir.exists() {
        if backup_dir.exists() {
            let _ = fs::remove_dir_all(&backup_dir);
        }
        copy_dir_recursive(managed_dir, &backup_dir)
            .map_err(|e| format!("Falha ao criar backup da versão anterior: {}", e))?;
    }

    if let Err(e) = apply_managed_install(managed_dir, version, mock_payload, simulate_fail) {
        if backup_dir.exists() {
            let _ = fs::remove_dir_all(managed_dir);
            let _ = copy_dir_recursive(&backup_dir, managed_dir);
            let _ = fs::remove_dir_all(&backup_dir);
            return Err(format!(
                "Falha na extração/instalação. A versão anterior foi restaurada com sucesso. Erro: {}",
                e
            ));
        }
        return Err(format!("Falha na instalação: {}", e));
    }

    if backup_dir.exists() {
        let _ = fs::remove_dir_all(backup_dir);
    }

    let ver_sub = managed_dir.join("versions").join(version);
    let exe_sub = ver_sub.join("pcsx2-qt.exe");
    let exe_path = if exe_sub.exists() {
        exe_sub
    } else {
        managed_dir.join("pcsx2-qt.exe")
    };
    inspect_installation(&exe_path)
}

fn apply_managed_install(
    managed_dir: &Path,
    version: &str,
    mock_payload: Option<&[u8]>,
    simulate_fail: bool,
) -> Result<(), String> {
    if simulate_fail {
        return Err("Falha simulada durante descompactação do pacote oficial.".to_string());
    }

    fs::create_dir_all(managed_dir)
        .map_err(|e| format!("Falha ao criar diretório gerenciado: {}", e))?;

    let is_nightly = version.to_lowercase().contains("nightly");
    let version_info = serde_json::json!({
        "version": version,
        "channel": if is_nightly { "nightly" } else { "stable" },
        "installed_at": Utc::now().to_rfc3339()
    });

    let ver_sub = managed_dir.join("versions").join(version);
    fs::create_dir_all(&ver_sub)
        .map_err(|e| format!("Falha ao criar diretório da versão {}: {}", version, e))?;

    let exe_sub = ver_sub.join("pcsx2-qt.exe");
    let exe_root = managed_dir.join("pcsx2-qt.exe");

    if let Some(bytes) = mock_payload {
        if bytes.starts_with(b"7z\xbc\xaf'\x1c") {
            let temp_archive = ver_sub.join("download.7z");
            fs::write(&temp_archive, bytes).map_err(|e| format!("Falha ao gravar arquivo temporário 7z: {}", e))?;
            if let Err(e) = sevenz_rust::decompress_file(&temp_archive, &ver_sub) {
                let _ = fs::remove_file(&temp_archive);
                return Err(format!("Falha ao descompactar pacote 7z oficial: {}", e));
            }
            let _ = fs::remove_file(&temp_archive);
            if !exe_sub.exists() {
                if let Ok(entries) = walkdir::WalkDir::new(&ver_sub).into_iter().collect::<Result<Vec<_>, _>>() {
                    for entry in entries {
                        if entry.file_name().to_string_lossy().eq_ignore_ascii_case("pcsx2-qt.exe") {
                            let _ = fs::copy(entry.path(), &exe_sub);
                            break;
                        }
                    }
                }
            }
            if exe_sub.exists() {
                let _ = fs::copy(&exe_sub, &exe_root);
            }
        } else {
            fs::write(&exe_sub, bytes).map_err(|e| format!("Falha ao gravar executável da versão: {}", e))?;
            fs::write(&exe_root, bytes).map_err(|e| format!("Falha ao gravar executável principal: {}", e))?;
        }
    } else {
        if !exe_sub.exists() {
            fs::write(&exe_sub, b"MZ_MOCK_PCSX2_QT_EXECUTABLE")
                .map_err(|e| format!("Falha ao criar executável da versão: {}", e))?;
        }
        if !exe_root.exists() {
            let _ = fs::write(&exe_root, b"MZ_MOCK_PCSX2_QT_EXECUTABLE");
        }
    }

    let inis_dir = managed_dir.join("inis");
    let bios_dir = managed_dir.join("bios");
    let covers_dir = managed_dir.join("covers");
    let _ = fs::create_dir_all(inis_dir);
    let _ = fs::create_dir_all(bios_dir);
    let _ = fs::create_dir_all(covers_dir);
    let _ = fs::write(managed_dir.join("portable.ini"), b"");
    let _ = fs::write(managed_dir.join("version.json"), version_info.to_string());

    let _ = fs::create_dir_all(ver_sub.join("inis"));
    let _ = fs::create_dir_all(ver_sub.join("bios"));
    let _ = fs::create_dir_all(ver_sub.join("covers"));
    let _ = fs::write(ver_sub.join("portable.ini"), b"");
    let _ = fs::write(ver_sub.join("version.json"), version_info.to_string());

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn export_covers_to_pcsx2(
    conn: &Connection,
    pcsx2_covers_dir: &Path,
    overwrite: bool,
) -> Result<CoversExportResult, String> {
    if !pcsx2_covers_dir.exists() {
        fs::create_dir_all(pcsx2_covers_dir)
            .map_err(|e| format!("Falha ao criar diretório de capas do PCSX2: {}", e))?;
    }

    let games = crate::rom_scanner::list_installed_games(conn)
        .map_err(|e| format!("Falha ao listar jogos da biblioteca: {}", e))?;

    let roms_covers_dir_str = database::get_config(conn, "covers_dir")
        .or_else(|| database::get_config(conn, "covers_directory"))
        .unwrap_or_default();
    let roms_covers_dir = if !roms_covers_dir_str.is_empty() {
        PathBuf::from(roms_covers_dir_str)
    } else {
        database::get_config(conn, "roms_dir")
            .map(|r| PathBuf::from(r).join("covers"))
            .unwrap_or_default()
    };

    let total_games = games.len();
    let mut exported = 0;
    let mut preserved = 0;
    let mut skipped_no_serial = 0;
    let mut errors = 0;

    for game in &games {
        let serial = match game.serial.as_deref().filter(|s| !s.trim().is_empty()) {
            Some(s) => s.trim(),
            None => {
                skipped_no_serial += 1;
                continue;
            }
        };

        let safe_serial: String = serial
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if safe_serial.is_empty() {
            skipped_no_serial += 1;
            continue;
        }

        let mut local_cover_path = None;
        if let Some(ref cover_ref) = game.cover_url {
            let decoded_name = if let Some(idx) = cover_ref.find("file=") {
                let param = &cover_ref[idx + 5..];
                let file_part = param.split('&').next().unwrap_or(param);
                urlencoding::decode(file_part)
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| file_part.to_string())
            } else {
                cover_ref.clone()
            };

            let candidate = roms_covers_dir.join(&decoded_name);
            if candidate.is_file() {
                local_cover_path = Some(candidate);
            } else {
                let direct = PathBuf::from(&decoded_name);
                if direct.is_file() {
                    local_cover_path = Some(direct);
                }
            }
        }
        if local_cover_path.is_none() {
            let clean = crate::rom_scanner::clean_title_from_filename(&game.filename);
            if let Some(cover_filename) = crate::rom_scanner::find_local_cover(
                &roms_covers_dir,
                Some(serial),
                &clean,
                &game.title,
            ) {
                local_cover_path = Some(roms_covers_dir.join(cover_filename));
            }
        }

        if let Some(source_cover_path) = local_cover_path {
            let ext = source_cover_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg")
                .to_lowercase();
            let dest_cover_path = pcsx2_covers_dir.join(format!("{}.{}", safe_serial, ext));

            let formatted_path = if safe_serial.len() == 9
                && safe_serial[..4].chars().all(|c| c.is_ascii_alphabetic())
                && safe_serial[4..].chars().all(|c| c.is_ascii_digit())
            {
                Some(pcsx2_covers_dir.join(format!("{}-{}.{}", &safe_serial[..4], &safe_serial[4..], ext)))
            } else {
                None
            };

            if dest_cover_path.exists() && !overwrite {
                preserved += 1;
            } else {
                match fs::copy(&source_cover_path, &dest_cover_path) {
                    Ok(_) => {
                        exported += 1;
                        if let Some(ref fp) = formatted_path {
                            if fp != &dest_cover_path && (overwrite || !fp.exists()) {
                                let _ = fs::copy(&source_cover_path, fp);
                            }
                        }
                    }
                    Err(_) => errors += 1,
                }
            }
        }
    }

    Ok(CoversExportResult {
        total_games,
        exported,
        preserved,
        skipped_no_serial,
        errors,
        target_directory: pcsx2_covers_dir.to_string_lossy().to_string(),
    })
}

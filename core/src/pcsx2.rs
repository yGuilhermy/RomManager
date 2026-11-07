use crate::database;
use rusqlite::Connection;
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
pub struct CoversExportResult {
    pub total_games: usize,
    pub exported: usize,
    pub preserved: usize,
    pub skipped_no_serial: usize,
    pub errors: usize,
    pub target_directory: String,
}

pub fn is_pcsx2_running() -> bool {
    #[cfg(windows)]
    if let Some(running)=crate::windows_processes::pcsx2_running() {return running;}
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
    input
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn extract_pe_version(bytes: &[u8]) -> Option<String> {
    let pv_tag = [
        b'P', 0, b'r', 0, b'o', 0, b'd', 0, b'u', 0, b'c', 0, b't', 0, b'V', 0, b'e', 0, b'r', 0,
        b's', 0, b'i', 0, b'o', 0, b'n', 0,
    ];
    let fv_tag = [
        b'F', 0, b'i', 0, b'l', 0, b'e', 0, b'V', 0, b'e', 0, b'r', 0, b's', 0, b'i', 0, b'o', 0,
        b'n', 0,
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
    use crate::cache::{EXE_VERSIONS, FileStamp};
    let stamp = (
        FileStamp::of(exe_path),
        exe_path
            .parent()
            .and_then(|p| FileStamp::of(&p.join("version.json"))),
    );
    if stamp.0.is_none() {
        return read_pcsx2_version(exe_path);
    }
    let key = exe_path.to_path_buf();
    if let Some(version) = EXE_VERSIONS.get(&key, &stamp) {
        return version;
    }
    let version = read_pcsx2_version(exe_path);
    EXE_VERSIONS.put(key, stamp, version.clone());
    version
}

fn read_pcsx2_version(exe_path: &Path) -> String {
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
        parent_dir.join("portable.ini").exists() || parent_dir.join("portable.txt").exists();
    let is_managed = parent_dir.join("version.json").exists()
        || parent_dir
            .to_string_lossy()
            .to_lowercase()
            .contains("managed_pcsx2");

    let data_dir = if is_portable {
        parent_dir.to_path_buf()
    } else {
        crate::cache::USER_DIR
            .get_or_init(|| {
                let mut documents = Command::new("powershell.exe");
                documents.args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "[Environment]::GetFolderPath('MyDocuments')",
                ]);
                #[cfg(windows)]
                let documents = {
                    use std::os::windows::process::CommandExt;
                    documents.creation_flags(0x08000000);
                    documents.output().ok()
                };
                #[cfg(not(windows))]
                let documents: Option<std::process::Output> = None;
                documents
                    .filter(|o| o.status.success())
                    .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()).join("PCSX2"))
                    .unwrap_or_else(|| {
                        PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
                            .join("Documents/PCSX2")
                    })
            })
            .clone()
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
    candidates.push(PathBuf::from(
        "C:\\Program Files (x86)\\PCSX2\\pcsx2-qt.exe",
    ));
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
            if info.is_managed
                && results.iter().any(|existing: &Pcsx2Installation| {
                    existing.is_managed && existing.version == info.version
                })
            {
                continue;
            }
            results.push(info);
        }
    }

    results
}

pub fn compute_sha1(data: &[u8]) -> String {
    let mut state = [
        0x67452301u32,
        0xEFCDAB89u32,
        0x98BADCFEu32,
        0x10325476u32,
        0xC3D2E1F0u32,
    ];

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
        } else if lower_name.contains("jap") || lower_name.contains("ntsc-j") || m.ends_with('0') {
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

                if let Some(sha1) = crate::cache::bios_sha1(&path) {
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
                            if ap_path == path
                                || ap_clean.eq_ignore_ascii_case(&path.to_string_lossy())
                            {
                                return true;
                            }
                            if let (Some(ap_name), Some(cur_name)) =
                                (ap_path.file_name(), path.file_name())
                            {
                                if ap_name
                                    .to_string_lossy()
                                    .eq_ignore_ascii_case(&cur_name.to_string_lossy())
                                {
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

    list.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
            .then_with(|| a.filename.cmp(&b.filename))
    });

    list
}

pub fn import_bios_archive(
    bios_dir: &Path,
    source_file: &Path,
) -> Result<BiosImportResult, String> {
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

        let mut existing_hashes: std::collections::HashSet<String> =
            scan_bios_directory(bios_dir, None)
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
                let file_part = raw_name
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(raw_name)
                    .to_lowercase();
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
            let file_part = raw_name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&raw_name)
                .to_lowercase();
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

            let base_name = if safe_name.is_empty() {
                "bios.bin".to_string()
            } else {
                safe_name
            };
            let mut final_name = base_name.clone();
            let mut target_path = bios_dir.join(&final_name);
            let mut counter = 1;
            while target_path.exists() {
                let stem = Path::new(&base_name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("bios");
                let ext_part = Path::new(&base_name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("bin");
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
            return Err(
                "Todas as BIOS encontradas no pacote ZIP já estão instaladas no emulador."
                    .to_string(),
            );
        }

        let first = imported[0].clone();
        let count = imported.len();
        return Ok(BiosImportResult {
            count,
            items: imported,
            message: format!(
                "{} arquivo(s) de BIOS importado(s) com sucesso a partir do arquivo ZIP.",
                count
            ),
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
        return pkg
            .items
            .into_iter()
            .next()
            .ok_or_else(|| "Nenhuma BIOS válida foi encontrada no arquivo ZIP.".to_string());
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

pub fn set_ini_value(ini_text: &str, target_section: &str, key: &str, value: &str) -> String {
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

pub fn current_installation(conn: &Connection) -> Result<Pcsx2Installation, String> {
    let exe = database::get_config(conn, "pcsx2_path")
        .ok_or("Configure o executável do PCSX2 em Configurações.")?;
    let mut installation = inspect_installation(Path::new(&sanitize_path(&exe)))?;
    if let Some(data) =
        database::get_config(conn, "pcsx2_data_dir").filter(|s| !s.trim().is_empty())
    {
        installation.data_dir = sanitize_path(&data);
    }
    installation.inis_dir = Path::new(&installation.data_dir)
        .join("inis")
        .to_string_lossy()
        .into();
    installation.bios_dir = resolve_folder(&installation, "Bios", "bios")
        .to_string_lossy()
        .into();
    installation.covers_dir = resolve_folder(&installation, "Covers", "covers")
        .to_string_lossy()
        .into();
    Ok(installation)
}
fn resolve_folder(installation: &Pcsx2Installation, key: &str, fallback: &str) -> PathBuf {
    let data = Path::new(&installation.data_dir);
    let ini = fs::read_to_string(data.join("inis/PCSX2.ini")).unwrap_or_default();
    let folder = crate::memory_cards::ini_value(&ini, "Folders", key)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback.into());
    let folder = PathBuf::from(folder);
    if folder.is_absolute() {
        folder
    } else {
        data.join(folder)
    }
}
pub fn native_folder(conn: &Connection, key: &str, fallback: &str) -> Result<PathBuf, String> {
    Ok(resolve_folder(&current_installation(conn)?, key, fallback))
}
pub fn installation_id(conn: &Connection) -> Result<String, String> {
    let i = current_installation(conn)?;
    let normalize = |p: &str| {
        fs::canonicalize(p)
            .unwrap_or_else(|_| PathBuf::from(p))
            .to_string_lossy()
            .to_lowercase()
    };
    Ok(crate::memory_cards::digest(
        format!("{}|{}", normalize(&i.path), normalize(&i.data_dir)).as_bytes(),
    ))
}
pub fn active_bios(conn: &Connection) -> Result<Option<String>, String> {
    let data = current_installation(conn)?.data_dir;
    let ini = fs::read_to_string(Path::new(&data).join("inis/PCSX2.ini")).unwrap_or_default();
    Ok(crate::memory_cards::ini_value(&ini, "Filenames", "BIOS")
        .filter(|s| !s.is_empty())
        .map(|p| {
            {
                let p = PathBuf::from(p);
                if p.is_absolute() {
                    p
                } else {
                    native_folder(conn, "Bios", "bios").unwrap().join(p)
                }
            }
            .to_string_lossy()
            .into_owned()
        }))
}
pub fn select_bios(conn: &Connection, source: &str) -> Result<(), String> {
    crate::memory_cards::writable()?;
    let path = PathBuf::from(sanitize_path(source));
    let folder = native_folder(conn, "Bios", "bios")?;
    if path.canonicalize().map_err(|e| e.to_string())?.parent()
        != Some(folder.canonicalize().map_err(|e| e.to_string())?.as_path())
    {
        return Err("Selecione uma BIOS da pasta configurada no PCSX2.".into());
    }
    if !scan_bios_directory(&folder, None)
        .iter()
        .any(|b| b.path == path.to_string_lossy() && b.is_valid)
    {
        return Err("BIOS inválida".into());
    }
    let target = Path::new(&current_installation(conn)?.data_dir).join("inis/PCSX2.ini");
    let ini = fs::read_to_string(&target).unwrap_or_default();
    let next = set_ini_value(
        &ini,
        "Filenames",
        "BIOS",
        &path.file_name().unwrap().to_string_lossy(),
    );
    fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    if target.exists() {
        fs::copy(
            &target,
            target.with_file_name(format!(
                "PCSX2.before-bios-{}.ini",
                crate::memory_cards::stamp()
            )),
        )
        .map_err(|e| e.to_string())?;
    }
    crate::game_resources::atomic_file(&target, next.as_bytes())
}
pub fn run_diagnostic(conn: &Connection) -> Result<Pcsx2Diagnostic, String> {
    let result = current_installation(conn);
    let path = database::get_config(conn, "pcsx2_path").unwrap_or_default();
    let mut diagnostic = Pcsx2Diagnostic {
        executable_valid: result.is_ok(),
        executable_path: path,
        executable_version: String::new(),
        executable_type: String::new(),
        is_running: is_pcsx2_running(),
        data_dir_valid: false,
        data_dir_writable: false,
        bios_dir_valid: false,
        bios_count: 0,
        active_bios_valid: false,
        covers_dir_valid: false,
        roms_dir_valid: database::get_config(conn, "roms_dir")
            .is_some_and(|p| Path::new(&p).is_dir()),
        warnings: vec![],
        errors: vec![],
    };
    match result {
        Ok(i) => {
            diagnostic.executable_version = i.version;
            diagnostic.executable_type = if i.is_portable {
                "Portátil"
            } else {
                "Instalação do usuário"
            }
            .into();
            diagnostic.data_dir_valid = Path::new(&i.data_dir).is_dir();
            diagnostic.data_dir_writable =
                fs::metadata(&i.data_dir).is_ok_and(|m| !m.permissions().readonly());
            diagnostic.bios_dir_valid = Path::new(&i.bios_dir).is_dir();
            let bios = scan_bios_directory(Path::new(&i.bios_dir), active_bios(conn)?.as_deref());
            diagnostic.bios_count = bios.iter().filter(|b| b.is_valid).count();
            diagnostic.active_bios_valid = bios.iter().any(|b| b.is_active && b.is_valid);
            diagnostic.covers_dir_valid = Path::new(&i.covers_dir).is_dir();
            if diagnostic.bios_count == 0 {
                diagnostic
                    .warnings
                    .push("Nenhuma BIOS válida encontrada.".into());
            }
        }
        Err(e) => diagnostic.errors.push(e),
    }
    if !diagnostic.roms_dir_valid {
        diagnostic.errors.push("Pasta de ROMs ausente.".into());
    }
    Ok(diagnostic)
}

pub fn export_covers_to_pcsx2(
    conn: &Connection,
    pcsx2_covers_dir: &Path,
    overwrite: bool,
) -> Result<CoversExportResult, String> {
    export_covers_for_game(conn, pcsx2_covers_dir, overwrite, None)
}
pub fn export_covers_for_game(
    conn: &Connection,
    pcsx2_covers_dir: &Path,
    overwrite: bool,
    game_id: Option<&str>,
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

    let games: Vec<_> = games
        .into_iter()
        .filter(|g| game_id.is_none_or(|id| id == g.id))
        .collect();
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
                Some(pcsx2_covers_dir.join(format!(
                    "{}-{}.{}",
                    &safe_serial[..4],
                    &safe_serial[4..],
                    ext
                )))
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

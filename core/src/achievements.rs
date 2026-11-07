use crate::{memory_cards as mc, rom_scanner};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

static MEMORY_VAULT: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static ENDPOINT_OVERRIDE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Clone, Debug, Default)]
pub struct BgSyncState {
    pub syncing: bool,
    pub sync_completed: usize,
    pub sync_total: usize,
    pub sync_current_game: Option<String>,
    pub cache_revision: u64,
}

static BG_SYNC: LazyLock<Mutex<BgSyncState>> = LazyLock::new(|| Mutex::new(BgSyncState::default()));

#[derive(Serialize, Deserialize, Clone)]
struct VaultSecret {
    username: String,
    token: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pcsx2AchievementsConfig {
    pub enabled: bool,
    pub hardcore_mode: bool,
    pub encore_mode: bool,
    pub spectator_mode: bool,
    pub notifications: bool,
    pub sound_effects: bool,
    pub overlays: bool,
    pub pcsx2_username: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RaAccountSummary {
    pub username: String,
    pub score: i64,
    pub softcore_score: i64,
    pub avatar_url: String,
    pub profile_url: String,
    pub synced_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RaAchievementItem {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub points: u32,
    pub badge_name: String,
    pub badge_url: String,
    pub badge_locked_url: String,
    pub kind: Option<String>,
    pub unlocked_casual: bool,
    pub unlocked_hardcore: bool,
    pub official_url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RaGameProgress {
    pub game_id: String,
    pub game_title: String,
    pub serial: Option<String>,
    pub format: String,
    pub file_hash: String,
    pub boot_executable: String,
    pub ra_game_id: u64,
    pub ra_title: String,
    pub icon_url: String,
    pub official_game_url: Option<String>,
    pub recognition_status: String,
    pub recognition_message: String,
    pub total_achievements: usize,
    pub unlocked_casual: usize,
    pub unlocked_hardcore: usize,
    pub points_total: u32,
    pub points_earned_casual: u32,
    pub points_earned_hardcore: u32,
    pub achievements: Vec<RaAchievementItem>,
    pub synced_at: Option<String>,
    pub from_cache: bool,
    pub offline: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RaGameSummaryRow {
    pub game_id: String,
    pub game_title: String,
    pub serial: Option<String>,
    pub format: String,
    pub ra_game_id: u64,
    pub ra_title: String,
    pub icon_url: String,
    pub official_game_url: Option<String>,
    pub recognition_status: String,
    pub recognition_message: String,
    pub total_achievements: usize,
    pub unlocked_casual: usize,
    pub unlocked_hardcore: usize,
    pub points_total: u32,
    pub points_earned_casual: u32,
    pub points_earned_hardcore: u32,
    pub synced_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AchievementsState {
    pub linked: bool,
    pub account: Option<RaAccountSummary>,
    pub pcsx2: Pcsx2AchievementsConfig,
    pub games: Vec<RaGameSummaryRow>,
    pub offline: bool,
    pub notice: Option<String>,
    pub syncing: bool,
    pub sync_completed: usize,
    pub sync_total: usize,
    pub sync_current_game: Option<String>,
    pub cache_revision: u64,
}

#[derive(Deserialize)]
pub struct LoginPayload {
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub token: String,
}

#[derive(Deserialize)]
pub struct GameProgressQuery {
    pub game_id: String,
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Deserialize)]
pub struct SyncGamePayload {
    pub game_id: String,
}

pub fn set_endpoint_override(url: Option<String>) {
    if let Ok(mut guard) = ENDPOINT_OVERRIDE.lock() {
        *guard = url;
    }
}

fn endpoint_url() -> String {
    if let Ok(guard) = ENDPOINT_OVERRIDE.lock() {
        if let Some(url) = guard.clone() {
            return url;
        }
    }
    if let Ok(url) = std::env::var("ROMMANAGER_RA_ENDPOINT") {
        if !url.trim().is_empty() {
            return url;
        }
    }
    "https://retroachievements.org/dorequest.php".to_string()
}

fn use_memory_vault() -> bool {
    ENDPOINT_OVERRIDE
        .lock()
        .ok()
        .is_some_and(|g| g.is_some())
        || std::env::var("ROMMANAGER_VAULT_MODE")
            .ok()
            .is_some_and(|v| v.eq_ignore_ascii_case("memory"))
}

fn vault_target_name(db_path: &Path) -> String {
    let key = rom_scanner::normalized_path_key(db_path);
    let hash = &mc::digest(key.as_bytes())[..16];
    format!("RomManager:RetroAchievements:{hash}")
}

#[cfg(windows)]
mod win_cred {
    use std::ffi::c_void;
    use std::ptr;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    struct CredentialW {
        flags: u32,
        cred_type: u32,
        target_name: *mut u16,
        comment: *mut u16,
        last_written: FileTime,
        credential_blob_size: u32,
        credential_blob: *mut u8,
        persist: u32,
        attribute_count: u32,
        attributes: *mut c_void,
        target_alias: *mut u16,
        user_name: *mut u16,
    }

    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
        fn CredReadW(
            target_name: *const u16,
            cred_type: u32,
            flags: u32,
            credential: *mut *mut CredentialW,
        ) -> i32;
        fn CredDeleteW(target_name: *const u16, cred_type: u32, flags: u32) -> i32;
        fn CredFree(buffer: *mut c_void);
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn write(target: &str, username: &str, secret_json: &str) -> Result<(), String> {
        let mut target_w = to_wide(target);
        let mut user_w = to_wide(username);
        let mut blob = secret_json.as_bytes().to_vec();
        let cred = CredentialW {
            flags: 0,
            cred_type: 1,
            target_name: target_w.as_mut_ptr(),
            comment: ptr::null_mut(),
            last_written: FileTime { low: 0, high: 0 },
            credential_blob_size: blob.len() as u32,
            credential_blob: blob.as_mut_ptr(),
            persist: 2,
            attribute_count: 0,
            attributes: ptr::null_mut(),
            target_alias: ptr::null_mut(),
            user_name: user_w.as_mut_ptr(),
        };
        let ok = unsafe { CredWriteW(&cred, 0) };
        if ok == 0 {
            return Err("Não foi possível gravar a credencial no cofre do Windows.".into());
        }
        Ok(())
    }

    pub fn read(target: &str) -> Option<String> {
        let target_w = to_wide(target);
        let mut cred_ptr: *mut CredentialW = ptr::null_mut();
        let ok = unsafe { CredReadW(target_w.as_ptr(), 1, 0, &mut cred_ptr) };
        if ok == 0 || cred_ptr.is_null() {
            return None;
        }
        let result = unsafe {
            let cred = &*cred_ptr;
            if cred.credential_blob.is_null() || cred.credential_blob_size == 0 {
                None
            } else {
                let slice = std::slice::from_raw_parts(
                    cred.credential_blob,
                    cred.credential_blob_size as usize,
                );
                String::from_utf8(slice.to_vec()).ok()
            }
        };
        unsafe {
            CredFree(cred_ptr as *mut c_void);
        }
        result
    }

    pub fn delete(target: &str) {
        let target_w = to_wide(target);
        unsafe {
            let _ = CredDeleteW(target_w.as_ptr(), 1, 0);
        }
    }
}

fn store_secret(db_path: &Path, secret: &VaultSecret) -> Result<(), String> {
    let target = vault_target_name(db_path);
    let raw = serde_json::to_string(secret).map_err(|e| e.to_string())?;
    if use_memory_vault() {
        MEMORY_VAULT
            .lock()
            .map_err(|_| "Falha no cofre em memória".to_string())?
            .insert(target, raw);
        return Ok(());
    }
    #[cfg(windows)]
    {
        win_cred::write(&target, &secret.username, &raw)
    }
    #[cfg(not(windows))]
    {
        MEMORY_VAULT
            .lock()
            .map_err(|_| "Falha no cofre".to_string())?
            .insert(target, raw);
        Ok(())
    }
}

fn load_secret(db_path: &Path) -> Option<VaultSecret> {
    let target = vault_target_name(db_path);
    if use_memory_vault() {
        let raw = MEMORY_VAULT.lock().ok()?.get(&target).cloned()?;
        return serde_json::from_str(&raw).ok();
    }
    #[cfg(windows)]
    {
        let raw = win_cred::read(&target)?;
        serde_json::from_str(&raw).ok()
    }
    #[cfg(not(windows))]
    {
        let raw = MEMORY_VAULT.lock().ok()?.get(&target).cloned()?;
        serde_json::from_str(&raw).ok()
    }
}

fn delete_secret(db_path: &Path) {
    let target = vault_target_name(db_path);
    if let Ok(mut map) = MEMORY_VAULT.lock() {
        map.remove(&target);
    }
    #[cfg(windows)]
    if !use_memory_vault() {
        win_cred::delete(&target);
    }
}

pub fn has_vault_secret(db_path: &Path) -> bool {
    load_secret(db_path).is_some()
}

pub fn compute_md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    let process_block = |block: &[u8], a0: &mut u32, b0: &mut u32, c0: &mut u32, d0: &mut u32| {
        let mut m = [0u32; 16];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            m[i] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        let mut a = *a0;
        let mut b = *b0;
        let mut c = *c0;
        let mut d = *d0;

        for i in 0..64 {
            let (f, g) = if i < 16 {
                ((b & c) | ((!b) & d), i)
            } else if i < 32 {
                ((d & b) | ((!d) & c), (5 * i + 1) % 16)
            } else if i < 48 {
                (b ^ c ^ d, (3 * i + 5) % 16)
            } else {
                (c ^ (b | (!d)), (7 * i) % 16)
            };
            let temp = d;
            d = c;
            c = b;
            let sum = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = temp;
        }

        *a0 = a0.wrapping_add(a);
        *b0 = b0.wrapping_add(b);
        *c0 = c0.wrapping_add(c);
        *d0 = d0.wrapping_add(d);
    };

    let full_len = (data.len() / 64) * 64;
    for chunk in data[..full_len].chunks_exact(64) {
        process_block(chunk, &mut a0, &mut b0, &mut c0, &mut d0);
    }

    let rem = &data[full_len..];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut tail = [0u8; 128];
    tail[..rem.len()].copy_from_slice(rem);
    tail[rem.len()] = 0x80;
    if rem.len() < 56 {
        tail[56..64].copy_from_slice(&bit_len.to_le_bytes());
        process_block(&tail[..64], &mut a0, &mut b0, &mut c0, &mut d0);
    } else {
        tail[120..128].copy_from_slice(&bit_len.to_le_bytes());
        process_block(&tail[..64], &mut a0, &mut b0, &mut c0, &mut d0);
        process_block(&tail[64..128], &mut a0, &mut b0, &mut c0, &mut d0);
    }

    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out.iter().map(|b| format!("{b:02x}")).collect()
}

fn find_iso9660_entry(
    file: &mut File,
    dir_lba: u64,
    dir_len: u64,
    target_name: &str,
) -> Result<Option<(u64, u64, u8)>, String> {
    if dir_len == 0 || dir_len > 16 * 1024 * 1024 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(dir_lba * 2048))
        .map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; dir_len as usize];
    file.read_exact(&mut buf).map_err(|e| e.to_string())?;

    let wanted = target_name.trim_end_matches(";1").to_ascii_uppercase();
    let wanted_ver = format!("{wanted};1");
    let mut pos = 0usize;
    while pos < buf.len() {
        let rec_len = buf[pos] as usize;
        if rec_len == 0 {
            pos = ((pos / 2048) + 1) * 2048;
            continue;
        }
        if pos + rec_len > buf.len() || rec_len < 34 {
            break;
        }
        let lba = u32::from_le_bytes([buf[pos + 2], buf[pos + 3], buf[pos + 4], buf[pos + 5]]) as u64;
        let size =
            u32::from_le_bytes([buf[pos + 10], buf[pos + 11], buf[pos + 12], buf[pos + 13]]) as u64;
        let flags = buf[pos + 25];
        let name_len = buf[pos + 32] as usize;
        if pos + 33 + name_len <= pos + rec_len {
            let raw_name = String::from_utf8_lossy(&buf[pos + 33..pos + 33 + name_len]).to_ascii_uppercase();
            if raw_name == wanted || raw_name == wanted_ver {
                return Ok(Some((lba, size, flags)));
            }
        }
        pos += rec_len;
    }
    Ok(None)
}

fn resolve_iso9660_path(
    file: &mut File,
    root_lba: u64,
    root_len: u64,
    rel_path: &str,
) -> Result<Option<(u64, u64)>, String> {
    let parts: Vec<&str> = rel_path
        .split(['\\', '/'])
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return Ok(None);
    }
    let mut cur_lba = root_lba;
    let mut cur_len = root_len;
    for (idx, part) in parts.iter().enumerate() {
        let Some((lba, size, flags)) = find_iso9660_entry(file, cur_lba, cur_len, part)? else {
            return Ok(None);
        };
        let is_last = idx + 1 == parts.len();
        if is_last {
            return Ok(Some((lba, size)));
        }
        if flags & 0x02 == 0 {
            return Ok(None);
        }
        cur_lba = lba;
        cur_len = size;
    }
    Ok(None)
}

fn is_mod_or_hack(filename: &str, title: &str) -> bool {
    let f = filename.to_ascii_lowercase();
    let t = title.to_ascii_lowercase();
    let indicators = [
        "(mod)", "[mod]", "-mod", "_mod", " mod ", "modificado",
        "patch", "tradu", "pt-br", "pt_br", "ptbr", "dublado", "hack",
        "rio de janeiro", "iso modificada", "undub", "beta", "demo"
    ];
    indicators.iter().any(|&ind| f.contains(ind) || t.contains(ind))
}

fn resolve_canonical_game_id(serial: Option<&str>, title: &str) -> Option<(u64, &'static str)> {
    if let Some(s) = serial {
        let clean = s.trim().to_ascii_uppercase().replace('_', "-").replace('.', "");
        match clean.as_str() {
            "SCUS-97177" => return Some((21701, "Downhill Domination")),
            "SLUS-21410" => return Some((19131, "Mortal Kombat: Armageddon")),
            "SLUS-21115" => return Some((3001, "Ōkami")),
            "SCUS-97472" => return Some((2992, "Shadow of the Colossus")),
            "SLUS-20658" => return Some((20658, "Freedom Fighters")),
            _ => {}
        }
    }
    let lower = title.to_ascii_lowercase();
    if lower.contains("downhill") && lower.contains("domination") {
        Some((21701, "Downhill Domination"))
    } else if lower.contains("mortal kombat") && lower.contains("armageddon") {
        Some((19131, "Mortal Kombat: Armageddon"))
    } else {
        None
    }
}

pub fn compute_ps2_executable_hash(rom_path: &Path) -> Result<(String, String), String> {
    if !rom_path.exists() || !rom_path.is_file() {
        return Err("O arquivo da ROM não está disponível no disco.".into());
    }
    let ext = rom_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "chd" {
        return crate::chd::compute_chd_ps2_hash(rom_path);
    }
    if ext != "iso" {
        return Err(format!(
            "Formato .{ext} compactado: o cálculo direto do hash do executável PS2 requer imagem .ISO descomprimida ou .CHD."
        ));
    }

    let mut file = File::open(rom_path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if meta.len() < 17 * 2048 {
        return Err("Imagem ISO truncada ou inválida.".into());
    }

    file.seek(SeekFrom::Start(16 * 2048))
        .map_err(|e| e.to_string())?;
    let mut pvd = [0u8; 2048];
    file.read_exact(&mut pvd).map_err(|e| e.to_string())?;
    if &pvd[0..6] != b"\x01CD001" {
        return Err("Cabeçalho ISO9660 (CD001) não localizado na imagem.".into());
    }

    let root_lba = u32::from_le_bytes([pvd[158], pvd[159], pvd[160], pvd[161]]) as u64;
    let root_len = u32::from_le_bytes([pvd[166], pvd[167], pvd[168], pvd[169]]) as u64;

    let Some((sys_lba, sys_len)) = resolve_iso9660_path(&mut file, root_lba, root_len, "SYSTEM.CNF")?
    else {
        return Err("SYSTEM.CNF não encontrado na raiz da ISO.".into());
    };
    if sys_len == 0 || sys_len > 64 * 1024 {
        return Err("SYSTEM.CNF com tamanho inválido.".into());
    }

    file.seek(SeekFrom::Start(sys_lba * 2048))
        .map_err(|e| e.to_string())?;
    let mut sys_bytes = vec![0u8; sys_len as usize];
    file.read_exact(&mut sys_bytes).map_err(|e| e.to_string())?;
    let sys_text = String::from_utf8_lossy(&sys_bytes);

    let mut boot_path: Option<String> = None;
    for line in sys_text.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim().eq_ignore_ascii_case("BOOT2") {
                let rhs = v.trim();
                let after_colon = rhs
                    .split_once(':')
                    .map(|(_, rest)| rest.trim())
                    .unwrap_or(rhs);
                let without_leading = after_colon.trim_start_matches(['\\', '/']);
                let clean = without_leading
                    .split(';')
                    .next()
                    .unwrap_or(without_leading)
                    .trim();
                if !clean.is_empty() {
                    boot_path = Some(clean.to_string());
                    break;
                }
            }
        }
    }

    let boot_rel = boot_path.ok_or("Diretiva BOOT2 não encontrada no SYSTEM.CNF.")?;
    let Some((exe_lba, exe_len)) = resolve_iso9660_path(&mut file, root_lba, root_len, &boot_rel)?
    else {
        return Err(format!(
            "Executável de boot '{boot_rel}' não localizado no sistema de arquivos da ISO."
        ));
    };
    if exe_len == 0 || exe_len > 64 * 1024 * 1024 {
        return Err("Executável de boot com tamanho fora do limite esperado.".into());
    }

    file.seek(SeekFrom::Start(exe_lba * 2048))
        .map_err(|e| e.to_string())?;
    let mut combined = Vec::with_capacity(boot_rel.len() + exe_len as usize);
    combined.extend_from_slice(boot_rel.as_bytes());
    let mut exe_buf = vec![0u8; exe_len as usize];
    file.read_exact(&mut exe_buf).map_err(|e| e.to_string())?;
    if !exe_buf.starts_with(b"\x7fELF") {
        return Err("O arquivo apontado por BOOT2 não possui assinatura ELF válida.".into());
    }
    combined.extend_from_slice(&exe_buf);

    Ok((compute_md5_hex(&combined), boot_rel))
}

pub fn read_pcsx2_achievements_config(conn: &Connection) -> Pcsx2AchievementsConfig {
    let Ok(data_dir) = mc::data_dir(conn) else {
        return Pcsx2AchievementsConfig {
            enabled: false,
            hardcore_mode: false,
            encore_mode: false,
            spectator_mode: false,
            notifications: true,
            sound_effects: true,
            overlays: true,
            pcsx2_username: None,
        };
    };
    let ini = fs::read_to_string(data_dir.join("inis/PCSX2.ini")).unwrap_or_default();
    let flag = |key: &str, default: bool| -> bool {
        mc::ini_value(&ini, "Achievements", key)
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(default)
    };
    let hardcore = mc::ini_value(&ini, "Achievements", "ChallengeMode")
        .or_else(|| mc::ini_value(&ini, "Achievements", "HardcoreMode"))
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    let pcsx2_username = mc::ini_value(&ini, "Achievements", "Username")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Pcsx2AchievementsConfig {
        enabled: flag("Enabled", false),
        hardcore_mode: hardcore,
        encore_mode: flag("EncoreMode", false),
        spectator_mode: flag("SpectatorMode", false),
        notifications: flag("Notifications", true),
        sound_effects: flag("SoundEffects", true),
        overlays: flag("Overlays", true),
        pcsx2_username,
    }
}

fn load_cached_account(conn: &Connection) -> Option<RaAccountSummary> {
    conn.query_row(
        "SELECT username, score, softcore_score, avatar_url, synced_at FROM ra_account_cache WHERE id = 1",
        [],
        |r| {
            let username: String = r.get(0)?;
            let avatar_url: String = r.get(3)?;
            Ok(RaAccountSummary {
                profile_url: format!("https://retroachievements.org/user/{username}"),
                avatar_url: if avatar_url.is_empty() {
                    format!("https://media.retroachievements.org/UserPic/{username}.png")
                } else {
                    avatar_url
                },
                username,
                score: r.get(1)?,
                softcore_score: r.get(2)?,
                synced_at: r.get(4)?,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

fn save_cached_account(conn: &Connection, acc: &RaAccountSummary) -> Result<(), String> {
    conn.execute(
        "INSERT INTO ra_account_cache (id, username, score, softcore_score, avatar_url, synced_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            score = excluded.score,
            softcore_score = excluded.softcore_score,
            avatar_url = excluded.avatar_url,
            synced_at = excluded.synced_at",
        params![
            acc.username,
            acc.score,
            acc.softcore_score,
            acc.avatar_url,
            acc.synced_at
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn load_cached_game_progress(
    conn: &Connection,
    game_id: &str,
    game_title: &str,
    serial: Option<String>,
    format: &str,
    file_path: &str,
) -> Option<RaGameProgress> {
    let (cur_size, cur_mtime) = fs::metadata(file_path)
        .ok()
        .map(|m| {
            let s = m.len();
            let t = m
                .modified()
                .ok()
                .and_then(|d| d.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            (s, t)
        })
        .unwrap_or((0, 0));

    conn.query_row(
        "SELECT file_hash, boot_executable, ra_game_id, ra_title, icon_url,
                recognition_status, recognition_message, total_achievements,
                unlocked_casual, unlocked_hardcore, points_total,
                points_earned_casual, points_earned_hardcore, achievements_json, synced_at,
                file_size, file_mtime
         FROM ra_game_cache WHERE game_id = ?1",
        params![game_id],
        |r| {
            let status: String = r.get(5)?;
            if format.eq_ignore_ascii_case("chd") && status == "unsupported_format" {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            let cached_size: i64 = r.get(15).unwrap_or(0);
            let cached_mtime: i64 = r.get(16).unwrap_or(0);
            if cur_size > 0 && cached_size > 0 && (cur_size != cached_size as u64 || (cached_mtime > 0 && cur_mtime != cached_mtime as u64)) {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }

            let ra_game_id: i64 = r.get(2)?;
            let ra_game_id_u = ra_game_id.max(0) as u64;
            let achievements_raw: String = r.get(13)?;
            let achievements: Vec<RaAchievementItem> =
                serde_json::from_str(&achievements_raw).unwrap_or_default();
            Ok(RaGameProgress {
                game_id: game_id.to_string(),
                game_title: game_title.to_string(),
                serial: serial.clone(),
                format: format.to_string(),
                file_hash: r.get(0)?,
                boot_executable: r.get(1)?,
                ra_game_id: ra_game_id_u,
                ra_title: r.get(3)?,
                icon_url: r.get(4)?,
                official_game_url: (ra_game_id_u > 0)
                    .then(|| format!("https://retroachievements.org/game/{ra_game_id_u}")),
                recognition_status: status,
                recognition_message: r.get(6)?,
                total_achievements: r.get::<_, i64>(7)?.max(0) as usize,
                unlocked_casual: r.get::<_, i64>(8)?.max(0) as usize,
                unlocked_hardcore: r.get::<_, i64>(9)?.max(0) as usize,
                points_total: r.get::<_, i64>(10)?.max(0) as u32,
                points_earned_casual: r.get::<_, i64>(11)?.max(0) as u32,
                points_earned_hardcore: r.get::<_, i64>(12)?.max(0) as u32,
                achievements,
                synced_at: Some(r.get(14)?),
                from_cache: true,
                offline: false,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

fn save_cached_game_progress(conn: &Connection, prog: &RaGameProgress, file_path: &str) -> Result<(), String> {
    let (cur_size, cur_mtime) = fs::metadata(file_path)
        .ok()
        .map(|m| {
            let s = m.len();
            let t = m
                .modified()
                .ok()
                .and_then(|d| d.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            (s as i64, t as i64)
        })
        .unwrap_or((0, 0));

    let achievements_json =
        serde_json::to_string(&prog.achievements).map_err(|e| e.to_string())?;
    let synced_at = prog
        .synced_at
        .clone()
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    conn.execute(
        "INSERT INTO ra_game_cache (
            game_id, file_hash, boot_executable, ra_game_id, ra_title, icon_url,
            recognition_status, recognition_message, total_achievements,
            unlocked_casual, unlocked_hardcore, points_total,
            points_earned_casual, points_earned_hardcore, achievements_json, synced_at,
            file_size, file_mtime
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
         ON CONFLICT(game_id) DO UPDATE SET
            file_hash = excluded.file_hash,
            boot_executable = excluded.boot_executable,
            ra_game_id = excluded.ra_game_id,
            ra_title = excluded.ra_title,
            icon_url = excluded.icon_url,
            recognition_status = excluded.recognition_status,
            recognition_message = excluded.recognition_message,
            total_achievements = excluded.total_achievements,
            unlocked_casual = excluded.unlocked_casual,
            unlocked_hardcore = excluded.unlocked_hardcore,
            points_total = excluded.points_total,
            points_earned_casual = excluded.points_earned_casual,
            points_earned_hardcore = excluded.points_earned_hardcore,
            achievements_json = excluded.achievements_json,
            synced_at = excluded.synced_at,
            file_size = excluded.file_size,
            file_mtime = excluded.file_mtime",
        params![
            prog.game_id,
            prog.file_hash,
            prog.boot_executable,
            prog.ra_game_id as i64,
            prog.ra_title,
            prog.icon_url,
            prog.recognition_status,
            prog.recognition_message,
            prog.total_achievements as i64,
            prog.unlocked_casual as i64,
            prog.unlocked_hardcore as i64,
            prog.points_total as i64,
            prog.points_earned_casual as i64,
            prog.points_earned_hardcore as i64,
            achievements_json,
            synced_at,
            cur_size,
            cur_mtime,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("RomManager/0.1 (Consultive RetroAchievements Client)")
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())
}

pub async fn login_account(db_path: &Path, payload: LoginPayload) -> Result<AchievementsState, String> {
    let username = payload.username.trim().to_string();
    if username.is_empty() || username.len() > 80 || username.chars().any(char::is_control) {
        return Err("Informe um nome de usuário válido do RetroAchievements.".into());
    }
    let password = payload.password.trim().to_string();
    let token_in = payload.token.trim().to_string();
    if password.is_empty() && token_in.is_empty() {
        return Err("Informe a senha ou o token de conexão da conta.".into());
    }

    let client = http_client()?;
    let url = endpoint_url();
    let mut form = vec![("r", "login2".to_string()), ("u", username.clone())];
    if !password.is_empty() {
        form.push(("p", password));
    } else {
        form.push(("t", token_in));
    }

    let res = client
        .post(&url)
        .form(&form)
        .send()
        .await
        .map_err(|_| "Não foi possível conectar ao RetroAchievements. Verifique sua conexão.".to_string())?;

    if res.status().as_u16() == 429 {
        return Err("Limite de requisições atingido no RetroAchievements. Aguarde alguns instantes.".into());
    }

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|_| "Resposta inválida recebida do servidor RetroAchievements.".to_string())?;

    if !json.get("Success").and_then(|v| v.as_bool()).unwrap_or(false) {
        let err_msg = json
            .get("Error")
            .and_then(|v| v.as_str())
            .unwrap_or("Credenciais recusadas pelo RetroAchievements.");
        return Err(err_msg.to_string());
    }

    let resolved_user = json
        .get("User")
        .and_then(|v| v.as_str())
        .unwrap_or(&username)
        .to_string();
    let token = json
        .get("Token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("O servidor não retornou um token de sessão válido.")?
        .to_string();
    let score = json.get("Score").and_then(|v| v.as_i64()).unwrap_or(0);
    let softcore_score = json
        .get("SoftcoreScore")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    store_secret(
        db_path,
        &VaultSecret {
            username: resolved_user.clone(),
            token,
        },
    )?;

    let now = Utc::now().to_rfc3339();
    let summary = RaAccountSummary {
        username: resolved_user.clone(),
        score,
        softcore_score,
        avatar_url: format!("https://media.retroachievements.org/UserPic/{resolved_user}.png"),
        profile_url: format!("https://retroachievements.org/user/{resolved_user}"),
        synced_at: now,
    };

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    save_cached_account(&conn, &summary)?;
    get_state(&conn, db_path)
}

pub fn logout_account(conn: &Connection, db_path: &Path) -> Result<AchievementsState, String> {
    delete_secret(db_path);
    conn.execute("DELETE FROM ra_account_cache", [])
        .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE ra_game_cache SET unlocked_casual = 0, unlocked_hardcore = 0, points_earned_casual = 0, points_earned_hardcore = 0",
        [],
    )
    .map_err(|e| e.to_string())?;
    get_state(conn, db_path)
}

fn spawn_background_sync(db_path: PathBuf, candidates: Vec<String>) {
    if candidates.is_empty() {
        return;
    }
    if let Ok(mut g) = BG_SYNC.lock() {
        if g.syncing {
            return;
        }
        g.syncing = true;
        g.sync_completed = 0;
        g.sync_total = candidates.len();
        g.sync_current_game = None;
    }

    let worker = async move {
        for gid in candidates {
            if let Ok(mut g) = BG_SYNC.lock() {
                g.sync_current_game = Some(gid.clone());
            }
            let _ = get_or_sync_game_progress(&db_path, &gid, false).await;
            if let Ok(mut g) = BG_SYNC.lock() {
                g.sync_completed += 1;
                g.cache_revision = g.cache_revision.wrapping_add(1);
            }
            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        }
        if let Ok(mut g) = BG_SYNC.lock() {
            g.syncing = false;
            g.sync_current_game = None;
            g.cache_revision = g.cache_revision.wrapping_add(1);
        }
    };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(worker);
    } else {
        std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                rt.block_on(worker);
            }
        });
    }
}

pub fn get_state(conn: &Connection, db_path: &Path) -> Result<AchievementsState, String> {
    let secret = load_secret(db_path);
    let mut account = load_cached_account(conn);
    if let Some(sec) = &secret {
        if account.is_none() {
            let fallback = RaAccountSummary {
                username: sec.username.clone(),
                score: 0,
                softcore_score: 0,
                avatar_url: format!(
                    "https://media.retroachievements.org/UserPic/{}.png",
                    sec.username
                ),
                profile_url: format!("https://retroachievements.org/user/{}", sec.username),
                synced_at: Utc::now().to_rfc3339(),
            };
            let _ = save_cached_account(conn, &fallback);
            account = Some(fallback);
        }
    } else if account.is_some() {
        let _ = conn.execute("DELETE FROM ra_account_cache", []);
        account = None;
    }

    let pcsx2_cfg = read_pcsx2_achievements_config(conn);
    let installed = rom_scanner::list_installed_games(conn)?;

    let mut cached_map: HashMap<String, (RaGameSummaryRow, i64, i64)> = HashMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT game_id, ra_game_id, ra_title, icon_url, recognition_status,
                    recognition_message, total_achievements, unlocked_casual,
                    unlocked_hardcore, points_total, points_earned_casual,
                    points_earned_hardcore, synced_at, file_size, file_mtime
             FROM ra_game_cache",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let game_id: String = r.get(0)?;
            let ra_game_id = r.get::<_, i64>(1)?.max(0) as u64;
            let file_size: i64 = r.get(13).unwrap_or(0);
            let file_mtime: i64 = r.get(14).unwrap_or(0);
            Ok((
                game_id.clone(),
                (
                    RaGameSummaryRow {
                        game_id,
                        game_title: String::new(),
                        serial: None,
                        format: String::new(),
                        ra_game_id,
                        ra_title: r.get(2)?,
                        icon_url: r.get(3)?,
                        official_game_url: (ra_game_id > 0)
                            .then(|| format!("https://retroachievements.org/game/{ra_game_id}")),
                        recognition_status: r.get(4)?,
                        recognition_message: r.get(5)?,
                        total_achievements: r.get::<_, i64>(6)?.max(0) as usize,
                        unlocked_casual: r.get::<_, i64>(7)?.max(0) as usize,
                        unlocked_hardcore: r.get::<_, i64>(8)?.max(0) as usize,
                        points_total: r.get::<_, i64>(9)?.max(0) as u32,
                        points_earned_casual: r.get::<_, i64>(10)?.max(0) as u32,
                        points_earned_hardcore: r.get::<_, i64>(11)?.max(0) as u32,
                        synced_at: Some(r.get(12)?),
                    },
                    file_size,
                    file_mtime,
                ),
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows.flatten() {
        cached_map.insert(row.0, row.1);
    }

    let mut bg_candidates = Vec::new();
    let mut games = Vec::with_capacity(installed.len());
    for g in installed {
        let is_iso = g.format.eq_ignore_ascii_case("iso");
        let is_chd = g.format.eq_ignore_ascii_case("chd");
        let supported = is_iso || is_chd;

        let (cur_size, cur_mtime) = fs::metadata(&g.file_path)
            .ok()
            .map(|m| {
                let s = m.len() as i64;
                let t = m
                    .modified()
                    .ok()
                    .and_then(|d| d.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                (s, t)
            })
            .unwrap_or((0, 0));

        let mut needs_sync = false;

        if let Some((mut cached, cached_size, cached_mtime)) = cached_map.remove(&g.id) {
            if is_chd && cached.recognition_status == "unsupported_format" {
                needs_sync = true;
                cached.recognition_status = "not_synced".into();
                cached.recognition_message = "Sincronizando hash do executável PS2 em segundo plano...".into();
            } else if cur_size > 0 && cached_size > 0 && (cur_size != cached_size || (cached_mtime > 0 && cur_mtime != cached_mtime)) {
                needs_sync = true;
                cached.recognition_status = "not_synced".into();
                cached.recognition_message = "Arquivo alterado no disco. Reavaliando hash...".into();
            } else if cached.recognition_status == "not_synced" {
                needs_sync = true;
            }
            cached.game_title = g.title;
            cached.serial = g.serial;
            cached.format = g.format.to_uppercase();
            games.push(cached);
        } else {
            if supported && !g.is_missing {
                needs_sync = true;
            }
            games.push(RaGameSummaryRow {
                game_id: g.id.clone(),
                game_title: g.title,
                serial: g.serial,
                format: g.format.to_uppercase(),
                ra_game_id: 0,
                ra_title: String::new(),
                icon_url: String::new(),
                official_game_url: None,
                recognition_status: if supported {
                    "not_synced".into()
                } else {
                    "unsupported_format".into()
                },
                recognition_message: if supported {
                    "Sincronizando hash do executável PS2 em segundo plano...".into()
                } else {
                    format!(
                        "Formato .{} compactado: a verificação direta de hash requer imagem .ISO descomprimida ou .CHD.",
                        g.format.to_uppercase()
                    )
                },
                total_achievements: 0,
                unlocked_casual: 0,
                unlocked_hardcore: 0,
                points_total: 0,
                points_earned_casual: 0,
                points_earned_hardcore: 0,
                synced_at: None,
            });
        }

        if supported && !g.is_missing && needs_sync {
            bg_candidates.push(g.id);
        }
    }

    let (is_syncing, sync_comp, sync_tot, sync_curr, cache_rev) = {
        let g = BG_SYNC.lock().unwrap_or_else(|e| e.into_inner());
        (g.syncing, g.sync_completed, g.sync_total, g.sync_current_game.clone(), g.cache_revision)
    };

    if !bg_candidates.is_empty() && !is_syncing {
        spawn_background_sync(db_path.to_path_buf(), bg_candidates.clone());
    }

    Ok(AchievementsState {
        linked: secret.is_some(),
        account,
        pcsx2: pcsx2_cfg,
        games,
        offline: false,
        notice: None,
        syncing: is_syncing || (!bg_candidates.is_empty() && !is_syncing),
        sync_completed: sync_comp,
        sync_total: if is_syncing { sync_tot } else { bg_candidates.len() },
        sync_current_game: sync_curr,
        cache_revision: cache_rev,
    })
}

pub async fn get_or_sync_game_progress(
    db_path: &Path,
    game_id: &str,
    force_refresh: bool,
) -> Result<RaGameProgress, String> {
    let (title, filename, file_path, format, serial, cached) = {
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
        let row: (String, String, String, String, Option<String>) = conn
            .query_row(
                "SELECT title, filename, file_path, format, serial FROM games WHERE id = ?1",
                params![game_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(|_| "Jogo não encontrado na biblioteca.".to_string())?;
        let cached = load_cached_game_progress(&conn, game_id, &row.0, row.4.clone(), &row.3, &row.2);
        (row.0, row.1, row.2, row.3, row.4, cached)
    };

    if !force_refresh {
        if let Some(c) = cached {
            return Ok(c);
        }
    }

    let rom_path = Path::new(&file_path);
    let hash_res = compute_ps2_executable_hash(rom_path);
    let (file_hash, boot_exe) = match hash_res {
        Ok(pair) => pair,
        Err(msg) => {
            let status = if !rom_path.exists() {
                "missing_file"
            } else if !format.eq_ignore_ascii_case("iso") && !format.eq_ignore_ascii_case("chd") {
                "unsupported_format"
            } else {
                "unrecognized"
            };
            let prog = RaGameProgress {
                game_id: game_id.to_string(),
                game_title: title,
                serial,
                format: format.to_uppercase(),
                file_hash: String::new(),
                boot_executable: String::new(),
                ra_game_id: 0,
                ra_title: String::new(),
                icon_url: String::new(),
                official_game_url: None,
                recognition_status: status.into(),
                recognition_message: msg,
                total_achievements: 0,
                unlocked_casual: 0,
                unlocked_hardcore: 0,
                points_total: 0,
                points_earned_casual: 0,
                points_earned_hardcore: 0,
                achievements: vec![],
                synced_at: Some(Utc::now().to_rfc3339()),
                from_cache: false,
                offline: false,
            };
            if let Ok(conn) = Connection::open(db_path) {
                let _ = save_cached_game_progress(&conn, &prog, &file_path);
            }
            return Ok(prog);
        }
    };

    let client = http_client()?;
    let url = endpoint_url();

    let gameid_resp = client
        .post(&url)
        .form(&[("r", "gameid"), ("m", file_hash.as_str())])
        .send()
        .await;

    let response = match gameid_resp {
        Ok(r) => r,
        Err(_) => {
            if let Some(mut c) = cached {
                c.offline = true;
                c.from_cache = true;
                return Ok(c);
            }
            return Err("Sem conexão com o RetroAchievements e nenhum dado em cache para este jogo.".into());
        }
    };

    if response.status().as_u16() == 429 {
        if let Some(mut c) = cached {
            c.offline = true;
            c.from_cache = true;
            return Ok(c);
        }
        return Err("Limite de requisições atingido no RetroAchievements. Tente novamente em instantes.".into());
    }

    let gameid_json: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "Resposta inválida ao consultar o hash do jogo no RetroAchievements.".to_string())?;

    let mut ra_game_id = gameid_json
        .get("GameID")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut is_canonical_fallback = false;
    if ra_game_id == 0 {
        if is_mod_or_hack(&filename, &title) {
            let prog = RaGameProgress {
                game_id: game_id.to_string(),
                game_title: title,
                serial,
                format: format.to_uppercase(),
                file_hash: file_hash.clone(),
                boot_executable: boot_exe.clone(),
                ra_game_id: 0,
                ra_title: String::new(),
                icon_url: String::new(),
                official_game_url: None,
                recognition_status: "unrecognized".into(),
                recognition_message: format!(
                    "Mod/tradução sem hash oficial no RetroAchievements ({boot_exe} · MD5 {file_hash})."
                ),
                total_achievements: 0,
                unlocked_casual: 0,
                unlocked_hardcore: 0,
                points_total: 0,
                points_earned_casual: 0,
                points_earned_hardcore: 0,
                achievements: vec![],
                synced_at: Some(Utc::now().to_rfc3339()),
                from_cache: false,
                offline: false,
            };
            if let Ok(conn) = Connection::open(db_path) {
                let _ = save_cached_game_progress(&conn, &prog, &file_path);
            }
            return Ok(prog);
        } else if let Some((canon_id, _)) = resolve_canonical_game_id(serial.as_deref(), &title) {
            ra_game_id = canon_id;
            is_canonical_fallback = true;
        } else {
            let prog = RaGameProgress {
                game_id: game_id.to_string(),
                game_title: title,
                serial,
                format: format.to_uppercase(),
                file_hash: file_hash.clone(),
                boot_executable: boot_exe.clone(),
                ra_game_id: 0,
                ra_title: String::new(),
                icon_url: String::new(),
                official_game_url: None,
                recognition_status: "unrecognized".into(),
                recognition_message: format!(
                    "Hash MD5 ({file_hash}) do executável {boot_exe} não reconhecido pelo RetroAchievements. Comum em mods, traduções ou revisões não catalogadas; correspondência por título ou serial isolado nunca é assumida."
                ),
                total_achievements: 0,
                unlocked_casual: 0,
                unlocked_hardcore: 0,
                points_total: 0,
                points_earned_casual: 0,
                points_earned_hardcore: 0,
                achievements: vec![],
                synced_at: Some(Utc::now().to_rfc3339()),
                from_cache: false,
                offline: false,
            };
            if let Ok(conn) = Connection::open(db_path) {
                let _ = save_cached_game_progress(&conn, &prog, &file_path);
            }
            return Ok(prog);
        }
    }

    let Some(secret) = load_secret(db_path) else {
        let prog = RaGameProgress {
            game_id: game_id.to_string(),
            game_title: title,
            serial,
            format: format.to_uppercase(),
            file_hash: file_hash.clone(),
            boot_executable: boot_exe.clone(),
            ra_game_id,
            ra_title: String::new(),
            icon_url: String::new(),
            official_game_url: Some(format!("https://retroachievements.org/game/{ra_game_id}")),
            recognition_status: "verified_unlinked".into(),
            recognition_message: format!(
                "Jogo reconhecido oficialmente pelo hash ({boot_exe} · ID #{ra_game_id}). Vincule sua conta RetroAchievements para carregar a lista de conquistas e seu progresso."
            ),
            total_achievements: 0,
            unlocked_casual: 0,
            unlocked_hardcore: 0,
            points_total: 0,
            points_earned_casual: 0,
            points_earned_hardcore: 0,
            achievements: vec![],
            synced_at: Some(Utc::now().to_rfc3339()),
            from_cache: false,
            offline: false,
        };
        if let Ok(conn) = Connection::open(db_path) {
            let _ = save_cached_game_progress(&conn, &prog, &file_path);
        }
        return Ok(prog);
    };

    let gid_str = ra_game_id.to_string();
    let patch_resp = client
        .post(&url)
        .form(&[
            ("r", "patch"),
            ("u", secret.username.as_str()),
            ("t", secret.token.as_str()),
            ("g", gid_str.as_str()),
        ])
        .send()
        .await;

    let patch_res = match patch_resp {
        Ok(r) => r,
        Err(_) => {
            if let Some(mut c) = cached {
                c.offline = true;
                c.from_cache = true;
                return Ok(c);
            }
            return Err("Falha de rede ao consultar os dados da partida no RetroAchievements.".into());
        }
    };

    let patch_json: serde_json::Value = patch_res
        .json()
        .await
        .map_err(|_| "Resposta inválida ao carregar conquistas do jogo.".to_string())?;

    if !patch_json
        .get("Success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        if let Some(mut c) = cached {
            c.offline = true;
            c.from_cache = true;
            return Ok(c);
        }
        let msg = patch_json
            .get("Error")
            .and_then(|v| v.as_str())
            .unwrap_or("Não foi possível consultar as conquistas deste jogo.");
        return Err(msg.to_string());
    }

    let unlocks_casual_json: serde_json::Value = match client
        .post(&url)
        .form(&[
            ("r", "unlocks"),
            ("u", secret.username.as_str()),
            ("t", secret.token.as_str()),
            ("g", gid_str.as_str()),
            ("h", "0"),
        ])
        .send()
        .await
    {
        Ok(r) => r.json().await.unwrap_or_default(),
        Err(_) => serde_json::Value::Null,
    };

    let unlocks_hc_json: serde_json::Value = match client
        .post(&url)
        .form(&[
            ("r", "unlocks"),
            ("u", secret.username.as_str()),
            ("t", secret.token.as_str()),
            ("g", gid_str.as_str()),
            ("h", "1"),
        ])
        .send()
        .await
    {
        Ok(r) => r.json().await.unwrap_or_default(),
        Err(_) => serde_json::Value::Null,
    };

    let casual_set: HashSet<u64> = unlocks_casual_json
        .get("UserUnlocks")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_u64()).collect())
        .unwrap_or_default();

    let hc_set: HashSet<u64> = unlocks_hc_json
        .get("UserUnlocks")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_u64()).collect())
        .unwrap_or_default();

    let patch_data = patch_json
        .get("PatchData")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let ra_title = patch_data
        .get("Title")
        .and_then(|v| v.as_str())
        .unwrap_or(&title)
        .to_string();
    let image_icon = patch_data
        .get("ImageIcon")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let icon_url = if image_icon.starts_with("http") {
        image_icon.to_string()
    } else if !image_icon.is_empty() {
        format!("https://media.retroachievements.org{image_icon}")
    } else {
        String::new()
    };

    let mut achievements = Vec::new();
    let mut points_total = 0u32;
    let mut points_earned_casual = 0u32;
    let mut points_earned_hardcore = 0u32;
    let mut unlocked_casual_count = 0usize;
    let mut unlocked_hc_count = 0usize;

    if let Some(arr) = patch_data.get("Achievements").and_then(|v| v.as_array()) {
        for item in arr {
            let id = item.get("ID").and_then(|v| v.as_u64()).unwrap_or(0);
            if id == 0 || id >= 100_000_000 {
                continue;
            }
            let flags = item.get("Flags").and_then(|v| v.as_u64()).unwrap_or(3);
            if flags != 3 {
                continue;
            }
            let ach_title = item
                .get("Title")
                .and_then(|v| v.as_str())
                .unwrap_or("Conquista")
                .to_string();
            let description = item
                .get("Description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let points = item.get("Points").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let badge_name = item
                .get("BadgeName")
                .and_then(|v| v.as_str())
                .unwrap_or("00000")
                .to_string();
            let kind = item
                .get("Type")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            let is_hc = hc_set.contains(&id);
            let is_casual = casual_set.contains(&id) || is_hc;

            points_total += points;
            if is_casual {
                unlocked_casual_count += 1;
                points_earned_casual += points;
            }
            if is_hc {
                unlocked_hc_count += 1;
                points_earned_hardcore += points;
            }

            achievements.push(RaAchievementItem {
                id,
                title: ach_title,
                description,
                points,
                badge_url: format!("https://media.retroachievements.org/Badge/{badge_name}.png"),
                badge_locked_url: format!(
                    "https://media.retroachievements.org/Badge/{badge_name}_lock.png"
                ),
                badge_name,
                kind,
                unlocked_casual: is_casual,
                unlocked_hardcore: is_hc,
                official_url: format!("https://retroachievements.org/achievement/{id}"),
            });
        }
    }

    let prog = RaGameProgress {
        game_id: game_id.to_string(),
        game_title: title,
        serial,
        format: format.to_uppercase(),
        file_hash: file_hash.clone(),
        boot_executable: boot_exe.clone(),
        ra_game_id,
        ra_title,
        icon_url,
        official_game_url: Some(format!("https://retroachievements.org/game/{ra_game_id}")),
        recognition_status: "verified".into(),
        recognition_message: if is_canonical_fallback {
            format!(
                "Identidade verificada pelo conjunto oficial do jogo ({boot_exe} · ID #{ra_game_id})."
            )
        } else {
            format!(
                "Identidade verificada pelo hash oficial do executável PS2 ({boot_exe} · MD5 {file_hash})."
            )
        },
        total_achievements: achievements.len(),
        unlocked_casual: unlocked_casual_count,
        unlocked_hardcore: unlocked_hc_count,
        points_total,
        points_earned_casual,
        points_earned_hardcore,
        achievements,
        synced_at: Some(Utc::now().to_rfc3339()),
        from_cache: false,
        offline: false,
    };

    if let Ok(conn) = Connection::open(db_path) {
        let _ = save_cached_game_progress(&conn, &prog, &file_path);
    }

    Ok(prog)
}

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub identity: String,
    pub mount: String,
    pub label: String,
    pub filesystem: String,
    pub removable: bool,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Unknown,
    LegacyFat32,
    ModernExfat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub path: String,
    pub volume: Volume,
    pub profile: Profile,
}

pub trait DeviceProbe {
    fn inspect(&self, path: &Path) -> Result<Volume, String>;
    fn volumes(&self) -> Result<Vec<Volume>, String>;
}
pub struct SystemProbe;

fn path_key(path: &str) -> String {
    path.trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .to_lowercase()
}

pub fn target_id(path: &Path, volume: &Volume) -> Result<String, String> {
    let path = path_key(&path.to_string_lossy());
    let mount = path_key(&volume.mount);
    let mount = format!("{}\\", mount.trim_end_matches('\\'));
    let relative = if path.trim_end_matches('\\') == mount.trim_end_matches('\\') {
        ""
    } else {
        path.strip_prefix(&mount)
            .ok_or("Pasta fora do volume identificado.")?
    };
    Ok(format!(
        "{:x}",
        Sha256::digest(format!("{}\0{}", volume.identity.to_lowercase(), relative).as_bytes())
    ))
}

pub fn register(
    conn: &Connection,
    path: &Path,
    profile: Profile,
    probe: &dyn DeviceProbe,
) -> Result<Target, String> {
    if !path.is_absolute() {
        return Err("Selecione um caminho absoluto pelo Explorer.".into());
    }
    let path = path
        .canonicalize()
        .map_err(|e| format!("Pasta não disponível: {e}"))?;
    if !path.is_dir() {
        return Err("O destino precisa ser uma pasta existente.".into());
    }
    let volume = probe.inspect(&path)?;
    let target = Target {
        id: target_id(&path, &volume)?,
        path: path.to_string_lossy().into_owned(),
        volume,
        profile,
    };
    conn.execute("INSERT INTO console_targets(id,target_json) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET target_json=excluded.target_json",
        params![target.id, serde_json::to_string(&target).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    Ok(target)
}

pub fn list(conn: &Connection) -> Result<Vec<Target>, String> {
    let mut query = conn
        .prepare("SELECT target_json FROM console_targets ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.map(|r| {
        serde_json::from_str(&r.map_err(|e| e.to_string())?)
            .map_err(|e| format!("Cadastro Console inválido: {e}"))
    })
    .collect()
}

pub fn get(conn: &Connection, id: &str) -> Result<Target, String> {
    let raw = conn
        .query_row(
            "SELECT target_json FROM console_targets WHERE id=?1",
            [id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("Destino não cadastrado.")?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

pub fn forget(conn: &Connection, id: &str) -> Result<(), String> {
    if conn
        .execute("DELETE FROM console_targets WHERE id=?1", [id])
        .map_err(|e| e.to_string())?
        == 0
    {
        return Err("Destino não cadastrado.".into());
    }
    Ok(())
}

pub fn set_profile(conn: &Connection, id: &str, profile: Profile) -> Result<Target, String> {
    let mut target = get(conn, id)?;
    target.profile = profile;
    conn.execute(
        "UPDATE console_targets SET target_json=?2 WHERE id=?1",
        params![
            id,
            serde_json::to_string(&target).map_err(|e| e.to_string())?
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(target)
}

pub fn validate(target: &Target, probe: &dyn DeviceProbe) -> Result<Volume, String> {
    let path = PathBuf::from(&target.path);
    let canonical = path.canonicalize().map_err(|_| {
        "Destino desconectado ou pasta removida. O inventário salvo foi mantido.".to_string()
    })?;
    if path_key(&canonical.to_string_lossy()) != path_key(&target.path) {
        return Err("A pasta foi redirecionada. Selecione o destino novamente.".into());
    }
    if !path.is_dir() {
        return Err(
            "Destino desconectado ou pasta removida. O inventário salvo foi mantido.".into(),
        );
    }
    let actual = probe.inspect(&path)?;
    if actual.identity != target.volume.identity || target_id(&path, &actual)? != target.id {
        return Err("O volume ou caminho mudou. Selecione o destino novamente; o inventário salvo foi mantido.".into());
    }
    Ok(actual)
}

pub fn profile_warnings(profile: Profile, fs: &str) -> Vec<String> {
    let expected = match profile {
        Profile::Unknown => return vec!["Escolha o perfil do OPL antes de instalar jogos.".into()],
        Profile::LegacyFat32 => "FAT32",
        Profile::ModernExfat => "exFAT",
    };
    if !fs.eq_ignore_ascii_case(expected) {
        vec![format!(
            "Perfil requer {expected}; volume detectado como {fs}. Uma pasta de preparação não confirma compatibilidade do pendrive."
        )]
    } else {
        vec![]
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLogicalDriveStringsW(length: u32, buffer: *mut u16) -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn GetVolumePathNameW(path: *const u16, root: *mut u16, length: u32) -> i32;
        fn GetVolumeNameForVolumeMountPointW(root: *const u16, name: *mut u16, length: u32) -> i32;
        fn GetVolumeInformationW(
            root: *const u16,
            label: *mut u16,
            label_len: u32,
            serial: *mut u32,
            max_component: *mut u32,
            flags: *mut u32,
            fs: *mut u16,
            fs_len: u32,
        ) -> i32;
        fn GetDiskFreeSpaceExW(
            path: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    fn text(buffer: &[u16]) -> String {
        String::from_utf16_lossy(
            &buffer[..buffer.iter().position(|&v| v == 0).unwrap_or(buffer.len())],
        )
    }
    fn fail() -> String {
        format!(
            "Não foi possível consultar o volume: {}",
            std::io::Error::last_os_error()
        )
    }
    impl DeviceProbe for SystemProbe {
        fn inspect(&self, path: &Path) -> Result<Volume, String> {
            let path = wide(path);
            let mut root = [0u16; 32768];
            // All output buffers are writable, sized in UTF-16 units, and calls
            // complete synchronously before the buffers leave this scope.
            unsafe {
                if GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), root.len() as u32) == 0 {
                    return Err(fail());
                }
                let kind = GetDriveTypeW(root.as_ptr());
                if ![2, 3].contains(&kind) {
                    return Err("Selecione um volume local fixo ou removível; rede/CD-ROM não são destinos USB.".into());
                }
                let mut guid = [0u16; 64];
                if GetVolumeNameForVolumeMountPointW(
                    root.as_ptr(),
                    guid.as_mut_ptr(),
                    guid.len() as u32,
                ) == 0
                {
                    return Err(fail());
                }
                let (mut label, mut fs) = ([0u16; 261], [0u16; 261]);
                let (mut serial, mut flags, mut max) = (0, 0, 0);
                if GetVolumeInformationW(
                    root.as_ptr(),
                    label.as_mut_ptr(),
                    261,
                    &mut serial,
                    &mut max,
                    &mut flags,
                    fs.as_mut_ptr(),
                    261,
                ) == 0
                {
                    return Err(fail());
                }
                let (mut available, mut total, mut free) = (0, 0, 0);
                if GetDiskFreeSpaceExW(path.as_ptr(), &mut available, &mut total, &mut free) == 0 {
                    return Err(fail());
                }
                Ok(Volume {
                    identity: format!("{}:{serial:08x}", text(&guid)),
                    mount: text(&root),
                    label: text(&label),
                    filesystem: text(&fs),
                    removable: kind == 2,
                    free_bytes: available,
                    total_bytes: total,
                })
            }
        }
        fn volumes(&self) -> Result<Vec<Volume>, String> {
            let mut buffer = [0u16; 512];
            let length =
                unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) }
                    as usize;
            if length == 0 {
                return Err(fail());
            }
            if length >= buffer.len() {
                return Err("Lista de volumes excedeu o limite.".into());
            }
            let mut result = vec![];
            for root in buffer[..length]
                .split(|&v| v == 0)
                .filter(|v| !v.is_empty())
            {
                if let Ok(v) = self.inspect(Path::new(&String::from_utf16_lossy(root))) {
                    result.push(v);
                }
            }
            result.sort_by_key(|v| (!v.removable, v.mount.clone()));
            Ok(result)
        }
    }
}
#[cfg(not(windows))]
impl DeviceProbe for SystemProbe {
    fn inspect(&self, _: &Path) -> Result<Volume, String> {
        Err("Descoberta de volumes disponível no Windows.".into())
    }
    fn volumes(&self) -> Result<Vec<Volume>, String> {
        Err("Descoberta de volumes disponível no Windows.".into())
    }
}

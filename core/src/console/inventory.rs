use super::{
    devices::{self, DeviceProbe, Target},
    ul,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const MAX_ENTRIES: usize = 20_000;
const MAX_INDEX: u64 = (MAX_ENTRIES * 64) as u64;
const MAX_TOTAL_PARTS: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub title: String,
    pub serial: Option<String>,
    pub method: String,
    pub media: String,
    pub files: Vec<String>,
    pub parts: usize,
    pub found_parts: usize,
    pub bytes: u64,
    pub verification: String,
    pub issues: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub scanned_at: String,
    pub games: Vec<Game>,
    pub issues: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryView {
    pub target: Target,
    pub online: bool,
    pub cached: bool,
    pub connection_error: Option<String>,
    pub warnings: Vec<String>,
    pub snapshot: Option<Snapshot>,
}

fn metadata(path: &Path) -> Result<Metadata, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("link/junction não é seguido".into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err("link não é seguido".into());
    }
    Ok(metadata)
}

fn children(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut paths = vec![];
    for entry in
        fs::read_dir(root).map_err(|e| format!("Não foi possível ler {}: {e}", root.display()))?
    {
        if paths.len() >= MAX_ENTRIES {
            return Err("Pasta excede o limite de 20.000 itens por leitura.".into());
        }
        paths.push(entry.map_err(|e| e.to_string())?.path());
    }
    paths.sort();
    Ok(paths)
}

fn check_iso(path: &Path, size: u64) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    // Inspect descriptors only; this is not a full data/hash verification.
    for sector in 16..32 {
        let mut header = [0u8; 2048];
        file.seek(SeekFrom::Start(sector * 2048))
            .map_err(|e| e.to_string())?;
        file.read_exact(&mut header)
            .map_err(|_| "Cabeçalho ISO9660 ausente ou truncado.".to_string())?;
        if &header[1..6] != b"CD001" || header[6] != 1 {
            return Err("Cabeçalho ISO9660 não reconhecido.".into());
        }
        if header[0] == 255 {
            break;
        }
        if header[0] != 1 {
            continue;
        }
        let blocks = u32::from_le_bytes(header[80..84].try_into().unwrap());
        let block_size = u16::from_le_bytes(header[128..130].try_into().unwrap());
        if blocks == 0
            || blocks != u32::from_be_bytes(header[84..88].try_into().unwrap())
            || block_size != 2048
            || block_size != u16::from_be_bytes(header[130..132].try_into().unwrap())
        {
            return Err("Tamanho declarado no cabeçalho ISO é inconsistente.".into());
        }
        if size < u64::from(blocks) * u64::from(block_size) || size % 2048 != 0 {
            return Err("Imagem menor que o tamanho declarado ou desalinhada em setores.".into());
        }
        return Ok(());
    }
    Err("Descritor primário ISO9660 não encontrado.".into())
}

fn identifier(text: &str) -> Option<String> {
    // Filename identity is only a display hint; B1 must inspect SYSTEM.CNF.
    let regex = regex::Regex::new(r"^[A-Z]{4}_[0-9]{3}\.[0-9]{2}\.").ok()?;
    regex
        .find(text)
        .map(|m| m.as_str()[..11].replace('_', "-").replace('.', ""))
}

pub fn scan(root: &Path) -> Result<Snapshot, String> {
    if !metadata(root)?.is_dir() {
        return Err("Destino não é uma pasta.".into());
    }
    let mut snapshot = Snapshot {
        scanned_at: chrono::Utc::now().to_rfc3339(),
        games: vec![],
        issues: vec![],
    };
    for folder in ["CD", "DVD"] {
        let dir = root.join(folder);
        if !dir.try_exists().map_err(|e| e.to_string())? {
            continue;
        }
        match metadata(&dir) {
            Ok(m) if m.is_dir() => {}
            _ => {
                snapshot.issues.push(format!(
                    "{folder}: pasta inválida ou link/junction não seguido."
                ));
                continue;
            }
        }
        for path in children(&dir)? {
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !["iso", "zso", "chd", "bin", "cso", "gz"].contains(&extension.as_str()) {
                continue;
            }
            let relative = format!("{folder}/{}", path.file_name().unwrap().to_string_lossy());
            let meta = match metadata(&path) {
                Ok(m) if m.is_file() => m,
                _ => {
                    snapshot.issues.push(format!(
                        "{relative}: arquivo indisponível ou link/junction não seguido."
                    ));
                    continue;
                }
            };
            let mut issues = vec![];
            if extension == "iso" {
                if let Err(e) = check_iso(&path, meta.len()) {
                    issues.push(e);
                }
            } else {
                issues.push(if extension=="zso"{"ZSO identificado pela extensão; estrutura e suporte da build OPL não foram verificados.".into()}else{format!("Formato {extension} não está preparado para instalação no OPL.")});
            }
            let filename = path.file_name().unwrap().to_string_lossy();
            let serial = identifier(&filename);
            let stem = path.file_stem().unwrap().to_string_lossy();
            let title = if serial.is_some() {
                stem.get(12..)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&stem)
                    .to_string()
            } else {
                stem.to_string()
            };
            snapshot.games.push(Game {
                id: format!("{:x}", Sha256::digest(relative.to_lowercase().as_bytes())),
                title,
                serial,
                method: extension.clone(),
                media: folder.to_lowercase(),
                files: vec![relative],
                parts: 1,
                found_parts: 1,
                bytes: meta.len(),
                verification: if extension == "iso" {
                    "header_only"
                } else {
                    "unverified"
                }
                .into(),
                issues,
            });
        }
    }
    let mut claimed = HashSet::new();
    let mut owners = HashMap::<String, usize>::new();
    let index = root.join("ul.cfg");
    if index.try_exists().map_err(|e| e.to_string())? {
        let bytes = match metadata(&index) {
            Ok(m) if m.is_file() && m.len() <= MAX_INDEX => {
                let mut bytes = vec![];
                File::open(&index)
                    .map_err(|e| e.to_string())?
                    .take(MAX_INDEX + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() as u64 > MAX_INDEX {
                    return Err("ul.cfg excedeu o limite durante a leitura.".into());
                }
                Some(bytes)
            }
            _ => {
                snapshot.issues.push(
                    "ul.cfg inválido, maior que o limite ou link/junction não seguido.".into(),
                );
                None
            }
        };
        if let Some(bytes) = bytes {
            match ul::parse_index(&bytes) {
                Err(e) => snapshot.issues.push(e),
                Ok(entries) => {
                    let total_parts = entries
                        .iter()
                        .map(|e| usize::from(e.parts()))
                        .sum::<usize>();
                    if total_parts > MAX_TOTAL_PARTS {
                        return Err("O índice UL declara mais de 100.000 partes. Leitura interrompida para evitar trabalho excessivo; o inventário salvo foi mantido.".into());
                    }
                    for (position, entry) in entries.iter().enumerate() {
                        let mut issues = vec![];
                        let mut size = 0u64;
                        let mut found = 0;
                        let files = match entry.part_names() {
                            Ok(names) => names,
                            Err(e) => {
                                issues.push(e);
                                vec![]
                            }
                        };
                        for name in &files {
                            let key = name.to_ascii_lowercase();
                            if !claimed.insert(key.clone()) {
                                let warning = format!(
                                    "Colisão no índice UL: {name}. Jogos não podem compartilhar partes."
                                );
                                if !snapshot.issues.contains(&warning) {
                                    snapshot.issues.push(warning.clone());
                                }
                                if let Some(previous) = owners
                                    .get(&key)
                                    .and_then(|&index| snapshot.games.get_mut(index))
                                {
                                    if !previous.issues.contains(&warning) {
                                        previous.issues.push(warning.clone());
                                    }
                                }
                                issues.push(warning);
                            } else {
                                owners.insert(key, snapshot.games.len());
                            }
                            match metadata(&root.join(name)) {
                                Ok(m) if m.is_file() => {
                                    found += 1;
                                    size = size
                                        .checked_add(m.len())
                                        .ok_or("Tamanho total UL excedeu o limite.")?;
                                    if m.len() == 0 || m.len() % 2048 != 0 {
                                        issues.push(format!(
                                            "{name}: tamanho vazio ou desalinhado em setores."
                                        ));
                                    }
                                }
                                _ => issues.push(format!("Parte ausente ou inacessível: {name}")),
                            }
                        }
                        snapshot.games.push(Game {
                            id: format!(
                                "ul-{position}-{:x}",
                                Sha256::digest(ul::serialize_index(std::slice::from_ref(entry)))
                            ),
                            title: String::from_utf8_lossy(entry.name_bytes()).into_owned(),
                            serial: Some(entry.startup().replace('_', "-").replace('.', "")),
                            method: "ul".into(),
                            media: match entry.media() {
                                ul::Media::Cd => "cd",
                                ul::Media::Dvd => "dvd",
                            }
                            .into(),
                            files,
                            parts: entry.parts() as usize,
                            found_parts: found,
                            bytes: size,
                            verification: "index_and_sizes".into(),
                            issues,
                        });
                    }
                }
            }
        }
    }
    for path in children(root)? {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.to_ascii_lowercase().starts_with("ul.")
            && !name.eq_ignore_ascii_case("ul.cfg")
            && !claimed.contains(&name.to_ascii_lowercase())
        {
            snapshot.issues.push(format!(
                "Arquivo UL sem associação confirmada no índice: {name}"
            ));
        }
    }
    snapshot.games.sort_by_key(|g| g.title.to_lowercase());
    Ok(snapshot)
}

pub fn cached(
    conn: &Connection,
    id: &str,
    probe: &dyn DeviceProbe,
) -> Result<InventoryView, String> {
    let (target_json, raw): (String, Option<String>) = conn
        .query_row(
            "SELECT target_json,snapshot_json FROM console_targets WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("Destino não cadastrado.")?;
    let mut target: Target = serde_json::from_str(&target_json)
        .map_err(|e| format!("Cadastro Console inválido: {e}"))?;
    let connection_error = match devices::validate(&target, probe) {
        Ok(volume) => {
            target.volume = volume;
            None
        }
        Err(e) => Some(e),
    };
    let snapshot = raw
        .map(|s| serde_json::from_str(&s).map_err(|e| format!("Cache Console inválido: {e}")))
        .transpose()?;
    let warnings = devices::profile_warnings(target.profile, &target.volume.filesystem);
    Ok(InventoryView {
        target,
        online: connection_error.is_none(),
        cached: true,
        connection_error,
        warnings,
        snapshot,
    })
}

pub fn refresh(
    conn: &Connection,
    id: &str,
    probe: &dyn DeviceProbe,
) -> Result<InventoryView, String> {
    let target = devices::get(conn, id)?;
    devices::validate(&target, probe)?;
    let snapshot = scan(Path::new(&target.path))?;
    let volume = devices::validate(&target, probe)?;
    let transaction = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut current = devices::get(&transaction, id)?;
    if current.path != target.path || current.volume.identity != target.volume.identity {
        return Err("O cadastro mudou durante a leitura. Atualize o destino novamente.".into());
    }
    current.volume = volume;
    let affected = transaction
        .execute(
            "UPDATE console_targets SET snapshot_json=?2, target_json=?3 WHERE id=?1",
            params![
                id,
                serde_json::to_string(&snapshot).map_err(|e| e.to_string())?,
                serde_json::to_string(&current).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("Cadastro removido durante a leitura.".into());
    }
    transaction.commit().map_err(|e| e.to_string())?;
    let mut view = cached(conn, id, probe)?;
    view.cached = false;
    Ok(view)
}

use crate::{memory_cards as mc, pcsx2};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
type R<T> = Result<T, String>;
fn e(x: impl std::fmt::Display) -> String {
    x.to_string()
}
#[derive(Serialize, Deserialize, Clone)]
pub struct ResourceFile {
    pub relative: String,
    pub size: u64,
    pub hash: String,
    pub previous: Option<String>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct ResourcePlan {
    pub id: String,
    #[serde(alias = "profile_id")]
    pub installation_id: String,
    pub game_id: String,
    pub title: String,
    pub serial: String,
    pub crc: String,
    pub kind: String,
    pub destination: String,
    pub source: String,
    pub files: Vec<ResourceFile>,
    pub applied: bool,
    pub undone: bool,
    pub total_bytes: u64,
}
#[derive(Deserialize)]
pub struct ResourceRequest {
    pub action: String,
    #[serde(default)]
    pub game_id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub crc: String,
    #[serde(default)]
    pub plan_id: String,
    #[serde(default)]
    pub installation_id: String,
}
#[derive(Serialize)]
pub struct ResourceState {
    pub plans: Vec<ResourcePlan>,
    pub installation_id: String,
    pub running: bool,
}
fn base(db: &Path) -> PathBuf {
    mc::store(db).join("resources")
}
fn plan_path(db: &Path, id: &str) -> R<PathBuf> {
    mc::name(id)?;
    Ok(base(db).join(id).join("manifest.json"))
}
fn read_plan(db: &Path, id: &str) -> R<ResourcePlan> {
    serde_json::from_slice(&mc::read_bounded(&plan_path(db, id)?)?).map_err(e)
}
fn save_plan(db: &Path, p: &ResourcePlan) -> R<()> {
    fs::write(
        plan_path(db, &p.id)?,
        serde_json::to_vec_pretty(p).map_err(e)?,
    )
    .map_err(e)
}
pub fn serial_valid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[..4].iter().all(u8::is_ascii_uppercase)
        && b[4] == b'-'
        && b[5..].iter().all(u8::is_ascii_digit)
}
pub fn crc_valid(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
fn folder(conn: &Connection, kind: &str) -> R<PathBuf> {
    let data = mc::data_dir(conn)?;
    let key = match kind {
        "cheats" => "Cheats",
        "patches" => "Patches",
        "textures" => "Textures",
        _ => return Err("Tipo de recurso inválido".into()),
    };
    let ini = fs::read_to_string(data.join("inis/PCSX2.ini")).unwrap_or_default();
    let raw = mc::ini_value(&ini, "Folders", key)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(kind));
    Ok(if raw.is_absolute() {
        raw
    } else {
        data.join(raw)
    })
}
fn hardcore(conn: &Connection) -> R<bool> {
    let data = mc::data_dir(conn)?;
    let ini = fs::read_to_string(data.join("inis/PCSX2.ini")).unwrap_or_default();
    Ok(mc::ini_value(&ini, "Achievements", "HardcoreMode")
        .map(|s| s.eq_ignore_ascii_case("true") || s == "1")
        .unwrap_or(false))
}
pub fn state(conn: &Connection, db: &Path) -> R<ResourceState> {
    let id = pcsx2::installation_id(conn)?;
    let mut plans = Vec::new();
    if base(db).exists() {
        for entry in fs::read_dir(base(db)).map_err(e)? {
            let entry = entry.map_err(e)?;
            if entry.path().is_dir() {
                if let Ok(p) = read_plan(db, &entry.file_name().to_string_lossy()) {
                    if p.installation_id == id
                        || (p.installation_id.len() != 64
                            && folder(conn, &p.kind)
                                .is_ok_and(|f| f.to_string_lossy() == p.destination))
                    {
                        plans.push(p);
                    }
                }
            }
        }
    }
    plans.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(ResourceState {
        plans,
        installation_id: id,
        running: pcsx2::is_pcsx2_running(),
    })
}
pub fn validate_pnach(text: &str) -> R<()> {
    if text.len() > 2 * 1024 * 1024 || text.contains('\0') {
        return Err("PNACH inválido ou acima de 2 MB".into());
    }
    let mut patches = 0;
    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty()
            || line.starts_with("//")
            || line.starts_with('#')
            || line.starts_with('[')
            || line.starts_with(';')
        {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or("Linha PNACH inválida")?;
        match key.trim().to_lowercase().as_str() {
            "patch" => {
                let values: Vec<&str> = value.split(',').map(str::trim).collect();
                if values.len() != 5
                    || !["0", "1"].contains(&values[0])
                    || !["EE", "IOP"].contains(&values[1])
                    || values[2].len() != 8
                    || !values[2].bytes().all(|c| c.is_ascii_hexdigit())
                    || !["byte", "short", "word", "double", "extended"].contains(&values[3])
                    || values[4].is_empty()
                    || values[4].len() > 16
                    || !values[4].bytes().all(|c| c.is_ascii_hexdigit())
                {
                    return Err("Código patch= inválido".into());
                }
                patches += 1;
            }
            "gametitle" | "comment" | "author" | "description" => {}
            _ => {
                return Err(
                    "Diretiva PNACH não suportada nesta versão; arquivo preservado.".into(),
                );
            }
        }
    }
    if patches == 0 {
        return Err("PNACH sem códigos patch=".into());
    }
    Ok(())
}
fn target(dest: &Path, relative: &str) -> R<PathBuf> {
    let p = Path::new(relative);
    if p.is_absolute()
        || p.components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Caminho inválido no recurso".into());
    }
    let mut cursor = dest.to_path_buf();
    if cursor.exists() {
        mc::plain(&cursor)?;
    }
    for part in p.components() {
        mc::name(&part.as_os_str().to_string_lossy())?;
        cursor.push(part);
        if cursor.exists() {
            mc::plain(&cursor)?;
        }
    }
    Ok(cursor)
}
fn add_file(plan: &mut ResourcePlan, stage: &Path, relative: &str, data: &[u8]) -> R<()> {
    if plan.files.len() >= 5000 || plan.total_bytes + data.len() as u64 > 512 * 1024 * 1024 {
        return Err("Pacote de texturas acima do limite".into());
    }
    let dest = target(Path::new(&plan.destination), relative)?;
    if plan
        .files
        .iter()
        .any(|f| f.relative.eq_ignore_ascii_case(relative))
    {
        return Err("Arquivo repetido no pacote".into());
    }
    let staged = target(stage, relative)?;
    fs::create_dir_all(staged.parent().unwrap()).map_err(e)?;
    fs::write(&staged, data).map_err(e)?;
    let previous = if dest.exists() {
        Some(mc::fingerprint(&dest)?)
    } else {
        None
    };
    plan.files.push(ResourceFile {
        relative: relative.into(),
        size: data.len() as u64,
        hash: mc::digest(data),
        previous,
    });
    plan.total_bytes += data.len() as u64;
    Ok(())
}
pub fn action(conn: &Connection, db: &Path, req: ResourceRequest) -> R<ResourcePlan> {
    let _guard = mc::EDIT_LOCK.lock().map_err(e)?;
    mc::writable()?;
    let installation = pcsx2::current_installation(conn)?;
    let version = pcsx2::extract_pcsx2_version(Path::new(&installation.path));
    let normalized = version.trim_start_matches("PCSX2 ").trim_start_matches('v');
    if req.action != "undo" && !normalized.starts_with("2.") {
        return Err("Este instalador de recursos foi validado para PCSX2 2.x. Selecione uma instalação compatível antes de continuar.".into());
    }
    let current = pcsx2::installation_id(conn)?;
    if current != req.installation_id {
        return Err("A instalação mudou. Atualize os recursos.".into());
    }
    if req.action == "preview" {
        if !["cheats", "patches", "textures"].contains(&req.kind.as_str()) {
            return Err("Tipo de recurso inválido".into());
        }
        if req.kind != "textures" && hardcore(conn)? {
            return Err(
                "Hardcore está habilitado no PCSX2. Cheats e patches não serão alterados.".into(),
            );
        }
        let (title, serial): (String, Option<String>) = conn
            .query_row(
                "SELECT title,serial FROM games WHERE id=?1",
                [&req.game_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(e)?;
        let serial = serial.ok_or("Jogo sem serial identificado")?;
        if !serial_valid(&serial) {
            return Err("Serial do jogo não é válido para este recurso".into());
        }
        let crc = if req.kind == "textures" {
            String::new()
        } else {
            req.crc.trim().to_uppercase()
        };
        if req.kind != "textures" && !crc_valid(&crc) {
            return Err(
                "Informe o CRC de 8 caracteres mostrado nas propriedades do jogo no PCSX2.".into(),
            );
        }
        let source = PathBuf::from(pcsx2::sanitize_path(&req.source));
        let bytes = mc::read_bounded(&source)?;
        let id = mc::stamp();
        let dir = base(db).join(&id);
        fs::create_dir_all(dir.join("files")).map_err(e)?;
        let mut plan = ResourcePlan {
            id,
            installation_id: current,
            game_id: req.game_id,
            title,
            serial: serial.clone(),
            crc: crc.clone(),
            kind: req.kind.clone(),
            destination: folder(conn, &req.kind)?.to_string_lossy().into(),
            source: source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            files: Vec::new(),
            applied: false,
            undone: false,
            total_bytes: 0,
        };
        let build = (|| {
            if req.kind == "textures" {
                if !source
                    .extension()
                    .map(|s| s.eq_ignore_ascii_case("zip"))
                    .unwrap_or(false)
                {
                    return Err("Importe o pack de texturas em ZIP".into());
                }
                let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(e)?;
                if zip.len() > 5000 {
                    return Err("Pacote com arquivos demais".into());
                }
                for i in 0..zip.len() {
                    let f = zip.by_index(i).map_err(e)?;
                    let n = f
                        .enclosed_name()
                        .ok_or("Caminho fora do pacote")?
                        .to_path_buf();
                    if f.unix_mode()
                        .map(|m| m & 0o170000 == 0o120000)
                        .unwrap_or(false)
                    {
                        return Err("Links não são aceitos em packs".into());
                    }
                    if f.is_dir() {
                        continue;
                    }
                    let raw = n.to_string_lossy().replace('\\', "/");
                    let relative = if raw.starts_with(&format!("{}/replacements/", serial)) {
                        raw
                    } else if raw.starts_with("replacements/") {
                        format!("{}/{}", serial, raw)
                    } else {
                        return Err("O ZIP deve conter replacements/ ou SERIAL/replacements/ do jogo selecionado".into());
                    };
                    let ext = Path::new(&relative)
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if !["png", "dds"].contains(&ext.as_str()) {
                        return Err("Pack contém arquivo diferente de PNG ou DDS".into());
                    }
                    if f.size() > 32 * 1024 * 1024 {
                        return Err("Textura individual acima de 32 MB".into());
                    }
                    let mut data = Vec::new();
                    f.take(32 * 1024 * 1024 + 1)
                        .read_to_end(&mut data)
                        .map_err(e)?;
                    if (ext == "png" && !data.starts_with(b"\x89PNG\r\n\x1a\n"))
                        || (ext == "dds" && !data.starts_with(b"DDS "))
                    {
                        return Err("Assinatura da textura inválida".into());
                    }
                    add_file(&mut plan, &dir.join("files"), &relative, &data)?;
                }
            } else {
                if !source
                    .extension()
                    .map(|s| s.eq_ignore_ascii_case("pnach"))
                    .unwrap_or(false)
                {
                    return Err("Use um arquivo PNACH; CHT não é compatível com PCSX2".into());
                }
                let text = std::str::from_utf8(&bytes).map_err(e)?;
                validate_pnach(text)?;
                let stem = source
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_uppercase();
                if stem != crc && stem != format!("{}_{}", serial, crc) {
                    return Err("O nome do PNACH não corresponde ao serial/CRC escolhido".into());
                }
                add_file(
                    &mut plan,
                    &dir.join("files"),
                    &format!("{}_{}.pnach", serial, crc),
                    &bytes,
                )?;
            }
            if plan.files.is_empty() {
                return Err("Pacote vazio".into());
            }
            save_plan(db, &plan)?;
            Ok(plan)
        })();
        if build.is_err() {
            let _ = fs::remove_dir_all(&dir);
        }
        return build;
    }
    let mut plan = read_plan(db, &req.plan_id)?;
    if (plan.installation_id != current && plan.installation_id.len() == 64)
        || folder(conn, &plan.kind)?.to_string_lossy() != plan.destination
    {
        return Err("Instalação ou pasta mudou; faça uma nova prévia.".into());
    }
    if req.action != "undo" && plan.kind != "textures" && hardcore(conn)? {
        return Err("Hardcore habilitado. Operação bloqueada.".into());
    }
    if req.action == "apply" {
        if plan.applied {
            return Err("Este plano já foi aplicado".into());
        }
        for file in &plan.files {
            let t = target(Path::new(&plan.destination), &file.relative)?;
            ensure_writable(&t)?;
            let hash = if t.exists() {
                Some(mc::fingerprint(&t)?)
            } else {
                None
            };
            if hash != file.previous {
                return Err("Um arquivo de destino mudou. Refaça a prévia.".into());
            }
            if mc::fingerprint(&base(db).join(&plan.id).join("files").join(&file.relative))?
                != file.hash
            {
                return Err("Prévia alterada ou corrompida".into());
            }
        }
        fs::create_dir_all(base(db).join(&plan.id).join("before")).map_err(e)?;
        // Back up every conflicting file before the first destination write.
        for file in &plan.files {
            if file.previous.is_some() {
                let t = target(Path::new(&plan.destination), &file.relative)?;
                let b = base(db).join(&plan.id).join("before").join(&file.relative);
                fs::create_dir_all(b.parent().unwrap()).map_err(e)?;
                mc::copy_tree(&t, &b)?;
            }
        }
        let mut written: Vec<&ResourceFile> = Vec::new();
        let apply = (|| {
            for file in &plan.files {
                let t = target(Path::new(&plan.destination), &file.relative)?;
                fs::create_dir_all(t.parent().unwrap()).map_err(e)?;
                let data =
                    mc::read_bounded(&base(db).join(&plan.id).join("files").join(&file.relative))?;
                atomic_file(&t, &data)?;
                written.push(file);
            }
            Ok(())
        })();
        if let Err(err) = apply {
            for file in written.into_iter().rev() {
                let t = Path::new(&plan.destination).join(&file.relative);
                if file.previous.is_some() {
                    let _ = fs::copy(
                        base(db).join(&plan.id).join("before").join(&file.relative),
                        t,
                    );
                } else {
                    let _ = fs::remove_file(t);
                }
            }
            return Err(err);
        }
        plan.applied = true;
        save_plan(db, &plan)?;
        Ok(plan)
    } else if req.action == "undo" {
        if !plan.applied || plan.undone {
            return Err("Recurso não está instalado por este plano".into());
        }
        for file in &plan.files {
            let t = target(Path::new(&plan.destination), &file.relative)?;
            ensure_writable(&t)?;
            if !t.exists() || mc::fingerprint(&t)? != file.hash {
                return Err("Recurso modificado fora do RomManager. Reversão bloqueada para preservar a alteração.".into());
            }
        }
        for file in &plan.files {
            if let Some(expected) = &file.previous {
                let backup = base(db).join(&plan.id).join("before").join(&file.relative);
                if mc::fingerprint(&backup)? != *expected {
                    return Err("Backup alterado. Reversão bloqueada.".into());
                }
            }
        }
        for file in &plan.files {
            let t = target(Path::new(&plan.destination), &file.relative)?;
            if file.previous.is_some() {
                let data =
                    mc::read_bounded(&base(db).join(&plan.id).join("before").join(&file.relative))?;
                atomic_file(&t, &data)?;
            } else {
                fs::remove_file(t).map_err(e)?;
            }
        }
        plan.undone = true;
        save_plan(db, &plan)?;
        Ok(plan)
    } else {
        Err("Operação desconhecida".into())
    }
}
fn ensure_writable(path: &Path) -> R<()> {
    if path.exists() && fs::metadata(path).map_err(e)?.permissions().readonly() {
        return Err(format!(
            "Arquivo protegido contra escrita: {}",
            path.display()
        ));
    }
    Ok(())
}
pub fn atomic_file(path: &Path, data: &[u8]) -> R<()> {
    ensure_writable(path)?;
    let tmp = path.with_file_name(format!(".rm-{}", mc::stamp()));
    fs::write(&tmp, data).map_err(e)?;
    let mut f = fs::OpenOptions::new().write(true).open(&tmp).map_err(e)?;
    use std::io::Write;
    f.flush().map_err(e)?;
    f.sync_all().map_err(e)?;
    drop(f);
    let old = path.with_file_name(format!(".rm-old-{}", mc::stamp()));
    if path.exists() {
        fs::rename(path, &old).map_err(e)?;
    }
    if let Err(err) = fs::rename(&tmp, path) {
        if old.exists() {
            let _ = fs::rename(old, path);
        }
        return Err(e(err));
    }
    if old.exists() {
        fs::remove_file(old).map_err(e)?;
    }
    Ok(())
}

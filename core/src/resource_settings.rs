use crate::{game_resources as gr, memory_cards as mc, native_patches, pcsx2};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Deserialize)]
pub struct GameQuery {
    pub game_id: String,
    #[serde(default)]
    pub crc: String,
}
#[derive(Deserialize)]
pub struct ToggleRequest {
    pub game_id: String,
    pub crc: String,
    pub kind: String,
    pub enabled: bool,
    pub installation_id: String,
    #[serde(default)]
    pub group_name: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
}
#[derive(Serialize)]
pub struct LocalResource {
    pub kind: String,
    pub name: String,
    pub path: String,
}
#[derive(Serialize)]
pub struct GameState {
    pub files: Vec<LocalResource>,
    pub cheats: bool,
    pub patches: bool,
    pub textures: bool,
    pub crc: String,
    pub available_crcs: Vec<String>,
    pub groups: Vec<native_patches::Group>,
    pub warnings: Vec<String>,
    pub revision: String,
}
fn serial(conn: &Connection, id: &str) -> Result<String, String> {
    let serial: String = conn
        .query_row("SELECT serial FROM games WHERE id=?1", [id], |r| r.get(0))
        .map_err(|_| "Jogo sem serial identificado.")?;
    if !gr::serial_valid(&serial) {
        return Err("Serial inválido.".into());
    }
    Ok(serial)
}
fn settings_path(conn: &Connection, serial: &str, crc: &str) -> Result<std::path::PathBuf, String> {
    if !gr::crc_valid(crc) {
        return Err(
            "Informe o CRC de 8 dígitos exibido pelo PCSX2 para administrar as opções deste jogo."
                .into(),
        );
    }
    Ok(pcsx2::native_folder(conn, "GameSettings", "gamesettings")?
        .join(format!("{serial}_{}.ini", crc.to_ascii_uppercase())))
}
pub fn state(conn: &Connection, query: &GameQuery) -> Result<GameState, String> {
    let serial = serial(conn, &query.game_id)?;
    let global = fs::read_to_string(
        Path::new(&pcsx2::current_installation(conn)?.data_dir).join("inis/PCSX2.ini"),
    )
    .unwrap_or_default();
    let mut native = native_patches::catalogue(conn, &serial, &query.crc, &global, "", false)?;
    let crc = native.crc.clone();
    let local = if gr::crc_valid(&crc) {
        let path = settings_path(conn, &serial, &crc)?;
        let legacy = path
            .parent()
            .unwrap()
            .join(format!("{}.ini", crc.to_ascii_uppercase()));
        fs::read_to_string(if path.exists() { path } else { legacy }).unwrap_or_default()
    } else {
        String::new()
    };
    let flag = |section, key, default| {
        mc::ini_value(&local, section, key)
            .or_else(|| mc::ini_value(&global, section, key))
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(default)
    };
    let mut files = Vec::new();
    for (kind, key) in [
        ("cheats", "Cheats"),
        ("patches", "Patches"),
        ("textures", "Textures"),
    ] {
        let folder = pcsx2::native_folder(conn, key, kind)?;
        if !folder.is_dir() {
            continue;
        }
        mc::plain(&folder)?;
        let root = if kind == "textures" {
            folder.join(&serial)
        } else {
            folder
        };
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .follow_links(false)
            .max_depth(if kind == "textures" { 4 } else { 1 })
        {
            let entry = entry.map_err(|e| e.to_string())?;
            mc::plain(entry.path())?;
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            let matches = if kind == "textures" {
                true
            } else {
                let upper = name.to_ascii_uppercase();
                upper.ends_with(".PNACH")
                    && (upper.starts_with(&format!("{serial}_"))
                        || (gr::crc_valid(&crc)
                            && upper == format!("{}.PNACH", crc.to_ascii_uppercase())))
            };
            if matches && files.len() < 2000 {
                files.push(LocalResource {
                    kind: kind.into(),
                    name: name.into(),
                    path: entry.path().to_string_lossy().into(),
                });
            }
        }
    }
    native_patches::apply_enabled(
        &mut native.groups,
        &global,
        &local,
        flag("EmuCore", "EnableCheats", false),
    );
    let revision = mc::digest(
        format!(
            "{}|{}|{}",
            global,
            local,
            serde_json::to_string(&native.groups).map_err(|e| e.to_string())?
        )
        .as_bytes(),
    );
    Ok(GameState {
        files,
        cheats: flag("EmuCore", "EnableCheats", false),
        patches: flag("EmuCore", "EnablePatches", true),
        textures: flag("EmuCore/GS", "LoadTextureReplacements", false),
        crc,
        available_crcs: native.crcs,
        groups: native.groups,
        warnings: native.warnings,
        revision,
    })
}
pub fn toggle(conn: &Connection, db: &Path, req: &ToggleRequest) -> Result<GameState, String> {
    let _lock = mc::EDIT_LOCK.lock().map_err(|e| e.to_string())?;
    mc::writable()?;
    if pcsx2::installation_id(conn)? != req.installation_id {
        return Err("A instalação mudou. Atualize a página.".into());
    }
    let installation = pcsx2::current_installation(conn)?;
    if !installation
        .version
        .trim_start_matches("PCSX2 ")
        .trim_start_matches('v')
        .starts_with("2.")
    {
        return Err("Opções por jogo disponíveis para PCSX2 2.x.".into());
    }
    let global = fs::read_to_string(Path::new(&installation.data_dir).join("inis/PCSX2.ini"))
        .unwrap_or_default();
    if req.enabled
        && req.kind != "textures"
        && mc::ini_value(&global, "Achievements", "HardcoreMode")
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    {
        return Err("Desative o Hardcore antes de habilitar cheats ou patches.".into());
    }
    let (section, key) = match req.kind.as_str() {
        "cheats" => ("EmuCore", "EnableCheats"),
        "patches" => ("EmuCore", "EnablePatches"),
        "textures" => ("EmuCore/GS", "LoadTextureReplacements"),
        _ => return Err("Tipo inválido".into()),
    };
    let serial = serial(conn, &req.game_id)?;
    let mut result = state(
        conn,
        &GameQuery {
            game_id: req.game_id.clone(),
            crc: req.crc.clone(),
        },
    )?;
    if let Some(group) = &req.group_name {
        if req.revision.as_deref() != Some(result.revision.as_str()) {
            return Err("A lista de recursos mudou. Atualize antes de alterar.".into());
        }
        if !result
            .groups
            .iter()
            .any(|g| g.kind == req.kind && g.name == *group && !g.automatic)
        {
            return Err("Recurso não encontrado ou sem controle individual.".into());
        }
    }
    let path = settings_path(conn, &serial, &req.crc)?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    mc::plain(path.parent().unwrap())?;
    let legacy = path
        .parent()
        .unwrap()
        .join(format!("{}.ini", req.crc.to_ascii_uppercase()));
    let source = if path.exists() { path.clone() } else { legacy };
    let original = if source.exists() {
        mc::plain(&source)?;
        String::from_utf8(mc::read_bounded(&source)?).map_err(|e| e.to_string())?
    } else {
        String::new()
    };
    if path.exists() {
        mc::backup(db, &path)?;
    }
    let next = if let Some(group) = &req.group_name {
        let section = if req.kind == "patches" {
            "Patches"
        } else {
            "Cheats"
        };
        let mut enabled = native_patches::ini_list(&original, section, "Enable");
        let mut disabled = native_patches::ini_list(&original, section, "Disable");
        enabled.retain(|n| n != group);
        disabled.retain(|n| n != group);
        if req.enabled {
            enabled.push(group.clone());
        } else if req.kind == "patches" {
            disabled.push(group.clone());
        }
        let text = native_patches::set_list(&original, section, "Enable", &enabled);
        let text = native_patches::set_list(&text, section, "Disable", &disabled);
        if req.kind == "cheats" && req.enabled {
            pcsx2::set_ini_value(&text, "EmuCore", "EnableCheats", "true")
        } else {
            text
        }
    } else {
        pcsx2::set_ini_value(
            &original,
            section,
            key,
            if req.enabled { "true" } else { "false" },
        )
    };
    gr::atomic_file(&path, next.as_bytes())?;
    let updated_global = global;
    let cheats = mc::ini_value(&next, "EmuCore", "EnableCheats")
        .or_else(|| mc::ini_value(&updated_global, "EmuCore", "EnableCheats"))
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    if req.group_name.is_none() {
        match req.kind.as_str() {
            "cheats" => result.cheats = req.enabled,
            "patches" => result.patches = req.enabled,
            "textures" => result.textures = req.enabled,
            _ => {}
        }
    } else {
        result.cheats = cheats;
    }
    native_patches::apply_enabled(&mut result.groups, &updated_global, &next, cheats);
    result.revision = mc::digest(
        format!(
            "{}|{}|{}",
            updated_global,
            next,
            serde_json::to_string(&result.groups).map_err(|e| e.to_string())?
        )
        .as_bytes(),
    );
    Ok(result)
}

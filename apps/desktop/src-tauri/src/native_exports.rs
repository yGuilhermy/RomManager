use serde::Deserialize;
use std::{io::Write, path::{Path, PathBuf}};
use tauri_plugin_dialog::DialogExt;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ExportRequest {
    Save { source: String },
    Extra { source: String },
    Profile { contents: String },
}

fn export_data(request: ExportRequest, root: &Path) -> Result<(Vec<u8>, String, String), String> {
    match request {
        ExportRequest::Save { source } => {
            let source = PathBuf::from(source);
            let allowed = root.canonicalize().map_err(|e| e.to_string())?;
            let resolved = source.canonicalize().map_err(|e| e.to_string())?;
            if !resolved.starts_with(allowed) || resolved.extension().and_then(|e| e.to_str()) != Some("psu") {
                return Err("Selecione um save exportado pelo RomManager.".into());
            }
            let bytes = rom_core::memory_cards::read_bounded(&resolved)?;
            let name = resolved.file_name().ok_or("Nome do save inválido")?.to_string_lossy().into_owned();
            Ok((bytes, name, "psu".into()))
        }
        ExportRequest::Extra { source } => {
            let allowed = root.parent().and_then(Path::parent).ok_or("Diretório de dados inválido")?.join("a6/downloads").canonicalize().map_err(|e|e.to_string())?;
            let source = PathBuf::from(source).canonicalize().map_err(|e|e.to_string())?;
            let extension = source.extension().and_then(|e|e.to_str()).ok_or("Arquivo sem extensão")?.to_ascii_lowercase();
            if !source.starts_with(&allowed) || !["psu","pnach","zip","max","cbs","xps","sps","xpo"].contains(&extension.as_str()) {
                return Err("Selecione um arquivo baixado pelo catálogo de Extras.".into());
            }
            let name=source.file_name().ok_or("Nome de arquivo inválido")?.to_string_lossy().into_owned();
            Ok((rom_core::memory_cards::read_bounded(&source)?,name,extension))
        }
        ExportRequest::Profile { contents } => {
            if contents.len() > 16 * 1024 * 1024 { return Err("Backup maior que 16 MiB.".into()); }
            serde_json::from_str::<serde_json::Value>(&contents).map_err(|e| e.to_string())?;
            Ok((contents.into_bytes(), "rommanager-profile.json".into(), "json".into()))
        }
    }
}

fn publish(path: &Path, bytes: &[u8], extension: &str) -> Result<(), String> {
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(extension)) {
        return Err(format!("O arquivo deve ter a extensão .{extension}."));
    }
    let parent = path.parent().ok_or("Pasta de destino inválida")?;
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    staged.write_all(bytes).map_err(|e| e.to_string())?;
    staged.as_file().sync_all().map_err(|e| e.to_string())?;
    staged.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn save_generated_file(app: tauri::AppHandle, request: ExportRequest) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = rom_core::memory_cards::store(&rom_core::get_db_path()).join("exports");
        let (bytes, name, extension) = export_data(request, &root)?;
        let chosen = app.dialog().file().set_title("Salvar arquivo exportado")
            .add_filter(extension.to_uppercase(), &[extension.as_str()]).set_file_name(name).blocking_save_file();
        let Some(chosen) = chosen else { return Ok(None); };
        let path = chosen.into_path().map_err(|e| e.to_string())?;
        publish(&path, &bytes, &extension)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_save_cannot_read_outside_export_store() {
        let dir = tempfile::tempdir().unwrap();
        let allowed = dir.path().join("exports"); std::fs::create_dir(&allowed).unwrap();
        let outside = dir.path().join("personal.psu"); std::fs::write(&outside, b"private").unwrap();
        assert!(export_data(ExportRequest::Save { source: outside.to_string_lossy().into() }, &allowed).is_err());
        let source = allowed.join("save.psu"); std::fs::write(&source, b"save bytes").unwrap();
        let (bytes, _, extension) = export_data(ExportRequest::Save { source: source.to_string_lossy().into() }, &allowed).unwrap();
        let target = dir.path().join("chosen.psu"); std::fs::write(&target, b"old").unwrap();
        publish(&target, &bytes, &extension).unwrap();
        assert_eq!(std::fs::read(target).unwrap(), b"save bytes");
        assert_eq!(std::fs::read(source).unwrap(), b"save bytes");
    }
    #[test]
    fn invalid_profile_and_extension_do_not_replace_existing_file() {
        assert!(export_data(ExportRequest::Profile { contents: "bad json".into() }, Path::new(".")).is_err());
        let dir = tempfile::tempdir().unwrap(); let target = dir.path().join("personal.ps2");
        std::fs::write(&target, b"original").unwrap();
        assert!(publish(&target, b"replacement", "psu").is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"original");
    }
    #[test]
    fn extras_export_accepts_only_downloaded_artifacts_without_a_game_installation() {
        let dir=tempfile::tempdir().unwrap();let root=dir.path().join("a4/exports");std::fs::create_dir_all(&root).unwrap();
        let downloads=dir.path().join("a6/downloads/job_1");std::fs::create_dir_all(&downloads).unwrap();let file=downloads.join("pack.zip");std::fs::write(&file,b"downloaded pack").unwrap();
        let (bytes,name,extension)=export_data(ExportRequest::Extra{source:file.to_string_lossy().into()},&root).unwrap();assert_eq!(bytes,b"downloaded pack");assert_eq!(name,"pack.zip");assert_eq!(extension,"zip");
        let private=dir.path().join("private.zip");std::fs::write(&private,b"private").unwrap();assert!(export_data(ExportRequest::Extra{source:private.to_string_lossy().into()},&root).is_err());
        for extension in ["max","cbs","xps","sps","xpo"] {
            let file=downloads.join(format!("save.{extension}"));std::fs::write(&file,b"downloaded save").unwrap();
            let (_,_,actual)=export_data(ExportRequest::Extra{source:file.to_string_lossy().into()},&root).unwrap();assert_eq!(actual,extension);
        }
    }
}

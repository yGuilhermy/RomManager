use std::path::Path;
use std::process::Command;

pub fn validate_pcsx2_executable(path: &Path) -> bool {
    if !path.exists() || !path.is_file() {
        return false;
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if ext.eq_ignore_ascii_case("exe")
            && path
                .file_stem()
                .and_then(|x| x.to_str())
                .map(|x| x.to_lowercase().starts_with("pcsx2"))
                .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

pub fn launch_pcsx2(pcsx2_path: &Path, rom_path: &Path) -> Result<(), String> {
    launch_pcsx2_with_args(pcsx2_path, rom_path, false)
}

pub fn launch_pcsx2_with_args(
    pcsx2_path: &Path,
    rom_path: &Path,
    fullscreen: bool,
) -> Result<(), String> {
    if !validate_pcsx2_executable(pcsx2_path) {
        return Err("O executável do PCSX2 não foi encontrado ou é inválido.".to_string());
    }

    if !rom_path.exists() || !rom_path.is_file() {
        return Err("O arquivo da ROM não existe no disco.".to_string());
    }

    let mut cmd = Command::new(pcsx2_path);
    if let Some(parent) = pcsx2_path.parent() {
        cmd.current_dir(parent);
    }
    cmd.arg("-batch");
    if fullscreen {
        cmd.arg("-fullscreen");
    }
    cmd.arg(rom_path);
    cmd.spawn()
        .map_err(|e| format!("Falha ao iniciar o PCSX2: {}", e))?;

    Ok(())
}

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
    launch_pcsx2_with_data_dir(pcsx2_path, rom_path, fullscreen, None)
}

pub fn launch_pcsx2_with_data_dir(
    pcsx2_path: &Path,
    rom_path: &Path,
    fullscreen: bool,
    data_dir: Option<&Path>,
) -> Result<(), String> {
    if !validate_pcsx2_executable(pcsx2_path) {
        return Err("O executável do PCSX2 não foi encontrado ou é inválido.".to_string());
    }

    if !rom_path.exists() || !rom_path.is_file() {
        return Err("O arquivo da ROM não existe no disco.".to_string());
    }

    if data_dir.is_some_and(|path| !path.is_dir()) {
        return Err("Pasta de dados do PCSX2 não encontrada.".into());
    }
    let mut cmd = launch_command(pcsx2_path, rom_path, fullscreen, data_dir);
    cmd.spawn()
        .map_err(|e| format!("Falha ao iniciar o PCSX2: {}", e))?;
    Ok(())
}

fn launch_command(
    pcsx2_path: &Path,
    rom_path: &Path,
    fullscreen: bool,
    data_dir: Option<&Path>,
) -> Command {
    let mut cmd = Command::new(pcsx2_path);
    if let Some(parent) = pcsx2_path.parent() {
        cmd.current_dir(parent);
    }
    cmd.arg("-batch");
    if let Some(data_dir) = data_dir {
        cmd.arg("-datapath").arg(data_dir);
    }
    if fullscreen {
        cmd.arg("-fullscreen");
    }
    cmd.arg(rom_path);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_launch_keeps_paths_with_spaces_and_does_not_override_video_options() {
        let exe = Path::new("C:/Emulators/pcsx2-qt.exe");
        let rom = Path::new("D:/My games/Gran Turismo 4.iso");
        let data = Path::new("D:/My PCSX2 data");
        let command = launch_command(exe, rom, false, Some(data));
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(
            args,
            vec![
                "-batch",
                "-datapath",
                data.to_str().unwrap(),
                rom.to_str().unwrap()
            ]
        );
        let command = launch_command(exe, rom, false, None);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec!["-batch", rom.to_str().unwrap()]
        );
    }
}

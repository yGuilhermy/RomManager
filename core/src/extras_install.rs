use crate::{
    extras_sources::{self, ExtraItem},
    game_resources as gr, memory_cards as mc,
    online_http::{self, OnlineHttp},
    pcsx2, resource_settings as rs,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize)]
pub struct ExtraRequest {
    #[serde(default)]
    pub save_confirmed: bool,
    pub action: String,
    pub kind: String,
    pub game_key: String,
    pub item_id: String,
    #[serde(default)]
    pub game_id: String,
    #[serde(default)]
    pub crc: String,
    #[serde(default)]
    pub installation_id: String,
    #[serde(default)]
    pub card: String,
    #[serde(default)]
    pub card_revision: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub group_name: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub title: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub source_url: String,
    pub version: String,
    pub applied: bool,
    pub message: String,
    pub plan_id: Option<String>,
    pub backup: Option<String>,
}
fn e(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub fn validate_request(r: &ExtraRequest) -> Result<(), String> {
    if r.action == "install"
        && r.kind == "saves"
        && r.game_key.starts_with("GFQ:")
        && !r.save_confirmed
    {
        return Err("Confirme a região e a revisão do save GameFAQs antes de aplicar".into());
    }
    if !["download", "install"].contains(&r.action.as_str())
        || !extras_sources::kind_valid(&r.kind)
        || r.game_key.len() > 32
        || r.item_id.len() != 64
        || !r.item_id.bytes().all(|c| c.is_ascii_hexdigit())
        || r.game_id.len() > 1024
        || r.card.len() > 100
        || r.installation_id.len() > 64
        || r.group_name.as_ref().is_some_and(|g| g.len() > 512)
    {
        return Err("Pedido de extra inválido".into());
    }
    if r.action == "install"
        && (r.installation_id.len() != 64
            || (r.kind == "saves" && (r.card.is_empty() || r.card_revision.is_empty()))
            || (r.kind != "saves" && (r.game_id.is_empty() || !gr::crc_valid(&r.crc))))
    {
        return Err("Escolha o destino e a revisão antes de aplicar".into());
    }
    Ok(())
}
pub fn unpack_save(bytes: &[u8], serial: &str) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(e)?;
    if zip.len() != 1 {
        return Err("O pacote de save deve conter exatamente um arquivo PSV".into());
    }
    let f = zip.by_index(0).map_err(e)?;
    if f.enclosed_name().is_none()
        || f.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        || f.size() > 64 * 1024 * 1024
        || !f.name().to_lowercase().ends_with(".psv")
    {
        return Err("Formato do pacote de save não suportado".into());
    }
    let mut bytes = Vec::new();
    f.take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(e)?;
    let psu = crate::save_formats::psv_to_psu(&bytes)?;
    let name = psu_name(&psu)?;
    if mc::save_serial(&name).as_deref() != Some(serial) {
        return Err("O save recebido pertence a outra edição do jogo".into());
    }
    Ok(psu)
}
fn psu_name(bytes: &[u8]) -> Result<String, String> {
    let raw = bytes.get(64..96).ok_or("PSU truncado")?;
    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
    let name = std::str::from_utf8(&raw[..end]).map_err(e)?;
    mc::name(name)?;
    Ok(name.into())
}
pub fn prepare_gamefaqs_save(db: &Path, source: &Path) -> Result<PathBuf, String> {
    let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("");
    let bytes = mc::read_bounded(source)?;
    let psu = if extension == "zip" {
        crate::gamefaqs::prepare_psu(&bytes, extension)?
    } else if extension == "max" {
        let staging = source
            .parent()
            .ok_or("Save sem pasta")?
            .join(format!("convert-{}", mc::stamp()));
        fs::create_dir(&staging).map_err(e)?;
        let result = (|| {
            let scratch = staging.join("scratch.ps2");
            mc::run_tool(db, &scratch, "format", &[], &staging)?;
            mc::run_tool(
                db,
                &scratch,
                "import",
                &[source.to_string_lossy().into()],
                &staging,
            )?;
            let saves = mc::inspect(&scratch)?.0;
            if saves.len() != 1 {
                return Err("O MAX não contém um único save de PS2".into());
            }
            mc::name(&saves[0].name)?;
            mc::run_tool(db, &scratch, "export", &[saves[0].name.clone()], &staging)?;
            mc::read_bounded(&staging.join(format!("{}.psu", saves[0].name)))
        })();
        let _ = fs::remove_dir_all(&staging);
        result?
    } else {
        return Err("Formato GameFAQs disponível apenas para download".into());
    };
    let name = psu_name(&psu)?;
    if mc::save_serial(&name).is_none() {
        return Err("O save não identifica uma edição PS2 reconhecida".into());
    }
    required_card_bytes(&psu)?;
    let target = source.with_extension("psu");
    gr::atomic_file(&target, &psu)?;
    Ok(target)
}
pub fn required_card_bytes(bytes: &[u8]) -> Result<u64, String> {
    let count =
        u32::from_le_bytes(bytes.get(4..8).ok_or("PSU truncado")?.try_into().unwrap()) as usize;
    if !(3..=4096).contains(&count) {
        return Err("PSU inválido".into());
    }
    let mut at = 1536usize;
    let mut total = (count as u64).div_ceil(2) * 1024 + 1024;
    for _ in 0..count - 2 {
        let size = u32::from_le_bytes(
            bytes
                .get(at + 4..at + 8)
                .ok_or("PSU truncado")?
                .try_into()
                .unwrap(),
        ) as usize;
        let allocated = size.div_ceil(1024) * 1024;
        at = at.checked_add(512 + allocated).ok_or("PSU inválido")?;
        if at > bytes.len() {
            return Err("PSU truncado".into());
        }
        total += allocated as u64;
    }
    Ok(total)
}
pub fn check_space(required: u64, available: u64) -> Result<(), String> {
    if required > available {
        Err(format!(
            "Espaço insuficiente: necessários {} KB, disponíveis {} KB. Escolha outro memory card.",
            required.div_ceil(1024),
            available / 1024
        ))
    } else {
        Ok(())
    }
}
pub fn disk_available(path: &Path) -> Result<u64, String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetDiskFreeSpaceExW(
                path: *const u16,
                available: *mut u64,
                total: *mut u64,
                free: *mut u64,
            ) -> i32;
        }
        let path = path
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>();
        let (mut available, mut total, mut free) = (0, 0, 0);
        if unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut available, &mut total, &mut free) } == 0
        {
            return Err("Não foi possível conferir o espaço livre no disco".into());
        }
        Ok(available)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Verificação de espaço em pasta disponível no desktop Windows".into())
    }
}
pub fn preflight_save(
    conn: &Connection,
    db: &Path,
    r: &ExtraRequest,
    bytes: &[u8],
) -> Result<(), String> {
    let state = mc::list_for(conn, db, Some(&r.card), false)?;
    if state.running {
        return Err("Feche o PCSX2 antes de importar o save".into());
    }
    if state.installation_id != r.installation_id {
        return Err("A instalação mudou; escolha o destino novamente".into());
    }
    if !state.tool_ready {
        return Err("Instale o myMCpp em Configurações para importar este save".into());
    }
    let card = state.cards.first().ok_or("Memory card não encontrado")?;
    if let Some(error) = &card.error {
        return Err(error.clone());
    }
    if card.revision != r.card_revision {
        return Err("O cartão mudou; selecione-o novamente antes de aplicar".into());
    }
    let save_name = psu_name(bytes)?;
    if card.saves.iter().any(|s| s.name == save_name) && !r.overwrite {
        return Err("Já existe este save no cartão. Confirme a substituição com backup ou escolha outro cartão.".into());
    }
    let required = required_card_bytes(bytes)?;
    if let Some(free) = card.free_bytes {
        check_space(required, free)?;
    }
    let directory = PathBuf::from(&state.directory);
    let free = disk_available(&directory)?;
    check_space(required.saturating_add(card.size).saturating_mul(2), free)?;
    Ok(())
}
fn compatible(conn: &Connection, r: &ExtraRequest, item: &ExtraItem) -> Result<(), String> {
    if !item.installable {
        return Err(item.install_note.clone());
    }
    if pcsx2::installation_id(conn)? != r.installation_id {
        return Err("A instalação mudou; atualize o destino".into());
    }
    let serial: Option<String> = conn
        .query_row(
            "SELECT serial FROM games WHERE id=?1",
            [&r.game_id],
            |row| row.get(0),
        )
        .map_err(e)?;
    if serial.is_none() || serial != item.serial {
        return Err("O recurso não corresponde ao serial do jogo escolhido".into());
    }
    if item.crc.as_deref() != Some(r.crc.to_ascii_uppercase().as_str()) {
        return Err("O CRC do recurso não corresponde à revisão escolhida".into());
    }
    if r.kind != "textures" {
        if crate::achievements::read_pcsx2_achievements_config(conn).hardcore_mode {
            return Err("Desative o Hardcore antes de aplicar cheats ou patches".into());
        }
        let name = r
            .group_name
            .as_deref()
            .ok_or("Escolha o código ou patch a ativar")?;
        if item.groups.iter().any(|g| g.automatic)
            || !item.groups.iter().any(|g| g.name == name && !g.automatic)
        {
            return Err(
                "Código sem controle individual; use a importação manual na biblioteca".into(),
            );
        }
    }
    mc::writable()?;
    Ok(())
}
async fn download(http: &OnlineHttp, item: &ExtraItem, dir: &Path) -> Result<PathBuf, String> {
    http.check()?;
    fs::create_dir_all(dir).map_err(e)?;
    if item.kind == "saves" && item.game_key.starts_with("GFQ:") {
        let (name, bytes) = crate::gamefaqs::download(http, item).await?;
        mc::name(&name)?;
        http.check()?;
        let path = dir.join(name);
        gr::atomic_file(&path, &bytes)?;
        return Ok(path);
    }
    let name = if item.kind == "saves" {
        format!("{}.psu", item.serial.as_deref().ok_or("Save sem serial")?)
    } else if item.kind == "textures" {
        format!(
            "{}_textures.zip",
            item.serial.as_deref().ok_or("Texturas sem serial")?
        )
    } else {
        format!("{}.pnach", item.crc.as_deref().ok_or("PNACH sem CRC")?)
    };
    let path = dir.join(name);
    let bytes = if item.kind == "textures" {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let mut total = 0usize;
        for f in &item.files {
            http.check()?;
            let data = extras_sources::verified(http, f, 32 * 1024 * 1024).await?;
            total += data.len();
            if total > 96 * 1024 * 1024 {
                return Err("Pack expandido acima do limite de 96 MB".into());
            }
            let prefix = format!("PCSX2/textures/{}/", item.serial.as_deref().unwrap());
            let relative = f
                .path
                .strip_prefix(&prefix)
                .ok_or("Pasta de texturas incompatível")?;
            let p = Path::new(relative);
            if p.components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("Caminho inválido no pack de texturas".into());
            }
            if (relative.ends_with(".png") && !data.starts_with(b"\x89PNG\r\n\x1a\n"))
                || (relative.ends_with(".dds") && !data.starts_with(b"DDS "))
            {
                return Err("Textura recebida em formato inválido".into());
            }
            zip.start_file(
                format!("{}/{relative}", item.serial.as_deref().unwrap()),
                zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .map_err(e)?;
            zip.write_all(&data).map_err(e)?;
        }
        zip.finish().map_err(e)?.into_inner()
    } else {
        let data = extras_sources::verified(
            http,
            &item.files[0],
            if item.kind == "saves" {
                32 * 1024 * 1024
            } else {
                2 * 1024 * 1024
            },
        )
        .await?;
        if item.kind == "saves" {
            unpack_save(&data, item.serial.as_deref().unwrap())?
        } else {
            let text = std::str::from_utf8(&data).map_err(e)?;
            if text.contains('\0') {
                return Err("PNACH binário não suportado".into());
            }
            data
        }
    };
    http.check()?;
    gr::atomic_file(&path, &bytes)?;
    Ok(path)
}
fn resource_request(
    r: &ExtraRequest,
    source: &Path,
    action: &str,
    plan_id: &str,
) -> gr::ResourceRequest {
    gr::ResourceRequest {
        action: action.into(),
        game_id: r.game_id.clone(),
        kind: r.kind.clone(),
        source: source.to_string_lossy().into(),
        crc: r.crc.to_ascii_uppercase(),
        plan_id: plan_id.into(),
        installation_id: r.installation_id.clone(),
    }
}
#[cfg(test)]
fn install_resource(
    conn: &Connection,
    db: &Path,
    r: &ExtraRequest,
    item: &ExtraItem,
    path: &Path,
) -> Result<String, String> {
    install_resource_checked(conn, db, r, item, path, || Ok(()))
}
fn install_resource_checked(
    conn: &Connection,
    db: &Path,
    r: &ExtraRequest,
    item: &ExtraItem,
    path: &Path,
    check: impl Fn() -> Result<(), String>,
) -> Result<String, String> {
    let _guard = mc::EDIT_LOCK.lock().map_err(e)?;
    compatible(conn, r, item)?;
    let plan = gr::action_locked(conn, db, resource_request(r, path, "preview", ""))?;
    if plan.files.iter().any(|f| f.previous.is_some()) && !r.overwrite {
        return Err(
            "Arquivos locais já existem. Confirme a substituição com backup para continuar.".into(),
        );
    }
    let settings = pcsx2::native_folder(conn, "GameSettings", "gamesettings")?.join(format!(
        "{}_{}.ini",
        item.serial.as_deref().ok_or("Serial ausente")?,
        r.crc.to_ascii_uppercase()
    ));
    let previous_settings = if settings.exists() {
        Some(mc::read_bounded(&settings)?)
    } else {
        None
    };
    check()?;
    let plan = gr::action_locked(conn, db, resource_request(r, path, "apply", &plan.id))?;
    let activation: Result<(), String> = (|| {
        let state = rs::state(
            conn,
            &rs::GameQuery {
                game_id: r.game_id.clone(),
                crc: r.crc.clone(),
            },
        )?;
        rs::toggle_locked(
            conn,
            db,
            &rs::ToggleRequest {
                game_id: r.game_id.clone(),
                crc: r.crc.clone(),
                kind: r.kind.clone(),
                enabled: true,
                installation_id: r.installation_id.clone(),
                group_name: r.group_name.clone(),
                revision: Some(state.revision),
            },
        )?;
        Ok(())
    })();
    if let Err(error) = activation {
        let rollback = gr::action_locked(conn, db, resource_request(r, path, "undo", &plan.id));
        let ini_rollback = if fs::read(&settings).ok() != previous_settings {
            if let Some(bytes) = previous_settings {
                gr::atomic_file(&settings, &bytes)
            } else if settings.exists() {
                fs::remove_file(&settings).map_err(e)
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };
        if let Err(ini_error) = ini_rollback {
            return Err(format!(
                "{error}. Falha ao restaurar as opções: {ini_error}. Consulte os backups da instalação."
            ));
        }
        return Err(match rollback {
            Ok(_) => format!("{error}. Instalação revertida; arquivos anteriores restaurados."),
            Err(rollback) => format!(
                "{error}. Não foi possível reverter agora: {rollback}. O recurso permanece registrado na biblioteca (plano {}). Feche o PCSX2 e use Remover e restaurar anteriores.",
                plan.id
            ),
        });
    }
    Ok(plan.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Env {
        root: PathBuf,
        db: PathBuf,
        conn: Connection,
    }
    impl Env {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("rm-extras-{}", mc::stamp()));
            fs::create_dir_all(root.join("inis")).unwrap();
            fs::write(root.join("pcsx2-qt.exe"), b"MZ_TEST").unwrap();
            fs::write(root.join("portable.ini"), b"").unwrap();
            fs::write(root.join("version.json"), br#"{"version":"2.8.2"}"#).unwrap();
            fs::write(
                root.join("inis/PCSX2.ini"),
                b"[EmuCore/GS]\nRenderer = 14\n",
            )
            .unwrap();
            let db = root.join("local.db");
            let conn = crate::database::initialize_db(&db).unwrap();
            crate::database::set_config(
                &conn,
                "pcsx2_path",
                root.join("pcsx2-qt.exe").to_str().unwrap(),
            )
            .unwrap();
            conn.execute("INSERT INTO games(id,title,filename,file_path,file_size,format,serial,date_added) VALUES('game','QA','game.iso','game.iso',0,'ISO','SLUS-21355','now')",[]).unwrap();
            Self { root, db, conn }
        }
        fn request(&self) -> ExtraRequest {
            ExtraRequest {
                save_confirmed: false,
                action: "install".into(),
                kind: "patches".into(),
                game_key: "SLUS-21355".into(),
                item_id: "a".repeat(64),
                game_id: "game".into(),
                crc: "60A42FF5".into(),
                installation_id: pcsx2::installation_id(&self.conn).unwrap(),
                card: String::new(),
                card_revision: String::new(),
                overwrite: true,
                group_name: Some("60 FPS".into()),
            }
        }
        fn source(&self) -> PathBuf {
            let path = self.root.join("60A42FF5.pnach");
            fs::write(&path, "[60 FPS]\npatch=1,EE,00100000,word,00000000\n").unwrap();
            path
        }
        fn item(&self) -> ExtraItem {
            ExtraItem {
                id: "a".repeat(64),
                game_key: "SLUS-21355".into(),
                title: "QA".into(),
                description: String::new(),
                kind: "patches".into(),
                serial: Some("SLUS-21355".into()),
                crc: Some("60A42FF5".into()),
                source: String::new(),
                source_url: String::new(),
                author: String::new(),
                license: String::new(),
                version: String::new(),
                bytes: 0,
                groups: crate::native_patches::parse(
                    "[60 FPS]\npatch=1,EE,00100000,word,00000000",
                    "patches",
                    "QA",
                    false,
                ),
                installable: true,
                install_note: String::new(),
                files: Vec::new(),
            }
        }
    }
    impl Drop for Env {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn extra_installs_and_activates_only_chosen_group_without_changing_renderer() {
        let env = Env::new();
        let id = install_resource(
            &env.conn,
            &env.db,
            &env.request(),
            &env.item(),
            &env.source(),
        )
        .unwrap();
        assert!(!id.is_empty());
        let ini =
            fs::read_to_string(env.root.join("gamesettings/SLUS-21355_60A42FF5.ini")).unwrap();
        assert!(ini.contains("Enable = 60 FPS"));
        assert_eq!(
            fs::read_to_string(env.root.join("inis/PCSX2.ini")).unwrap(),
            "[EmuCore/GS]\nRenderer = 14\n"
        );
    }
    #[test]
    fn failed_activation_restores_previous_pnach_and_marks_plan_undone() {
        let env = Env::new();
        fs::create_dir(env.root.join("patches")).unwrap();
        let previous = env.root.join("patches/SLUS-21355_60A42FF5.pnach");
        fs::write(&previous, b"old personal patch").unwrap();
        fs::write(env.root.join("gamesettings"), b"block directory creation").unwrap();
        let error = install_resource(
            &env.conn,
            &env.db,
            &env.request(),
            &env.item(),
            &env.source(),
        )
        .unwrap_err();
        assert!(error.contains("restaurados"), "{error}");
        assert_eq!(fs::read(previous).unwrap(), b"old personal patch");
        assert!(
            gr::state(&env.conn, &env.db)
                .unwrap()
                .plans
                .iter()
                .any(|p| p.undone)
        );
    }
    #[test]
    fn wrong_edition_crc_and_hardcore_block_before_destination_writes() {
        let env = Env::new();
        let mut request = env.request();
        request.crc = "00000000".into();
        assert!(
            install_resource(&env.conn, &env.db, &request, &env.item(), &env.source()).is_err()
        );
        assert!(!env.root.join("patches").exists());
        fs::write(
            env.root.join("inis/PCSX2.ini"),
            b"[Achievements]\nChallengeMode = true\n",
        )
        .unwrap();
        assert!(
            install_resource(
                &env.conn,
                &env.db,
                &env.request(),
                &env.item(),
                &env.source()
            )
            .unwrap_err()
            .contains("Hardcore")
        );
        assert!(!env.root.join("patches").exists());
    }
    #[test]
    fn insufficient_space_has_actionable_error() {
        let message = check_space(100 * 1024, 50 * 1024).unwrap_err();
        assert!(
            message.contains("100 KB")
                && message.contains("50 KB")
                && message.contains("outro memory card")
        );
        assert!(check_space(10, 10).is_ok());
    }
    #[test]
    fn cancellation_at_commit_boundary_preserves_destination() {
        let env = Env::new();
        let error = install_resource_checked(
            &env.conn,
            &env.db,
            &env.request(),
            &env.item(),
            &env.source(),
            || Err("cancelled before commit".into()),
        )
        .unwrap_err();
        assert!(error.contains("cancelled"));
        assert!(!env.root.join("patches").exists());
    }
}
pub async fn run(
    db: PathBuf,
    jobs: crate::jobs::JobManager,
    id: String,
    r: ExtraRequest,
) -> Result<Receipt, String> {
    let http = OnlineHttp::new(&db, false, Some((jobs.clone(), id.clone())))?;
    jobs.task_progress(&id, 5.0, "Conferindo recurso e versão da fonte")
        .map_err(e)?;
    let found = extras_sources::items(&http, &r.kind, &r.game_key).await?;
    let item = found
        .items
        .into_iter()
        .find(|i| i.id == r.item_id)
        .ok_or("A fonte mudou; atualize os resultados antes de baixar")?;
    if r.action == "install" && !item.installable {
        return Err(item.install_note.clone());
    }
    if r.action == "install" && r.kind != "saves" {
        compatible(&Connection::open(&db).map_err(e)?, &r, &item)?;
    }
    jobs.task_progress(&id, 20.0, "Baixando e verificando arquivos")
        .map_err(e)?;
    let dir = online_http::root(&db).join("downloads").join(&id);
    let mut path = download(&http, &item, &dir).await?;
    if r.action == "install" && r.game_key.starts_with("GFQ:") {
        let db2 = db.clone();
        let source = path.clone();
        path = tokio::task::spawn_blocking(move || prepare_gamefaqs_save(&db2, &source))
            .await
            .map_err(e)??;
    }
    http.check()?;
    let mut receipt = Receipt {
        title: item.title.clone(),
        kind: r.kind.clone(),
        path: path.to_string_lossy().into(),
        sha256: mc::fingerprint(&path)?,
        size: fs::metadata(&path).map_err(e)?.len(),
        source_url: item.source_url.clone(),
        version: item.version.clone(),
        applied: false,
        message: "Arquivo pronto para salvar".into(),
        plan_id: None,
        backup: None,
    };
    if r.action == "install" {
        jobs.task_progress(&id, 85.0, "Conferindo destino e criando backup")
            .map_err(e)?;
        http.check()?;
        let db2 = db.clone();
        let request = r.clone();
        let item = item.clone();
        let source = path.clone();
        let cancellation = http.clone();
        let applied=tokio::task::spawn_blocking(move||{
            let conn=Connection::open(&db2).map_err(e)?;
            if request.kind=="saves"{
                let _guard=mc::EDIT_LOCK.lock().map_err(e)?;cancellation.check()?;
                let bytes=mc::read_bounded(&source)?;preflight_save(&conn,&db2,&request,&bytes)?;
                let action:mc::CardAction=serde_json::from_value(serde_json::json!({"action":"import","card":request.card,"revision":request.card_revision,"installation_id":request.installation_id,"source":source,"overwrite":request.overwrite})).map_err(e)?;
                cancellation.check()?;let result=mc::action_locked(&conn,&db2,action)?;Ok((None,result.backup))
            }else{install_resource_checked(&conn,&db2,&request,&item,&source,||cancellation.check()).map(|id|(Some(id),None))}
        }).await.map_err(e)??;
        receipt.applied = true;
        receipt.plan_id = applied.0;
        receipt.backup = applied.1;
        receipt.message = if r.kind == "saves" {
            "Save importado com backup"
        } else {
            "Recurso instalado e ativado no jogo"
        }
        .into();
    }
    let out = online_http::root(&db).join("operations");
    fs::create_dir_all(&out).map_err(e)?;
    gr::atomic_file(
        &out.join(format!("{id}.json")),
        &serde_json::to_vec(&receipt).map_err(e)?,
    )?;
    Ok(receipt)
}

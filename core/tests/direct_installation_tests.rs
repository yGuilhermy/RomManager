use rom_core::{database, pcsx2};
use std::fs;

#[test]
fn direct_installation_preserves_native_options_and_resolves_custom_folders() {
    let root = std::env::temp_dir().join(format!("rm-direct-{}", rom_core::memory_cards::stamp()));
    fs::create_dir_all(root.join("inis")).unwrap();
    fs::write(
        root.join("pcsx2-qt.exe"),
        b"MZ_SYNTHETIC_EXECUTABLE_FOR_TESTS",
    )
    .unwrap();
    fs::write(root.join("portable.ini"), b"").unwrap();
    let ini = "[Folders]\nMemoryCards = custom-cards\nBios = custom-bios\n[EmuCore/GS]\nRenderer = 14\nAspectRatio = 4:3\n";
    fs::write(root.join("inis/PCSX2.ini"), ini).unwrap();
    let conn = database::initialize_db(&root.join("local.db")).unwrap();
    database::set_config(
        &conn,
        "pcsx2_path",
        root.join("pcsx2-qt.exe").to_str().unwrap(),
    )
    .unwrap();
    let installation = pcsx2::current_installation(&conn).unwrap();
    assert_eq!(installation.data_dir, root.to_string_lossy());
    assert_eq!(
        pcsx2::native_folder(&conn, "Bios", "bios").unwrap(),
        root.join("custom-bios")
    );
    assert_eq!(
        rom_core::memory_cards::cards_dir(&conn).unwrap(),
        root.join("custom-cards")
    );
    assert_eq!(
        fs::read_to_string(root.join("inis/PCSX2.ini")).unwrap(),
        ini
    );
    assert!(conn.prepare("SELECT * FROM pcsx2_profiles").is_err());
    drop(conn);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_profiles_are_archived_once_without_writing_the_emulator() {
    let root = std::env::temp_dir().join(format!("rm-migrate-{}", rom_core::memory_cards::stamp()));
    fs::create_dir_all(&root).unwrap();
    let db = root.join("local.db");
    let conn = database::initialize_db(&db).unwrap();
    conn.execute_batch("CREATE TABLE pcsx2_profiles(id TEXT,name TEXT,executable_path TEXT,data_dir TEXT,bios_path TEXT,renderer TEXT,aspect_ratio TEXT,fullscreen INTEGER,is_active INTEGER,created_at TEXT,updated_at TEXT);").unwrap();
    conn.execute("INSERT INTO pcsx2_profiles VALUES('old','Personal','C:/PCSX2/pcsx2-qt.exe','D:/PCSX2',NULL,'Vulkan','4:3',0,1,'now','now')", []).unwrap();
    database::set_config(&conn, "pcsx2_path", "C:/PCSX2/pcsx2-qt.exe").unwrap();
    drop(conn);
    let conn = database::initialize_db(&db).unwrap();
    let archive = database::get_config(&conn, "legacy_pcsx2_profiles_backup").unwrap();
    assert!(archive.contains("Personal"));
    assert_eq!(
        database::get_config(&conn, "pcsx2_data_dir").as_deref(),
        Some("D:/PCSX2")
    );
    assert!(conn.prepare("SELECT * FROM pcsx2_profiles").is_err());
    drop(conn);
    let conn = database::initialize_db(&db).unwrap();
    assert_eq!(
        database::get_config(&conn, "legacy_pcsx2_profiles_backup").unwrap(),
        archive
    );
    drop(conn);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_titles_decode_metadata_and_extract_serial_without_exposing_only_codes() {
    use rom_core::memory_cards::{icon_title, save_serial};
    assert_eq!(
        save_serial("BASCUS-97316YAOTWTD!").as_deref(),
        Some("SCUS-97316")
    );
    assert_eq!(
        save_serial("BASLUS20946SAVE").as_deref(),
        Some("SLUS-20946")
    );
    assert!(save_serial("config").is_none());
    let mut icon = vec![0u8; 964];
    icon[..4].copy_from_slice(b"PS2D");
    let (text, _, _) = encoding_rs::SHIFT_JIS.encode("Gran Turismo 4");
    icon[192..192 + text.len()].copy_from_slice(&text);
    assert_eq!(icon_title(&icon).as_deref(), Some("Gran Turismo 4"));
    assert!(icon_title(b"invalid").is_none());
}

#[test]
fn native_resource_switches_preserve_settings_and_do_not_change_other_games() {
    use rom_core::resource_settings::{self, GameQuery, ToggleRequest};
    let root = std::env::temp_dir().join(format!("rm-flags-{}", rom_core::memory_cards::stamp()));
    fs::create_dir_all(root.join("inis")).unwrap();
    fs::create_dir_all(root.join("gamesettings")).unwrap();
    fs::create_dir_all(root.join("cheats")).unwrap();
    fs::write(root.join("portable.ini"), b"").unwrap();
    fs::write(
        root.join("pcsx2-qt.exe"),
        b"MZ_SYNTHETIC_EXECUTABLE_FOR_TESTS",
    )
    .unwrap();
    fs::write(root.join("version.json"), r#"{"version":"2.8.2"}"#).unwrap();
    let global = "[EmuCore/GS]\nRenderer = 14\n[EmuCore]\nEnableCheats = false\n";
    fs::write(root.join("inis/PCSX2.ini"), global).unwrap();
    let original = "[EmuCore/GS]\nUpscaleMultiplier = 3\n[Unknown]\nOption = keep\n";
    let file = root.join("gamesettings/SLUS-20946_1234ABCD.ini");
    fs::write(&file, original).unwrap();
    fs::write(
        root.join("cheats/SLUS-20946_1234ABCD.pnach"),
        b"// own cheat",
    )
    .unwrap();
    let db = root.join("local.db");
    let c = database::initialize_db(&db).unwrap();
    database::set_config(
        &c,
        "pcsx2_path",
        root.join("pcsx2-qt.exe").to_str().unwrap(),
    )
    .unwrap();
    c.execute("INSERT INTO games(id,title,filename,file_path,file_size,format,serial,date_added) VALUES ('test','GTA','a.iso','a.iso',0,'ISO','SLUS-20946','now')",[]).unwrap();
    let mut req = ToggleRequest {
        group_name:None, revision:None,
        game_id: "test".into(),
        crc: "1234ABCD".into(),
        kind: "cheats".into(),
        enabled: true,
        installation_id: pcsx2::installation_id(&c).unwrap(),
    };
    let state = resource_settings::toggle(&c, &db, &req).unwrap();
    assert!(state.cheats);
    assert_eq!(state.files.len(), 1);
    req.enabled = false;
    assert!(!resource_settings::toggle(&c, &db, &req).unwrap().cheats);
    req.kind = "textures".into();
    req.enabled = true;
    assert!(resource_settings::toggle(&c, &db, &req).unwrap().textures);
    req.kind = "patches".into();
    req.enabled = false;
    assert!(!resource_settings::toggle(&c, &db, &req).unwrap().patches);
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("UpscaleMultiplier = 3"));
    assert!(text.contains("Option = keep"));
    assert_eq!(
        fs::read_to_string(root.join("inis/PCSX2.ini")).unwrap(),
        global
    );
    req.installation_id = "stale".into();
    assert!(resource_settings::toggle(&c, &db, &req).is_err());
    req.installation_id = pcsx2::installation_id(&c).unwrap();
    req.crc = "../bad".into();
    assert!(resource_settings::toggle(&c, &db, &req).is_err());
    req.crc="1234ABCD".into(); req.kind="cheats".into(); req.enabled=true;
    fs::write(root.join("inis/PCSX2.ini"),format!("{global}\n[Achievements]\nHardcoreMode = true\n")).unwrap();
    assert!(resource_settings::toggle(&c,&db,&req).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(),text);
    req.enabled=false;
    assert!(!resource_settings::toggle(&c,&db,&req).unwrap().cheats);
    let before=fs::read(&file).unwrap();
    let mut permissions=fs::metadata(&file).unwrap().permissions();
    permissions.set_readonly(true);fs::set_permissions(&file,permissions.clone()).unwrap();
    req.kind="textures".into(); req.enabled=false;
    assert!(resource_settings::toggle(&c,&db,&req).is_err());
    assert_eq!(fs::read(&file).unwrap(),before);
    permissions.set_readonly(false);fs::set_permissions(&file,permissions).unwrap();
    assert!(
        resource_settings::state(
            &c,
            &GameQuery {
                game_id: "test".into(),
                crc: String::new()
            }
        )
        .is_ok()
    );
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

use rom_core::{database, pcsx2, resource_settings as rs};
use std::{fs, io::Write, path::PathBuf};
struct Fixture {
    root: PathBuf,
    db: PathBuf,
    conn: rusqlite::Connection,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "rm-native-patches-{}",
            rom_core::memory_cards::stamp()
        ));
        for dir in ["resources", "inis", "gamesettings", "cheats", "patches"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("pcsx2-qt.exe"), b"MZ_TEST").unwrap();
        fs::write(root.join("portable.ini"), b"").unwrap();
        fs::write(root.join("version.json"), r#"{"version":"2.8.2"}"#).unwrap();
        fs::write(
            root.join("inis/PCSX2.ini"),
            "[EmuCore]\nEnableWideScreenPatches = true\n[EmuCore/GS]\nRenderer = 14\n",
        )
        .unwrap();
        let db = root.join("local.db");
        let conn = database::initialize_db(&db).unwrap();
        database::set_config(
            &conn,
            "pcsx2_path",
            root.join("pcsx2-qt.exe").to_str().unwrap(),
        )
        .unwrap();
        conn.execute("INSERT OR REPLACE INTO games(id,title,filename,file_path,file_size,format,serial,date_added) VALUES ('midnight','Midnight Club 3','game.iso','game.iso',0,'ISO','SLUS-21355','now')",[]).unwrap();
        let mut zip =
            zip::ZipWriter::new(fs::File::create(root.join("resources/patches.zip")).unwrap());
        zip.start_file(
            "SLUS-21355_60A42FF5.pnach",
            zip::write::FileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"[Widescreen 16:9]\nauthor=Author\npatch=1,EE,00100000,word,0\n[60 FPS]\ndescription=Requires 300% EE Overclock\npatch=1,EE,00100004,word,0\n").unwrap();
        zip.finish().unwrap();
        Self { root, db, conn }
    }
    fn read(&self, crc: &str) -> rs::GameState {
        rs::state(
            &self.conn,
            &rs::GameQuery {
                game_id: "midnight".into(),
                crc: crc.into(),
            },
        )
        .unwrap()
    }
    fn toggle(
        &self,
        name: &str,
        kind: &str,
        enabled: bool,
        revision: String,
    ) -> Result<rs::GameState, String> {
        rs::toggle(
            &self.conn,
            &self.db,
            &rs::ToggleRequest {
                game_id: "midnight".into(),
                crc: "60A42FF5".into(),
                kind: kind.into(),
                enabled,
                installation_id: pcsx2::installation_id(&self.conn).unwrap(),
                group_name: Some(name.into()),
                revision: Some(revision),
            },
        )
    }
}

#[test]
fn compatibility_switch_preserves_optional_patch_selections_and_graphics() {
    let f=Fixture::new();
    let state=f.read("60A42FF5");f.toggle("60 FPS","patches",true,state.revision).unwrap();
    let req=rs::ToggleRequest {game_id:"midnight".into(),crc:"60A42FF5".into(),kind:"patches".into(),enabled:false,installation_id:pcsx2::installation_id(&f.conn).unwrap(),group_name:None,revision:None};
    let state=rs::toggle(&f.conn,&f.db,&req).unwrap();assert!(!state.patches);assert!(state.groups.iter().all(|g|g.enabled));
    assert!(fs::read_to_string(f.root.join("inis/PCSX2.ini")).unwrap().contains("Renderer = 14"));
}
#[test]
fn disabling_cheat_execution_preserves_named_selection() {
    let f=Fixture::new();fs::write(f.root.join("cheats/SLUS-21355_60A42FF5.pnach"),"[Infinite nitro]\npatch=1,EE,00000000,word,0\n").unwrap();
    let state=f.read("60A42FF5");let state=f.toggle("Infinite nitro","cheats",true,state.revision).unwrap();assert!(state.groups.iter().find(|g|g.kind=="cheats").unwrap().selected);
    let req=rs::ToggleRequest {game_id:"midnight".into(),crc:"60A42FF5".into(),kind:"cheats".into(),enabled:false,installation_id:pcsx2::installation_id(&f.conn).unwrap(),group_name:None,revision:None};
    let state=rs::toggle(&f.conn,&f.db,&req).unwrap();let cheat=state.groups.iter().find(|g|g.kind=="cheats").unwrap();assert!(cheat.selected);assert!(!cheat.enabled);
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn bundled_midnight_groups_are_discovered_without_manual_crc_and_keep_native_lists() {
    let f = Fixture::new();
    let state = f.read("");
    assert_eq!(state.crc, "60A42FF5");
    assert_eq!(state.groups.len(), 2);
    assert!(
        state
            .groups
            .iter()
            .any(|g| g.name == "Widescreen 16:9" && g.enabled && g.bundled)
    );
    let state = f.toggle("60 FPS", "patches", true, state.revision).unwrap();
    assert!(state.groups.iter().any(|g| g.name == "60 FPS" && g.enabled));
    let state = f
        .toggle("Widescreen 16:9", "patches", false, state.revision)
        .unwrap();
    assert!(
        !state
            .groups
            .iter()
            .find(|g| g.name == "Widescreen 16:9")
            .unwrap()
            .enabled
    );
    let ini = fs::read_to_string(f.root.join("gamesettings/SLUS-21355_60A42FF5.ini")).unwrap();
    assert!(ini.contains("Enable = 60 FPS"));
    assert!(ini.contains("Disable = Widescreen 16:9"));
    assert!(
        fs::read_to_string(f.root.join("inis/PCSX2.ini"))
            .unwrap()
            .contains("Renderer = 14")
    );
}
#[test]
fn named_cheats_prefer_local_definitions_and_preserve_other_enabled_groups() {
    let f = Fixture::new();
    fs::write(
        f.root.join("cheats/SLUS-21355_60A42FF5.pnach"),
        "[Money\\Infinite]\ncomment=Own cheat\npatch=1,EE,00000000,word,0\n",
    )
    .unwrap();
    fs::write(
        f.root.join("gamesettings/SLUS-21355_60A42FF5.ini"),
        "[Cheats]\nEnable = Existing cheat\n[Unknown]\nUserOption = keep\n",
    )
    .unwrap();
    let state = f.read("60A42FF5");
    let state = f
        .toggle("Money\\Infinite", "cheats", true, state.revision)
        .unwrap();
    assert!(state.cheats);
    assert!(state.groups.iter().any(|g| g.kind == "cheats" && g.enabled));
    let ini = fs::read_to_string(f.root.join("gamesettings/SLUS-21355_60A42FF5.ini")).unwrap();
    assert!(ini.contains("Enable = Existing cheat"));
    assert!(ini.contains("Enable = Money\\Infinite"));
    assert!(ini.contains("UserOption = keep"));
    assert!(
        f.toggle("Money\\Infinite", "cheats", false, "stale".into())
            .is_err()
    );
    assert!(
        f.toggle("not-installed", "cheats", true, state.revision)
            .is_err()
    );
}
#[test]
fn unlabelled_local_patches_suppress_bundled_definitions_and_crc_does_not_leak() {
    let f = Fixture::new();
    assert!(f.read("DEADBEEF").groups.is_empty());
    fs::write(
        f.root.join("patches/SLUS-21355_60A42FF5.pnach"),
        "patch=1,EE,00000000,word,0\n[Local option]\npatch=1,EE,00000004,word,0\n",
    )
    .unwrap();
    let state = f.read("60A42FF5");
    assert!(!state.groups.iter().any(|g| g.bundled));
    assert!(!state.warnings.is_empty());
}

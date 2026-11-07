use rom_core::{database, game_resources as gr};
use std::{fs, io::Write, path::PathBuf};
struct Env {
    root: PathBuf,
    db: PathBuf,
    conn: rusqlite::Connection,
}
impl Env {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("rm-resource-{}", rom_core::memory_cards::stamp()));
        fs::create_dir_all(root.join("data/inis")).unwrap();
        fs::write(
            root.join("pcsx2-qt.exe"),
            b"MZ_SYNTHETIC_EXECUTABLE_FOR_TESTS",
        )
        .unwrap();
        fs::write(root.join("version.json"), r#"{"version":"2.8.2"}"#).unwrap();
        let db = root.join("local.db");
        let c = database::initialize_db(&db).unwrap();
        database::set_config(&c,"pcsx2_path",root.join("pcsx2-qt.exe").to_str().unwrap()).unwrap();
        database::set_config(&c,"pcsx2_data_dir",root.join("data").to_str().unwrap()).unwrap();
        c.execute("INSERT INTO games(id,title,filename,file_path,file_size,format,serial,date_added) VALUES ('game','QA','game.iso','game.iso',0,'ISO','SLUS-20946','now')",[]).unwrap();
        Self { root, db, conn: c }
    }
    fn req(&self, action: &str, kind: &str, source: &str, id: &str) -> gr::ResourceRequest {
        gr::ResourceRequest {
            action: action.into(),
            game_id: "game".into(),
            kind: kind.into(),
            source: source.into(),
            crc: "1234ABCD".into(),
            plan_id: id.into(),
            installation_id: rom_core::pcsx2::installation_id(&self.conn).unwrap(),
        }
    }
    fn pnach(&self) -> PathBuf {
        let p = self.root.join("1234ABCD.pnach");
        fs::write(&p, "gametitle=QA\npatch=1,EE,00100000,word,00000000\n").unwrap();
        p
    }
    fn zip(&self, files: &[(&str, &[u8])]) -> PathBuf {
        let p = self.root.join("pack.zip");
        let mut z = zip::ZipWriter::new(fs::File::create(&p).unwrap());
        for (n, b) in files {
            z.start_file(*n, zip::write::FileOptions::default())
                .unwrap();
            z.write_all(b).unwrap();
        }
        z.finish().unwrap();
        p
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn serial_crc_and_pnach_are_checked() {
    assert!(gr::serial_valid("SLUS-20946"));
    assert!(!gr::serial_valid("../20946"));
    assert!(gr::crc_valid("1234abcd"));
    assert!(!gr::crc_valid("not-a-crc"));
    assert!(gr::validate_pnach("patch=1,EE,00100000,word,00000000").is_ok());
    for bad in [
        "",
        "patch=1,HOST,00100000,word,00000000",
        "exec=cmd.exe",
        "patch=1,EE,../../xx,word,0",
    ] {
        assert!(gr::validate_pnach(bad).is_err());
    }
}
#[test]
fn preview_apply_and_undo_preserve_previous_file() {
    let e = Env::new();
    let p = e.pnach();
    let target = e.root.join("data/cheats/SLUS-20946_1234ABCD.pnach");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"personal original").unwrap();
    let plan = gr::action(
        &e.conn,
        &e.db,
        e.req("preview", "cheats", &p.to_string_lossy(), ""),
    )
    .unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"personal original");
    assert_eq!(plan.files.len(), 1);
    gr::action(&e.conn, &e.db, e.req("apply", "", "", &plan.id)).unwrap();
    assert_eq!(fs::read(&target).unwrap(), fs::read(p).unwrap());
    assert!(gr::action(&e.conn, &e.db, e.req("apply", "", "", &plan.id)).is_err());
    gr::action(&e.conn, &e.db, e.req("undo", "", "", &plan.id)).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"personal original");
}
#[test]
fn changed_destination_or_installation_prevents_apply() {
    let e = Env::new();
    let p = e.pnach();
    let plan = gr::action(
        &e.conn,
        &e.db,
        e.req("preview", "cheats", &p.to_string_lossy(), ""),
    )
    .unwrap();
    let target = e.root.join("data/cheats/SLUS-20946_1234ABCD.pnach");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"external edit").unwrap();
    assert!(gr::action(&e.conn, &e.db, e.req("apply", "", "", &plan.id)).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"external edit");
    let mut r = e.req("apply", "", "", &plan.id);
    r.installation_id = "other".into();
    assert!(gr::action(&e.conn, &e.db, r).is_err());
}
#[test]
fn undo_preserves_external_changes() {
    let e = Env::new();
    let p = e.pnach();
    let plan = gr::action(
        &e.conn,
        &e.db,
        e.req("preview", "patches", &p.to_string_lossy(), ""),
    )
    .unwrap();
    gr::action(&e.conn, &e.db, e.req("apply", "", "", &plan.id)).unwrap();
    let target = e.root.join("data/patches/SLUS-20946_1234ABCD.pnach");
    fs::write(&target, b"external").unwrap();
    assert!(gr::action(&e.conn, &e.db, e.req("undo", "", "", &plan.id)).is_err());
    assert_eq!(fs::read(target).unwrap(), b"external");
}
#[test]
fn hardcore_and_wrong_crc_reject_preview() {
    let e = Env::new();
    let p = e.pnach();
    let mut r = e.req("preview", "cheats", &p.to_string_lossy(), "");
    r.crc = "BAD".into();
    assert!(gr::action(&e.conn, &e.db, r).is_err());
    fs::write(
        e.root.join("data/inis/PCSX2.ini"),
        "[Achievements]\nHardcoreMode = true\n",
    )
    .unwrap();
    assert!(
        gr::action(
            &e.conn,
            &e.db,
            e.req("preview", "cheats", &p.to_string_lossy(), "")
        )
        .is_err()
    );
    assert!(!e.root.join("data/cheats").exists());
}
#[test]
fn texture_pack_install_and_undo() {
    let e = Env::new();
    let p = e.zip(&[(
        "SLUS-20946/replacements/0000000000000000.png",
        b"\x89PNG\r\n\x1a\nsynthetic",
    )]);
    let plan = gr::action(
        &e.conn,
        &e.db,
        e.req("preview", "textures", &p.to_string_lossy(), ""),
    )
    .unwrap();
    assert!(plan.crc.is_empty());
    assert!(!e.root.join("data/textures").exists());
    gr::action(&e.conn, &e.db, e.req("apply", "", "", &plan.id)).unwrap();
    let target = e
        .root
        .join("data/textures/SLUS-20946/replacements/0000000000000000.png");
    assert!(target.exists());
    gr::action(&e.conn, &e.db, e.req("undo", "", "", &plan.id)).unwrap();
    assert!(!target.exists());
}
#[test]
fn malicious_or_mismatched_texture_archives_are_rejected() {
    for name in [
        "../escape.png",
        "replacements/../../escape.png",
        "SLUS-00000/replacements/a.png",
        "replacements/program.exe",
        "replacements/CON.png",
        "replacements/sub:ads/a.png",
    ] {
        let e = Env::new();
        let p = e.zip(&[(name, b"\x89PNG\r\n\x1a\nxx")]);
        assert!(
            gr::action(
                &e.conn,
                &e.db,
                e.req("preview", "textures", &p.to_string_lossy(), "")
            )
            .is_err(),
            "{}",
            name
        );
        assert!(!e.root.join("data/textures").exists());
    }
}
#[test]
fn duplicate_and_fake_textures_are_rejected() {
    for files in [
        vec![("replacements/a.png", b"fake".as_slice())],
        vec![
            ("replacements/a.png", b"\x89PNG\r\n\x1a\na".as_slice()),
            ("replacements/A.png", b"\x89PNG\r\n\x1a\nb".as_slice()),
        ],
    ] {
        let e = Env::new();
        let p = e.zip(&files);
        assert!(
            gr::action(
                &e.conn,
                &e.db,
                e.req("preview", "textures", &p.to_string_lossy(), "")
            )
            .is_err()
        );
    }
}

#[test]
fn incompatible_emulator_version_is_blocked() {
    let e = Env::new();
    let p = e.pnach();
    fs::write(e.root.join("version.json"), r#"{"version":"1.6.0"}"#).unwrap();
    assert!(
        gr::action(
            &e.conn,
            &e.db,
            e.req("preview", "cheats", &p.to_string_lossy(), "")
        )
        .is_err()
    );
}

#[test]
fn undo_remains_available_after_version_or_hardcore_changes() {
    let e = Env::new();
    let p = e.pnach();
    let plan = gr::action(
        &e.conn,
        &e.db,
        e.req("preview", "cheats", &p.to_string_lossy(), ""),
    )
    .unwrap();
    gr::action(&e.conn, &e.db, e.req("apply", "", "", &plan.id)).unwrap();
    fs::write(e.root.join("version.json"), r#"{"version":"1.6.0"}"#).unwrap();
    fs::write(
        e.root.join("data/inis/PCSX2.ini"),
        "[Achievements]\nHardcoreMode = true\n",
    )
    .unwrap();
    gr::action(&e.conn, &e.db, e.req("undo", "", "", &plan.id)).unwrap();
    assert!(
        !e.root
            .join("data/cheats/SLUS-20946_1234ABCD.pnach")
            .exists()
    );
}

#[cfg(windows)]
#[test]
fn readonly_destination_is_rejected_without_committing_or_leaving_temp_files() {
    let e = Env::new();
    let p = e.pnach();
    let target = e.root.join("data/cheats/SLUS-20946_1234ABCD.pnach");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"personal original").unwrap();
    let plan = gr::action(
        &e.conn,
        &e.db,
        e.req("preview", "cheats", &p.to_string_lossy(), ""),
    )
    .unwrap();
    let mut permissions = fs::metadata(&target).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&target, permissions).unwrap();
    let result = gr::action(&e.conn, &e.db, e.req("apply", "", "", &plan.id));
    let preserved = fs::read(&target).unwrap() == b"personal original";
    let count = fs::read_dir(target.parent().unwrap()).unwrap().count();
    let mut permissions = fs::metadata(&target).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&target, permissions).unwrap();
    assert!(result.is_err());
    assert!(
        preserved,
        "A failed operation must preserve the destination"
    );
    assert_eq!(count, 1, "No temporary resource files should remain");
}

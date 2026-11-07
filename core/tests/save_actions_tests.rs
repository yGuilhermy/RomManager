use rom_core::{database, memory_cards as mc, pcsx2};
use std::{fs, path::PathBuf};
struct Fixture {
    root: PathBuf,
    db: PathBuf,
    conn: rusqlite::Connection,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rm-save-actions-{}", mc::stamp()));
        fs::create_dir_all(root.join("inis")).unwrap();
        fs::write(root.join("portable.ini"), b"").unwrap();
        fs::write(root.join("pcsx2-qt.exe"), b"MZ_TEST").unwrap();
        for name in ["Source", "Destination"] {
            let path = root.join("memcards").join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("_pcsx2_superblock"), [0; 8192]).unwrap();
        }
        for name in ["BASLUS-20946", "OTHER-SAVE"] {
            let p = root.join("memcards/Source").join(name);
            fs::create_dir_all(&p).unwrap();
            fs::write(p.join("DATA"), name.as_bytes()).unwrap();
        }
        let db = root.join("local.db");
        let conn = database::initialize_db(&db).unwrap();
        database::set_config(
            &conn,
            "pcsx2_path",
            root.join("pcsx2-qt.exe").to_str().unwrap(),
        )
        .unwrap();
        Self { root, db, conn }
    }
    fn request(&self, action: &str, overwrite: bool) -> mc::CardAction {
        serde_json::from_value(serde_json::json!({"action":action,"card":"Source","save":"BASLUS-20946","target":"Destination","revision":mc::fingerprint(&self.root.join("memcards/Source")).unwrap(),"target_revision":mc::fingerprint(&self.root.join("memcards/Destination")).unwrap(),"overwrite":overwrite,"installation_id":pcsx2::installation_id(&self.conn).unwrap()})).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn move_between_folder_cards_needs_no_tool_and_preserves_other_saves_with_verified_backups() {
    let f = Fixture::new();
    let before = mc::fingerprint(&f.root.join("memcards/Source/OTHER-SAVE")).unwrap();
    let result = mc::action(&f.conn, &f.db, f.request("move-save", false)).unwrap();
    assert!(!f.root.join("memcards/Source/BASLUS-20946").exists());
    assert_eq!(
        fs::read(f.root.join("memcards/Destination/BASLUS-20946/DATA")).unwrap(),
        b"BASLUS-20946"
    );
    assert_eq!(
        mc::fingerprint(&f.root.join("memcards/Source/OTHER-SAVE")).unwrap(),
        before
    );
    let backup = PathBuf::from(result.backup.unwrap());
    assert!(backup.join("origem/Source/BASLUS-20946/DATA").is_file());
    assert!(
        backup
            .join("destino/Destination/_pcsx2_superblock")
            .is_file()
    );
}
#[test]
fn delete_save_keeps_a_recoverable_card_backup_and_rejects_stale_revisions() {
    let f = Fixture::new();
    let mut request = f.request("delete-save", false);
    request.revision = "stale".into();
    assert!(mc::action(&f.conn, &f.db, request).is_err());
    let result = mc::action(&f.conn, &f.db, f.request("delete-save", false)).unwrap();
    assert!(f.root.join("memcards/Source/OTHER-SAVE/DATA").exists());
    assert!(!f.root.join("memcards/Source/BASLUS-20946").exists());
    assert!(
        PathBuf::from(result.backup.unwrap())
            .join("origem/Source/BASLUS-20946/DATA")
            .is_file()
    );
}
#[test]
fn collisions_and_stale_destination_leave_both_cards_unchanged_until_overwrite_is_explicit() {
    let f = Fixture::new();
    let dest = f.root.join("memcards/Destination/BASLUS-20946");
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("DATA"), b"previous").unwrap();
    let a = mc::fingerprint(&f.root.join("memcards/Source")).unwrap();
    let b = mc::fingerprint(&f.root.join("memcards/Destination")).unwrap();
    assert!(mc::action(&f.conn, &f.db, f.request("move-save", false)).is_err());
    let mut stale = f.request("move-save", true);
    stale.target_revision = "stale".into();
    assert!(mc::action(&f.conn, &f.db, stale).is_err());
    assert_eq!(mc::fingerprint(&f.root.join("memcards/Source")).unwrap(), a);
    assert_eq!(
        mc::fingerprint(&f.root.join("memcards/Destination")).unwrap(),
        b
    );
    let result = mc::action(&f.conn, &f.db, f.request("move-save", true)).unwrap();
    let backup = PathBuf::from(result.backup.unwrap());
    assert_eq!(
        fs::read(backup.join("destino/Destination/BASLUS-20946/DATA")).unwrap(),
        b"previous"
    );
}

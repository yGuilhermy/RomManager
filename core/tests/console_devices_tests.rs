use rom_core::console::devices::{self, DeviceProbe, Profile, Volume};
use rom_core::database;
use std::{fs, path::Path};

struct Probe {
    identity: String,
    available: bool,
    filesystem: String,
}
impl DeviceProbe for Probe {
    fn inspect(&self, _path: &Path) -> Result<Volume, String> {
        if !self.available {
            return Err("disconnected".into());
        }
        Ok(Volume {
            identity: self.identity.clone(),
            mount: _path
                .ancestors()
                .last()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            label: "Fixture".into(),
            filesystem: self.filesystem.clone(),
            removable: true,
            free_bytes: 128,
            total_bytes: 256,
        })
    }
    fn volumes(&self) -> Result<Vec<Volume>, String> {
        Ok(vec![self.inspect(Path::new("Q:\\"))?])
    }
}
fn probe(id: &str) -> Probe {
    Probe {
        identity: id.into(),
        available: true,
        filesystem: "FAT32".into(),
    }
}
fn fixture() -> (std::path::PathBuf, rusqlite::Connection) {
    let root = std::env::temp_dir().join(format!("rm-console-{}", rom_core::memory_cards::stamp()));
    fs::create_dir_all(root.join("usb/DVD")).unwrap();
    let conn = database::initialize_db(&root.join("db.sqlite")).unwrap();
    (root, conn)
}

#[test]
fn selection_persists_and_is_idempotent_without_touching_destination() {
    let (root, conn) = fixture();
    let usb = root.join("usb");
    let first = devices::register(&conn, &usb, Profile::Unknown, &probe("volume-a")).unwrap();
    let again = devices::register(&conn, &usb, Profile::LegacyFat32, &probe("volume-a")).unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(devices::list(&conn).unwrap().len(), 1);
    assert_eq!(
        devices::get(&conn, &first.id).unwrap().profile,
        Profile::LegacyFat32
    );
    assert_eq!(fs::read_dir(&usb).unwrap().count(), 1);
    devices::forget(&conn, &first.id).unwrap();
    assert!(usb.join("DVD").is_dir());
    drop(conn);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reused_drive_letter_never_authorizes_a_different_volume() {
    let (root, conn) = fixture();
    let selected =
        devices::register(&conn, &root.join("usb"), Profile::Unknown, &probe("a")).unwrap();
    assert!(devices::validate(&selected, &probe("b")).is_err());
    assert!(
        devices::validate(
            &selected,
            &Probe {
                available: false,
                ..probe("a")
            }
        )
        .is_err()
    );
    assert!(devices::validate(&selected, &probe("a")).is_ok());
    drop(conn);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn profile_does_not_infer_opl_version_or_override_detected_filesystem() {
    assert!(!devices::profile_warnings(Profile::Unknown, "FAT32").is_empty());
    assert!(!devices::profile_warnings(Profile::LegacyFat32, "exFAT").is_empty());
    assert!(!devices::profile_warnings(Profile::ModernExfat, "NTFS").is_empty());
    assert!(devices::profile_warnings(Profile::ModernExfat, "exFAT").is_empty());
    assert!(devices::profile_warnings(Profile::LegacyFat32, "FAT32").is_empty());
}

#[test]
fn drive_letter_is_not_part_of_the_registered_identity() {
    let mut volume = probe("volume-a").inspect(Path::new("Q:\\")).unwrap();
    volume.mount = "Q:\\".into();
    let first = devices::target_id(Path::new("Q:\\OPL"), &volume).unwrap();
    volume.mount = "R:\\".into();
    assert_eq!(
        first,
        devices::target_id(Path::new("R:\\OPL"), &volume).unwrap()
    );
    volume.identity = "volume-b".into();
    assert_ne!(
        first,
        devices::target_id(Path::new("R:\\OPL"), &volume).unwrap()
    );
}

#[test]
fn missing_folder_is_not_registered_and_removed_folder_is_not_validated() {
    let (root, conn) = fixture();
    assert!(
        devices::register(&conn, &root.join("missing"), Profile::Unknown, &probe("a")).is_err()
    );
    let selected =
        devices::register(&conn, &root.join("usb"), Profile::Unknown, &probe("a")).unwrap();
    fs::remove_dir(root.join("usb/DVD")).unwrap();
    fs::remove_dir(root.join("usb")).unwrap();
    assert!(devices::validate(&selected, &probe("a")).is_err());
    drop(conn);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn real_windows_probe_reads_identity_and_space_without_creating_files() {
    let volume = devices::SystemProbe
        .inspect(&std::env::current_dir().unwrap())
        .unwrap();
    assert!(volume.identity.to_ascii_lowercase().contains("volume{"));
    assert!(volume.total_bytes > 0);
    assert!(!volume.filesystem.is_empty());
}

use rom_core::{
    console::{
        devices::{self, DeviceProbe, Profile, Volume},
        inventory,
    },
    database,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn fixture() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "rm-console-inventory-{}",
        rom_core::memory_cards::stamp()
    ));
    fs::create_dir_all(p.join("DVD")).unwrap();
    fs::create_dir_all(p.join("CD")).unwrap();
    p
}
fn iso(path: &Path) {
    let mut b = vec![0u8; 2048 * 20];
    b[32768..32775].copy_from_slice(b"\x01CD001\x01");
    b[32848..32852].copy_from_slice(&20u32.to_le_bytes());
    b[32852..32856].copy_from_slice(&20u32.to_be_bytes());
    b[32896..32898].copy_from_slice(&2048u16.to_le_bytes());
    b[32898..32900].copy_from_slice(&2048u16.to_be_bytes());
    fs::write(path, b).unwrap();
}
fn ul(name: &[u8], parts: u8) -> [u8; 64] {
    let mut b = [0; 64];
    b[..name.len()].copy_from_slice(name);
    b[32..35].copy_from_slice(b"ul.");
    b[35..46].copy_from_slice(b"SLUS_213.55");
    b[47] = parts;
    b[48] = 0x14;
    b[53] = 8;
    b
}

#[test]
fn mixed_library_groups_ul_parts_preserves_mods_and_reports_missing_and_orphan_files() {
    let root = fixture();
    iso(&root.join("DVD/SLUS_213.55.Original.iso"));
    iso(&root.join("DVD/SLUS_213.55.Mod.iso"));
    fs::write(root.join("ul.cfg"), ul(b"DVD fixture", 2)).unwrap();
    fs::write(root.join("ul.B8239645.SLUS_213.55.00"), vec![0u8; 2048]).unwrap();
    fs::write(root.join("ul.DEADBEEF.SLUS_000.00.00"), b"orphan").unwrap();
    let before = fs::read(root.join("ul.cfg")).unwrap();
    let scan = inventory::scan(&root).unwrap();
    assert_eq!(scan.games.len(), 3);
    let game = scan.games.iter().find(|g| g.method == "ul").unwrap();
    assert_eq!(game.parts, 2);
    assert_eq!(game.found_parts, 1);
    assert_eq!(game.bytes, 2048);
    assert!(game.issues.iter().any(|s| s.contains(".01")));
    assert!(scan.issues.iter().any(|s| s.contains("DEADBEEF")));
    assert_eq!(fs::read(root.join("ul.cfg")).unwrap(), before);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_iso_and_truncated_index_are_diagnosed_not_called_verified() {
    let root = fixture();
    fs::write(root.join("DVD/bad.iso"), b"not ISO").unwrap();
    fs::write(root.join("ul.cfg"), [0; 65]).unwrap();
    let scan = inventory::scan(&root).unwrap();
    assert_eq!(scan.games.len(), 1);
    assert!(!scan.games[0].issues.is_empty());
    assert!(!scan.issues.is_empty());
    assert_eq!(scan.games[0].verification, "header_only");
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn identical_ul_names_are_reported_as_a_collision() {
    let root = fixture();
    let one = ul(b"Test", 1);
    fs::write(
        root.join("ul.cfg"),
        [one.as_slice(), one.as_slice()].concat(),
    )
    .unwrap();
    let scan = inventory::scan(&root).unwrap();
    assert_eq!(scan.games.len(), 2);
    assert!(
        scan.issues
            .iter()
            .any(|s| s.to_lowercase().contains("colis"))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn complete_but_shared_ul_parts_are_reported_on_both_games() {
    let root = fixture();
    let record = ul(b"Test", 1);
    fs::write(
        root.join("ul.cfg"),
        [record.as_slice(), record.as_slice()].concat(),
    )
    .unwrap();
    fs::write(root.join("ul.58C3E2CE.SLUS_213.55.00"), vec![0u8; 2048]).unwrap();
    let snapshot = inventory::scan(&root).unwrap();
    fs::remove_dir_all(root).unwrap();
    assert_eq!(snapshot.games.len(), 2);
    assert!(snapshot.games.iter().all(|g| g.found_parts == 1));
    assert!(
        snapshot
            .games
            .iter()
            .all(|g| g.issues.iter().any(|s| s.to_lowercase().contains("colis"))),
        "Both owners must appear in the warning filter"
    );
}

#[test]
fn excessive_declared_parts_are_rejected_before_unbounded_filesystem_work() {
    let root = fixture();
    let record = ul(b"Test", 255);
    fs::write(root.join("ul.cfg"), record.repeat(393)).unwrap();
    let result = inventory::scan(&root);
    fs::remove_dir_all(root).unwrap();
    assert!(
        result.is_err(),
        "A small hostile index must not expand into hundreds of thousands of file operations"
    );
}

#[test]
fn offline_volume_info_is_from_last_scan_without_reverting_a_concurrent_profile_change() {
    use std::cell::Cell;
    struct UpdatingProbe<'a> {
        conn: &'a rusqlite::Connection,
        id: String,
        calls: Cell<usize>,
    }
    impl DeviceProbe for UpdatingProbe<'_> {
        fn inspect(&self, path: &Path) -> Result<Volume, String> {
            let calls = self.calls.get();
            self.calls.set(calls + 1);
            if calls == 1 {
                devices::set_profile(self.conn, &self.id, Profile::ModernExfat)?;
            }
            let mut volume = Probe {
                id: "a",
                online: true,
            }
            .inspect(path)?;
            volume.free_bytes = 50;
            Ok(volume)
        }
        fn volumes(&self) -> Result<Vec<Volume>, String> {
            Ok(vec![])
        }
    }
    let root = fixture();
    let db = root.parent().unwrap().join(format!(
        "rm-console-free-{}.sqlite",
        rom_core::memory_cards::stamp()
    ));
    let conn = database::initialize_db(&db).unwrap();
    let target = devices::register(
        &conn,
        &root,
        Profile::Unknown,
        &Probe {
            id: "a",
            online: true,
        },
    )
    .unwrap();
    inventory::refresh(
        &conn,
        &target.id,
        &UpdatingProbe {
            conn: &conn,
            id: target.id.clone(),
            calls: Cell::new(0),
        },
    )
    .unwrap();
    let view = inventory::cached(
        &conn,
        &target.id,
        &Probe {
            id: "a",
            online: false,
        },
    )
    .unwrap();
    drop(conn);
    fs::remove_file(db).unwrap();
    fs::remove_dir_all(root).unwrap();
    assert_eq!(view.target.profile, Profile::ModernExfat);
    assert_eq!(
        view.target.volume.free_bytes, 50,
        "Offline space must be from the last scan, not registration"
    );
}

#[test]
fn offline_reader_does_not_mix_a_new_snapshot_with_old_volume_metadata() {
    use std::cell::Cell;
    struct ReplacingProbe<'a> {
        conn: &'a rusqlite::Connection,
        id: String,
        target: String,
        snapshot: String,
        changed: Cell<bool>,
    }
    impl DeviceProbe for ReplacingProbe<'_> {
        fn inspect(&self, _: &Path) -> Result<Volume, String> {
            if !self.changed.replace(true) {
                self.conn
                    .execute(
                        "UPDATE console_targets SET target_json=?2,snapshot_json=?3 WHERE id=?1",
                        rusqlite::params![self.id, self.target, self.snapshot],
                    )
                    .map_err(|e| e.to_string())?;
            }
            Err("offline".into())
        }
        fn volumes(&self) -> Result<Vec<Volume>, String> {
            Ok(vec![])
        }
    }
    let root = fixture();
    let db = root.parent().unwrap().join(format!(
        "rm-console-coherent-{}.sqlite",
        rom_core::memory_cards::stamp()
    ));
    let conn = database::initialize_db(&db).unwrap();
    let probe = Probe {
        id: "a",
        online: true,
    };
    let target = devices::register(&conn, &root, Profile::Unknown, &probe).unwrap();
    let old = inventory::refresh(&conn, &target.id, &probe).unwrap();
    let old_stamp = old.snapshot.as_ref().unwrap().scanned_at.clone();
    let mut newer_target = target.clone();
    newer_target.volume.free_bytes = 50;
    let mut newer_snapshot = old.snapshot.unwrap();
    newer_snapshot.scanned_at = chrono::Utc::now().to_rfc3339();
    assert_ne!(newer_snapshot.scanned_at, old_stamp);
    let view = inventory::cached(
        &conn,
        &target.id,
        &ReplacingProbe {
            conn: &conn,
            id: target.id.clone(),
            target: serde_json::to_string(&newer_target).unwrap(),
            snapshot: serde_json::to_string(&newer_snapshot).unwrap(),
            changed: Cell::new(false),
        },
    )
    .unwrap();
    drop(conn);
    fs::remove_file(db).unwrap();
    fs::remove_dir_all(root).unwrap();
    let expected = if view.snapshot.unwrap().scanned_at == old_stamp {
        100
    } else {
        50
    };
    assert_eq!(
        view.target.volume.free_bytes, expected,
        "Cached snapshot and volume metadata must come from the same DB read"
    );
}
struct Probe {
    id: &'static str,
    online: bool,
}

#[test]
fn volume_replacement_during_scan_does_not_publish_the_new_cache() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct SwitchingProbe(AtomicUsize);
    impl DeviceProbe for SwitchingProbe {
        fn inspect(&self, path: &Path) -> Result<Volume, String> {
            Probe {
                id: if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    "a"
                } else {
                    "replacement"
                },
                online: true,
            }
            .inspect(path)
        }
        fn volumes(&self) -> Result<Vec<Volume>, String> {
            Ok(vec![])
        }
    }
    let root = fixture();
    let db = root.parent().unwrap().join(format!(
        "rm-console-swap-{}.sqlite",
        rom_core::memory_cards::stamp()
    ));
    let conn = database::initialize_db(&db).unwrap();
    let probe = Probe {
        id: "a",
        online: true,
    };
    let target = devices::register(&conn, &root, Profile::Unknown, &probe).unwrap();
    let before = inventory::refresh(&conn, &target.id, &probe).unwrap();
    iso(&root.join("DVD/new.iso"));
    assert!(inventory::refresh(&conn, &target.id, &SwitchingProbe(AtomicUsize::new(0))).is_err());
    let after = inventory::cached(&conn, &target.id, &probe).unwrap();
    assert_eq!(
        after.snapshot.unwrap().scanned_at,
        before.snapshot.unwrap().scanned_at
    );
    drop(conn);
    fs::remove_file(db).unwrap();
    fs::remove_dir_all(root).unwrap();
}
impl DeviceProbe for Probe {
    fn inspect(&self, path: &Path) -> Result<Volume, String> {
        if !self.online {
            return Err("offline".into());
        }
        Ok(Volume {
            identity: self.id.into(),
            mount: path
                .ancestors()
                .last()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            label: "Test".into(),
            filesystem: "FAT32".into(),
            removable: true,
            free_bytes: 100,
            total_bytes: 200,
        })
    }
    fn volumes(&self) -> Result<Vec<Volume>, String> {
        Ok(vec![])
    }
}
#[test]
fn offline_cache_survives_restart_and_refresh_failure_on_reused_volume() {
    let root = fixture();
    let db = root.parent().unwrap().join(format!(
        "rm-console-db-{}.sqlite",
        rom_core::memory_cards::stamp()
    ));
    let conn = database::initialize_db(&db).unwrap();
    let probe = Probe {
        id: "a",
        online: true,
    };
    let target = devices::register(&conn, &root, Profile::Unknown, &probe).unwrap();
    iso(&root.join("DVD/Test.iso"));
    let current = inventory::refresh(&conn, &target.id, &probe).unwrap();
    drop(conn);
    let conn = database::initialize_db(&db).unwrap();
    let cached = inventory::cached(
        &conn,
        &target.id,
        &Probe {
            id: "a",
            online: false,
        },
    )
    .unwrap();
    assert!(!cached.online);
    assert!(cached.cached);
    assert_eq!(cached.snapshot.as_ref().unwrap().games.len(), 1);
    assert!(
        inventory::refresh(
            &conn,
            &target.id,
            &Probe {
                id: "b",
                online: true
            }
        )
        .is_err()
    );
    let after = inventory::cached(&conn, &target.id, &probe).unwrap();
    assert_eq!(
        after.snapshot.unwrap().scanned_at,
        current.snapshot.unwrap().scanned_at
    );
    drop(conn);
    fs::remove_file(db).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn directory_junction_is_not_followed() {
    use std::process::Command;
    let root = fixture();
    let outside = fixture();
    iso(&outside.join("secret.iso"));
    fs::remove_dir(root.join("DVD")).unwrap();
    let link = root.join("DVD");
    let code = Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .status()
        .unwrap();
    assert!(code.success());
    let result = inventory::scan(&root).unwrap();
    assert!(result.games.is_empty());
    assert!(!result.issues.is_empty());
    fs::remove_dir(link).unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

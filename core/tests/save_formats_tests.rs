use rom_core::save_formats::*;
use std::{fs, path::PathBuf};
fn root() -> PathBuf {
    let p = std::env::temp_dir().join(format!("rm-psu-{}", rom_core::memory_cards::stamp()));
    fs::create_dir_all(&p).unwrap();
    p
}
fn entry(name: &str, mode: u16, len: u32) -> Vec<u8> {
    let mut b = vec![0; 512];
    b[..2].copy_from_slice(&mode.to_le_bytes());
    b[4..8].copy_from_slice(&len.to_le_bytes());
    b[64..64 + name.len()].copy_from_slice(name.as_bytes());
    b
}
fn psu() -> Vec<u8> {
    let mut b = entry("BASLUS-TEST", 0x8427, 3);
    b.extend(entry(".", 0x8427, 3));
    b.extend(entry("..", 0x8427, 0));
    b.extend(entry("DATA", 0x8497, 4));
    b.extend(b"test");
    b.resize(3072, 0);
    b
}
#[test]
fn psu_folder_roundtrip_preserves_payload_and_metadata() {
    let p = root();
    let b = psu();
    let name = psu_to_folder(&b, &p).unwrap();
    assert_eq!(fs::read(p.join(&name).join("DATA")).unwrap(), b"test");
    let restored = folder_to_psu(&p.join(&name)).unwrap();
    assert_eq!(restored, b);
    fs::remove_dir_all(p).unwrap();
}
#[test]
fn psu_rejects_traversal_and_truncation_before_creating_folder() {
    for mut b in [psu(), psu(), vec![0; 30]] {
        if b.len() > 2048 {
            b[64..96].fill(0);
            b[64..69].copy_from_slice(b"../xx");
        }
        let p = root();
        assert!(psu_to_folder(&b, &p).is_err());
        assert_eq!(fs::read_dir(&p).unwrap().count(), 0);
        fs::remove_dir(p).unwrap();
    }
}
#[test]
fn folder_conversion_rejects_unknown_metadata_and_links() {
    let p = root();
    let save = p.join("SAVE");
    fs::create_dir(&save).unwrap();
    fs::write(save.join("DATA"), b"x").unwrap();
    fs::write(save.join("_pcsx2_index"), "DATA:\n  unknown: 1\n").unwrap();
    assert!(folder_to_psu(&save).is_err());
    fs::remove_dir_all(p).unwrap();
}

#[test]
fn folder_index_accepts_pcsx2_flow_maps() {
    let p = root();
    let save = p.join("SAVE");
    fs::create_dir(&save).unwrap();
    fs::write(save.join("DATA"), b"x").unwrap();
    fs::write(save.join("_pcsx2_index"),"{$ROOT: {timeCreated: 1700000000,timeModified: 1700000001},DATA: {order: 1,timeCreated: 1700000000,timeModified: 1700000001}}").unwrap();
    let bytes = folder_to_psu(&save).unwrap();
    assert!(bytes.len() > 2048);
    fs::remove_dir_all(p).unwrap();
}

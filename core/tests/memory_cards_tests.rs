use rom_core::memory_cards::*;
#[test]
fn summary_defers_card_content_reads_until_selection() {
    let root = std::env::temp_dir().join(format!("rm-summary-{}", stamp()));
    std::fs::create_dir_all(root.join("memcards")).unwrap();
    std::fs::write(root.join("memcards/Broken.ps2"), b"not a card").unwrap();
    let db = root.join("local.db");
    let conn = rom_core::database::initialize_db(&db).unwrap();
    std::fs::write(
        root.join("pcsx2-qt.exe"),
        b"MZ_SYNTHETIC_EXECUTABLE_FOR_TESTS",
    )
    .unwrap();
    std::fs::write(root.join("portable.ini"), b"").unwrap();
    rom_core::database::set_config(
        &conn,
        "pcsx2_path",
        root.join("pcsx2-qt.exe").to_str().unwrap(),
    )
    .unwrap();
    let summary = list_for(&conn, &db, None, true).unwrap();
    assert_eq!(summary.cards.len(), 1);
    assert!(summary.cards[0].revision.is_empty());
    assert!(summary.cards[0].error.is_none());
    let detail = list_for(&conn, &db, Some("Broken.ps2"), false).unwrap();
    assert!(detail.cards[0].error.is_some());
    assert!(!detail.cards[0].revision.is_empty());
    assert!(list_for(&conn, &db, Some("../outside"), false).is_err());
    drop(conn);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn windows_names_and_paths_are_rejected() {
    for n in ["../x", "a/b", "CON.ps2", "NUL", "a:", ".hidden", "x."] {
        assert!(name(n).is_err(), "{}", n);
    }
    assert!(name("Meu cartão.ps2").is_ok());
}
#[test]
fn malformed_cards_are_rejected_without_panic() {
    for b in [
        vec![],
        vec![0; 1024],
        b"Sony PS2 Memory Card Format ".to_vec(),
    ] {
        assert!(CardReader::open(&b).is_err());
    }
}
#[test]
fn ini_resolves_exact_section_and_key() {
    let ini = "[Folders]\nMemoryCards = cards\n[MemoryCards]\nSlot1_Filename = Test.ps2\n";
    assert_eq!(
        ini_value(ini, "Folders", "MemoryCards"),
        Some("cards".into())
    );
    assert_eq!(
        ini_value(ini, "MemoryCards", "Slot1_Filename"),
        Some("Test.ps2".into())
    );
}

fn fixture() -> Vec<u8> {
    let mut b = vec![0u8; 64 * 1024];
    b[..28].copy_from_slice(b"Sony PS2 Memory Card Format ");
    fn w32(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    b[40..42].copy_from_slice(&512u16.to_le_bytes());
    b[42..44].copy_from_slice(&2u16.to_le_bytes());
    w32(&mut b, 48, 64);
    w32(&mut b, 52, 6);
    w32(&mut b, 56, 58);
    w32(&mut b, 60, 0);
    w32(&mut b, 80, 1);
    w32(&mut b, 1024, 2);
    for n in 0..58 {
        w32(
            &mut b,
            2048 + n * 4,
            if n < 3 { 0xffffffff } else { 0x7fffffff },
        );
    }
    fn ent(b: &mut [u8], at: usize, name: &str, mode: u16, len: u32, cluster: u32) {
        b[at..at + 2].copy_from_slice(&mode.to_le_bytes());
        b[at + 4..at + 8].copy_from_slice(&len.to_le_bytes());
        b[at + 16..at + 20].copy_from_slice(&cluster.to_le_bytes());
        b[at + 64..at + 64 + name.len()].copy_from_slice(name.as_bytes());
    }
    // Root has a chained second cluster for its third directory entry.
    w32(&mut b, 2048, 0x80000003);
    w32(&mut b, 2048 + 12, 0xffffffff);
    ent(&mut b, 6 * 1024, ".", 0x8427, 3, 0);
    ent(&mut b, 6 * 1024 + 512, "..", 0x8427, 0, 0);
    ent(&mut b, 9 * 1024, "BASLUS-20946RMTEST", 0x8427, 3, 1);
    w32(&mut b, 2048 + 4, 0x80000004);
    w32(&mut b, 2048 + 16, 0xffffffff);
    ent(&mut b, 7 * 1024, ".", 0x8427, 3, 1);
    ent(&mut b, 7 * 1024 + 512, "..", 0x8427, 0, 0);
    ent(&mut b, 10 * 1024, "DATA", 0x8497, 12, 2);
    b
}
#[test]
fn reads_raw_and_ecc_cards() {
    let raw = fixture();
    let ecc: Vec<u8> = raw
        .chunks(512)
        .flat_map(|p| p.iter().copied().chain([0; 16]))
        .collect();
    for b in [raw, ecc] {
        let c = CardReader::open(&b).unwrap();
        let saves = c.saves().unwrap();
        assert_eq!(saves[0].name, "BASLUS-20946RMTEST");
        assert_eq!(saves[0].size, 12);
        assert!(c.free().unwrap() > 40 * 1024);
    }
}

#[test]
fn deleting_a_raw_save_reclaims_its_clusters_without_resizing_or_touching_payload_bytes() {
    let original = fixture();
    let path = std::env::temp_dir().join(format!("rm-delete-{}.ps2", stamp()));
    std::fs::write(&path, &original).unwrap();
    let before = CardReader::open(&original).unwrap().free().unwrap();
    delete_save_file(&path, "BASLUS-20946RMTEST").unwrap();
    let after = std::fs::read(&path).unwrap();
    let card = CardReader::open(&after).unwrap();
    assert!(card.saves().unwrap().is_empty());
    assert_eq!(card.free().unwrap(), before + 3 * 1024);
    assert_eq!(after.len(), original.len());
    for (offset, (&a, &b)) in original.iter().zip(&after).enumerate() {
        if a != b {
            assert!((2048..2560).contains(&offset) || (9 * 1024..9 * 1024 + 2).contains(&offset));
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn shared_or_cyclic_save_clusters_block_deletion_without_changing_the_image() {
    let mut original = fixture();
    original[2048 + 8..2048 + 12].copy_from_slice(&0x80000001u32.to_le_bytes());
    let path = std::env::temp_dir().join(format!("rm-delete-invalid-{}.ps2", stamp()));
    std::fs::write(&path, &original).unwrap();
    assert!(delete_save_file(&path, "BASLUS-20946RMTEST").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn rejects_cyclic_and_out_of_range_fat() {
    let mut b = fixture();
    b[2048..2052].copy_from_slice(&0x80000000u32.to_le_bytes());
    assert!(CardReader::open(&b).unwrap().saves().is_err());
    let mut b = fixture();
    b[60..64].copy_from_slice(&900u32.to_le_bytes());
    assert!(CardReader::open(&b).is_err());
}
#[test]
fn missing_tool_and_changed_card_preserve_source() {
    let root = std::env::temp_dir().join(format!("rm-a4-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("test.ps2");
    std::fs::write(&path, fixture()).unwrap();
    let before = fingerprint(&path).unwrap();
    assert!(run_tool(&root.join("db"), &path, "format", &[], &root).is_err());
    assert_eq!(before, fingerprint(&path).unwrap());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn publication_never_overwrites_an_existing_destination() {
    let root = std::env::temp_dir().join(format!("rm-publish-{}", stamp()));
    std::fs::create_dir_all(&root).unwrap();
    let src = root.join("source");
    let dst = root.join("target");
    std::fs::write(&src, b"new").unwrap();
    std::fs::write(&dst, b"personal").unwrap();
    assert!(publish_new(&src, &dst).is_err());
    assert_eq!(std::fs::read(&dst).unwrap(), b"personal");
    std::fs::remove_file(&dst).unwrap();
    publish_new(&src, &dst).unwrap();
    assert_eq!(std::fs::read(&dst).unwrap(), b"new");
    std::fs::remove_dir_all(root).unwrap();
}

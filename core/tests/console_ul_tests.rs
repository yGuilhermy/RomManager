use rom_core::console::ul::{Media, name_checksum, parse_index, serialize_index};

fn entry(name: &[u8], parts: u8, media: u8) -> [u8; 64] {
    let mut bytes = [0; 64];
    bytes[..name.len()].copy_from_slice(name);
    bytes[32..35].copy_from_slice(b"ul.");
    bytes[35..46].copy_from_slice(b"SLUS_213.55");
    bytes[47] = parts;
    bytes[48] = media;
    bytes[53] = 8;
    bytes
}

#[test]
fn empty_and_mixed_index_round_trip_without_changing_reserved_bytes() {
    assert!(parse_index(&[]).unwrap().is_empty());
    let mut cd = entry(b"CD fixture", 1, 0x12);
    cd[63] = 0xa5;
    let dvd = entry(b"DVD fixture", 2, 0x14);
    let bytes = [cd.as_slice(), dvd.as_slice()].concat();
    let games = parse_index(&bytes).unwrap();
    assert_eq!(games.len(), 2);
    assert_eq!(games[0].media(), Media::Cd);
    assert_eq!(games[1].media(), Media::Dvd);
    assert_eq!(games[1].startup(), "SLUS_213.55");
    assert_eq!(games[1].part_names().unwrap().len(), 2);
    assert_eq!(serialize_index(&games), bytes);
}

#[test]
fn full_width_names_need_no_terminator_and_crc_stops_at_first_nul() {
    let raw = entry(b"12345678901234567890123456789012", 255, 0x14);
    let games = parse_index(&raw).unwrap();
    assert_eq!(games[0].name_bytes().len(), 32);
    assert!(games[0].part_names().unwrap()[254].ends_with(".fe"));
    assert_eq!(
        name_checksum(b"Test\0ignored").unwrap(),
        name_checksum(b"Test").unwrap()
    );
    assert!(name_checksum(&[b'A'; 33]).is_err());
}

#[test]
fn checksum_matches_pinned_opl_reference_vectors() {
    for (name, expected) in [
        ("Test", 0x58c3_e2ce),
        ("CD fixture", 0xee5b_bbb8),
        ("DVD fixture", 0xb823_9645),
        ("12345678901234567890123456789012", 0x9a86_b8b2),
    ] {
        assert_eq!(name_checksum(name.as_bytes()).unwrap(), expected);
    }
    let game = parse_index(&entry(b"DVD fixture", 2, 0x14)).unwrap();
    assert_eq!(
        game[0].part_names().unwrap(),
        ["ul.B8239645.SLUS_213.55.00", "ul.B8239645.SLUS_213.55.01"]
    );
}

#[test]
fn malformed_index_does_not_return_a_partial_library() {
    for length in [1, 63, 65, 127] {
        assert!(parse_index(&vec![0; length]).is_err());
    }
    let good = entry(b"Fixture", 1, 0x14);
    for (offset, value) in [(32, b'x'), (35, b'/'), (39, b'/'), (47, 0), (48, 0xff)] {
        let mut bad = good;
        bad[offset] = value;
        assert!(
            parse_index(&[good.as_slice(), bad.as_slice()].concat()).is_err(),
            "offset {offset}"
        );
    }
}

#[test]
fn non_ascii_names_are_preserved_but_not_guessed_for_part_paths() {
    let raw = entry(b"Caf\xe9", 1, 0x14);
    let games = parse_index(&raw).unwrap();
    assert_eq!(games[0].name_bytes(), b"Caf\xe9");
    assert!(games[0].part_names().is_err());
    assert_eq!(serialize_index(&games), raw);
}

#[test]
fn unsafe_or_empty_startup_is_rejected() {
    let mut raw = entry(b"Fixture", 1, 0x14);
    raw[35..47].fill(0);
    assert!(parse_index(&raw).is_err());
    raw[35..47].copy_from_slice(b"../escape123");
    assert!(parse_index(&raw).is_err());
}

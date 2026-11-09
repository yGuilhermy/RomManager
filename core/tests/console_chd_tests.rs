use rom_core::console::chd::inspect;
use std::{fs, path::PathBuf};
fn file(tag: &[u8; 4], text: &[u8], unit: u32) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rm-chd-metadata-{}.chd",
        rom_core::memory_cards::stamp()
    ));
    let mut b = vec![0u8; 140 + text.len()];
    b[..8].copy_from_slice(b"MComprHD");
    b[8..12].copy_from_slice(&124u32.to_be_bytes());
    b[12..16].copy_from_slice(&5u32.to_be_bytes());
    b[32..40].copy_from_slice(&(u64::from(unit) * 20).to_be_bytes());
    b[48..56].copy_from_slice(&124u64.to_be_bytes());
    b[56..60].copy_from_slice(&(unit * 4).to_be_bytes());
    b[60..64].copy_from_slice(&unit.to_be_bytes());
    b[124..128].copy_from_slice(tag);
    b[128..132].copy_from_slice(&(text.len() as u32).to_be_bytes());
    b[140..].copy_from_slice(text);
    fs::write(&path, b).unwrap();
    path
}
#[test]
fn dvd_metadata_estimates_decompressed_size_without_using_compressed_file_size() {
    let path = file(b"DVD ", b"", 2048);
    let info = inspect(&path).unwrap();
    assert_eq!(info.representation, "dvd");
    assert_eq!(info.iso_bytes, Some(40960));
    assert!(info.blocked_reason.is_none());
    fs::remove_file(path).unwrap();
}
#[test]
fn cd_container_is_not_assumed_to_be_a_dvd_even_when_mode1() {
    let path = file(
        b"CHT2",
        b"TRACK:1 TYPE:MODE1 SUBTYPE:NONE FRAMES:20 PREGAP:0 PGTYPE:MODE1 PGSUB:NONE POSTGAP:0\0",
        2448,
    );
    let info = inspect(&path).unwrap();
    assert_eq!(info.representation, "cd_mode1");
    assert_eq!(info.iso_bytes, Some(40960));
    assert!(info.blocked_reason.is_some());
    fs::remove_file(path).unwrap();
}

#[test]
fn cd_frame_count_cannot_exceed_logical_sector_count() {
    let path = file(
        b"CHT2",
        b"TRACK:1 TYPE:MODE1 SUBTYPE:NONE FRAMES:21 PREGAP:0 POSTGAP:0",
        2448,
    );
    let result = inspect(&path);
    fs::remove_file(path).unwrap();
    assert!(
        result.is_err(),
        "21 frames cannot fit in a container with 20 logical units"
    );
}

#[test]
fn cd_with_unproven_gaps_or_unit_size_has_no_iso_estimate() {
    for (text, unit) in [
        (
            b"TRACK:1 TYPE:MODE1 SUBTYPE:NONE FRAMES:20 PREGAP:1 POSTGAP:0".as_slice(),
            2448,
        ),
        (
            b"TRACK:1 TYPE:MODE1 SUBTYPE:NONE FRAMES:20 PREGAP:0 POSTGAP:0".as_slice(),
            2048,
        ),
    ] {
        let path = file(b"CHT2", text, unit);
        let info = inspect(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(info.iso_bytes.is_none());
        assert!(info.blocked_reason.is_some());
    }
}
#[test]
fn parent_and_unknown_formats_are_not_prepared_as_standalone_iso() {
    let path = file(b"DVD ", b"", 2048);
    let mut b = fs::read(&path).unwrap();
    b[104] = 1;
    fs::write(&path, b).unwrap();
    assert!(
        inspect(&path)
            .unwrap()
            .blocked_reason
            .unwrap()
            .contains("parent")
    );
    fs::remove_file(path).unwrap();
    let path = file(b"XXXX", b"", 2048);
    assert_eq!(inspect(&path).unwrap().representation, "unknown");
    fs::remove_file(path).unwrap();
}
#[test]
fn metadata_loops_out_of_bounds_and_unsupported_versions_are_rejected() {
    for scenario in 0..3 {
        let path = file(b"DVD ", b"", 2048);
        let mut b = fs::read(&path).unwrap();
        match scenario {
            0 => b[132..140].copy_from_slice(&124u64.to_be_bytes()),
            1 => b[48..56].copy_from_slice(&999999u64.to_be_bytes()),
            _ => b[12..16].copy_from_slice(&4u32.to_be_bytes()),
        };
        fs::write(&path, b).unwrap();
        assert!(inspect(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn mixed_dvd_cd_metadata_is_rejected_and_multitrack_is_explicitly_blocked() {
    for mixed in [true, false] {
        let path = file(
            if mixed { b"DVD " } else { b"CHT2" },
            b"TRACK:1 TYPE:MODE1 SUBTYPE:NONE FRAMES:20",
            2448,
        );
        let mut bytes = fs::read(&path).unwrap();
        let next = bytes.len() as u64;
        bytes[132..140].copy_from_slice(&next.to_be_bytes());
        let text = b"TRACK:2 TYPE:AUDIO SUBTYPE:NONE FRAMES:20";
        bytes.extend_from_slice(b"CHT2");
        bytes.extend_from_slice(&(text.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&0u64.to_be_bytes());
        bytes.extend_from_slice(text);
        fs::write(&path, bytes).unwrap();
        if mixed {
            assert!(inspect(&path).is_err());
        } else {
            let info = inspect(&path).unwrap();
            assert_eq!(info.representation, "cd_tracks");
            assert!(info.iso_bytes.is_none());
            assert!(info.blocked_reason.is_some());
        }
        fs::remove_file(path).unwrap();
    }
}

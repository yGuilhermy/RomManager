use rom_core::save_formats::psv_to_psu;

fn sample() -> Vec<u8> {
    let mut b = vec![0u8; 224];
    b[..4].copy_from_slice(b"\0VSP");
    b[56..60].copy_from_slice(&44u32.to_le_bytes());
    b[60..64].copy_from_slice(&2u32.to_le_bytes());
    b[100..104].copy_from_slice(&1u32.to_le_bytes());
    b[120..124].copy_from_slice(&3u32.to_le_bytes());
    b[124..128].copy_from_slice(&0x8427u32.to_le_bytes());
    b[128..142].copy_from_slice(b"BASLUS-21488QA");
    b[176..180].copy_from_slice(&4u32.to_le_bytes());
    b[180..184].copy_from_slice(&0x8497u32.to_le_bytes());
    b[184..188].copy_from_slice(b"DATA");
    b[216..220].copy_from_slice(&220u32.to_le_bytes());
    b[220..].copy_from_slice(b"save");
    b
}
#[test]
fn psv_conversion_preserves_ps2_payload_and_metadata() {
    let b = sample();
    let psu = psv_to_psu(&b).unwrap();
    assert_eq!(&psu[64..78], b"BASLUS-21488QA");
    assert_eq!(&psu[1536 + 64..1536 + 68], b"DATA");
    assert_eq!(&psu[2048..2052], b"save");
    assert_eq!(psu.len(), 3072);
}
#[test]
fn psv_rejects_wrong_platform_offsets_truncation_and_unsafe_names() {
    let mut b = sample();
    b[60] = 1;
    assert!(psv_to_psu(&b).is_err());
    let mut b = sample();
    b[216..220].copy_from_slice(&10u32.to_le_bytes());
    assert!(psv_to_psu(&b).is_err());
    assert!(psv_to_psu(&sample()[..223]).is_err());
    let mut b = sample();
    b[184..189].copy_from_slice(b"../xx");
    assert!(psv_to_psu(&b).is_err());
}

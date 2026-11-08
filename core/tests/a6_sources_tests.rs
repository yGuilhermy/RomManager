use rom_core::{api_roms, catalog_refresh, extras_sources};
#[test]
fn source_serials_and_cheat_tables_do_not_guess_missing_editions() {
    assert_eq!(
        extras_sources::normalized_serial("SCUS_973.16").as_deref(),
        Some("SCUS-97316")
    );
    assert!(extras_sources::normalized_serial("éé12345").is_none());
    assert!(extras_sources::normalized_serial("../../SCUS97316").is_none());
    let entries = extras_sources::parse_cheat_table(
        "| Game Title | Serial | CRC |\n| Game A | SLUS-12345 | ABCD1234 |\n| Unknown | None | 01234567 |\n| bad | XX | nope |",
    );
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].1, "");
}
#[test]
fn catalog_metadata_preserves_stable_ids_and_rejects_traversal_and_empty_snapshots() {
    let part = "PS2-1G1R-P1-1G1R";
    let rows=catalog_refresh::part_items(part,br#"{"files":[{"name":"Game (USA).chd","size":"1234"},{"name":"metadata.xml","size":"8"}]}"#).unwrap();
    assert_eq!(rows[0].id, format!("{part}:Game (USA).chd"));
    assert_eq!(rows[0].region, "USA");
    assert!(
        catalog_refresh::part_items(part, br#"{"files":[{"name":"../escape.chd","size":"10"}]}"#)
            .is_err()
    );
    assert!(catalog_refresh::part_items(part, br#"{"files":[]}"#).is_err());
    assert!(catalog_refresh::validate(&[rows[0].clone(), rows[0].clone()]).is_err());
    let query =
        api_roms::query_catalog_items(&rows, Some("Game"), None, None, None, None, None, 1, 24);
    assert_eq!(query.total, 1);
}
#[test]
fn native_pnach_metadata_and_inline_comments_are_accepted_without_arbitrary_directives() {
    use rom_core::game_resources::validate_pnach;
    assert!(validate_pnach("[Widescreen]\ngsaspectratio=16:9\ngsinterlacemode=1\npatch=1,EE,00100000,word,00000000 // explanation\n").is_ok());
    assert!(validate_pnach("gsaspectratio=1:0\npatch=1,EE,00100000,word,00000000").is_err());
    assert!(validate_pnach("exec=malicious.exe\npatch=1,EE,00100000,word,00000000").is_err());
}

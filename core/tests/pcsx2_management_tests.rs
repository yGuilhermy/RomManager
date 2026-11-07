use rom_core::database::{initialize_db};
use rom_core::exe_runner::{launch_pcsx2_with_args, validate_pcsx2_executable};
use rom_core::pcsx2::{compute_sha1, detect_installations, export_covers_to_pcsx2, extract_pcsx2_version, identify_bios, import_bios_archive, import_bios_file, inspect_installation, parse_scph_model, run_diagnostic, sanitize_path, scan_bios_directory};
use std::fs;
use std::path::Path;

#[test]
fn sha1_calculation_matches_known_test_vectors() {
    let empty_sha1 = compute_sha1(b"");
    assert_eq!(empty_sha1, "da39a3ee5e6b4b0d3255bfef95601890afd80709");

    let quick_brown_fox = compute_sha1(b"The quick brown fox jumps over the lazy dog");
    assert_eq!(quick_brown_fox, "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");
}

#[tokio::test]
async fn pcsx2_installation_detection_and_portable_mode() {
    let test_dir = Path::new("target/test_pcsx2_install");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let portable_dir = test_dir.join("portable_pcsx2");
    fs::create_dir_all(&portable_dir).expect("Create portable dir");
    let exe_path = portable_dir.join("pcsx2-qt.exe");
    fs::write(&exe_path, b"MZ_MOCK_EXE").expect("Write mock exe");
    fs::write(portable_dir.join("portable.ini"), b"").expect("Write portable ini");
    fs::create_dir_all(portable_dir.join("inis")).expect("Create inis dir");
    fs::create_dir_all(portable_dir.join("bios")).expect("Create bios dir");

    let inspected = inspect_installation(&exe_path).expect("Inspect should succeed");
    assert!(inspected.is_portable);
    assert!(!inspected.is_managed);
    assert_eq!(inspected.path, exe_path.to_string_lossy());
    assert_eq!(inspected.data_dir, portable_dir.to_string_lossy());
    assert_eq!(
        inspected.bios_dir,
        portable_dir.join("bios").to_string_lossy()
    );

    let detected = detect_installations(Some(&exe_path.to_string_lossy()), Some(test_dir));
    assert!(
        detected
            .iter()
            .any(|i| i.path == exe_path.to_string_lossy())
    );

    let invalid_path = test_dir.join("nonexistent_pcsx2.exe");
    let err = inspect_installation(&invalid_path);
    assert!(err.is_err());
}

#[tokio::test]
async fn bios_scanning_identification_and_duplicate_detection() {
    let test_dir = Path::new("target/test_pcsx2_bios");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let bios_dir = test_dir.join("bios");
    fs::create_dir_all(&bios_dir).expect("Create bios dir");

    let mut bios_content = vec![0u8; 4 * 1024 * 1024];
    bios_content[0..16].copy_from_slice(b"PS2_BIOS_USA_001");

    let bios_file1 = bios_dir.join("SCPH-39001_USA_v01.60.bin");
    fs::write(&bios_file1, &bios_content).expect("Write bios 1");

    let bios_file2_duplicate = bios_dir.join("ps2-usa-copy.bin");
    fs::write(&bios_file2_duplicate, &bios_content).expect("Write duplicate bios");

    let mut bios_eu_content = vec![0u8; 4 * 1024 * 1024];
    bios_eu_content[0..16].copy_from_slice(b"PS2_BIOS_EUR_002");
    let bios_eu = bios_dir.join("SCPH-70004_EUR_v02.00.bin");
    fs::write(&bios_eu, &bios_eu_content).expect("Write bios EU");

    let scanned = scan_bios_directory(&bios_dir, Some(&bios_file1.to_string_lossy()));
    assert_eq!(scanned.len(), 3);

    let usa1 = scanned
        .iter()
        .find(|b| b.filename == "SCPH-39001_USA_v01.60.bin")
        .unwrap();
    assert_eq!(usa1.region, "USA / NTSC-U");
    assert_eq!(usa1.model, "SCPH-39001");
    assert_eq!(usa1.version, "v01.60");
    assert!(usa1.is_valid);
    assert!(usa1.is_active);
    assert!(usa1.is_duplicate);

    let usa_dup = scanned
        .iter()
        .find(|b| b.filename == "ps2-usa-copy.bin")
        .unwrap();
    assert_eq!(usa_dup.sha1, usa1.sha1);
    assert!(usa_dup.is_duplicate);
    assert!(!usa_dup.is_active);

    let eu = scanned
        .iter()
        .find(|b| b.filename == "SCPH-70004_EUR_v02.00.bin")
        .unwrap();
    assert_eq!(eu.region, "Europe / PAL");
    assert_eq!(eu.model, "SCPH-70004");
    assert_eq!(eu.version, "v02.00");
    assert!(!eu.is_duplicate);

    let (m, r, v) = identify_bios("scph10000.bin", "ad039130da4f9ffab03b4122d4f20ecb2ce43c08");
    assert_eq!(m, "SCPH-10000");
    assert_eq!(r, "Japan / NTSC-J");
    assert_eq!(v, "v01.00");
}

#[tokio::test]
async fn bios_import_validates_size_and_prevents_duplicate_content() {
    let test_dir = Path::new("target/test_pcsx2_bios_import");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let bios_dir = test_dir.join("installed_bios");
    let staging_dir = test_dir.join("user_files");
    fs::create_dir_all(&staging_dir).expect("Create staging dir");

    let invalid_small_file = staging_dir.join("corrupted.bin");
    fs::write(&invalid_small_file, b"TOO_SMALL").expect("Write small file");

    let err_small = import_bios_file(&bios_dir, &invalid_small_file);
    assert!(err_small.is_err());
    assert!(err_small.unwrap_err().contains("tamanho"));

    let mut valid_content = vec![0u8; 4 * 1024 * 1024];
    valid_content[0..13].copy_from_slice(b"ORIGINAL_BIOS");
    let user_bios = staging_dir.join("My Dumped BIOS (USA).bin");
    fs::write(&user_bios, &valid_content).expect("Write valid user bios");

    let imported = import_bios_file(&bios_dir, &user_bios).expect("Import valid bios");
    assert_eq!(imported.size, 4 * 1024 * 1024);
    assert!(bios_dir.join(&imported.filename).exists());

    let user_bios_rename = staging_dir.join("Renamed Duplicate.bin");
    fs::write(&user_bios_rename, &valid_content).expect("Write duplicate");

    let err_dup = import_bios_file(&bios_dir, &user_bios_rename);
    assert!(err_dup.is_err());
    assert!(err_dup.unwrap_err().contains("BIOS duplicada"));
}

#[tokio::test]
async fn covers_export_to_pcsx2_directory_maps_serials_and_preserves_custom_covers() {
    let test_dir = Path::new("target/test_pcsx2_covers_export");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let db_path = test_dir.join("library.db");
    let mut conn = initialize_db(&db_path).expect("Init db");

    let roms_dir = test_dir.join("roms");
    let covers_dir = test_dir.join("covers");
    let pcsx2_covers_dir = test_dir.join("pcsx2_covers");
    fs::create_dir_all(&roms_dir).expect("Create roms");
    fs::create_dir_all(&covers_dir).expect("Create covers");
    fs::create_dir_all(&pcsx2_covers_dir).expect("Create pcsx2 covers");

    let _ = conn.execute(
        "INSERT INTO configs (key, value) VALUES ('roms_dir', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![roms_dir.to_string_lossy().to_string()],
    );
    let _ = conn.execute(
        "INSERT INTO configs (key, value) VALUES ('covers_dir', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![covers_dir.to_string_lossy().to_string()],
    );

    let rom1 = roms_dir.join("Gran Turismo 4 (USA).iso");
    fs::write(&rom1, b"ISO_DATA_HEADER").expect("Write rom 1");
    rom_core::rom_scanner::scan_roms_directory(&mut conn, &roms_dir, None)
        .await
        .expect("Scan");

    let _ = conn.execute(
        "UPDATE games SET serial = 'SCUS-97328' WHERE filename LIKE '%Gran Turismo%'",
        [],
    );

    let source_cover = covers_dir.join("Gran Turismo 4.jpg");
    fs::write(&source_cover, b"JPEG_COVER_CONTENT").expect("Write cover");

    let result = export_covers_to_pcsx2(&conn, &pcsx2_covers_dir, false).expect("Export covers");
    assert_eq!(result.exported, 1);
    assert_eq!(result.preserved, 0);

    let target_cover = pcsx2_covers_dir.join("SCUS-97328.jpg");
    assert!(target_cover.exists());
    assert_eq!(fs::read(&target_cover).unwrap(), b"JPEG_COVER_CONTENT");

    fs::write(&target_cover, b"USER_CUSTOM_HIGH_RES_ART").expect("User custom cover");

    let result_preserved =
        export_covers_to_pcsx2(&conn, &pcsx2_covers_dir, false).expect("Export without overwrite");
    assert_eq!(result_preserved.preserved, 1);
    assert_eq!(result_preserved.exported, 0);
    assert_eq!(
        fs::read(&target_cover).unwrap(),
        b"USER_CUSTOM_HIGH_RES_ART"
    );

    let result_overwrite =
        export_covers_to_pcsx2(&conn, &pcsx2_covers_dir, true).expect("Export with overwrite");
    assert_eq!(result_overwrite.exported, 1);
    assert_eq!(fs::read(&target_cover).unwrap(), b"JPEG_COVER_CONTENT");
}

#[tokio::test]
async fn diagnostic_reports_errors_for_missing_paths_and_alerts_when_pcsx2_missing() {
    let test_dir = Path::new("target/test_pcsx2_diagnostic");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let db_path = test_dir.join("diagnostic.db");
    let conn = initialize_db(&db_path).expect("Init db");

    let _ = conn.execute(
        "UPDATE configs SET value = 'C:\\NonexistentFolder\\pcsx2.exe' WHERE key IN ('pcsx2_path', 'pcsx2_executable_path')",
        [],
    );

    let diag = run_diagnostic(&conn).expect("Run diagnostic");
    assert!(!diag.executable_valid);
    assert!(!diag.roms_dir_valid);
    assert!(diag.errors.iter().any(|e| e.contains("não foi encontrado")));
    assert!(diag.errors.iter().any(|e| e.contains("ROMs")));
}

#[tokio::test]
async fn generic_scph_identification_and_corrupted_fallback() {
    assert_eq!(parse_scph_model("scph50001.bin").unwrap(), "SCPH-50001");
    assert_eq!(parse_scph_model("SCPH-70012.bin").unwrap(), "SCPH-70012");
    assert_eq!(parse_scph_model("scph_30004r.bin").unwrap(), "SCPH-30004R");
    assert_eq!(parse_scph_model("corrupted_dump.bin"), None);

    let (m1, r1, v1) = identify_bios("scph50001.bin", "");
    assert_eq!(m1, "SCPH-50001");
    assert_eq!(r1, "USA / NTSC-U");
    assert_eq!(v1, "v01.90");

    let (m2, r2, v2) = identify_bios("SCPH-70012.bin", "");
    assert_eq!(m2, "SCPH-70012");
    assert_eq!(r2, "USA / NTSC-U");
    assert_eq!(v2, "v02.00");

    let (mf, rf, vf) = identify_bios(
        "corrupted_dump.bin",
        "0000000000000000000000000000000000000000",
    );
    assert_eq!(mf, "PlayStation 2 BIOS");
    assert_eq!(rf, "Global / Outro");
    assert_eq!(vf, "v01.xx");
}

#[tokio::test]
async fn launch_pcsx2_validates_and_handles_missing_files() {
    let test_dir = Path::new("target/test_pcsx2_launch");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let fake_exe = test_dir.join("not_pcsx2.txt");
    fs::write(&fake_exe, b"NOT_EXE").unwrap();
    let fake_rom = test_dir.join("game.iso");
    fs::write(&fake_rom, b"ISO").unwrap();

    let err_invalid_exe = launch_pcsx2_with_args(&fake_exe, &fake_rom, true);
    assert!(err_invalid_exe.is_err());
    assert!(
        err_invalid_exe
            .unwrap_err()
            .contains("não foi encontrado ou é inválido")
    );
    assert!(!validate_pcsx2_executable(&fake_exe));

    let valid_exe_path = test_dir.join("pcsx2-qt.exe");
    fs::write(&valid_exe_path, b"MZ_EXE").unwrap();
    assert!(validate_pcsx2_executable(&valid_exe_path));
    let missing_rom = test_dir.join("missing.iso");
    let err_missing_rom = launch_pcsx2_with_args(&valid_exe_path, &missing_rom, false);
    assert!(err_missing_rom.is_err());
    assert!(err_missing_rom.unwrap_err().contains("não existe no disco"));
}

#[tokio::test]
async fn version_detection_extracts_real_pe_or_reports_unknown() {
    let test_dir = Path::new("target/test_pcsx2_version_detection");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let exe_with_filename_ver = test_dir.join("pcsx2-v2.1.0-windows-x64-Qt.exe");
    fs::write(&exe_with_filename_ver, b"MZ_RAW_BIN").unwrap();
    let ver_filename = extract_pcsx2_version(&exe_with_filename_ver);
    assert_eq!(ver_filename, "PCSX2 v2.1.0");

    let mut pe_mock_bytes = vec![0u8; 1024];
    let tag = [
        b'P', 0, b'r', 0, b'o', 0, b'd', 0, b'u', 0, b'c', 0, b't', 0, b'V', 0, b'e', 0, b'r', 0,
        b's', 0, b'i', 0, b'o', 0, b'n', 0,
    ];
    pe_mock_bytes[100..100 + tag.len()].copy_from_slice(&tag);
    let ver_bytes = [
        b'2', 0, b'.', 0, b'8', 0, b'.', 0, b'2', 0, b'.', 0, b'0', 0, 0, 0,
    ];
    pe_mock_bytes[100 + tag.len() + 2..100 + tag.len() + 2 + ver_bytes.len()]
        .copy_from_slice(&ver_bytes);
    let pe_exe = test_dir.join("pcsx2-qt.exe");
    fs::write(&pe_exe, &pe_mock_bytes).unwrap();
    let ver_pe = extract_pcsx2_version(&pe_exe);
    assert_eq!(ver_pe, "PCSX2 v2.8.2.0");

    let unknown_exe = test_dir.join("pcsx2_generic.exe");
    fs::write(&unknown_exe, b"MZ_NOTHING_SPECIAL").unwrap();
    let ver_unknown = extract_pcsx2_version(&unknown_exe);
    assert_eq!(ver_unknown, "Versão desconhecida");
}

#[tokio::test]
async fn bios_import_rejects_invalid_file_extensions() {
    let test_dir = Path::new("target/test_pcsx2_bios_ext");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let bios_dir = test_dir.join("bios");
    let invalid_script = test_dir.join("payload.bat");
    fs::write(&invalid_script, vec![0u8; 1024 * 1024]).unwrap();

    let err = import_bios_file(&bios_dir, &invalid_script);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("Extensão inválida"));
}

#[tokio::test]
async fn covers_export_resolves_encoded_url_and_formats_serial() {
    let test_dir = Path::new("target/test_pcsx2_covers_encoded");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let db_path = test_dir.join("covers_encoded.db");
    let mut conn = initialize_db(&db_path).expect("Init db");

    let roms_dir = test_dir.join("roms");
    let covers_dir = test_dir.join("covers");
    let pcsx2_covers_dir = test_dir.join("pcsx2_covers");
    fs::create_dir_all(&roms_dir).unwrap();
    fs::create_dir_all(&covers_dir).unwrap();
    fs::create_dir_all(&pcsx2_covers_dir).unwrap();

    let _ = conn.execute(
        "INSERT INTO configs (key, value) VALUES ('roms_dir', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![roms_dir.to_string_lossy().to_string()],
    );
    let _ = conn.execute(
        "INSERT INTO configs (key, value) VALUES ('covers_dir', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![covers_dir.to_string_lossy().to_string()],
    );

    let rom = roms_dir.join("Shadow of the Colossus.iso");
    fs::write(&rom, b"ISO_HEADER").unwrap();
    rom_core::rom_scanner::scan_roms_directory(&mut conn, &roms_dir, None)
        .await
        .expect("Scan");

    let cover_file = covers_dir.join("Shadow of the Colossus.jpg");
    fs::write(&cover_file, b"COVER_DATA_JPEG").unwrap();

    let _ = conn.execute(
        "UPDATE games SET serial = 'SCUS97472', cover_url = 'http://127.0.0.1:3000/api/covers/serve?file=Shadow%20of%20the%20Colossus.jpg' WHERE filename LIKE '%Shadow%'",
        [],
    );

    let res = export_covers_to_pcsx2(&conn, &pcsx2_covers_dir, false).expect("Export");
    assert_eq!(res.exported, 1);
    assert!(pcsx2_covers_dir.join("SCUS97472.jpg").exists());
    assert!(pcsx2_covers_dir.join("SCUS-97472.jpg").exists());
}

#[tokio::test]
async fn sanitize_path_trims_quotes_and_whitespace() {
    assert_eq!(
        sanitize_path("  \"C:\\Games\\test.iso\"  "),
        "C:\\Games\\test.iso"
    );
    assert_eq!(
        sanitize_path("'D:\\Emulators\\pcsx2.exe'"),
        "D:\\Emulators\\pcsx2.exe"
    );
}

#[tokio::test]
async fn zip_with_multiple_bios_files_is_inspected_and_imported_reporting_count() {
    use std::io::Write;
    let test_dir = Path::new("target/test_pcsx2_bios_zip");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let bios_dir = test_dir.join("installed_bios");
    let staging_dir = test_dir.join("user_zips");
    fs::create_dir_all(&staging_dir).expect("Create staging dir");

    let zip_file_path = staging_dir.join("ps2_bios_pack.zip");
    let zip_file = fs::File::create(&zip_file_path).expect("Create zip file");
    let mut zip_writer = zip::ZipWriter::new(zip_file);
    let options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let mut bios1_data = vec![0u8; 4 * 1024 * 1024];
    bios1_data[0..14].copy_from_slice(b"ZIP_BIOS_USA_1");
    zip_writer
        .start_file("SCPH-39001.bin", options)
        .expect("Start file 1");
    zip_writer.write_all(&bios1_data).expect("Write file 1");

    let mut bios2_data = vec![0u8; 4 * 1024 * 1024];
    bios2_data[0..14].copy_from_slice(b"ZIP_BIOS_EUR_2");
    zip_writer
        .start_file("SCPH-70004.bin", options)
        .expect("Start file 2");
    zip_writer.write_all(&bios2_data).expect("Write file 2");

    zip_writer
        .start_file("readme.txt", options)
        .expect("Start readme");
    zip_writer
        .write_all(b"PS2 Bios instructions")
        .expect("Write readme");

    zip_writer.finish().expect("Finish zip");

    let result = import_bios_archive(&bios_dir, &zip_file_path).expect("Import bios zip");
    assert_eq!(result.count, 2);
    assert_eq!(result.items.len(), 2);
    assert!(bios_dir.join("SCPH-39001.bin").exists());
    assert!(bios_dir.join("SCPH-70004.bin").exists());

    let single_via_zip = import_bios_file(&bios_dir, &zip_file_path);
    assert!(single_via_zip.is_err());
}

#[tokio::test]
async fn zip_without_valid_bios_files_is_rejected() {
    use std::io::Write;
    let test_dir = Path::new("target/test_pcsx2_bios_invalid_zip");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let bios_dir = test_dir.join("installed_bios");
    let zip_file_path = test_dir.join("not_bios.zip");
    let zip_file = fs::File::create(&zip_file_path).expect("Create zip file");
    let mut zip_writer = zip::ZipWriter::new(zip_file);
    let options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip_writer
        .start_file("document.txt", options)
        .expect("Start doc");
    zip_writer.write_all(b"Not a bios dump").expect("Write doc");
    zip_writer.finish().expect("Finish zip");

    let err = import_bios_archive(&bios_dir, &zip_file_path);
    assert!(err.is_err());
    assert!(
        err.unwrap_err()
            .contains("não contém nenhum arquivo de BIOS")
    );
}

#[tokio::test]
async fn active_bios_is_sorted_at_top_of_list() {
    let test_dir = Path::new("target/test_pcsx2_bios_active_top");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("Create test dir");

    let bios_dir = test_dir.join("bios");
    fs::create_dir_all(&bios_dir).expect("Create bios dir");

    let mut b1 = vec![0u8; 4 * 1024 * 1024];
    b1[0..10].copy_from_slice(b"BIOS_AAA_1");
    let path_a = bios_dir.join("SCPH-10000.bin");
    fs::write(&path_a, &b1).expect("Write bios a");

    let mut b2 = vec![0u8; 4 * 1024 * 1024];
    b2[0..10].copy_from_slice(b"BIOS_ZZZ_2");
    let path_z = bios_dir.join("SCPH-90006.bin");
    fs::write(&path_z, &b2).expect("Write bios z");

    let scanned = scan_bios_directory(&bios_dir, Some(&path_z.to_string_lossy()));
    assert_eq!(scanned.len(), 2);
    assert_eq!(scanned[0].filename, "SCPH-90006.bin");
    assert!(scanned[0].is_active);
    assert!(!scanned[1].is_active);
}

use super::*;

#[test]
fn validation_wide_diagnostic_budget_is_deterministic() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    write_minimal_workspace(&fixture)?;
    for index in 0..300 {
        fixture.write(format!("extra-{index:03}.txt"), b"")?;
    }

    let first = validate_workspace(&fixture.root);
    let second = validate_workspace(&fixture.root);
    assert_eq!(first.diagnostics, second.diagnostics);
    assert_eq!(first.diagnostics.len(), 256);
    assert_eq!(
        first.diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1004").count(),
        255
    );
    assert_eq!(
        first.diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1204").count(),
        1
    );
    assert!(
        first.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("diagnostic budget"))
    );
    Ok(())
}

#[test]
fn component_registry_is_bounded_before_scanning() {
    let mut contract = fixture_contract();
    contract.adapters.clear();
    contract.members = (0..257)
        .map(|index| MemberContract {
            id: format!("component-{index}"),
            root: format!("crates/component-{index}"),
            kind: MemberKind::Foundation,
            dependencies: Vec::new(),
            allowed_entries: Vec::new(),
        })
        .collect();
    let mut diagnostics = ValidationDiagnostics::default();

    validate_contract_identity(&contract, &mut diagnostics);

    assert!(diagnostics.into_vec().iter().any(|diagnostic| {
        diagnostic.code == "ZRYNA-A1001" && diagnostic.message.contains("257 components")
    }));
}

#[test]
fn expected_size_and_contract_source_bind_the_scan_snapshot() -> Result<(), Box<dyn Error>> {
    let size_fixture = TempFixture::new()?;
    let controlled = size_fixture.write("controlled.zry", b"first")?;
    let expected_size = fs::metadata(&controlled)?.len();
    fs::write(&controlled, b"larger")?;
    let size_result = read_bounded_utf8_with_expected_size(
        &controlled,
        Some(Path::new("controlled.zry")),
        32,
        "ZRYNA-A1203",
        "restore the file",
        expected_size,
    );
    assert!(matches!(size_result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1203"));

    let contract_fixture = TempFixture::new()?;
    contract_fixture.write("zryna.workspace.json", b"contract-b")?;
    let contract = fixture_contract();
    let policy = ScanPolicy::new(&contract, "contract-a");
    let mut scan_diagnostics = ValidationDiagnostics::default();
    let mut state = ScanState::new(FIXTURE_LIMITS);
    scan_path(
        &contract_fixture.root,
        &contract_fixture.root,
        &policy,
        0,
        &mut state,
        &mut scan_diagnostics,
    );
    assert!(state.halted);
    assert!(has_code(&scan_diagnostics.into_vec(), "ZRYNA-A1203"));

    let mut final_diagnostics = ValidationDiagnostics::default();
    validate_contract_unchanged(&contract_fixture.root, "contract-a", &mut final_diagnostics);
    assert!(has_code(&final_diagnostics.into_vec(), "ZRYNA-A1203"));
    Ok(())
}

#[test]
fn excludes_only_declared_generated_directories() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    fixture.write("target/not-utf8.bin", &[0xff])?;
    fixture.write(".zryna/cache/not-utf8.bin", &[0xff])?;
    fixture.write(".zryna/out/not-utf8.bin", &[0xff])?;
    fixture.write("node_modules/not-utf8.bin", &[0xff])?;
    fixture.write("adapters/typescript-6/node_modules/not-utf8.bin", &[0xff])?;

    let (completed, diagnostics) = scan_fixture(&fixture.root, FIXTURE_LIMITS);
    assert!(completed);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:#?}");
    Ok(())
}

#[test]
fn inspects_nested_generated_names_and_unknown_output_siblings() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    fixture.write("crates/example/src/target/not-utf8.bin", &[0xff])?;
    fixture.write(".zryna/other/not-utf8.bin", &[0xff])?;

    let (_, diagnostics) = scan_fixture(&fixture.root, FIXTURE_LIMITS);
    assert_eq!(diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1203").count(), 2);
    Ok(())
}

#[test]
fn rejects_invalid_utf8_and_oversized_regular_files() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    fixture.write("invalid.zry", &[0xff])?;
    fixture.write("large.zry", b"12345")?;
    fixture.write("z-after.zry", &[0xff])?;
    let limits = ScanLimits { file_bytes: 4, ..FIXTURE_LIMITS };

    let (completed, diagnostics) = scan_fixture(&fixture.root, limits);
    assert!(!completed);
    assert_eq!(diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1203").count(), 1);
    assert!(has_code(&diagnostics, "ZRYNA-A1204"));
    Ok(())
}

#[test]
fn aggregate_entry_and_depth_budgets_halt_globally() -> Result<(), Box<dyn Error>> {
    let aggregate = TempFixture::new()?;
    aggregate.write("a.zry", b"123")?;
    aggregate.write("b.zry", b"456")?;
    let (_, aggregate_diagnostics) =
        scan_fixture(&aggregate.root, ScanLimits { total_bytes: 5, ..FIXTURE_LIMITS });
    assert_eq!(aggregate_diagnostics.len(), 1);
    assert_eq!(aggregate_diagnostics[0].code, "ZRYNA-A1204");

    let invalid_aggregate = TempFixture::new()?;
    invalid_aggregate.write("a.zry", &[0xff, 0xff, 0xff])?;
    invalid_aggregate.write("b.zry", &[0xff, 0xff, 0xff])?;
    let (invalid_completed, invalid_diagnostics) =
        scan_fixture(&invalid_aggregate.root, ScanLimits { total_bytes: 5, ..FIXTURE_LIMITS });
    assert!(!invalid_completed);
    assert!(has_code(&invalid_diagnostics, "ZRYNA-A1203"));
    assert!(has_code(&invalid_diagnostics, "ZRYNA-A1204"));

    let entries = TempFixture::new()?;
    entries.write("a.zry", b"a")?;
    entries.write("b.zry", b"b")?;
    let (entry_completed, entry_diagnostics) =
        scan_fixture(&entries.root, ScanLimits { entries: 2, ..FIXTURE_LIMITS });
    assert!(!entry_completed);
    assert_eq!(entry_diagnostics.len(), 1);
    assert_eq!(entry_diagnostics[0].code, "ZRYNA-A1204");

    let depth = TempFixture::new()?;
    depth.write("a/b/value.zry", b"value")?;
    let (depth_completed, depth_diagnostics) =
        scan_fixture(&depth.root, ScanLimits { depth: 1, ..FIXTURE_LIMITS });
    assert!(!depth_completed);
    assert_eq!(depth_diagnostics.len(), 1);
    assert_eq!(depth_diagnostics[0].code, "ZRYNA-A1204");
    Ok(())
}

#[test]
fn diagnostic_budget_reserves_one_terminal_diagnostic() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    fixture.write("a.zry", &[0xff])?;
    fixture.write("b.zry", &[0xff])?;
    let (completed, diagnostics) =
        scan_fixture(&fixture.root, ScanLimits { diagnostics: 1, ..FIXTURE_LIMITS });

    assert!(!completed);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "ZRYNA-A1204");
    assert!(diagnostics[0].message.contains("diagnostic budget"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_symlink_and_socket_at_excluded_paths() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;

    let symlink_fixture = TempFixture::new()?;
    let destination = symlink_fixture.directory("real-target")?;
    symlink(&destination, symlink_fixture.path("target"))?;
    let (_, symlink_diagnostics) = scan_fixture(&symlink_fixture.root, FIXTURE_LIMITS);
    assert!(has_code(&symlink_diagnostics, "ZRYNA-A1201"));

    let socket_fixture = TempFixture::new()?;
    let _listener = UnixListener::bind(socket_fixture.path("target"))?;
    let (_, socket_diagnostics) = scan_fixture(&socket_fixture.root, FIXTURE_LIMITS);
    assert!(has_code(&socket_diagnostics, "ZRYNA-A1201"));
    Ok(())
}

#[cfg(windows)]
#[test]
fn rejects_windows_directory_reparse_points() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    let destination = fixture.directory("real-target")?;
    let junction = fixture.path("target");
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&destination)
        .status()?;
    assert!(status.success(), "failed to create the junction fixture");
    let (_, diagnostics) = scan_fixture(&fixture.root, FIXTURE_LIMITS);
    assert!(has_code(&diagnostics, "ZRYNA-A1201"));
    Ok(())
}

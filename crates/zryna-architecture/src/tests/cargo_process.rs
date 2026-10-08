use super::*;

#[test]
fn bounded_process_reader_drains_without_growing_past_limit() -> Result<(), Box<dyn Error>> {
    let input = vec![b'x'; 32 * 1024];

    let output = read_process_stream(std::io::Cursor::new(input), 1024)?;

    assert_eq!(output.bytes.len(), 1024);
    assert!(output.exceeded);
    Ok(())
}

#[test]
fn cargo_metadata_output_limits_and_format_fail_closed() {
    let clean = BoundedProcessStream { bytes: Vec::new(), exceeded: false };
    let oversized = BoundedProcessStream { bytes: Vec::new(), exceeded: true };
    let mut oversized_diagnostics = ValidationDiagnostics::default();
    let oversized_result = validate_cargo_process_output(
        successful_exit_status(),
        &oversized,
        &clean,
        &mut oversized_diagnostics,
    );
    assert!(oversized_result.is_none());
    assert!(has_code(&oversized_diagnostics.into_vec(), "ZRYNA-A1204"));

    let unsupported = BoundedProcessStream {
        bytes: br#"{"version":2,"packages":[],"workspace_members":[],"workspace_root":".","resolve":{"nodes":[]}}"#
            .to_vec(),
        exceeded: false,
    };
    let mut format_diagnostics = ValidationDiagnostics::default();
    let format_result = validate_cargo_process_output(
        successful_exit_status(),
        &unsupported,
        &clean,
        &mut format_diagnostics,
    );
    assert!(format_result.is_none());
    assert!(has_code(&format_diagnostics.into_vec(), "ZRYNA-A1101"));
}

#[test]
fn cargo_metadata_package_and_edge_budgets_fail_closed() {
    let mut package_heavy = empty_metadata();
    package_heavy.packages = (0..=MAX_CARGO_PACKAGES).map(metadata_package).collect();
    let mut package_diagnostics = ValidationDiagnostics::default();
    assert!(!validate_cargo_metadata_limits(&package_heavy, &mut package_diagnostics));
    assert!(has_code(&package_diagnostics.into_vec(), "ZRYNA-A1204"));

    let mut edge_heavy = empty_metadata();
    edge_heavy.resolve = Some(CargoMetadataResolve {
        nodes: vec![CargoMetadataNode {
            id: "source".to_owned(),
            deps: (0..=MAX_CARGO_EDGES).map(metadata_edge).collect(),
        }],
    });
    let mut edge_diagnostics = ValidationDiagnostics::default();
    assert!(!validate_cargo_metadata_limits(&edge_heavy, &mut edge_diagnostics));
    assert!(has_code(&edge_diagnostics.into_vec(), "ZRYNA-A1204"));
}

#[test]
fn detects_optional_cargo_input_creation_and_noncanonical_spelling() -> Result<(), Box<dyn Error>> {
    let created_fixture = TempFixture::new()?;
    let mut snapshots = Vec::new();
    let mut snapshot_diagnostics = ValidationDiagnostics::default();
    read_optional_cargo_input_snapshots(
        &created_fixture.root,
        &mut snapshots,
        &mut snapshot_diagnostics,
    );
    assert!(snapshot_diagnostics.is_empty());
    assert_eq!(snapshots.len(), 2);
    created_fixture.write(".cargo/config.toml", b"[net]\noffline = true\n")?;
    let mut changed_diagnostics = ValidationDiagnostics::default();
    validate_cargo_inputs_unchanged(&created_fixture.root, &snapshots, &mut changed_diagnostics);
    assert!(has_code(&changed_diagnostics.into_vec(), "ZRYNA-A1203"));

    let spelling_fixture = TempFixture::new()?;
    spelling_fixture.write(".cargo/Config.toml", b"[net]\noffline = true\n")?;
    let mut spelling_diagnostics = ValidationDiagnostics::default();
    read_optional_cargo_input_snapshots(
        &spelling_fixture.root,
        &mut Vec::new(),
        &mut spelling_diagnostics,
    );
    assert!(has_code(&spelling_diagnostics.into_vec(), "ZRYNA-A1003"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn cargo_metadata_deadline_survives_a_descendant_holding_output_pipes() -> Result<(), Box<dyn Error>>
{
    use std::os::unix::fs::PermissionsExt;

    let fixture = TempFixture::new()?;
    let fake_cargo = fixture.write("fake-cargo", b"#!/bin/sh\n(sleep 5) &\nexit 0\n")?;
    fs::set_permissions(&fake_cargo, fs::Permissions::from_mode(0o700))?;
    let started = std::time::Instant::now();
    let mut diagnostics = ValidationDiagnostics::default();

    let metadata = load_cargo_metadata_with_executable(
        &fixture.root,
        false,
        fake_cargo.into_os_string(),
        std::time::Duration::from_millis(100),
        &mut diagnostics,
    );

    assert!(metadata.is_none());
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(has_code(&diagnostics.into_vec(), "ZRYNA-A1204"));
    Ok(())
}

use super::*;

#[cfg(unix)]
#[test]
fn bounded_reader_detects_same_size_replacement() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    let controlled = fixture.write("controlled.zry", b"first")?;
    let replacement = fixture.write("replacement.zry", b"other")?;
    let result = read_bounded_utf8_with_hooks(
        &controlled,
        ControlledReadPolicy {
            diagnostic_path: Some(Path::new("controlled.zry")),
            max_bytes: 32,
            unavailable_code: "ZRYNA-A1203",
            unavailable_guidance: "restore the file",
            expected_size: None,
        },
        || {
            assert!(fs::remove_file(&controlled).is_ok());
            assert!(fs::rename(&replacement, &controlled).is_ok());
        },
        || {},
    );

    assert!(matches!(result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1203"));
    Ok(())
}

#[cfg(windows)]
#[test]
fn bounded_reader_denies_same_size_replacement_on_windows() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    let controlled = fixture.write("controlled.zry", b"first")?;
    let result = read_bounded_utf8_with_hooks(
        &controlled,
        ControlledReadPolicy {
            diagnostic_path: Some(Path::new("controlled.zry")),
            max_bytes: 32,
            unavailable_code: "ZRYNA-A1203",
            unavailable_guidance: "restore the file",
            expected_size: None,
        },
        || {
            assert!(fs::remove_file(&controlled).is_err());
        },
        || {},
    )
    .unwrap_or_else(|diagnostic| panic!("unexpected diagnostic: {diagnostic:?}"));

    assert_eq!(result, ("first".to_string(), 5));
    Ok(())
}

#[test]
fn different_files_never_share_an_identity() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    let left = fixture.write("left.zry", b"same")?;
    let right = fixture.write("right.zry", b"same")?;
    let left_handle = same_file::Handle::from_file(fs::File::open(left)?)?;
    let right_handle = same_file::Handle::from_file(fs::File::open(right)?)?;

    assert_ne!(left_handle, right_handle);
    Ok(())
}

#[test]
fn oversized_contract_and_manifest_fail_before_parsing() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    let contract_size = usize::try_from(MAX_CONTRACT_BYTES + 1)?;
    fixture.write("zryna.workspace.json", &vec![b' '; contract_size])?;
    let contract_result = load_contract(&fixture.root);
    assert!(matches!(contract_result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1204"));

    let manifest_size = usize::try_from(MAX_MANIFEST_BYTES + 1)?;
    let manifest = fixture.write("Cargo.toml", &vec![b' '; manifest_size])?;
    let mut diagnostics = ValidationDiagnostics::default();
    assert!(read_toml_with_source(&manifest, &mut diagnostics).is_none());
    let diagnostics = diagnostics.into_vec();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "ZRYNA-A1204");
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_unreadable_and_mid_read_modified_files() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TempFixture::new()?;
    let unreadable = fixture.write("unreadable.zry", b"value")?;
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))?;
    let unreadable_result = read_bounded_utf8(
        &unreadable,
        Some(Path::new("unreadable.zry")),
        32,
        "ZRYNA-A1203",
        "restore the file",
    );
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600))?;
    assert!(matches!(unreadable_result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1203"));

    let modified = fixture.write("modified.zry", b"value")?;
    let modified_result = read_bounded_utf8_with_hooks(
        &modified,
        ControlledReadPolicy {
            diagnostic_path: Some(Path::new("modified.zry")),
            max_bytes: 32,
            unavailable_code: "ZRYNA-A1203",
            unavailable_guidance: "restore the file",
            expected_size: None,
        },
        || {},
        || assert!(fs::write(&modified, b"changed").is_ok()),
    );
    assert!(matches!(modified_result, Err(diagnostic) if diagnostic.code == "ZRYNA-A1203"));
    Ok(())
}

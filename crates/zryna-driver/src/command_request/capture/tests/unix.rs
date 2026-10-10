use std::{
    fs, io,
    os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink},
    path::Path,
};

use super::{CapturedRequest, Fixture, VALID, owner_fixture_evidence, rejected};

fn private() -> io::Result<Fixture> {
    let fixture = Fixture::new()?;
    fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o600))?;
    Ok(fixture)
}

#[test]
fn actual_private_file_retains_same_handle_and_recovers() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    let captured = CapturedRequest::capture(&path, Some("MODE")).expect("private capture");
    let original = captured.file.file().metadata()?.ino();
    for _ in 0..3 {
        captured.revalidate().expect("unchanged retained input");
        assert_eq!(captured.file.file().metadata()?.ino(), original);
        assert!(matches!(captured.request().value(), Some("on")));
    }
    drop(captured);
    CapturedRequest::capture(&path, Some("MODE")).expect("fresh capture");
    Ok(())
}

#[test]
fn retained_identity_accepts_regular_objects_and_rejects_link_metadata() -> io::Result<()> {
    use cap_std::fs::MetadataExt as _;

    let fixture = private()?;
    let directory =
        cap_std::fs::Dir::open_ambient_dir(&fixture.root, cap_std::ambient_authority())?;
    for path in ["nested", "nested/request.json"] {
        let metadata = directory.symlink_metadata(path)?;
        assert_eq!(
            super::super::platform::identity(&metadata).expect("retained regular identity"),
            (metadata.dev(), metadata.ino())
        );
    }
    symlink(fixture.path(), fixture.root.join("alias.json"))?;
    let metadata = directory.symlink_metadata("alias.json")?;
    assert!(metadata.is_symlink(), "independent no-follow metadata");
    assert!(super::super::platform::identity(&metadata).is_err());
    Ok(())
}

#[test]
fn actual_permission_modes_and_post_capture_changes_reject() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    for mode in [0o000, 0o400, 0o644, 0o660, 0o601, 0o700, 0o1600, 0o2600, 0o4600] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
        rejected(&path, Some("MODE"));
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let captured = CapturedRequest::capture(&path, Some("MODE")).expect("private capture");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    assert!(captured.revalidate().is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    assert!(captured.revalidate().is_err(), "changed ctime is retained");
    drop(captured);
    CapturedRequest::capture(&path, Some("MODE")).expect("permission recovery");
    Ok(())
}

#[test]
fn actual_hardlinks_reject_before_and_after_capture() -> io::Result<()> {
    let fixture = private()?;
    let alias = fixture.root.join("alias.json");
    fs::hard_link(fixture.path(), &alias)?;
    rejected(&fixture.path(), Some("MODE"));
    fs::remove_file(&alias)?;
    let captured = CapturedRequest::capture(&fixture.path(), Some("MODE")).expect("one link");
    fs::hard_link(fixture.path(), &alias)?;
    assert!(captured.revalidate().is_err());
    fs::remove_file(alias)?;
    drop(captured);
    CapturedRequest::capture(&fixture.path(), Some("MODE")).expect("link recovery");
    Ok(())
}

#[test]
fn final_and_ancestor_symlinks_reject_without_following() -> io::Result<()> {
    let fixture = private()?;
    let alias = fixture.root.join("alias.json");
    symlink(fixture.path(), &alias)?;
    rejected(&alias, Some("MODE"));
    let alias_dir = fixture.root.join("alias-dir");
    symlink(fixture.root.join("nested"), &alias_dir)?;
    rejected(&alias_dir.join("request.json"), Some("MODE"));
    CapturedRequest::capture(&fixture.path(), Some("MODE")).expect("link rejection recovery");
    Ok(())
}

#[test]
fn pathname_substitution_cannot_select_new_value() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    let captured = CapturedRequest::capture(&path, Some("MODE")).expect("original capture");
    fs::rename(&path, fixture.root.join("original.json"))?;
    fs::write(&path, VALID.replace("on\"", "no\""))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    assert!(matches!(captured.request().value(), Some("on")));
    assert!(captured.revalidate().is_err());
    drop(captured);
    let fresh = CapturedRequest::capture(&path, Some("MODE")).expect("new explicit capture");
    assert!(matches!(fresh.request().value(), Some("no")));
    Ok(())
}

#[test]
fn ancestor_substitution_rejects_retained_input() -> io::Result<()> {
    let fixture = private()?;
    let captured = CapturedRequest::capture(&fixture.path(), Some("MODE")).expect("capture");
    fs::rename(fixture.root.join("nested"), fixture.root.join("original-dir"))?;
    fs::create_dir(fixture.root.join("nested"))?;
    fs::write(fixture.path(), VALID)?;
    fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o600))?;
    assert!(captured.revalidate().is_err());
    drop(captured);
    CapturedRequest::capture(&fixture.path(), Some("MODE")).expect("ancestor recovery");
    Ok(())
}

#[test]
fn same_size_value_mutation_and_oversize_reject() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    let captured = CapturedRequest::capture(&path, Some("MODE")).expect("capture");
    fs::write(&path, VALID.replace("on\"", "no\""))?;
    assert!(captured.revalidate().is_err());
    assert!(matches!(captured.request().value(), Some("on")));
    drop(captured);
    let mut bytes = VALID.as_bytes().to_vec();
    bytes.resize(4096, b' ');
    fs::write(&path, &bytes)?;
    let exact = CapturedRequest::capture(&path, Some("MODE")).expect("4096 bytes");
    exact.revalidate().expect("exact byte bound stable");
    drop(exact);
    bytes.push(b' ');
    fs::write(&path, bytes)?;
    rejected(&path, Some("MODE"));
    fs::write(&path, VALID)?;
    CapturedRequest::capture(&path, Some("MODE")).expect("size recovery");
    Ok(())
}

#[test]
fn relative_parent_paths_directories_and_source_mismatch_reject() -> io::Result<()> {
    let fixture = private()?;
    rejected(Path::new("request.json"), Some("MODE"));
    rejected(&fixture.root.join("nested/../nested/request.json"), Some("MODE"));
    rejected(&fixture.root.join("nested"), Some("MODE"));
    rejected(&fixture.path(), None);
    rejected(&fixture.path(), Some("OTHER"));
    fs::write(fixture.path(), "invalid")?;
    rejected(&fixture.path(), Some("MODE"));
    fs::write(fixture.path(), VALID)?;
    CapturedRequest::capture(&fixture.path(), Some("MODE")).expect("recovery");
    Ok(())
}

#[test]
fn actual_foreign_owner_rejects_when_privileged_fixture_is_possible() -> io::Result<()> {
    let effective_uid = nix::unistd::geteuid();
    if !effective_uid.is_root() {
        owner_fixture_evidence(format_args!(
            "foreign-owner fixture platform=unix stage=eligibility outcome=unavailable effective_uid={} required_effective_uid=0",
            effective_uid.as_raw()
        ))?;
        return Ok(()); // A non-root test process cannot create an actual foreign-owned fixture.
    }
    owner_fixture_evidence(format_args!(
        "foreign-owner fixture platform=unix stage=eligibility outcome=eligible effective_uid=0"
    ))?;
    let fixture = private()?;
    let foreign = nix::unistd::Uid::from_raw(65534);
    nix::unistd::chown(&fixture.path(), Some(foreign), None).map_err(io::Error::other)?;
    assert_eq!(fs::metadata(fixture.path())?.uid(), foreign.as_raw());
    owner_fixture_evidence(format_args!(
        "foreign-owner fixture platform=unix stage=setup outcome=established owner_uid=65534"
    ))?;
    rejected(&fixture.path(), Some("MODE"));
    owner_fixture_evidence(format_args!(
        "foreign-owner fixture platform=unix stage=rejection outcome=observed"
    ))?;
    nix::unistd::chown(&fixture.path(), Some(nix::unistd::geteuid()), None)
        .map_err(io::Error::other)?;
    CapturedRequest::capture(&fixture.path(), Some("MODE")).expect("owner recovery");
    owner_fixture_evidence(format_args!(
        "foreign-owner fixture platform=unix stage=recovery outcome=observed"
    ))?;
    Ok(())
}

#[test]
fn same_file_content_mutation_between_capture_reads_rejects_and_recovers() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    let changed = VALID.replace("on\"", "no\"");
    assert_eq!(changed.len(), VALID.len());
    let result = CapturedRequest::capture_with_after_first_read(&path, Some("MODE"), || {
        fs::write(&path, &changed)
    });
    assert!(result.is_err());
    assert!(fs::read(&path)?.as_slice().eq(changed.as_bytes()));
    fs::write(&path, VALID)?;
    let fresh = CapturedRequest::capture(&path, Some("MODE")).expect("content recovery");
    fresh.revalidate().expect("stable recovered capture");
    assert!(matches!(fresh.request().value(), Some("on")));
    Ok(())
}

#[test]
fn final_path_substitution_between_capture_reads_rejects_and_recovers() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    let result = CapturedRequest::capture_with_after_first_read(&path, Some("MODE"), || {
        fs::rename(&path, fixture.root.join("original.json"))?;
        fs::write(&path, VALID.replace("on\"", "no\""))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
    });
    assert!(result.is_err());
    assert!(fs::read(&path)?.as_slice().eq(VALID.replace("on\"", "no\"").as_bytes()));
    assert!(fixture.root.join("original.json").is_file());
    let fresh = CapturedRequest::capture(&path, Some("MODE")).expect("substitution recovery");
    fresh.revalidate().expect("stable replacement capture");
    assert!(matches!(fresh.request().value(), Some("no")));
    Ok(())
}

#[test]
fn privacy_mutation_between_capture_reads_rejects_and_recovers() -> io::Result<()> {
    let fixture = private()?;
    let path = fixture.path();
    let result = CapturedRequest::capture_with_after_first_read(&path, Some("MODE"), || {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
    });
    assert!(result.is_err());
    assert_eq!(fs::metadata(&path)?.mode() & 0o7777, 0o644);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let fresh = CapturedRequest::capture(&path, Some("MODE")).expect("privacy recovery");
    fresh.revalidate().expect("stable private capture");
    Ok(())
}

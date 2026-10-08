use super::*;

#[test]
fn rejects_unsafe_paths() {
    assert!(safe_relative_path("crates/zryna-ir"));
    assert!(!safe_relative_path("../outside"));
    assert!(!safe_relative_path("C:\\outside"));
    assert!(!safe_relative_path("crates\\zryna-ir"));
    for value in [
        "C:/outside",
        "C:relative",
        "//server/share",
        "crates//sample",
        "crates/./sample",
        "crates/sample/",
        "crates/naïve",
    ] {
        assert!(!safe_relative_path(value), "accepted nonportable path {value:?}");
    }
    for value in [
        "CON",
        "con.txt",
        "PRN",
        "AUX.log",
        "NUL",
        "COM1",
        "LPT9.txt",
        "bad:name",
        "trailing.",
        "trailing ",
    ] {
        assert!(!portable_path_segment(value), "accepted reserved segment {value:?}");
    }
    assert!(portable_path_segment("com10"));
}

#[test]
fn validates_canonical_ids() {
    assert!(valid_id("zryna-backend-native"));
    assert!(!valid_id("ZRYNA-native"));
    assert!(!valid_id("7-native"));
    assert!(!valid_id("zryna--native"));
}

#[test]
fn binds_component_kinds_to_portable_roots() {
    let mut contract = fixture_contract();
    contract.adapters[0].root = "crates/typescript-6".to_owned();
    contract.members = vec![
        MemberContract {
            id: "core".to_owned(),
            root: "apps/core".to_owned(),
            kind: MemberKind::Foundation,
            dependencies: Vec::new(),
            allowed_entries: Vec::new(),
        },
        MemberContract {
            id: "cli".to_owned(),
            root: "crates/cli".to_owned(),
            kind: MemberKind::Application,
            dependencies: Vec::new(),
            allowed_entries: Vec::new(),
        },
    ];
    let mut diagnostics = ValidationDiagnostics::default();

    validate_contract_paths(&contract, &mut diagnostics);

    let diagnostics = diagnostics.into_vec();
    assert_eq!(diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1003").count(), 3);
}

#[test]
fn proves_required_root_shapes_and_component_inventory() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    fixture.write("Cargo.toml", b"")?;
    fixture.directory("Cargo.lock")?;
    fixture.write("zryna.workspace.json", b"{}")?;
    fixture.directory("apps/zryna")?;
    fixture.directory("crates/core")?;
    fixture.directory("crates/rogue")?;
    fixture.directory("adapters/typescript-6")?;
    let contract = WorkspaceContract {
        schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
        version: CONTRACT_VERSION,
        profile: CONTRACT_PROFILE.to_owned(),
        members: vec![
            MemberContract {
                id: "zryna".to_owned(),
                root: "apps/zryna".to_owned(),
                kind: MemberKind::Application,
                dependencies: Vec::new(),
                allowed_entries: Vec::new(),
            },
            cargo_graph_member("core", &[]),
        ],
        adapters: vec![AdapterContract {
            id: "typescript-6".to_owned(),
            root: "adapters/typescript-6".to_owned(),
            protocol_version: 1,
            toolchain: "@typescript/typescript6@6.0.2".to_owned(),
            allowed_entries: Vec::new(),
        }],
        outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
    };
    let mut diagnostics = ValidationDiagnostics::default();

    validate_required_root_shapes(&fixture.root, &mut diagnostics);
    validate_component_containers(&fixture.root, &contract, &mut diagnostics);

    let diagnostics = diagnostics.into_vec();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "ZRYNA-A1005" && diagnostic.path() == Some("Cargo.lock")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "ZRYNA-A1005" && diagnostic.path() == Some("crates/rogue")
    }));
    Ok(())
}

#[test]
fn detects_noncanonical_component_directory_spelling() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    fixture.directory("crates/Sample")?;
    let contract = WorkspaceContract {
        schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
        version: CONTRACT_VERSION,
        profile: CONTRACT_PROFILE.to_owned(),
        members: vec![cargo_graph_member("sample", &[])],
        adapters: Vec::new(),
        outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
    };
    let canonical_root = fs::canonicalize(&fixture.root)?;
    let mut diagnostics = ValidationDiagnostics::default();

    validate_paths(&canonical_root, &contract, &mut diagnostics);

    assert!(diagnostics.into_vec().iter().any(|diagnostic| {
        diagnostic.code == "ZRYNA-A1003" && diagnostic.path() == Some("crates/sample")
    }));
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_case_colliding_filesystem_siblings() -> Result<(), Box<dyn Error>> {
    let first = TempFixture::new()?;
    first.write("Foo.txt", b"first")?;
    first.write("foo.txt", b"second")?;
    let second = TempFixture::new()?;
    second.write("foo.txt", b"second")?;
    second.write("Foo.txt", b"first")?;

    let (_, first_diagnostics) = scan_fixture(&first.root, FIXTURE_LIMITS);
    let (_, second_diagnostics) = scan_fixture(&second.root, FIXTURE_LIMITS);
    let collision = |diagnostics: Vec<zryna_diagnostics::Diagnostic>| {
        diagnostics
            .into_iter()
            .find(|diagnostic| {
                diagnostic.code == "ZRYNA-A1003"
                    && diagnostic.message.contains("collide under the portable path identity")
            })
            .expect("missing portable collision diagnostic")
    };
    assert_eq!(collision(first_diagnostics), collision(second_diagnostics));
    Ok(())
}

#[test]
fn current_repository_satisfies_the_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report = validate_workspace(&root);
    assert!(report.is_valid(), "architecture diagnostics: {:#?}", report.diagnostics);
}

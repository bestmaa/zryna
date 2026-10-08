use super::*;

#[test]
fn cargo_metadata_proves_alias_kinds_targets_and_optional_edges() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    let contract = write_cargo_graph_fixture(&fixture)?;
    let mut acquisition_diagnostics = ValidationDiagnostics::default();
    let metadata = load_cargo_metadata(&fixture.root, false, &mut acquisition_diagnostics)
        .ok_or("Cargo metadata fixture failed")?;
    assert!(acquisition_diagnostics.is_empty());

    let consumer = metadata
        .packages
        .iter()
        .find(|package| package.name == "consumer")
        .ok_or("missing consumer metadata")?;
    assert_complete_dependency_forms(consumer);

    let mut diagnostics = ValidationDiagnostics::default();
    let graph =
        validate_resolved_cargo_graph(&fixture.root, &contract, &metadata, &mut diagnostics);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {:#?}", diagnostics.values);
    let expected: BTreeSet<String> = contract
        .members
        .iter()
        .find(|member| member.id == "consumer")
        .ok_or("missing consumer contract")?
        .dependencies
        .iter()
        .cloned()
        .collect();
    assert_eq!(graph.get("consumer"), Some(&expected));

    for missing_dependency in
        ["base-normal", "base-dev", "base-build", "base-windows", "base-optional"]
    {
        assert_undeclared_internal_edge_rejected(
            &fixture,
            &contract,
            &metadata,
            missing_dependency,
        )?;
    }

    let mut incomplete = contract.clone();
    incomplete
        .members
        .iter_mut()
        .find(|member| member.id == "consumer")
        .ok_or("missing consumer contract")?
        .dependencies
        .retain(|dependency| dependency != "base-build");
    let mut incomplete_diagnostics = ValidationDiagnostics::default();
    validate_resolved_cargo_graph(
        &fixture.root,
        &incomplete,
        &metadata,
        &mut incomplete_diagnostics,
    );
    let first_report = validation_report(incomplete_diagnostics);
    assert!(has_code(&first_report.diagnostics, "ZRYNA-A1101"));

    let mut shuffled = metadata.clone();
    shuffled.packages.reverse();
    shuffled.workspace_members.reverse();
    if let Some(resolve) = &mut shuffled.resolve {
        resolve.nodes.reverse();
        for node in &mut resolve.nodes {
            node.deps.reverse();
            for dependency in &mut node.deps {
                dependency.dep_kinds.reverse();
            }
        }
    }
    let mut shuffled_diagnostics = ValidationDiagnostics::default();
    validate_resolved_cargo_graph(&fixture.root, &incomplete, &shuffled, &mut shuffled_diagnostics);
    assert_eq!(first_report.diagnostics, validation_report(shuffled_diagnostics).diagnostics);
    Ok(())
}

#[test]
fn rejects_unregistered_resolved_local_packages() -> Result<(), Box<dyn Error>> {
    let fixture = TempFixture::new()?;
    let mut contract = write_cargo_graph_fixture(&fixture)?;
    let mut acquisition_diagnostics = ValidationDiagnostics::default();
    let metadata = load_cargo_metadata(&fixture.root, false, &mut acquisition_diagnostics)
        .ok_or("Cargo metadata fixture failed")?;
    contract.members.retain(|member| member.id != "base-windows");
    let mut diagnostics = ValidationDiagnostics::default();

    validate_resolved_cargo_graph(&fixture.root, &contract, &metadata, &mut diagnostics);

    let diagnostics = diagnostics.into_vec();
    assert!(has_code(&diagnostics, "ZRYNA-A1005"));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "ZRYNA-A1101"
            && diagnostic.message.contains("unregistered local package 'base-windows'")
    }));
    Ok(())
}

#[test]
fn actual_graph_drives_layer_and_cycle_diagnostics() {
    let foundation = cargo_graph_member("foundation", &[]);
    let backend = MemberContract {
        id: "backend".to_owned(),
        root: "crates/backend".to_owned(),
        kind: MemberKind::Backend,
        dependencies: Vec::new(),
        allowed_entries: Vec::new(),
    };
    let contract = WorkspaceContract {
        schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
        version: CONTRACT_VERSION,
        profile: CONTRACT_PROFILE.to_owned(),
        members: vec![foundation.clone(), backend],
        adapters: Vec::new(),
        outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
    };
    let forbidden = BTreeMap::from([
        ("foundation".to_owned(), BTreeSet::from(["backend".to_owned()])),
        ("backend".to_owned(), BTreeSet::new()),
    ]);
    let mut forbidden_diagnostics = ValidationDiagnostics::default();
    validate_dependency_graph(&contract, Some(&forbidden), &mut forbidden_diagnostics);
    assert!(has_code(&forbidden_diagnostics.into_vec(), "ZRYNA-A1102"));

    let mut peer = foundation;
    peer.id = "peer".to_owned();
    peer.root = "crates/peer".to_owned();
    let cycle_contract = WorkspaceContract {
        members: vec![cargo_graph_member("foundation", &[]), peer],
        ..contract
    };
    let cycle: InternalDependencyGraph = BTreeMap::from([
        ("foundation".to_owned(), BTreeSet::from(["peer".to_owned()])),
        ("peer".to_owned(), BTreeSet::from(["foundation".to_owned()])),
    ]);
    let mut cycle_diagnostics = ValidationDiagnostics::default();
    validate_dependency_graph(&cycle_contract, Some(&cycle), &mut cycle_diagnostics);
    assert!(has_code(&cycle_diagnostics.into_vec(), "ZRYNA-A1103"));
}

#[test]
fn permanent_phase_graph_forbids_compiler_and_backend_provider_edges() {
    let syntax = cargo_graph_member("syntax", &[]);
    let frontend = MemberContract {
        id: "frontend".to_owned(),
        root: "crates/frontend".to_owned(),
        kind: MemberKind::Frontend,
        dependencies: vec!["syntax".to_owned()],
        allowed_entries: Vec::new(),
    };
    let semantics = MemberContract {
        id: "semantics".to_owned(),
        root: "crates/semantics".to_owned(),
        kind: MemberKind::Compiler,
        dependencies: vec!["syntax".to_owned()],
        allowed_entries: Vec::new(),
    };
    let backend = MemberContract {
        id: "backend".to_owned(),
        root: "crates/backend".to_owned(),
        kind: MemberKind::Backend,
        dependencies: Vec::new(),
        allowed_entries: Vec::new(),
    };
    let contract = WorkspaceContract {
        schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
        version: CONTRACT_VERSION,
        profile: CONTRACT_PROFILE.to_owned(),
        members: vec![syntax, frontend, semantics, backend],
        adapters: Vec::new(),
        outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
    };
    let graph = BTreeMap::from([
        ("syntax".to_owned(), BTreeSet::new()),
        ("frontend".to_owned(), BTreeSet::from(["syntax".to_owned()])),
        ("semantics".to_owned(), BTreeSet::from(["frontend".to_owned(), "syntax".to_owned()])),
        ("backend".to_owned(), BTreeSet::from(["frontend".to_owned()])),
    ]);
    let mut diagnostics = ValidationDiagnostics::default();

    validate_dependency_graph(&contract, Some(&graph), &mut diagnostics);

    let diagnostics = diagnostics.into_vec();
    assert_eq!(diagnostics.iter().filter(|diagnostic| diagnostic.code == "ZRYNA-A1102").count(), 2);
    assert!(allowed_layer_edge(MemberKind::Frontend, MemberKind::Foundation));
    assert!(allowed_layer_edge(MemberKind::Compiler, MemberKind::Foundation));
    assert!(!allowed_layer_edge(MemberKind::Compiler, MemberKind::Frontend));
    assert!(!allowed_layer_edge(MemberKind::Backend, MemberKind::Frontend));
}

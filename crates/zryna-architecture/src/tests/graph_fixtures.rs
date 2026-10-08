use super::*;

pub(super) fn cargo_graph_member(id: &str, dependencies: &[&str]) -> MemberContract {
    MemberContract {
        id: id.to_owned(),
        root: format!("crates/{id}"),
        kind: MemberKind::Foundation,
        dependencies: dependencies.iter().map(ToString::to_string).collect(),
        allowed_entries: vec!["Cargo.toml".to_owned(), "README.md".to_owned(), "src".to_owned()],
    }
}

pub(super) fn write_cargo_graph_fixture(
    fixture: &TempFixture,
) -> Result<WorkspaceContract, Box<dyn Error>> {
    let dependency_ids = ["base-normal", "base-dev", "base-build", "base-windows", "base-optional"];
    let mut members: Vec<MemberContract> =
        dependency_ids.iter().map(|id| cargo_graph_member(id, &[])).collect();
    members.push(cargo_graph_member("consumer", &dependency_ids));
    let contract = WorkspaceContract {
        schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
        version: CONTRACT_VERSION,
        profile: CONTRACT_PROFILE.to_owned(),
        members,
        adapters: Vec::new(),
        outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
    };
    let workspace_members = contract
        .members
        .iter()
        .map(|member| format!("\"{}\"", member.root))
        .collect::<Vec<_>>()
        .join(", ");
    fixture.write(
        "Cargo.toml",
        format!("[workspace]\nresolver = \"2\"\nmembers = [{workspace_members}]\n").as_bytes(),
    )?;
    for id in dependency_ids {
        fixture.write(
            format!("crates/{id}/Cargo.toml"),
            format!("[package]\nname = \"{id}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n")
                .as_bytes(),
        )?;
        fixture.write(format!("crates/{id}/src/lib.rs"), b"//! Graph fixture.\n")?;
    }
    fixture.write(
        "crates/consumer/Cargo.toml",
        br#"[package]
name = "consumer"
version = "0.1.0"
edition = "2024"

[dependencies]
normal_alias = { package = "base-normal", path = "../base-normal" }
optional_alias = { package = "base-optional", path = "../base-optional", optional = true }

[dev-dependencies]
dev_alias = { package = "base-dev", path = "../base-dev" }

[build-dependencies]
build_alias = { package = "base-build", path = "../base-build" }

[target.'cfg(windows)'.dependencies]
windows_alias = { package = "base-windows", path = "../base-windows" }
"#,
    )?;
    fixture.write("crates/consumer/src/lib.rs", b"//! Graph consumer.\n")?;
    Ok(contract)
}

pub(super) fn assert_undeclared_internal_edge_rejected(
    fixture: &TempFixture,
    contract: &WorkspaceContract,
    metadata: &CargoMetadataDocument,
    missing_dependency: &str,
) -> Result<(), Box<dyn Error>> {
    let mut incomplete = contract.clone();
    incomplete
        .members
        .iter_mut()
        .find(|member| member.id == "consumer")
        .ok_or("missing consumer contract")?
        .dependencies
        .retain(|dependency| dependency != missing_dependency);
    let mut diagnostics = ValidationDiagnostics::default();
    validate_resolved_cargo_graph(&fixture.root, &incomplete, metadata, &mut diagnostics);
    assert!(
        has_code(&diagnostics.into_vec(), "ZRYNA-A1101"),
        "undeclared {missing_dependency} edge was accepted"
    );
    Ok(())
}

pub(super) fn assert_complete_dependency_forms(consumer: &CargoMetadataPackage) {
    let aliases: BTreeSet<&str> = consumer
        .dependencies
        .iter()
        .filter_map(|dependency| dependency.rename.as_deref())
        .collect();
    assert!(
        ["normal_alias", "dev_alias", "build_alias", "windows_alias", "optional_alias"]
            .into_iter()
            .all(|alias| aliases.contains(alias))
    );
    assert!(
        consumer.dependencies.iter().any(|dependency| dependency.kind.as_deref() == Some("dev"))
    );
    assert!(
        consumer.dependencies.iter().any(|dependency| dependency.kind.as_deref() == Some("build"))
    );
    assert!(
        consumer
            .dependencies
            .iter()
            .any(|dependency| dependency.target.as_deref() == Some("cfg(windows)"))
    );
    assert!(consumer.dependencies.iter().any(|dependency| dependency.optional));
}

pub(super) fn metadata_package(index: usize) -> CargoMetadataPackage {
    CargoMetadataPackage {
        id: format!("package-{index}"),
        name: format!("package-{index}"),
        manifest_path: format!("crates/package-{index}/Cargo.toml"),
        source: Some("registry+fixture".to_owned()),
        dependencies: Vec::new(),
    }
}

pub(super) fn metadata_edge(index: usize) -> CargoMetadataNodeDependency {
    CargoMetadataNodeDependency {
        name: format!("dependency-{index}"),
        pkg: format!("package-{index}"),
        dep_kinds: Vec::new(),
    }
}

pub(super) fn empty_metadata() -> CargoMetadataDocument {
    CargoMetadataDocument {
        version: 1,
        packages: Vec::new(),
        workspace_members: Vec::new(),
        workspace_root: ".".to_owned(),
        resolve: Some(CargoMetadataResolve { nodes: Vec::new() }),
    }
}

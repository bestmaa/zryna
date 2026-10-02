use super::*;
#[path = "permanent_phase_graph_h1.rs"]
mod h1;
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

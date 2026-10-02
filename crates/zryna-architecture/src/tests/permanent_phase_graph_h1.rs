use super::*;

#[test]
fn command_ir_accepts_declared_syntax_foundation_but_rejects_a_hidden_provider_edge() {
    let syntax = cargo_graph_member("syntax", &[]);
    let frontend = MemberContract {
        id: "frontend".to_owned(),
        root: "crates/frontend".to_owned(),
        kind: MemberKind::Frontend,
        dependencies: vec!["syntax".to_owned()],
        allowed_entries: Vec::new(),
    };
    let ir = MemberContract {
        id: "ir".to_owned(),
        root: "crates/ir".to_owned(),
        kind: MemberKind::Compiler,
        dependencies: vec!["syntax".to_owned()],
        allowed_entries: Vec::new(),
    };
    let contract = WorkspaceContract {
        schema: "./schemas/zryna-workspace-v1.schema.json".to_owned(),
        version: CONTRACT_VERSION,
        profile: CONTRACT_PROFILE.to_owned(),
        members: vec![syntax, frontend, ir],
        adapters: Vec::new(),
        outputs: vec!["target".to_owned(), ".zryna/cache".to_owned(), ".zryna/out".to_owned()],
    };
    let mut graph = BTreeMap::from([
        ("syntax".to_owned(), BTreeSet::new()),
        ("frontend".to_owned(), BTreeSet::from(["syntax".to_owned()])),
        ("ir".to_owned(), BTreeSet::from(["syntax".to_owned()])),
    ]);
    let mut accepted = ValidationDiagnostics::default();
    validate_dependency_graph(&contract, Some(&graph), &mut accepted);
    assert!(accepted.into_vec().is_empty());
    graph.get_mut("ir").expect("IR node").insert("frontend".to_owned());
    let mut rejected = ValidationDiagnostics::default();
    validate_dependency_graph(&contract, Some(&graph), &mut rejected);
    assert!(has_code(&rejected.into_vec(), "ZRYNA-A1102"));
}

use super::*;

#[test]
fn graph_edges_selection_and_language_replay_reject_before_emitter() {
    let (input, pure, native) = fixture();
    let seal = verify(&input, &pure, &native).expect("original graph");
    let mut changed = input.clone();
    changed.edges.push(("A".into(), "C".into()));
    rejects_emit(&seal, &changed, &pure, &native, INVALID);
    changed = input.clone();
    changed.root = "B".into();
    rejects_emit(&seal, &changed, &pure, &native, INVALID);
    changed = input.clone();
    changed.selections[0].policy_version.push_str("-changed");
    rejects_emit(&seal, &changed, &pure, &native, UNSUPPORTED);
    changed = input.clone();
    changed.language = Language::ControlFlowV1;
    rejects_emit(&seal, &changed, &pure, &native, PROFILE);
    changed = input.clone();
    changed.instances[0].rows.insert(Row::JavaScriptNode);
    rejects_emit(&seal, &changed, &pure, &native, INVALID);
}

#[test]
fn missing_duplicate_extra_or_relocated_issuers_reject_before_emitter() {
    let (input, pure, native) = fixture();
    let seal = verify(&input, &pure, &native).expect("original graph");
    rejects_emit(&seal, &input, &pure, &BTreeMap::new(), INVALID);
    let mut changed = pure.clone();
    changed.instances.remove("B");
    rejects_emit(&seal, &input, &changed, &native, INVALID);
    changed = pure.clone();
    changed.instances.insert("C".into(), pure_authority("C"));
    rejects_emit(&seal, &input, &changed, &native, INVALID);
    let mut changed = native.clone();
    changed.insert("D".into(), native["C"].clone());
    rejects_emit(&seal, &input, &pure, &changed, INVALID);
    changed = BTreeMap::from([("B".into(), native["C"].clone())]);
    rejects_emit(&seal, &input, &pure, &changed, INVALID);
}

#[test]
fn transplanted_and_reissued_native_and_pure_source_maps_reject_before_emitter() {
    let (input, pure, native) = fixture();
    let seal = verify(&input, &pure, &native).expect("original graph");
    let mut changed = native.clone();
    changed.get_mut("C").expect("C").sources = native_sources();
    rejects_emit(&seal, &input, &pure, &changed, INVALID);
    // Equal declaration bytes and valid newly issued seals still have different map authority.
    changed.insert("C".into(), native_authority());
    assert_eq!(
        changed["C"].declarations.declaration_sha256(),
        native["C"].declarations.declaration_sha256()
    );
    rejects_emit(&seal, &input, &pure, &changed, INVALID);
    let mut changed = pure.clone();
    changed.instances.insert("A".into(), pure_authority("A"));
    rejects_emit(&seal, &input, &changed, &native, INVALID);
}

#[test]
fn cycles_orphans_duplicates_and_dangling_edges_reject_before_emitter() {
    let (input, pure, native) = fixture();
    let seal = verify(&input, &pure, &native).expect("original graph");
    for edges in [
        vec![("A", "B"), ("B", "C"), ("C", "A")],
        vec![("A", "B")],
        vec![("A", "B"), ("B", "C"), ("B", "C")],
        vec![("A", "B"), ("B", "missing")],
    ] {
        let mut changed = input.clone();
        changed.edges = edges.into_iter().map(|(from, to)| (from.into(), to.into())).collect();
        rejects_emit(&seal, &changed, &pure, &native, INVALID);
    }
}

#[test]
fn forged_binding_closure_witness_or_retained_issuer_reject_before_emitter() {
    let (input, pure, native) = fixture();
    let mut seal = verify(&input, &pure, &native).expect("original graph");
    seal.graph_binding[0] ^= 1;
    rejects_emit(&seal, &input, &pure, &native, INVALID);
    seal = verify(&input, &pure, &native).expect("original graph");
    seal.native.get_mut("C").expect("C").declaration_sha256[0] ^= 1;
    rejects_emit(&seal, &input, &pure, &native, INVALID);
    seal = verify(&input, &pure, &native).expect("original graph");
    seal.closures.get_mut("A").expect("A").clear();
    rejects_emit(&seal, &input, &pure, &native, INVALID);
    seal = verify(&input, &pure, &native).expect("original graph");
    seal.witnesses.insert("C".into(), vec!["A".into(), "C".into()]);
    rejects_emit(&seal, &input, &pure, &native, INVALID);
    seal = verify(&input, &pure, &native).expect("original graph");
    seal.issuers.insert("C".into(), native_authority());
    rejects_emit(&seal, &input, &pure, &native, INVALID);
    seal = verify(&input, &pure, &native).expect("original graph");
    seal.issuers.remove("C");
    rejects_emit(&seal, &input, &pure, &native, INVALID);
}

use super::*;

#[test]
fn shortest_then_bytewise_path_is_derived_from_actual_edges() {
    let (mut input, mut pure, native) = fixture();
    let mut node = input.instances[1].clone();
    node.id = "D".into();
    input.instances.push(node);
    pure.instances.insert("D".into(), pure_authority("D"));
    input.edges.extend([("A".into(), "D".into()), ("D".into(), "C".into())]);
    input.edges.reverse();
    let seal = verify(&input, &pure, &native).expect("diamond");
    assert_eq!(seal.witnesses["C"], ["A", "B", "C"]);
    input.edges.push(("A".into(), "C".into()));
    let seal = verify(&input, &pure, &native).expect("actual shortest edge");
    assert_eq!(seal.witnesses["C"], ["A", "C"]);
    input
        .edges
        .retain(|edge| edge != &("B".into(), "C".into()) && edge != &("A".into(), "C".into()));
    let seal = verify(&input, &pure, &native).expect("different genuine route");
    assert_eq!(seal.witnesses["C"], ["A", "D", "C"]);
    assert!(seal.closures["B"].is_empty());
    assert_eq!(seal.closures["A"], BTreeSet::from(["C".into()]));
}

#[test]
fn unrequested_host_effects_resource_reservations_and_incompatible_rows_fail_closed() {
    let (input, pure, native) = fixture();
    let seal = verify(&input, &pure, &native).expect("original graph");
    let mut changed = input.clone();
    changed.instances[0].reservation.timers = 1;
    rejects_emit(&seal, &changed, &pure, &native, UNSUPPORTED);
    changed = input.clone();
    changed.instances[2].rows = BTreeSet::from([Row::UniversalNative]);
    rejects_emit(&seal, &changed, &pure, &native, UNSUPPORTED);
    changed = input.clone();
    let requirement = super::super::super::model::Requirement {
        capability: super::super::super::model::Capability::Clock,
        interface: "wasi:clocks/monotonic-clock@0.2.12".into(),
    };
    changed.instances[0].requirements.insert(requirement.clone());
    rejects_emit(&seal, &changed, &pure, &native, UNSUPPORTED);
    changed = input.clone();
    changed.selections[0].approved.insert(requirement);
    rejects_emit(&seal, &changed, &pure, &native, UNSUPPORTED);
    changed = input.clone();
    changed.selections[0].ceilings[0] = 1;
    rejects_emit(&seal, &changed, &pure, &native, UNSUPPORTED);
    changed = input.clone();
    changed.selections.push(selection(Row::NativeHost));
    rejects_emit(&seal, &changed, &pure, &native, INVALID);
}

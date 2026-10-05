//! Two fixed faults through the retained native provider and canonical target execution.

use super::super::{execute_with_fault, execution::PreparedRun, observation::Fault};
use super::*;
use crate::{
    CommandFailure, PublishedOwnershipBundle,
    ownership_pipeline::native_frontend,
    ownership_publication::{PublicationPhase, publish_after_staging_with_checkpoint_for_test},
    runtime::NodeRuntimeCapability,
};
use std::cell::{Cell, RefCell};

const IDS: [&str; 2] = ["vec-fault-2-1", "vec-fault-2-2"];
const REGISTRY_SHA: &str = "34cd29a5f146d77e7163b32d21e71e4f5a1fc5fd50f688d197de8bef9b38a508";

#[derive(Clone, Copy, Debug)]
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    expect(dead_code, reason = "source-write mutation probes require Linux retained handles")
)]
enum Mutation {
    Execution,
    Manifest,
    Commit,
}

#[derive(Default)]
struct Checkpoints {
    execution: Cell<usize>,
    publication: RefCell<Vec<String>>,
    mutated: Cell<bool>,
}

fn mutate(root: &Path, checkpoints: &Checkpoints) {
    fs::write(root.join("main.zry"), "export function score(): i32 { return 99; }\n")
        .expect("actual retained source mutation at the selected checkpoint");
    checkpoints.mutated.set(true);
}

fn native_run(
    request: &DataOwnershipBuildRequest,
    fault: Option<Fault>,
    mutation: Option<Mutation>,
    checkpoints: &Checkpoints,
) -> Result<PublishedOwnershipBundle, CommandFailure> {
    native_frontend::run_for_test(request, ("score".to_owned(), vec![]), |success, retained| {
        let run = PreparedRun::new(success, fault)?;
        let node =
            NodeRuntimeCapability::discover(success.node_runtime(), success.workspace_root())
                .map_err(super::super::execution_diagnostic)?;
        publish_after_staging_with_checkpoint_for_test(
            success,
            &|phase| {
                checkpoints.publication.borrow_mut().push(format!("{phase:?}"));
                if matches!(
                    (mutation, phase),
                    (Some(Mutation::Manifest), PublicationPhase::Manifest)
                        | (Some(Mutation::Commit), PublicationPhase::Commit)
                ) {
                    mutate(&request.workspace_root, checkpoints);
                }
                retained()
            },
            |transaction, output| {
                run.execute(&node, transaction, output, &|| {
                    checkpoints.execution.set(checkpoints.execution.get() + 1);
                    if matches!(mutation, Some(Mutation::Execution))
                        && checkpoints.execution.get() == 2
                    {
                        mutate(&request.workspace_root, checkpoints);
                    }
                    retained()
                })
            },
        )
    })
}

fn expected(case: Option<&Value>) -> (Value, Vec<crate::OwnershipTraceEvent>) {
    case.map_or_else(
        || (serde_json::json!({"kind":"returned","value":{"type":"i32","value":13}}), vec![]),
        |case| {
            (
                case["expected"].clone(),
                serde_json::from_value(case["trace"].clone()).expect("fixed test authority"),
            )
        },
    )
}

fn fault(case: Option<&Value>) -> Option<Fault> {
    case.map(|case| {
        Fault::new(
            u32::try_from(case["fault"]["code"].as_u64().expect("fixed test authority"))
                .expect("fixed test authority"),
            u32::try_from(case["fault"]["ordinal"].as_u64().expect("fixed test authority"))
                .expect("fixed test authority"),
        )
        .expect("frozen bounded logical selector")
    })
}

fn target_name(target: TargetSelection) -> &'static str {
    match target {
        TargetSelection::JavaScript => "javascript",
        TargetSelection::WebAssembly => "webassembly",
        TargetSelection::Native => "native",
        _ => panic!("selected single target only"),
    }
}

fn assert_observation(
    bundle: &PublishedOwnershipBundle,
    target: TargetSelection,
    case: Option<&Value>,
) {
    let (outcome, trace) = expected(case);
    assert_eq!(bundle.results().len(), 1, "exact target census");
    let result = &bundle.results()[0];
    assert_eq!(
        serde_json::to_value(result.target()).expect("fixed test authority"),
        target_name(target)
    );
    assert_eq!(serde_json::to_value(result.outcome()).expect("fixed test authority"), outcome);
    assert_eq!(result.trace(), trace, "entire independent frozen cleanup trace");
    let bytes = fs::read(bundle.manifest_path()).expect("complete published manifest");
    crate::decode_ownership_manifest_v3(&bytes).expect("canonical strict manifest admission");
    let manifest: Value = serde_json::from_slice(&bytes).expect("fixed test authority");
    assert_eq!(manifest["command"], "run");
    assert_eq!(manifest["targets"], serde_json::json!([target_name(target)]));
    assert_eq!(
        manifest["results"],
        serde_json::to_value(bundle.results()).expect("fixed test authority")
    );
    assert_eq!(manifest["sources"].as_array().expect("fixed test authority").len(), 1);
}

fn retain(
    label: &str,
    bundle: &PublishedOwnershipBundle,
    request: &DataOwnershipBuildRequest,
    checkpoints: &Checkpoints,
    case: Option<&Value>,
    provider: &str,
) {
    let Some(destination) = std::env::var_os("ZRYNA_M3_NATIVE_FAULT_EVIDENCE") else {
        return;
    };
    let root = std::path::PathBuf::from(destination);
    assert!(root.is_absolute());
    assert!(fs::symlink_metadata(&root).expect("fixed test authority").is_dir());
    let root = root.join(label);
    fs::create_dir(&root).expect("create-only observation evidence");
    for (relative, bytes) in inventory(bundle.path()) {
        let path = root.join("bundle").join(relative);
        fs::create_dir_all(path.parent().expect("fixed test authority"))
            .expect("fixed test authority");
        fs::write(path, bytes).expect("fixed test authority");
    }
    let source = fs::read(request.workspace_root.join("main.zry")).expect("fixed test authority");
    fs::write(root.join("main.zry"), &source).expect("fixed test authority");
    let metadata = serde_json::json!({
        "provider":provider,"case":case.map_or("score13", |c| c["id"].as_str().expect("fixed test authority")),
        "platform":std::env::consts::OS,"registry_sha256":REGISTRY_SHA,
        "source_sha256":format!("{:x}",Sha256::digest(source)),
        "results":bundle.results(),"execution_checkpoints":checkpoints.execution.get(),
        "publication_checkpoints":*checkpoints.publication.borrow(),
        "runtime":"canonical driver runner/decoder; explicit Node target runtime",
        "full_m3_fault_corpus":false,"installed_no_node_acceptance":false,"public_activation":false,
    });
    fs::write(
        root.join("observation.json"),
        serde_json::to_vec_pretty(&metadata).expect("fixed test authority"),
    )
    .expect("fixed test authority");
}

#[test]
fn two_frozen_faults_and_score13_execute_with_retained_native_provider() {
    let _guard = route_guard();
    assert_eq!(format!("{:x}", Sha256::digest(REGISTRY)), REGISTRY_SHA);
    let registry = registry();
    let cases = registry["faults"]
        .as_array()
        .expect("fixed test authority")
        .iter()
        .filter(|case| IDS.contains(&case["id"].as_str().expect("fixed test authority")))
        .collect::<Vec<_>>();
    assert_eq!(
        cases.iter().map(|c| c["id"].as_str().expect("fixed test authority")).collect::<Vec<_>>(),
        IDS
    );
    let positive = registry["valid"]
        .as_array()
        .expect("fixed test authority")
        .iter()
        .find(|c| c["id"] == "vec")
        .expect("fixed test authority");
    assert_eq!(positive["expected"], 13);
    let mut observations = 0;
    for target in
        [TargetSelection::JavaScript, TargetSelection::WebAssembly, TargetSelection::Native]
    {
        if target == TargetSelection::Native
            && !cfg!(all(target_os = "linux", target_arch = "x86_64"))
        {
            continue; // Existing contract excludes native execution on other hosts.
        }
        for case in std::iter::once(None).chain(cases.iter().map(|c| Some(*c))) {
            let workspace = fixture_workspace();
            install(workspace.root(), &registry, "vec");
            let source = fs::read(workspace.root().join("main.zry")).expect("fixed test authority");
            let request = request(workspace.root(), target);
            let mut baseline = None;
            for provider in ["bootstrap", "native-retained"] {
                let checkpoints = Checkpoints::default();
                let bundle = if provider == "bootstrap" {
                    let prepared = prepare_data_ownership_for_test(
                        &request,
                        Some(("score".to_owned(), vec![])),
                    )
                    .expect("canonical bootstrap preparation");
                    execute_with_fault(&prepared, fault(case))
                } else {
                    native_run(&request, fault(case), None, &checkpoints)
                }
                .expect("real emitted target execution and complete publication");
                assert_observation(&bundle, target, case);
                if provider == "native-retained" {
                    assert_eq!(checkpoints.execution.get(), 3);
                    let stage = match target {
                        TargetSelection::JavaScript => "JavaScript",
                        TargetSelection::WebAssembly => "WebAssembly",
                        TargetSelection::Native => "Native",
                        _ => unreachable!(),
                    };
                    assert_eq!(*checkpoints.publication.borrow(), [stage, "Manifest", "Commit"]);
                }
                let actual = inventory(bundle.path());
                if let Some(expected) = &baseline {
                    assert_eq!(
                        &actual, expected,
                        "complete provider-independent artifacts/manifest"
                    );
                } else {
                    baseline = Some(actual);
                }
                let id =
                    case.map_or("score13", |c| c["id"].as_str().expect("fixed test authority"));
                retain(
                    &format!("{}-{id}-{provider}", target_name(target)),
                    &bundle,
                    &request,
                    &checkpoints,
                    case,
                    provider,
                );
                fs::remove_dir_all(bundle.path()).expect("test-owned complete bundle cleanup");
                assert_no_artifacts(workspace.root());
                assert_eq!(
                    fs::read(workspace.root().join("main.zry")).expect("fixed test authority"),
                    source
                );
                observations += 1;
            }
        }
    }
    let expected = if cfg!(all(target_os = "linux", target_arch = "x86_64")) { 18 } else { 12 };
    assert_eq!(observations, expected);
    println!(
        "retained native M3: {observations} actual target observations; two frozen faults + score13; full corpus open"
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn source_mutation_fails_at_execution_manifest_and_commit_without_partial_output() {
    let _guard = route_guard();
    let registry = registry();
    // Preserve the current cold-root limitation separately: publication creates .zryna and
    // changes the captured workspace root before execution. Do not weaken the source guard.
    let cold = fixture_workspace();
    install(cold.root(), &registry, "vec");
    let checkpoints = Checkpoints::default();
    let failure =
        native_run(&request(cold.root(), TargetSelection::JavaScript), None, None, &checkpoints)
            .expect_err(
                "cold output-root creation is not yet admitted by the retained source owner",
            );
    assert_eq!(failure.kind(), CommandFailureKind::Source);
    assert_eq!(failure.diagnostics()[0].code(), "ZRYNA-D3004");
    assert_eq!(checkpoints.execution.get(), 0);
    assert_no_artifacts(cold.root());
    println!("retained native cold output root: D3004 before execution; owner coordination open");
    for mutation in [Mutation::Execution, Mutation::Manifest, Mutation::Commit] {
        let workspace = fixture_workspace();
        install(workspace.root(), &registry, "vec");
        crate::ArtifactOutputRoot::prepare_for_workspace(workspace.root())
            .expect("prepare the fixture output root before capturing source identity");
        let request = request(workspace.root(), TargetSelection::JavaScript);
        let checkpoints = Checkpoints::default();
        let failure = native_run(&request, None, Some(mutation), &checkpoints)
            .expect_err("actual source mutation must reject before commit");
        assert!(
            checkpoints.mutated.get(),
            "selected guard {mutation:?} checkpoint actually reached: {failure:?}"
        );
        assert_eq!(failure.kind(), CommandFailureKind::Source);
        assert_eq!(failure.diagnostics()[0].code(), "ZRYNA-D3004");
        assert_no_artifacts(workspace.root());
        println!("retained native source guard {mutation:?}: actual mutation rejected; no output");
    }
}

//! Remaining frozen logical fault rows through the canonical retained-source runner.

use super::*;

const GROUPS: [&str; 7] =
    ["string", "vec", "handles", "weak-live", "owned-aggregate", "owned-vec", "owned-shared"];

fn sources(root: &Path, registry: &Value, fixture: &str) -> BTreeMap<String, Vec<u8>> {
    let row = registry["fixtures"]
        .as_array()
        .expect("frozen fixture inventory")
        .iter()
        .find(|row| row["id"] == fixture)
        .expect("frozen fixture");
    let mut names = vec![("main.zry", row)];
    if let Some(id) = row["dependency"].as_str() {
        names.push((
            "math.zry",
            registry["fixtures"]
                .as_array()
                .expect("frozen fixture inventory")
                .iter()
                .find(|row| row["id"] == id)
                .expect("frozen dependency"),
        ));
    }
    names
        .into_iter()
        .map(|(name, authority)| {
            let bytes = fs::read(root.join(name)).expect("actual installed source");
            assert_eq!(format!("{:x}", Sha256::digest(&bytes)), authority["sha256"]);
            (name.to_owned(), bytes)
        })
        .collect()
}

fn outcome(row: &Value, injected: bool) -> Value {
    if injected {
        row["expected"].clone()
    } else {
        serde_json::json!({"kind":"returned","value":{"type":"i32","value":row["expected"]}})
    }
}

fn retain_row(
    label: &str,
    bundle: &PublishedOwnershipBundle,
    request: &DataOwnershipBuildRequest,
    source: &BTreeMap<String, Vec<u8>>,
    value: &Value,
) {
    let Some(parent) = std::env::var_os("ZRYNA_M3_NATIVE_CORPUS_EVIDENCE") else {
        return;
    };
    let parent = std::path::PathBuf::from(parent);
    assert!(parent.is_absolute() && parent.is_dir());
    let destination = parent.join(label);
    fs::create_dir(&destination).expect("create-only corpus row");
    for (name, bytes) in source {
        let path = destination.join("sources").join(name);
        fs::create_dir_all(path.parent().expect("source parent")).expect("source directory");
        fs::write(path, bytes).expect("retained frozen source");
    }
    for (name, bytes) in inventory(bundle.path()) {
        let path = destination.join("bundle").join(name);
        fs::create_dir_all(path.parent().expect("bundle parent")).expect("bundle directory");
        fs::write(path, bytes).expect("complete retained bundle");
    }
    fs::remove_dir_all(bundle.path()).expect("remove test-owned complete bundle");
    assert_no_artifacts(&request.workspace_root);
    fs::write(
        destination.join("observation.json"),
        serde_json::to_vec_pretty(value).expect("actual observation JSON"),
    )
    .expect("retained actual observation");
}

fn matches_frozen_observation(
    bundle: &PublishedOwnershipBundle,
    authority: &Value,
    injected: bool,
    target: TargetSelection,
) -> bool {
    let expected = outcome(authority, injected);
    let trace: Vec<crate::OwnershipTraceEvent> = if injected {
        serde_json::from_value(authority["trace"].clone()).expect("entire frozen trace")
    } else {
        vec![]
    };
    let observed = bundle.results();
    let correct = observed.len() == 1
        && observed[0].target()
            == match target {
                TargetSelection::JavaScript => OwnershipTarget::JavaScript,
                TargetSelection::WebAssembly => OwnershipTarget::WebAssembly,
                TargetSelection::Native => OwnershipTarget::Native,
                _ => unreachable!(),
            }
        && serde_json::to_value(observed[0].outcome()).expect("typed outcome") == expected
        && observed[0].trace() == trace;
    let manifest = crate::decode_ownership_manifest_v3(
        &fs::read(bundle.manifest_path()).expect("actual manifest bytes"),
    )
    .expect("strict canonical manifest admission");
    assert_eq!(manifest.results(), observed);
    correct
}

fn row(
    registry: &Value,
    authority: &Value,
    injected: bool,
    target: TargetSelection,
) -> Result<(), String> {
    let fixture = authority["fixture"].as_str().expect("frozen fixture identity");
    let case = if injected {
        authority["id"].as_str().expect("frozen fault identity").to_owned()
    } else {
        format!("positive-{fixture}")
    };
    let workspace = fixture_workspace();
    install(workspace.root(), registry, fixture);
    let source = sources(workspace.root(), registry, fixture);
    assert!(!workspace.root().join(".zryna").exists(), "cold native first capture");
    let request = request(workspace.root(), target);
    let mut baseline = None;
    let mut failures = Vec::new();
    for provider in ["native-retained", "bootstrap"] {
        let checkpoints = Checkpoints::default();
        let selection = fault(injected.then_some(authority));
        let result = if provider == "native-retained" {
            native_run(&request, selection, None, &checkpoints)
        } else {
            prepare_data_ownership_for_test(&request, Some(("score".to_owned(), vec![])))
                .and_then(|prepared| execute_with_fault(&prepared, selection))
        };
        let label = format!("{}-{case}-{provider}", target_name(target));
        let bundle = match result {
            Ok(bundle) => bundle,
            Err(error) => {
                if let Some(parent) = std::env::var_os("ZRYNA_M3_NATIVE_CORPUS_EVIDENCE") {
                    let path = std::path::PathBuf::from(parent).join(&label);
                    fs::create_dir(&path).expect("create-only failed row");
                    let value = serde_json::json!({"case":case,"fixture":fixture,
                        "target":target_name(target),"provider":provider,
                        "failure_kind":format!("{:?}",error.kind()),"diagnostics":error.diagnostics(),
                        "public_activation":false});
                    fs::write(path.join("failure.json"), value.to_string())
                        .expect("retained real failure");
                }
                failures.push(format!("{label}: {error:?}"));
                assert_no_artifacts(workspace.root());
                continue;
            }
        };
        if !matches_frozen_observation(&bundle, authority, injected, target) {
            failures.push(format!(
                "{label}: outcome or entire frozen trace differs: {:?}",
                bundle.results()
            ));
        }
        let observed = bundle.results();
        let inventory = inventory(bundle.path());
        if let Some(expected) = &baseline {
            if &inventory != expected {
                failures.push(format!("{label}: complete provider bundles differ"));
            }
        } else {
            baseline = Some(inventory);
        }
        let stage = match target {
            TargetSelection::JavaScript => "JavaScript",
            TargetSelection::WebAssembly => "WebAssembly",
            TargetSelection::Native => "Native",
            _ => unreachable!(),
        };
        if provider == "native-retained" {
            assert_eq!(checkpoints.execution.get(), 3);
            assert_eq!(*checkpoints.publication.borrow(), [stage, "Manifest", "Commit"]);
        }
        assert_eq!(sources(workspace.root(), registry, fixture), source);
        let value = serde_json::json!({"case":case,"fixture":fixture,"injected":injected,
            "provider":provider,"target":target_name(target),"platform":std::env::consts::OS,
            "fault":if injected { authority["fault"].clone() } else { Value::Null },
            "registry_sha256":REGISTRY_SHA,"results":observed,
            "sources":source.iter().map(|(name,bytes)|(name.clone(),format!("{:x}",Sha256::digest(bytes)))).collect::<BTreeMap<_,_>>(),
            "native_first":true,"execution_checkpoints":checkpoints.execution.get(),
            "publication_checkpoints":*checkpoints.publication.borrow(),"cleanup_entries":0,
            "public_activation":false,"installed_no_cargo_acceptance":false});
        retain_row(&label, &bundle, &request, &source, &value);
        if std::env::var_os("ZRYNA_M3_NATIVE_CORPUS_EVIDENCE").is_none() {
            fs::remove_dir_all(bundle.path()).expect("test-owned bundle cleanup");
            assert_no_artifacts(workspace.root());
        }
        println!("retained corpus {label}: real execution and cleanup complete");
    }
    if failures.is_empty() { Ok(()) } else { Err(failures.join("\n")) }
}

#[test]
fn remaining_frozen_fault_groups_execute_through_retained_source() {
    let _guard = route_guard();
    assert_eq!(format!("{:x}", Sha256::digest(REGISTRY)), REGISTRY_SHA);
    let registry = registry();
    let faults = registry["faults"].as_array().expect("frozen faults");
    assert_eq!(faults.len(), 26);
    let remaining = faults
        .iter()
        .filter(|case| !IDS.contains(&case["id"].as_str().expect("frozen fault identity")))
        .collect::<Vec<_>>();
    assert_eq!(remaining.len(), 24);
    let positives = GROUPS.map(|fixture| {
        registry["valid"]
            .as_array()
            .expect("frozen positives")
            .iter()
            .find(|case| case["id"] == fixture && case["fixture"] == fixture)
            .expect("frozen group calibration")
    });
    let mut failures = Vec::new();
    let mut rows = 0;
    for target in
        [TargetSelection::JavaScript, TargetSelection::WebAssembly, TargetSelection::Native]
    {
        if target == TargetSelection::Native
            && !cfg!(all(target_os = "linux", target_arch = "x86_64"))
        {
            continue;
        }
        for (authority, injected) in positives
            .iter()
            .map(|case| (*case, false))
            .chain(remaining.iter().map(|case| (*case, true)))
        {
            if let Err(failure) = row(&registry, authority, injected, target) {
                failures.push(failure);
            }
            rows += 1;
        }
    }
    assert_eq!(rows, if cfg!(all(target_os = "linux", target_arch = "x86_64")) { 93 } else { 62 });
    assert!(failures.is_empty(), "retained corpus failures:\n{}", failures.join("\n"));
    println!(
        "retained corpus: {rows} complete provider pairs; 24 new logical faults + 7 positive calibrations"
    );
}

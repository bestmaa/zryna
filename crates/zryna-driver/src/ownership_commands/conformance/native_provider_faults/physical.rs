//! Real allocation failure through the unchanged, bounded native invocation runner.

use super::*;

const ENV: &str = "ZRYNA_M3_NATIVE_PHYSICAL_EVIDENCE";

fn retain(
    case: &str,
    provider: &str,
    source: &BTreeMap<String, Vec<u8>>,
    bundle: &PublishedOwnershipBundle,
    observation: &Value,
) {
    let Some(parent) = std::env::var_os(ENV) else { return };
    let parent = std::path::PathBuf::from(parent);
    assert!(parent.is_absolute() && parent.is_dir());
    let destination = parent.join(format!("native-{case}-{provider}"));
    fs::create_dir(&destination).expect("create-only physical observation");
    for (category, files) in [("sources", source.clone()), ("bundle", inventory(bundle.path()))] {
        for (name, bytes) in files {
            let path = destination.join(category).join(name);
            fs::create_dir_all(path.parent().expect("retained parent"))
                .expect("retained directory");
            fs::write(path, bytes).expect("retained actual bytes");
        }
    }
    fs::write(
        destination.join("observation.json"),
        serde_json::to_vec_pretty(observation).expect("actual observation JSON"),
    )
    .expect("actual physical observation");
}

fn frozen_source(root: &Path, registry: &Value, fixture: &str) -> BTreeMap<String, Vec<u8>> {
    let fixtures = registry["fixtures"].as_array().expect("frozen fixture inventory");
    let entry = fixtures.iter().find(|row| row["id"] == fixture).expect("frozen entrypoint");
    let mut fixture_sources = vec![("main.zry", fixture)];
    if fixture == "vec" {
        assert!(entry.get("dependency").is_none(), "frozen Vec has no import");
    } else {
        let dependency = entry["dependency"].as_str().expect("frozen imported dependency");
        fixture_sources.push(("math.zry", dependency));
    }
    fixture_sources
        .into_iter()
        .map(|(name, id)| {
            let authority =
                fixtures.iter().find(|row| row["id"] == id).expect("frozen source authority");
            let bytes = fs::read(root.join(name)).expect("actual frozen source bytes");
            assert_eq!(format!("{:x}", Sha256::digest(&bytes)), authority["sha256"]);
            (name.to_owned(), bytes)
        })
        .collect()
}

fn pair(registry: &Value, fixture: &str, physical: Option<(&str, u32)>) {
    let injected = physical.is_some();
    let workspace = fixture_workspace();
    install(workspace.root(), registry, fixture);
    assert!(!workspace.root().join(".zryna").exists(), "native first cold capture");
    let source = frozen_source(workspace.root(), registry, fixture);
    let oracle_id = physical.map_or(fixture, |(id, _)| id);
    let oracle = registry[if injected { "faults" } else { "valid" }]
        .as_array()
        .expect("frozen observation authorities")
        .iter()
        .find(|row| row["id"] == oracle_id)
        .expect("frozen result and cleanup oracle");
    assert_eq!(oracle["fixture"], fixture);
    let expected = if injected {
        // Frozen logical rows supply cleanup traces. Physical allocation injection always
        // returns Allocation, including handles whose logical trace oracle is Refcount.
        serde_json::json!({"kind":"trapped","code":"zryna.trap.allocation-v1"})
    } else {
        serde_json::json!({"kind":"returned","value":{"type":"i32","value":oracle["expected"]}})
    };
    let trace: Vec<crate::OwnershipTraceEvent> = if injected {
        serde_json::from_value(oracle["trace"].clone()).expect("entire frozen cleanup trace")
    } else {
        vec![]
    };
    let case = physical.map_or_else(
        || format!("positive-{fixture}"),
        |(_, ordinal)| format!("{fixture}-physical-{ordinal}"),
    );
    let request = request(workspace.root(), TargetSelection::Native);
    let mut baseline = None;
    for provider in ["native-retained", "bootstrap"] {
        let checkpoints = Checkpoints::default();
        // Mode 6 selects the actual allocate_bytes hook before malloc. This is not code-2
        // logical injection, even though the source cleanup oracle is a logical fault row.
        let fault = physical.map(|(_, ordinal)| Fault::physical_allocation(ordinal));
        let bundle = if provider == "native-retained" {
            native_run(&request, fault, None, &checkpoints)
        } else {
            prepare_data_ownership_for_test(&request, Some(("score".to_owned(), vec![])))
                .and_then(|prepared| execute_with_fault(&prepared, fault))
        }
        .unwrap_or_else(|error| panic!("{case} {provider}: {error:?}"));
        let result = bundle.results();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target(), OwnershipTarget::Native);
        assert_eq!(
            serde_json::to_value(result[0].outcome()).expect("actual typed outcome"),
            expected
        );
        assert_eq!(result[0].trace(), trace);
        let manifest = crate::decode_ownership_manifest_v3(
            &fs::read(bundle.manifest_path()).expect("actual manifest bytes"),
        )
        .expect("strict canonical physical manifest");
        assert_eq!(manifest.results(), result);
        let files = inventory(bundle.path());
        if let Some(expected) = &baseline {
            assert_eq!(&files, expected, "entire provider bundle");
        } else {
            baseline = Some(files);
        }
        if provider == "native-retained" {
            assert_eq!(checkpoints.execution.get(), 3);
            assert_eq!(*checkpoints.publication.borrow(), ["Native", "Manifest", "Commit"]);
        }
        for (name, bytes) in &source {
            assert_eq!(
                fs::read(workspace.root().join(name)).expect("actual frozen source bytes"),
                *bytes
            );
        }
        let value = serde_json::json!({"case":case,"fixture":fixture,"injected":injected,
            "provider":provider,"target":"native","platform":"linux","registry_sha256":REGISTRY_SHA,
            "fault":physical.map(|(_, ordinal)| serde_json::json!({"mode":"physical-allocation","code":6,"ordinal":ordinal,"command":0x2600_0000_u32 | ordinal})),
            "trace_oracle":physical.map(|(id, _)| id),
            "results":result,"sources":source.iter().map(|(n,b)|(n.clone(),format!("{:x}",Sha256::digest(b)))).collect::<BTreeMap<_,_>>(),
            "native_first":true,"execution_checkpoints":checkpoints.execution.get(),
            "publication_checkpoints":*checkpoints.publication.borrow(),"cleanup_entries":0,
            "zero_live_finalization":true,"physical_allocation_release_counts":Value::Null,
            "public_activation":false,"installed_no_cargo_acceptance":false});
        retain(&case, provider, &source, &bundle, &value);
        // The canonical executable emitted its frame only after finish_invocation drained
        // scratch records and rejected live owned/control allocations. No count telemetry exists.
        fs::remove_dir_all(bundle.path()).expect("remove test-owned complete bundle");
        assert_no_artifacts(workspace.root());
        println!("physical {case} {provider}: canonical native execution and zero-live cleanup");
    }
}

#[test]
fn owned_shared_physical_group_executes_through_retained_source() {
    let _guard = route_guard();
    assert_eq!(format!("{:x}", Sha256::digest(REGISTRY)), REGISTRY_SHA);
    let registry = registry();
    pair(&registry, "owned-shared", None);
    pair(&registry, "owned-shared", Some(("owned-shared-fault-2-2", 4)));
    println!(
        "physical corpus: 2 complete provider pairs; 1 physical probe + 1 positive calibration"
    );
}

#[test]
fn string_physical_group_executes_through_retained_source() {
    let _guard = route_guard();
    assert_eq!(format!("{:x}", Sha256::digest(REGISTRY)), REGISTRY_SHA);
    let registry = registry();
    pair(&registry, "string", None);
    pair(&registry, "string", Some(("string-fault-2-1", 2)));
    pair(&registry, "string", Some(("string-fault-2-2", 4)));
    println!(
        "physical corpus: 3 complete provider pairs; 2 physical probes + 1 positive calibration"
    );
}

#[test]
fn vec_physical_group_executes_through_retained_source() {
    let _guard = route_guard();
    assert_eq!(format!("{:x}", Sha256::digest(REGISTRY)), REGISTRY_SHA);
    let registry = registry();
    pair(&registry, "vec", None);
    pair(&registry, "vec", Some(("vec-fault-2-1", 2)));
    pair(&registry, "vec", Some(("vec-fault-2-2", 3)));
    pair(&registry, "vec", Some(("vec-fault-2-3", 4)));
    println!(
        "physical corpus: 4 complete provider pairs; 3 physical probes + 1 positive calibration"
    );
}

#[test]
fn owned_vec_physical_group_executes_through_retained_source() {
    let _guard = route_guard();
    assert_eq!(format!("{:x}", Sha256::digest(REGISTRY)), REGISTRY_SHA);
    let registry = registry();
    pair(&registry, "owned-vec", None);
    pair(&registry, "owned-vec", Some(("owned-vec-fault-2-6", 10)));
    println!(
        "physical corpus: 2 complete provider pairs; 1 physical probe + 1 positive calibration"
    );
}

#[test]
fn handles_physical_group_executes_through_retained_source() {
    let _guard = route_guard();
    assert_eq!(format!("{:x}", Sha256::digest(REGISTRY)), REGISTRY_SHA);
    let registry = registry();
    pair(&registry, "handles", None);
    pair(&registry, "handles", Some(("handles-fault-2-1", 1)));
    pair(&registry, "handles", Some(("handles-fault-2-1", 2)));
    pair(&registry, "handles", Some(("handles-fault-4-1", 3)));
    pair(&registry, "handles", Some(("handles-fault-4-2", 4)));
    pair(&registry, "handles", Some(("handles-fault-4-3", 5)));
    println!(
        "physical corpus: 6 complete provider pairs; 5 physical probes + 1 positive calibration"
    );
}

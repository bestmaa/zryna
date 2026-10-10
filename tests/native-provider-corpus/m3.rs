use crate::{Context, closures, compare, corpus, providers, runtime};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use zryna_abi::{Invocation, ScalarOutcome, ScalarTrapCode};
use zryna_source::SourceMap;

pub fn run(context: &mut Context) {
    let registry = corpus::registry(context, "m3");
    let inventory = corpus::fixture_files(&context.root.join("tests/m3-fixtures"));
    assert_eq!(inventory.len(), 95, "complete frozen M3 source inventory");
    let fixtures = registry["fixtures"].as_array().expect("frozen contextual sources");
    assert_eq!(fixtures.len(), 25);
    let files = inventory
        .iter()
        .map(|name| {
            let path = format!("tests/m3-fixtures/{name}");
            let text = corpus::text(context, &path);
            (path, text)
        })
        .collect::<Vec<_>>();
    let workspace = corpus::install(context, "m3-complete", &files);
    for relative in inventory {
        let source = format!("tests/m3-fixtures/{relative}");
        context.case(format!("m3-source:{relative}"), "m3", &source, |context| {
            if let Some(fixture) = fixtures.iter().find(|fixture| fixture["path"] == source) {
                let _ = corpus::checked_text(context, fixture);
                if let Some(dependency) = fixture["dependency"].as_str() {
                    let dependency = fixtures
                        .iter()
                        .find(|fixture| fixture["id"] == dependency)
                        .expect("exact dependency");
                    fs::write(
                        workspace.join("tests/m3-fixtures/conformance/math.zry"),
                        corpus::checked_text(context, dependency),
                    )
                    .expect("original contextual dependency");
                }
            }
            let result = closures::m3(context, &workspace, &source, |sources| {
                downstream(context, sources, &source)
            });
            if relative == "borrow-exclusive-nonreference.zry" {
                assert_eq!(result["syntax"], "rejected");
                assert_eq!(
                    result["codes"],
                    json!(["ZRYNA-Y4002"]),
                    "known mandatory-verifier hostile source"
                );
            } else {
                assert_eq!(
                    result["syntax"], "accepted",
                    "all other frozen sources form verified v4 syntax"
                );
            }
            result
        });
    }
    let valid = registry["valid"].as_array().expect("fixed positive corpus");
    let invalid = registry["invalid"].as_array().expect("fixed negative corpus");
    let runtime_invalid =
        registry["runtimeInvalid"].as_array().expect("fixed runtime rejection corpus");
    let faults = registry["faults"].as_array().expect("fixed fault corpus");
    assert_eq!((valid.len(), invalid.len(), runtime_invalid.len(), faults.len()), (15, 4, 2, 26));
    let required_valid = valid
        .iter()
        .chain(runtime_invalid)
        .chain(faults)
        .map(|case| case["fixture"].as_str().expect("fixture ID"))
        .collect::<BTreeSet<_>>();
    for fixture in fixtures {
        let id = fixture["id"].as_str().expect("fixture ID");
        let source = fixture["path"].as_str().expect("fixture source");
        context.case(format!("m3-context:{id}"), "m3", source, |context| {
            let files = corpus::m3_files(context, &registry, fixture);
            let workspace = corpus::install(context, &format!("m3-context-{id}"), &files);
            let result = closures::m3(context, &workspace, "src/main.zry", |sources| {
                downstream(context, sources, "src/main.zry")
            });
            if required_valid.contains(id) {
                assert_eq!(result["semantic"], "accepted", "fixed source-valid contextual fixture");
            }
            for case in
                invalid.iter().filter(|case| case["fixture"] == id && case["code"] != "ZRYNA-B2102")
            {
                assert_eq!(result["semantic"], "rejected", "source-negative contextual fixture");
                assert!(
                    result["codes"]
                        .as_array()
                        .expect("negative codes")
                        .iter()
                        .any(|code| code == &case["code"])
                );
            }
            result
        });
    }
    for case in valid {
        invocation_case(
            context,
            &registry,
            case,
            "m3-run",
            json!({"type":"i32","value":case["expected"]}),
        );
    }
    for case in runtime_invalid {
        assert_eq!(case["expectedTrap"], "zryna.trap.bounds-v1");
        invocation_case(
            context,
            &registry,
            case,
            "m3-runtime-invalid",
            json!({"type":"trap","value":case["expectedTrap"]}),
        );
    }
    for case in invalid {
        let id = case["id"].as_str().expect("rejection ID");
        let fixture = fixtures
            .iter()
            .find(|fixture| fixture["id"] == case["fixture"])
            .expect("negative fixture");
        let source = fixture["path"].as_str().expect("source path");
        context.case(format!("m3-invalid:{id}"), "m3", source, |context| {
            let files = corpus::m3_files(context, &registry, fixture);
            let sources = corpus::sources(&files);
            if case["code"] == "ZRYNA-B2102" {
                let [a, b] = programs(context, &sources, "src/main.zry").expect("ABI-negative source must lower");
                compare::m3(&a, &b);
                let invocation = Invocation::new(case["export"].as_str().expect("export").to_owned(), runtime::m3_arguments(&case["arguments"]));
                let a = a.verified_ir().scalar_abi().prepare_invocation(invocation.clone()).expect_err("frozen ABI rejection");
                let b = b.verified_ir().scalar_abi().prepare_invocation(invocation).expect_err("native ABI rejection");
                assert_eq!(a, b);
                assert_eq!(a.code(), case["code"].as_str().expect("ABI code"));
                json!({"syntax":"accepted","semantic":"accepted","artifacts":"not-emitted","runtime":"rejected",
                    "abi_code":a.code(),"owning_phase":"scalar-abi-admission"})
            } else {
                let result = programs(context, &sources, "src/main.zry").expect_err("source negative must reject");
                assert_eq!(result["semantic"], "rejected");
                assert!(result["codes"].as_array().expect("negative codes").iter().any(|code| code == &case["code"]));
                result
            }
        });
    }
    for case in faults {
        context.blocked(&format!("m3-fault:{}", case["id"].as_str().expect("fault ID")), "m3",
            "canonical injected-fault execution and typed cleanup observation are private; this receipt does not substitute byte equality for actual fault outcomes");
    }
}

fn programs(
    context: &Context,
    sources: &SourceMap,
    entry: &str,
) -> Result<[zryna_semantics::data_ownership_v1::VerifiedProgram; 2], Value> {
    let [a, b] = providers::pair4(context, sources)?;
    let entry = corpus::entry(sources, entry);
    let lower = |syntax: &zryna_frontend::syntax_v4::ProjectSyntaxSnapshot| {
        match zryna_semantics::data_ownership_v1::SemanticInput::try_new(syntax, sources, entry) {
            Some(input) => zryna_semantics::data_ownership_v1::lower(input),
            None => {
                assert!(syntax.is_bound_to(sources));
                assert!(
                    !syntax.diagnostics().is_empty(),
                    "input rejection must carry owning diagnostics"
                );
                Err(syntax.diagnostics().to_vec())
            }
        }
    };
    let a = lower(&a);
    let b = lower(&b);
    match (a, b) {
        (Ok(a), Ok(b)) => Ok([a, b]),
        (Err(a), Err(b)) => Err(
            json!({"syntax":"accepted","semantic":"rejected","artifacts":"not-emitted","runtime":"not-run",
            "diagnostics":compare::diagnostics(&a, &b, sources),"codes":compare::codes(&a)}),
        ),
        _ => panic!("M3 semantic acceptance differs"),
    }
}

fn downstream(context: &Context, sources: &SourceMap, entry: &str) -> Value {
    match programs(context, sources, entry) {
        Ok([a, b]) => {
            compare::m3(&a, &b);
            json!({"syntax":"accepted","semantic":"accepted","artifacts":"matched","runtime":"not-run",
                "artifact_hashes":compare::emit3(&a).compare(&compare::emit3(&b))})
        }
        Err(result) => result,
    }
}

fn invocation_case(
    context: &mut Context,
    registry: &Value,
    case: &Value,
    prefix: &str,
    expected: Value,
) {
    let id = case["id"].as_str().expect("stable runtime case ID");
    let fixture = registry["fixtures"]
        .as_array()
        .expect("fixtures")
        .iter()
        .find(|fixture| fixture["id"] == case["fixture"])
        .expect("runtime fixture");
    let source = fixture["path"].as_str().expect("original fixture");
    context.case(format!("{prefix}:{id}"), "m3", source, |context| {
        let files = corpus::m3_files(context, registry, fixture);
        let sources = corpus::sources(&files);
        let [a, b] =
            programs(context, &sources, "src/main.zry").expect("fixed runtime source must lower");
        compare::m3(&a, &b);
        let aa = compare::emit3(&a);
        let ba = compare::emit3(&b);
        let hashes = aa.compare(&ba);
        let arguments = runtime::m3_arguments(&case["arguments"]);
        let export = case["export"].as_str().expect("export");
        let observations = runtime::portable_pair(
            context,
            &format!("{prefix}-{id}"),
            [&aa, &ba],
            export,
            &arguments,
            &expected,
            true,
        );
        let native =
            native(context, &format!("{prefix}-{id}"), [&a, &b], export, arguments, &expected);
        json!({"syntax":"accepted","semantic":"accepted","artifacts":"matched","runtime":"executed",
            "artifact_hashes":hashes,"portable_observations":observations,"native":native})
    });
}

fn native(
    context: &Context,
    label: &str,
    programs: [&zryna_semantics::data_ownership_v1::VerifiedProgram; 2],
    export: &str,
    arguments: Vec<zryna_abi::ScalarValue>,
    expected: &Value,
) -> Value {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return json!({"status":"platform-unavailable","reason":"existing native linking and execution require Linux x86-64"});
    }
    let workspace = corpus::install(context, &format!("native-{label}"), &[]);
    let output = zryna_driver::ArtifactOutputRoot::prepare_for_workspace(&workspace)
        .expect("native output capability");
    let limits = zryna_driver::NativeProcessLimits::default();
    let toolchain =
        zryna_driver::discover_linux_native_toolchain(limits).expect("authenticated GNU toolchain");
    let invocation = Invocation::new(export.to_owned(), arguments);
    let a = zryna_driver::prepare_data_ownership_executable(
        programs[0],
        invocation.clone(),
        &output,
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        limits,
    )
    .expect("actual bootstrap M3 link");
    let b = zryna_driver::prepare_data_ownership_executable(
        programs[1],
        invocation,
        &output,
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        limits,
    )
    .expect("actual native-provider M3 link");
    assert_eq!(a.object_bytes(), b.object_bytes());
    assert_eq!(
        a.executable_bytes(),
        b.executable_bytes(),
        "same pinned toolchain linked executable bytes"
    );
    let a = zryna_driver::publish_data_ownership_executable(&a, &output, "bootstrap")
        .expect("create-only bootstrap executable");
    let b = zryna_driver::publish_data_ownership_executable(&b, &output, "native")
        .expect("create-only native executable");
    let a = zryna_driver::run_native_invocation(&a, limits).expect("actual bootstrap M3 run");
    let b = zryna_driver::run_native_invocation(&b, limits).expect("actual native-provider M3 run");
    assert_eq!(a, b);
    let expected = if expected["type"] == "trap" {
        ScalarOutcome::Trapped { code: ScalarTrapCode::Bounds }
    } else {
        ScalarOutcome::Returned { value: runtime::scalar(expected) }
    };
    assert_eq!(a, expected, "frozen actual native outcome");
    json!({"status":"executed","outcome":a})
}

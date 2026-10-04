use crate::{Context, closures, compare, corpus, providers, runtime};
use serde_json::{Value, json};
use zryna_source::SourceMap;

pub fn run(context: &mut Context) {
    let registry = corpus::registry(context, "m2");
    let inventory = corpus::fixture_files(&context.root.join("tests/m2-fixtures"));
    assert_eq!(inventory.len(), 14, "complete frozen M2 source inventory");
    let fixtures = registry["fixtureFiles"].as_array().expect("frozen M2 files");
    assert_eq!(fixtures.len(), 14);
    let files = fixtures
        .iter()
        .map(|fixture| {
            (
                fixture["path"].as_str().expect("path").to_owned(),
                corpus::checked_text(context, fixture),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        inventory.iter().map(|name| format!("tests/m2-fixtures/{name}")).collect::<Vec<_>>(),
        files.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(),
        "registry covers exact source inventory"
    );
    let workspace = corpus::install(context, "m2-complete", &files);
    for relative in inventory {
        let source = format!("tests/m2-fixtures/{relative}");
        context.case(format!("m2-source:{relative}"), "m2", &source, |context| {
            closures::m2(context, &workspace, &source, |sources| {
                compare_downstream(context, sources, &source, None)
            })
        });
    }
    let invalid = registry["invalidCases"].as_array().expect("negative registry");
    assert_eq!(invalid.len(), 9);
    for case in invalid {
        let id = case["id"].as_str().expect("frozen rejection ID");
        let source = case["entrypoint"].as_str().expect("negative entrypoint");
        context.case(format!("m2-invalid:{id}"), "m2", source, |context| {
            let result = closures::m2(context, &workspace, source, |sources| {
                compare_downstream(context, sources, source, Some(&case["diagnosticCodes"]))
            });
            if result["semantic"] == "not-entered" {
                assert_eq!(
                    result["codes"], case["diagnosticCodes"],
                    "fixed resolution/syntax diagnostic codes"
                );
            } else {
                assert_eq!(
                    result["semantic"], "rejected",
                    "frozen negative cannot become accepted"
                );
            }
            result
        });
    }
    let valid = registry["validCases"].as_array().expect("valid registry");
    assert_eq!(valid.len(), 20);
    let source = registry["graph"]["entrypoint"].as_str().expect("frozen graph entry");
    for case in valid {
        let id = case["id"].as_str().expect("frozen invocation ID");
        context.case(format!("m2-run:{id}"), "m2", source, |context| {
            let result = closures::m2(context, &workspace, source, |sources| {
                let [a, b] = programs(context, sources, source).expect("frozen valid graph must lower");
                compare::m2(&a, &b);
                let a = compare::emit2(&a);
                let b = compare::emit2(&b);
                let hashes = a.compare(&b);
                assert_frozen_artifacts(&hashes, &registry);
                let observations = runtime::portable_pair(context, &format!("m2-run-{id}"), [&a, &b],
                    case["export"].as_str().expect("export"), &runtime::typed_arguments(&case["arguments"]),
                    &case["expected"], false);
                json!({"syntax":"accepted","semantic":"accepted","artifacts":"matched","runtime":"executed",
                    "artifact_hashes":hashes,"portable_observations":observations,"native_execution":"blocked-private-api"})
            });
            assert_eq!(result["semantic"], "accepted", "fixed positive is executed");
            result
        });
    }
}

fn assert_frozen_artifacts(hashes: &Value, registry: &Value) {
    let expected = registry["graph"]["buildArtifacts"].as_array().expect("frozen artifacts");
    assert_eq!(expected.len(), 3);
    for artifact in expected {
        let target = artifact["target"].as_str().expect("frozen target");
        let key = if target == "native" { "native-object" } else { target };
        assert_eq!(hashes[key]["sha256"], artifact["sha256"], "frozen {target} artifact digest");
        assert_eq!(hashes[key]["bytes"], artifact["bytes"], "frozen {target} artifact length");
    }
}

fn programs(
    context: &Context,
    sources: &SourceMap,
    entry: &str,
) -> Result<[zryna_ir::control_flow_v1::VerifiedProgram; 2], Value> {
    let [a, b] = providers::pair3(context, sources)?;
    let entry = corpus::entry(sources, entry);
    let lower = |syntax: &zryna_frontend::syntax_v3::ProjectSyntaxSnapshot| {
        match zryna_semantics::control_flow_v1::SemanticInput::try_new(syntax, sources, entry) {
            Some(input) => zryna_semantics::control_flow_v1::lower(input),
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
        _ => panic!("M2 semantic acceptance differs"),
    }
}

fn compare_downstream(
    context: &Context,
    sources: &SourceMap,
    entry: &str,
    expected: Option<&Value>,
) -> Value {
    match programs(context, sources, entry) {
        Ok([a, b]) => {
            assert!(expected.is_none(), "frozen negative cannot lower");
            compare::m2(&a, &b);
            json!({"syntax":"accepted","semantic":"accepted","artifacts":"matched","runtime":"not-run",
                "artifact_hashes":compare::emit2(&a).compare(&compare::emit2(&b))})
        }
        Err(result) => {
            if let Some(expected) = expected {
                assert_eq!(&result["codes"], expected, "frozen negative codes");
            }
            result
        }
    }
}

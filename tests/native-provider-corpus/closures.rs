use crate::{Context, compare, corpus, providers};
use serde_json::{Value, json};
use std::path::Path;
use zryna_diagnostics::Diagnostic;
use zryna_driver::{ModuleClosureError, WorkspaceSourceRoot, capture_native_workspace_sources};
use zryna_source::SourceMap;

fn diagnostics(error: &ModuleClosureError) -> Vec<Diagnostic> {
    match error {
        ModuleClosureError::Frontend(error) if error.diagnostics().is_empty() => {
            vec![Diagnostic::error(
                error.code(),
                None,
                error.to_string(),
                "verify the pinned Node.js runtime and TypeScript frontend, then retry",
            )]
        }
        _ => error.diagnostics().to_vec(),
    }
}

fn rejection(left: &ModuleClosureError, right: &ModuleClosureError) -> Value {
    let left = diagnostics(left);
    let right = diagnostics(right);
    // Independent discovery runs allocate separate nominal SourceMap identities. The stable
    // diagnostic serialization contains canonical file indices and byte spans, not that nonce.
    // Source-capture rejection does not return a SourceMap, so never render against a foreign map.
    let evidence = serde_json::to_value(&left).expect("canonical closure diagnostics");
    let native = serde_json::to_value(&right).expect("native closure diagnostics");
    assert!(!left.is_empty());
    assert_eq!(evidence, native, "exact canonical source-capture rejection");
    json!({"syntax":"rejected","semantic":"not-entered","artifacts":"not-emitted",
        "runtime":"not-run","diagnostics":evidence,"codes":compare::codes(&left)})
}

fn edges(edges: &[zryna_driver::ModuleEdge]) -> Vec<Value> {
    edges.iter().map(|edge| {
        let span = |span: zryna_source::Span| json!([span.file().index(), span.start(), span.end()]);
        json!({"importer":edge.importer().as_str(),"target":edge.target().as_str(),
            "specifier":edge.specifier(),"imported":edge.imported(),"local":edge.local(),
            "declaration":span(edge.declaration_span()),"specifier_span":span(edge.specifier_span()),
            "imported_span":span(edge.imported_span()),"local_span":span(edge.local_span())})
    }).collect()
}

pub fn m2(
    context: &Context,
    workspace: &Path,
    entry: &str,
    check: impl FnOnce(&SourceMap) -> Value,
) -> Value {
    let root = WorkspaceSourceRoot::capture(workspace).expect("no-follow workspace capability");
    let captured = capture_native_workspace_sources(&root, corpus::path(entry));
    let worker = zryna_driver::discover_module_closure(
        &root,
        corpus::path(entry),
        &providers::worker3(context),
    );
    match captured {
        Err(native) => {
            let worker =
                worker.expect_err("native source rejection must also reject bootstrap discovery");
            rejection(&worker, &native)
        }
        Ok(captured) => {
            let sources = captured.sources().clone();
            match (worker, captured.verify_v3()) {
                (Ok(worker), Ok(native)) => {
                    assert_eq!(worker.modules(), native.closure().modules());
                    assert_eq!(edges(worker.edges()), edges(native.closure().edges()));
                    assert_eq!(worker.graph_sha256(), native.closure().graph_sha256());
                    let registry = corpus::registry(context, "m2");
                    if registry["graph"]["entrypoint"] == entry {
                        assert_eq!(
                            corpus::hex(native.closure().graph_sha256()),
                            registry["graph"]["sha256"].as_str().expect("frozen graph identity")
                        );
                    }
                    providers::snapshots_match(context, worker.syntax(), native.closure().syntax());
                    native.revalidate().expect("retained original source graph before downstream");
                    let mut result = check(&sources);
                    result["source_graph_sha256"] =
                        json!(corpus::hex(native.closure().graph_sha256()));
                    native.revalidate().expect("retained original source graph after downstream");
                    result
                }
                (Err(worker), Err(native)) => {
                    let evidence = rejection(&worker, &native);
                    let direct = providers::pair3(context, &sources)
                        .expect_err("source-bound syntax rejection");
                    assert_eq!(direct["codes"], evidence["codes"]);
                    direct
                }
                _ => panic!("native/bootstrap authenticated closure acceptance differs"),
            }
        }
    }
}

pub fn m3(
    context: &Context,
    workspace: &Path,
    entry: &str,
    check: impl FnOnce(&SourceMap) -> Value,
) -> Value {
    let root = WorkspaceSourceRoot::capture(workspace).expect("no-follow workspace capability");
    let captured = capture_native_workspace_sources(&root, corpus::path(entry));
    let worker = zryna_driver::discover_ownership_module_closure(
        &root,
        corpus::path(entry),
        &providers::worker4(context),
    );
    match captured {
        Err(native) => {
            let worker =
                worker.expect_err("native source rejection must also reject bootstrap discovery");
            rejection(&worker, &native)
        }
        Ok(captured) => {
            let sources = captured.sources().clone();
            match (worker, captured.verify_v4()) {
                (Ok(worker), Ok(native)) => {
                    assert_eq!(worker.modules(), native.closure().modules());
                    assert_eq!(edges(worker.edges()), edges(native.closure().edges()));
                    assert_eq!(worker.graph_sha256(), native.closure().graph_sha256());
                    providers::snapshots_match(context, worker.syntax(), native.closure().syntax());
                    native.revalidate().expect("retained original source graph before downstream");
                    let mut result = check(&sources);
                    result["source_graph_sha256"] =
                        json!(corpus::hex(native.closure().graph_sha256()));
                    native.revalidate().expect("retained original source graph after downstream");
                    result
                }
                (Err(worker), Err(native)) => {
                    let evidence = rejection(&worker, &native);
                    let direct = providers::pair4(context, &sources)
                        .expect_err("source-bound syntax rejection");
                    assert_eq!(direct["codes"], evidence["codes"]);
                    direct
                }
                _ => panic!("native/bootstrap authenticated closure acceptance differs"),
            }
        }
    }
}

//! Test-only complete sealed observations; no production serializer or provider activation.
#![forbid(unsafe_code)]
#![recursion_limit = "256"]
mod projection;
mod providers;

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

struct Context {
    root: PathBuf,
    node: PathBuf,
    output: PathBuf,
}
mod corpus {
    use sha2::{Digest, Sha256};
    pub fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
}
mod compare {
    use serde_json::Value;
    use zryna_diagnostics::{Diagnostic, render_json};
    use zryna_source::SourceMap;
    pub fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics.iter().map(Diagnostic::code).collect()
    }
    pub fn diagnostics(left: &[Diagnostic], right: &[Diagnostic], sources: &SourceMap) -> Value {
        assert!(!left.is_empty());
        let a = render_json(left, sources).expect("source-bound worker diagnostics");
        let b = render_json(right, sources).expect("source-bound native diagnostics");
        assert_eq!(a, b, "exact owning diagnostics");
        serde_json::from_str(&a).expect("diagnostic JSON")
    }
}
fn path(value: &str) -> NormalizedSourcePath {
    NormalizedSourcePath::new(value).expect("canonical inventory path")
}
fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().expect("typed inventory field")
}
fn source_rows(sources: &SourceMap) -> Vec<Value> {
    (0..sources.len())
        .map(|index| {
            let id = sources
                .verify_file_id(u32::try_from(index).expect("bounded file count"))
                .expect("canonical FileId");
            let file = sources.source(id).expect("original source authority");
            json!({"file_id":file.id().index(),
        "path":file.path().as_str(),"sha256":corpus::digest(file.text().as_bytes()),
        "bytes":file.text().len()})
        })
        .collect()
}
fn edges(edges: &[zryna_driver::ModuleEdge]) -> Vec<Value> {
    edges.iter().map(|edge| {
        let span = |span: zryna_source::Span| json!([span.file().index(),span.start(),span.end()]);
        json!({"importer":edge.importer().as_str(),"target":edge.target().as_str(),
            "specifier":edge.specifier(),"imported":edge.imported(),"local":edge.local(),
            "declaration":span(edge.declaration_span()),"specifier_span":span(edge.specifier_span()),
            "imported_span":span(edge.imported_span()),"local_span":span(edge.local_span())})
    }).collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn observation(program: &impl std::fmt::Debug, getters: Value) -> Value {
    let raw_debug = format!("{program:?}");
    assert!(raw_debug.len() <= 2 * 1024 * 1024, "bounded complete sealed observation");
    json!({"schema_version":1,"raw_debug":raw_debug,"getters":getters})
}
fn lower(context: &Context, case: &Value, sources: &SourceMap) -> [Value; 2] {
    let entry = sources.file_id(&path(text(case, "entrypoint"))).expect("explicit entrypoint");
    match text(case, "profile") {
        "m1" => {
            let pair = providers::pair2(context, sources).expect("accepted original syntax");
            pair.map(|syntax| {
                let lowered = zryna_driver::lower_verified_syntax(&syntax, sources)
                    .expect("accepted original M1 IR");
                observation(lowered.program(), projection::m1(lowered.program()))
            })
        }
        "m2" => {
            let pair = providers::pair3(context, sources).expect("accepted original syntax");
            pair.map(|syntax| {
                let input = zryna_semantics::control_flow_v1::SemanticInput::try_new(
                    &syntax, sources, entry,
                )
                .expect("bound semantic input");
                let program = zryna_semantics::control_flow_v1::lower(input)
                    .expect("accepted original M2 IR");
                observation(&program, projection::m2(&program))
            })
        }
        "m3" => {
            let pair = providers::pair4(context, sources).expect("accepted original syntax");
            pair.map(|syntax| {
                let input = zryna_semantics::data_ownership_v1::SemanticInput::try_new(
                    &syntax, sources, entry,
                )
                .expect("bound semantic input");
                let program = zryna_semantics::data_ownership_v1::lower(input)
                    .expect("accepted original M3 IR including ABI-negative case");
                observation(program.verified_ir(), projection::m3(program.verified_ir()))
            })
        }
        _ => panic!("unrecognized profile"),
    }
}
fn captured(
    context: &Context,
    case: &Value,
    workspace: &Path,
) -> (SourceMap, Value, Vec<Value>, [Value; 2]) {
    let root = zryna_driver::WorkspaceSourceRoot::capture(workspace)
        .expect("no-follow original source capability");
    let entry = path(text(case, "entrypoint"));
    match text(case, "profile") {
        "m2" => {
            let capture =
                zryna_driver::native_frontend::capture_control_flow_sources(&root, entry.clone())
                    .expect("native original capture");
            let sources = capture.sources().clone();
            let worker =
                zryna_driver::discover_module_closure(&root, entry, &providers::worker3(context))
                    .expect("independent bootstrap discovery");
            let verified = capture.verify_v3().expect("mandatory source-bound v3 verifier");
            assert_eq!(worker.modules(), verified.closure().modules());
            assert_eq!(edges(worker.edges()), edges(verified.closure().edges()));
            assert_eq!(worker.graph_sha256(), verified.closure().graph_sha256());
            providers::snapshots_match(context, worker.syntax(), verified.closure().syntax());
            verified.revalidate().expect("retained source before IR");
            let observations = lower(context, case, &sources);
            verified.revalidate().expect("retained source after IR");
            (
                sources,
                json!(hex(verified.closure().graph_sha256())),
                edges(verified.closure().edges()),
                observations,
            )
        }
        "m3" => {
            let capture = zryna_driver::capture_native_workspace_sources(&root, entry.clone())
                .expect("native original capture");
            let sources = capture.sources().clone();
            let worker = zryna_driver::discover_ownership_module_closure(
                &root,
                entry,
                &providers::worker4(context),
            )
            .expect("independent bootstrap discovery");
            let verified = capture.verify_v4().expect("mandatory source-bound v4 verifier");
            assert_eq!(worker.modules(), verified.closure().modules());
            assert_eq!(edges(worker.edges()), edges(verified.closure().edges()));
            assert_eq!(worker.graph_sha256(), verified.closure().graph_sha256());
            providers::snapshots_match(context, worker.syntax(), verified.closure().syntax());
            verified.revalidate().expect("retained source before IR");
            let observations = lower(context, case, &sources);
            verified.revalidate().expect("retained source after IR");
            (
                sources,
                json!(hex(verified.closure().graph_sha256())),
                edges(verified.closure().edges()),
                observations,
            )
        }
        _ => panic!("capture only applies to module profiles"),
    }
}
fn main() {
    let args = std::env::args_os().skip(1).map(PathBuf::from).collect::<Vec<_>>();
    assert_eq!(args.len(), 4, "collector ROOT NODE INVENTORY OUTPUT");
    assert!(args.iter().all(|path| path.is_absolute()));
    let [root, node, inventory, output]: [PathBuf; 4] = args.try_into().expect("four arguments");
    assert!(root.is_dir() && node.is_file() && inventory.is_file());
    assert!(!output.starts_with(&root) && !output.exists(), "external create-only output");
    fs::create_dir(&output).expect("owned output");
    let input = fs::read(&inventory).expect("inventory bytes");
    let inventory: Value = serde_json::from_slice(&input).expect("inventory JSON");
    assert_eq!(inventory["cases"].as_array().expect("cases").len(), 107);
    let context = Context { root, node, output };
    let mut records = Vec::new();
    for (ordinal, case) in inventory["cases"].as_array().expect("cases").iter().enumerate() {
        eprintln!("sealed IR {}", text(case, "id"));
        let directory = context.output.join(format!("case-{ordinal:03}"));
        fs::create_dir(&directory).expect("create-only case");
        let workspace = directory.join("sources");
        fs::create_dir(&workspace).expect("isolated original context");
        let mut files = Vec::new();
        for source in case["sources"].as_array().expect("source closure") {
            let origin = context.root.join(text(source, "origin"));
            let data = fs::read(origin).expect("exact original bytes");
            assert_eq!(corpus::digest(&data), text(source, "sha256"));
            assert_eq!(data.len() as u64, source["bytes"].as_u64().expect("source length"));
            let relative = text(source, "path");
            assert_eq!(path(relative).as_str(), relative);
            let destination = workspace.join(relative);
            fs::create_dir_all(destination.parent().expect("source parent"))
                .expect("source directories");
            fs::write(destination, &data).expect("exact source copy");
            files.push(SourceFileInput {
                path: relative.to_owned(),
                text: String::from_utf8(data).expect("original UTF-8"),
            });
        }
        let (sources, graph, actual_edges, observations) = if case["profile"] == "m1" {
            let sources = SourceMap::build(files).expect("same canonical source map");
            let observed = lower(&context, case, &sources);
            (sources, Value::Null, vec![], observed)
        } else {
            captured(&context, case, &workspace)
        };
        let actual_sources = source_rows(&sources);
        for (actual, expected) in
            actual_sources.iter().zip(case["sources"].as_array().expect("sources"))
        {
            for field in ["file_id", "path", "sha256", "bytes"] {
                assert_eq!(actual[field], expected[field]);
            }
        }
        assert_eq!(actual_sources.len(), case["sources"].as_array().expect("sources").len());
        assert_eq!(graph, case["graph_sha256"], "independently derived graph");
        assert_eq!(json!(actual_edges), case["edges"], "targets and spans outside graph encoding");
        let artifacts = observations.into_iter().enumerate().map(|(side,value)| {
            let provider = if side == 0 {"worker"} else {"native"};
            let relative = format!("case-{ordinal:03}/{provider}.json");
            let data = serde_json::to_vec(&value).expect("complete observation JSON");
            assert!(data.len() <= 8 * 1024 * 1024,"bounded getter projection");
            fs::write(context.output.join(&relative),&data).expect("retain both sides including divergence");
            json!({"provider":provider,"path":relative,"sha256":corpus::digest(&data),"bytes":data.len()})
        }).collect::<Vec<_>>();
        records.push(json!({"id":case["id"],"profile":case["profile"],"entrypoint":case["entrypoint"],
            "sources":actual_sources,"edges":actual_edges,"graph_sha256":graph,"observations":artifacts}));
    }
    println!(
        "{}",
        json!({"schema_version":1,"inventory_sha256":corpus::digest(&input),
        "public_activation":false,"observation_format":"revision-bound-sealed-Debug-plus-getters-v1",
        "cases":records})
    );
}

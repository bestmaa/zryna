use crate::{Context, compare};
use serde_json::{Value, json};
use std::ffi::OsString;
use zryna_diagnostics::Diagnostic;
use zryna_frontend::{
    FrontendCapabilities, ProviderExpectation, ProviderExpectationV3, ProviderExpectationV4,
    WorkerFrontend, WorkerFrontendV3, WorkerFrontendV4, WorkerLimits, WorkerLimitsV3,
    WorkerLimitsV4, WorkerSpec, WorkerSpecV3, WorkerSpecV4, native_lexer::lex, native_parser,
    syntax_v2, syntax_v3, syntax_v4,
};
use zryna_source::SourceMap;

fn location(
    context: &Context,
    version: u32,
) -> (std::path::PathBuf, Vec<OsString>, std::path::PathBuf) {
    let script = if version == 2 {
        "src/worker.mjs".to_owned()
    } else {
        format!("src/worker-v{version}.mjs")
    };
    // Preserve a normal Windows cwd; canonicalization can introduce Node-incompatible verbatim paths.
    (context.node.clone(), vec![script.into()], context.root.join("adapters/typescript-6"))
}

pub fn worker2(context: &Context) -> WorkerFrontend {
    let (node, args, cwd) = location(context, 2);
    WorkerFrontend::new(
        WorkerSpec::new(
            node,
            args,
            cwd,
            ProviderExpectation::new(
                "typescript-6",
                "6.0.3",
                2,
                FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
            )
            .expect("exact v2 handshake"),
            WorkerLimits::default(),
        )
        .expect("bounded worker2"),
    )
}
pub fn worker3(context: &Context) -> WorkerFrontendV3 {
    let (node, args, cwd) = location(context, 3);
    WorkerFrontendV3::new(
        WorkerSpecV3::new(
            node,
            args,
            cwd,
            ProviderExpectationV3::new("typescript-6", "6.0.3").expect("exact v3 handshake"),
            WorkerLimitsV3::default(),
        )
        .expect("bounded worker3"),
    )
}
pub fn worker4(context: &Context) -> WorkerFrontendV4 {
    let (node, args, cwd) = location(context, 4);
    WorkerFrontendV4::new(
        WorkerSpecV4::new(
            node,
            args,
            cwd,
            ProviderExpectationV4::new("typescript-6", "6.0.3").expect("exact v4 handshake"),
            WorkerLimitsV4::default(),
        )
        .expect("bounded worker4"),
    )
}

fn transport(error: zryna_frontend::WorkerError) -> Vec<Diagnostic> {
    if !error.diagnostics().is_empty() {
        return error.diagnostics().to_vec();
    }
    vec![Diagnostic::error(
        error.code(),
        None,
        error.to_string(),
        "verify the pinned Node.js runtime and TypeScript frontend, then retry",
    )]
}

fn compare_result<T: serde::Serialize>(
    context: &Context,
    left: Result<T, Vec<Diagnostic>>,
    right: Result<T, Vec<Diagnostic>>,
    sources: &SourceMap,
) -> Result<[T; 2], Value> {
    match (left, right) {
        (Ok(left), Ok(right)) => {
            snapshots_match(context, &left, &right);
            Ok([left, right])
        }
        (Err(left), Err(right)) => {
            let diagnostics = compare::diagnostics(&left, &right, sources);
            Err(json!({"syntax":"rejected","semantic":"not-entered","artifacts":"not-emitted",
                "runtime":"not-run","diagnostics":diagnostics,"codes":compare::codes(&left)}))
        }
        _ => panic!("worker/native syntax acceptance differs"),
    }
}

pub fn snapshots_match<T: serde::Serialize>(context: &Context, left: &T, right: &T) {
    let a = serde_json::to_vec(left).expect("worker syntax");
    let b = serde_json::to_vec(right).expect("native syntax");
    if a != b {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let ordinal = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let worker = context.output.join(format!("syntax-mismatch-{ordinal}-worker.json"));
        let native = context.output.join(format!("syntax-mismatch-{ordinal}-native.json"));
        std::fs::write(&worker, &a).expect("retain complete worker divergence");
        std::fs::write(&native, &b).expect("retain complete native divergence");
        panic!(
            "exact verified syntax differs: {} {} versus {} {}",
            worker.display(),
            crate::corpus::digest(&a),
            native.display(),
            crate::corpus::digest(&b)
        );
    }
}

pub fn pair2(
    context: &Context,
    sources: &SourceMap,
) -> Result<[syntax_v2::ProjectSyntaxSnapshot; 2], Value> {
    let left = worker2(context).analyze_verified(sources).map_err(transport);
    let right = lex(sources)
        .map_err(|error| vec![error.diagnostic().clone()])
        .and_then(|tokens| {
            native_parser::parse_v2_recovering_candidate(sources, &tokens)
                .map_err(|error| vec![error.diagnostic().clone()])
        })
        .and_then(|raw| syntax_v2::verify_snapshot(raw, sources));
    compare_result(context, left, right, sources)
}
pub fn pair3(
    context: &Context,
    sources: &SourceMap,
) -> Result<[syntax_v3::ProjectSyntaxSnapshot; 2], Value> {
    let left = worker3(context).analyze_verified_v3(sources).map_err(transport);
    let right = lex(sources)
        .map_err(|error| vec![error.diagnostic().clone()])
        .and_then(|tokens| {
            native_parser::v3::parse_v3_straight_line_candidate(sources, &tokens)
                .map_err(|error| vec![error.diagnostic().clone()])
        })
        .and_then(|raw| syntax_v3::verify_snapshot(raw, sources));
    compare_result(context, left, right, sources)
}
pub fn pair4(
    context: &Context,
    sources: &SourceMap,
) -> Result<[syntax_v4::ProjectSyntaxSnapshot; 2], Value> {
    let left = worker4(context).analyze_verified_v4(sources).map_err(transport);
    let right = lex(sources)
        .map_err(|error| vec![error.diagnostic().clone()])
        .and_then(|tokens| {
            native_parser::v4::parse_v4_candidate(sources, &tokens)
                .map_err(|error| vec![error.diagnostic().clone()])
        })
        .and_then(|raw| syntax_v4::verify_snapshot(raw, sources));
    compare_result(context, left, right, sources)
}

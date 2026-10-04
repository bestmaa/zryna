use crate::{Context, compare, corpus, providers, runtime};
use serde_json::{Value, json};
use zryna_abi::{Invocation, ScalarOutcome};

pub fn run(context: &mut Context) {
    let registry = corpus::registry(context, "m1");
    assert_eq!(registry["cases"].as_array().expect("M1 cases").len(), 3);
    let source = registry["entrypoint"].as_str().expect("M1 source");
    context.case("m1-source:valid".to_owned(), "m1", source, |context| {
        let sources = corpus::sources(&[(source.to_owned(), corpus::text(context, source))]);
        let [left, right] = providers::pair2(context, &sources).expect("registry source must have verified syntax");
        let left = zryna_driver::lower_verified_syntax(&left, &sources).expect("M1 registry semantic acceptance");
        let right = zryna_driver::lower_verified_syntax(&right, &sources).expect("native M1 semantic acceptance");
        compare::m1(left.program(), right.program());
        let artifacts = compare::emit1(left.program()).compare(&compare::emit1(right.program()));
        json!({"syntax":"accepted","semantic":"accepted","artifacts":"matched","runtime":"not-run",
            "artifact_hashes":artifacts,"source_sha256":corpus::digest(corpus::text(context, source).as_bytes())})
    });
    for (id, key) in [("invalid-any", "invalidSource"), ("bool-gated", "gatedBooleanSource")] {
        let source = registry[key]["path"].as_str().expect("negative source");
        context.case(format!("m1-source:{id}"), "m1", source, |context| {
            let sources = corpus::sources(&[(source.to_owned(), corpus::text(context, source))]);
            let [left, right] = providers::pair2(context, &sources).expect("negative M1 verified syntax");
            let left = zryna_driver::lower_verified_syntax(&left, &sources).expect_err("frozen negative must reject");
            let right = zryna_driver::lower_verified_syntax(&right, &sources).expect_err("native negative must reject");
            assert!(compare::codes(&left).contains(&registry[key]["expectedCode"].as_str().expect("frozen code")));
            json!({"syntax":"accepted","semantic":"rejected","artifacts":"not-emitted","runtime":"not-run",
                "diagnostics":compare::diagnostics(&left, &right, &sources)})
        });
    }
    for case in registry["cases"].as_array().expect("fixed invocations") {
        let id = case["id"].as_str().expect("stable M1 ID");
        context.case(format!("m1-run:{id}"), "m1", source, |context| {
            let sources = corpus::sources(&[(source.to_owned(), corpus::text(context, source))]);
            let [left, right] = providers::pair2(context, &sources).expect("M1 source syntax");
            let left = zryna_driver::lower_verified_syntax(&left, &sources).expect("M1 source IR");
            let right = zryna_driver::lower_verified_syntax(&right, &sources).expect("native M1 source IR");
            compare::m1(left.program(), right.program());
            let a = compare::emit1(left.program());
            let b = compare::emit1(right.program());
            let artifacts = a.compare(&b);
            let export = registry["export"].as_str().expect("M1 export");
            let arguments = runtime::typed_arguments(&case["arguments"]);
            let expected = case["expected"].clone();
            let observations = runtime::portable_pair(context, &format!("m1-{id}"), [&a, &b], export, &arguments, &expected, false);
            let native = native(context, id, &sources, export, arguments, &expected);
            json!({"syntax":"accepted","semantic":"accepted","artifacts":"matched","runtime":"executed",
                "artifact_hashes":artifacts,"portable_observations":observations,"native":native})
        });
    }
}

fn native(
    context: &Context,
    id: &str,
    sources: &zryna_source::SourceMap,
    export: &str,
    arguments: Vec<zryna_abi::ScalarValue>,
    expected: &Value,
) -> Value {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return json!({"status":"platform-unavailable","reason":"existing native linking and execution require Linux x86-64"});
    }
    let workspace = corpus::install(context, &format!("m1-native-{id}"), &[]);
    let output = zryna_driver::ArtifactOutputRoot::prepare_for_workspace(&workspace)
        .expect("native output capability");
    let limits = zryna_driver::NativeProcessLimits::default();
    let toolchain = zryna_driver::discover_linux_native_toolchain(limits)
        .expect("authenticated GNU native toolchain");
    let invocation = Invocation::new(export.to_owned(), arguments);
    let worker = providers::worker2(context);
    let built = zryna_driver::compile_native_invocation(
        &worker,
        sources,
        &output,
        "bootstrap",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        invocation.clone(),
        limits,
    )
    .expect("actual M1 bootstrap link");
    let candidate = Native;
    let native = zryna_driver::compile_native_invocation(
        &candidate,
        sources,
        &output,
        "native",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        invocation,
        limits,
    )
    .expect("actual M1 native-provider link");
    let left_bytes = std::fs::read(built.artifact().path()).expect("bootstrap executable bytes");
    let right_bytes = std::fs::read(native.artifact().path()).expect("native executable bytes");
    assert_eq!(left_bytes, right_bytes, "same toolchain linked executable parity");
    let left = zryna_driver::run_native_invocation(built.artifact(), limits)
        .expect("actual bootstrap executable run");
    let right = zryna_driver::run_native_invocation(native.artifact(), limits)
        .expect("actual native-provider executable run");
    assert_eq!(left, right);
    assert_eq!(left, ScalarOutcome::Returned { value: runtime::scalar(expected) });
    json!({"status":"executed","outcome":left,"executable_sha256":corpus::digest(&left_bytes)})
}

struct Native;
impl zryna_frontend::VerifiedFrontendProvider for Native {
    fn analyze_verified(
        &self,
        sources: &zryna_source::SourceMap,
    ) -> Result<zryna_frontend::syntax_v2::ProjectSyntaxSnapshot, zryna_frontend::WorkerError> {
        let tokens =
            zryna_frontend::native_lexer::lex(sources).expect("real native lexical authority");
        let raw = zryna_frontend::native_parser::parse_v2_recovering_candidate(sources, &tokens)
            .expect("real native candidate");
        Ok(zryna_frontend::syntax_v2::verify_snapshot(raw, sources)
            .expect("mandatory native syntax verifier"))
    }
}

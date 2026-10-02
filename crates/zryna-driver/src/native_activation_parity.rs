//! Bounded downstream parity evidence for #414, without public provider activation.
//! Both candidates use one `SourceMap` and the existing mandatory syntax/IR verifiers.
//! These tests do not prove resolver, manifest, linked-executable or no-Node install parity.

use std::{env, ffi::OsString, path::PathBuf};

use zryna_backend_javascript as javascript;
use zryna_backend_native as native;
use zryna_backend_webassembly as webassembly;
use zryna_diagnostics::{Diagnostic, render_json};
use zryna_frontend::{
    FrontendCapabilities, ProviderExpectation, ProviderExpectationV3, ProviderExpectationV4,
    WorkerFrontend, WorkerFrontendV3, WorkerFrontendV4, WorkerLimits, WorkerLimitsV3,
    WorkerLimitsV4, WorkerSpec, WorkerSpecV3, WorkerSpecV4,
    native_lexer::lex,
    native_parser::{
        parse_v2_recovering_candidate, v3::parse_v3_straight_line_candidate, v4::parse_v4_candidate,
    },
    syntax_v2, syntax_v3, syntax_v4,
};
use zryna_ir::control_flow_v1 as m2_ir;
use zryna_semantics::{control_flow_v1 as m2, data_ownership_v1 as m3};
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

fn worker_location(version: u32) -> (PathBuf, Vec<OsString>, PathBuf) {
    let node = env::var_os("ZRYNA_NODE").map_or_else(
        || {
            let output = std::process::Command::new("node")
                .args(["-p", "process.execPath"])
                .output()
                .expect("pinned Node must be installed");
            assert!(output.status.success());
            PathBuf::from(String::from_utf8(output.stdout).expect("Node path UTF-8").trim())
        },
        PathBuf::from,
    );
    let adapter = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../adapters/typescript-6")
        .canonicalize()
        .expect("bootstrap adapter directory");
    let script = if version == 2 {
        "src/worker.mjs".to_owned()
    } else {
        format!("src/worker-v{version}.mjs")
    };
    (node, vec![OsString::from(script)], adapter)
}

fn sources(files: &[(&str, &str)]) -> SourceMap {
    SourceMap::build(
        files
            .iter()
            .rev()
            .map(|(path, text)| SourceFileInput {
                path: (*path).to_owned(),
                text: (*text).to_owned(),
            })
            .collect(),
    )
    .expect("bounded immutable sources")
}

fn v2_pair(sources: &SourceMap) -> [syntax_v2::ProjectSyntaxSnapshot; 2] {
    let (node, args, adapter) = worker_location(2);
    let expected = ProviderExpectation::new(
        "typescript-6",
        "6.0.3",
        2,
        FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
    )
    .expect("exact v2 expectation");
    let worker = WorkerFrontend::new(
        WorkerSpec::new(node, args, adapter, expected, WorkerLimits::default())
            .expect("bounded v2 worker"),
    )
    .analyze_verified(sources)
    .expect("verified bootstrap v2");
    let lexed = lex(sources).expect("bounded native lexer");
    let raw = parse_v2_recovering_candidate(sources, &lexed).expect("native v2 candidate");
    let native = syntax_v2::verify_snapshot(raw, sources).expect("verified native v2");
    [worker, native]
}

fn v3_pair(sources: &SourceMap) -> [syntax_v3::ProjectSyntaxSnapshot; 2] {
    let (node, args, adapter) = worker_location(3);
    let expected =
        ProviderExpectationV3::new("typescript-6", "6.0.3").expect("exact v3 expectation");
    let worker = WorkerFrontendV3::new(
        WorkerSpecV3::new(node, args, adapter, expected, WorkerLimitsV3::default())
            .expect("bounded v3 worker"),
    )
    .analyze_verified_v3(sources)
    .expect("verified bootstrap v3");
    let lexed = lex(sources).expect("bounded native lexer");
    let raw = parse_v3_straight_line_candidate(sources, &lexed).expect("native v3 candidate");
    let native = syntax_v3::verify_snapshot(raw, sources).expect("verified native v3");
    [worker, native]
}

fn v4_pair(sources: &SourceMap) -> [syntax_v4::ProjectSyntaxSnapshot; 2] {
    let (node, args, adapter) = worker_location(4);
    let expected =
        ProviderExpectationV4::new("typescript-6", "6.0.3").expect("exact v4 expectation");
    let worker = WorkerFrontendV4::new(
        WorkerSpecV4::new(node, args, adapter, expected, WorkerLimitsV4::default())
            .expect("bounded v4 worker"),
    )
    .analyze_verified_v4(sources)
    .expect("verified bootstrap v4");
    let lexed = lex(sources).expect("bounded native lexer");
    let raw = parse_v4_candidate(sources, &lexed).expect("native v4 candidate");
    let native = syntax_v4::verify_snapshot(raw, sources).expect("verified native v4");
    [worker, native]
}

fn entry(sources: &SourceMap) -> zryna_source::FileId {
    sources
        .file_id(&NormalizedSourcePath::new("src/main.zry").expect("entry path"))
        .expect("entry authority")
}

fn assert_diagnostics_equal(left: &[Diagnostic], right: &[Diagnostic], sources: &SourceMap) {
    assert!(!left.is_empty(), "negative case must actually produce diagnostics");
    assert!(left.iter().all(|diagnostic| diagnostic.primary_span().is_some()));
    assert_eq!(
        render_json(left, sources).expect("bootstrap diagnostics"),
        render_json(right, sources).expect("native diagnostics"),
        "exact diagnostic codes, messages, guidance, order, paths and byte spans"
    );
}

#[test]
fn m1_verified_ir_and_three_target_artifacts_are_provider_independent() {
    let sources = sources(&[(
        "src/main.zry",
        concat!(
            "// π: keep UTF-8 byte spans and CRLF\r\n",
            "export function add(left: i32, right: i32): i32 { return left + right + 1; }\r\n",
            "export function constant(): i32 { return 2147483647; }\r\n",
        ),
    )]);
    let [worker, candidate] = v2_pair(&sources);
    let left = zryna_driver::lower_verified_syntax(&worker, &sources).expect("bootstrap IR");
    let right = zryna_driver::lower_verified_syntax(&candidate, &sources).expect("native IR");
    assert!(left.diagnostics().is_empty());
    assert!(right.diagnostics().is_empty());
    let left_functions = left.program().functions().collect::<Vec<_>>();
    let right_functions = right.program().functions().collect::<Vec<_>>();
    assert_eq!(left_functions.len(), 2);
    assert_eq!(left_functions.len(), right_functions.len());
    for (left, right) in left_functions.iter().zip(&right_functions) {
        assert_eq!(left.export_name(), right.export_name());
        assert_eq!(left.parameters(), right.parameters());
        assert_eq!(left.return_type(), right.return_type());
        assert_eq!(left.expressions(), right.expressions());
        assert_eq!(left.body(), right.body());
    }
    assert_eq!(
        javascript::emit(left.program()).expect("bootstrap JS"),
        javascript::emit(right.program()).expect("native JS")
    );
    assert_eq!(
        webassembly::emit(left.program()).expect("bootstrap Wasm").bytes(),
        webassembly::emit(right.program()).expect("native Wasm").bytes()
    );
    let target =
        native::select_object_target(native::NATIVE_OBJECT_TARGET).expect("fixed ELF target");
    let left_mir = zryna_native_mir::lower(left.program()).expect("bootstrap sealed MIR");
    let right_mir = zryna_native_mir::lower(right.program()).expect("native sealed MIR");
    assert_eq!(
        native::emit_object(&left_mir, target).expect("bootstrap ELF").bytes(),
        native::emit_object(&right_mir, target).expect("native ELF").bytes()
    );
}

#[test]
fn m1_semantic_rejections_keep_exact_provider_independent_diagnostics() {
    for text in [
        "// π\r\nexport function f(): i32 { return missing; }\r\n",
        "export function f(): i32 { return 2147483648; }",
        "export function f(value: i32): i32 { return true + value; }",
    ] {
        let sources = sources(&[("src/main.zry", text)]);
        let [worker, candidate] = v2_pair(&sources);
        let left = zryna_driver::lower_verified_syntax(&worker, &sources).expect_err("invalid M1");
        let right =
            zryna_driver::lower_verified_syntax(&candidate, &sources).expect_err("invalid M1");
        assert_diagnostics_equal(&left, &right, &sources);
    }
}

fn assert_m2_ir_equal(left: &m2_ir::VerifiedProgram, right: &m2_ir::VerifiedProgram) {
    assert_eq!(left.entry_module(), right.entry_module());
    assert_eq!(left.scalar_abi(), right.scalar_abi());
    let left_modules = left.modules().collect::<Vec<_>>();
    let right_modules = right.modules().collect::<Vec<_>>();
    assert_eq!(left_modules.len(), right_modules.len());
    for (left, right) in left_modules.iter().zip(&right_modules) {
        assert_eq!(left.id(), right.id());
        assert_eq!(left.source_file(), right.source_file());
        let left_functions = left.functions().collect::<Vec<_>>();
        let right_functions = right.functions().collect::<Vec<_>>();
        assert_eq!(left_functions.len(), right_functions.len());
        for (left, right) in left_functions.iter().zip(&right_functions) {
            assert_eq!(left.id(), right.id());
            assert_eq!(
                left.parameters().collect::<Vec<_>>(),
                right.parameters().collect::<Vec<_>>()
            );
            assert_eq!(left.result(), right.result());
            assert_eq!(left.span(), right.span());
            let left_blocks = left.blocks().collect::<Vec<_>>();
            let right_blocks = right.blocks().collect::<Vec<_>>();
            assert_eq!(left_blocks.len(), right_blocks.len());
            for (left, right) in left_blocks.iter().zip(&right_blocks) {
                assert_eq!(left.id(), right.id());
                assert_eq!(
                    left.parameters().collect::<Vec<_>>(),
                    right.parameters().collect::<Vec<_>>()
                );
                let left_instructions = left.instructions().collect::<Vec<_>>();
                let right_instructions = right.instructions().collect::<Vec<_>>();
                assert_eq!(left_instructions.len(), right_instructions.len());
                for (left, right) in left_instructions.iter().zip(&right_instructions) {
                    assert_eq!(left.result(), right.result());
                    assert_eq!(left.ty(), right.ty());
                    assert_eq!(left.span(), right.span());
                    assert_eq!(left.kind(), right.kind());
                }
                assert_eq!(left.terminator().span(), right.terminator().span());
                assert_eq!(left.terminator().kind(), right.terminator().kind());
            }
        }
    }
}

#[test]
fn m2_corpus_verified_ir_and_three_target_artifacts_are_provider_independent() {
    let sources = sources(&[
        ("src/main.zry", include_str!("../../../tests/m2-fixtures/valid/main.zry")),
        ("src/math.zry", include_str!("../../../tests/m2-fixtures/valid/math.zry")),
    ]);
    let [worker, candidate] = v3_pair(&sources);
    let left = m2::lower(
        m2::SemanticInput::try_new(&worker, &sources, entry(&sources))
            .expect("bootstrap semantic authority"),
    )
    .expect("bootstrap M2 IR");
    let right = m2::lower(
        m2::SemanticInput::try_new(&candidate, &sources, entry(&sources))
            .expect("native semantic authority"),
    )
    .expect("native M2 IR");
    assert_eq!(left.modules().len(), 2, "real cross-module source set");
    assert_m2_ir_equal(&left, &right);
    assert_eq!(
        javascript::emit_control_flow(&left).expect("bootstrap JS"),
        javascript::emit_control_flow(&right).expect("native JS")
    );
    assert_eq!(
        webassembly::emit_control_flow(&left).expect("bootstrap Wasm").bytes(),
        webassembly::emit_control_flow(&right).expect("native Wasm").bytes()
    );
    let target =
        native::select_object_target(native::NATIVE_OBJECT_TARGET).expect("fixed ELF target");
    let left_mir = zryna_native_mir::control_flow_v1::lower(&left).expect("bootstrap sealed MIR");
    let right_mir = zryna_native_mir::control_flow_v1::lower(&right).expect("native sealed MIR");
    assert_eq!(
        native::control_flow_v1::emit_object(&left_mir, target).expect("bootstrap ELF").bytes(),
        native::control_flow_v1::emit_object(&right_mir, target).expect("native ELF").bytes()
    );
}

#[test]
fn m2_semantic_rejections_keep_exact_provider_independent_diagnostics() {
    for text in [
        "// π\r\nexport function f(): i32 { return missing; }\r\n",
        "export function f(): i32 { return true; }",
    ] {
        let sources = sources(&[("src/main.zry", text)]);
        let [worker, candidate] = v3_pair(&sources);
        let left = m2::lower(
            m2::SemanticInput::try_new(&worker, &sources, entry(&sources))
                .expect("bootstrap authority"),
        )
        .expect_err("invalid M2");
        let right = m2::lower(
            m2::SemanticInput::try_new(&candidate, &sources, entry(&sources))
                .expect("native authority"),
        )
        .expect_err("invalid M2");
        assert_diagnostics_equal(&left, &right, &sources);
    }
}

#[test]
fn m3_copy_aggregate_three_target_artifacts_are_provider_independent() {
    let sources = sources(&[(
        "src/main.zry",
        include_str!("../../../tests/m3-fixtures/conformance/array.zry"),
    )]);
    let [worker, candidate] = v4_pair(&sources);
    let left = m3::lower(
        m3::SemanticInput::try_new(&worker, &sources, entry(&sources))
            .expect("bootstrap authority"),
    )
    .expect("bootstrap Copy IR");
    let right = m3::lower(
        m3::SemanticInput::try_new(&candidate, &sources, entry(&sources))
            .expect("native authority"),
    )
    .expect("native Copy IR");
    assert_eq!(left.verified_ir().scalar_abi(), right.verified_ir().scalar_abi());
    assert_eq!(
        javascript::emit_data_ownership(left.verified_ir(), left.runtime_abi())
            .expect("bootstrap JS"),
        javascript::emit_data_ownership(right.verified_ir(), right.runtime_abi())
            .expect("native JS")
    );
    assert_eq!(
        webassembly::emit_data_ownership(left.verified_ir(), left.runtime_abi())
            .expect("bootstrap Wasm")
            .bytes(),
        webassembly::emit_data_ownership(right.verified_ir(), right.runtime_abi())
            .expect("native Wasm")
            .bytes()
    );
    let target =
        native::select_object_target(native::NATIVE_OBJECT_TARGET).expect("fixed ELF target");
    let left_mir =
        zryna_native_mir::data_ownership_v1::lower(left.verified_ir(), left.runtime_abi())
            .expect("bootstrap sealed MIR");
    let right_mir =
        zryna_native_mir::data_ownership_v1::lower(right.verified_ir(), right.runtime_abi())
            .expect("native sealed MIR");
    assert_eq!(
        native::data_ownership_v1::emit_object(&left_mir, target).expect("bootstrap ELF").bytes(),
        native::data_ownership_v1::emit_object(&right_mir, target).expect("native ELF").bytes()
    );
}

#[test]
fn m3_semantic_rejections_keep_exact_provider_independent_diagnostics() {
    for text in [
        "// π\r\nexport function f(): i32 { return missing; }\r\n",
        "export function f(): i32 { return true; }",
        include_str!("../../../tests/m3-fixtures/conformance/owned-aggregate-body.zry"),
    ] {
        let sources = sources(&[("src/main.zry", text)]);
        let [worker, candidate] = v4_pair(&sources);
        let left = m3::lower(
            m3::SemanticInput::try_new(&worker, &sources, entry(&sources))
                .expect("bootstrap authority"),
        )
        .expect_err("invalid M3");
        let right = m3::lower(
            m3::SemanticInput::try_new(&candidate, &sources, entry(&sources))
                .expect("native authority"),
        )
        .expect_err("invalid M3");
        assert_diagnostics_equal(&left, &right, &sources);
    }
}

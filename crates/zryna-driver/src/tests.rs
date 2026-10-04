use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    JavaScriptBuildError, JavaScriptOutputRoot, NativeObjectBuildError, SourceToIrError,
    WebAssemblyBuildError, compile_javascript, compile_native_object, compile_to_verified_ir,
    compile_webassembly, lower_verified_syntax,
};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use super::{compile_native_invocation, discover_linux_native_toolchain, run_native_invocation};
use zryna_diagnostics::{Diagnostic, Severity, render_structured};
use zryna_frontend::{
    FrontendCapabilities, ProviderExpectation, WorkerFrontend, WorkerLimits, WorkerSpec,
    syntax_v2::{self, RawDiagnosticLocation, RawProviderDiagnostic},
};
use zryna_ir::{Expr, ExprId, ExprKind, Function, Program, Type, verify};
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

static NEXT_JAVASCRIPT_ROOT: AtomicU64 = AtomicU64::new(0);

struct JavaScriptRoot {
    workspace: PathBuf,
    output: JavaScriptOutputRoot,
}

impl JavaScriptRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_JAVASCRIPT_ROOT.fetch_add(1, Ordering::Relaxed);
        let workspace = env::temp_dir()
            .join(format!("zryna-driver-javascript-{}-{label}-{sequence}", std::process::id()));
        fs::create_dir_all(workspace.join(".zryna/out"))
            .expect("declared JavaScript fixture root must be created");
        let output = JavaScriptOutputRoot::for_workspace(&workspace)
            .expect("JavaScript fixture output must validate");
        Self { workspace, output }
    }

    fn path(&self) -> &Path {
        self.output.path()
    }

    const fn output(&self) -> &JavaScriptOutputRoot {
        &self.output
    }
}

impl Drop for JavaScriptRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.workspace);
    }
}

fn node_executable() -> PathBuf {
    for variable in ["ZRYNA_TEST_NODE", "NODE"] {
        if let Some(configured) = env::var_os(variable) {
            let configured = PathBuf::from(configured);
            assert!(
                configured.is_absolute() && configured.is_file(),
                "configured Node.js executable must be an existing absolute file"
            );
            return configured;
        }
    }
    let executable = if cfg!(windows) { "node.exe" } else { "node" };
    env::split_paths(&env::var_os("PATH").expect("test PATH must exist"))
        .map(|directory| directory.join(executable))
        .find(|candidate| candidate.is_file())
        .expect("Node.js must be installed for the source-to-IR integration suite")
}

pub(super) fn run_node_module(module: &Path, script: &str, extra_arguments: &[&Path]) -> Output {
    let mut command = Command::new(node_executable());
    command.arg("--input-type=module").arg("--eval").arg(script).arg(module);
    command.args(extra_arguments);
    crate::process_spawn::output(&mut command).expect("Node.js integration harness must start")
}

pub(super) fn typescript_frontend() -> WorkerFrontend {
    let adapter_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters/typescript-6");
    assert!(
        adapter_root.is_absolute() && adapter_root.is_dir(),
        "TypeScript adapter root must be an existing absolute directory"
    );
    let expected = ProviderExpectation::new(
        "typescript-6",
        "6.0.3",
        syntax_v2::PROTOCOL_VERSION,
        FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
    )
    .expect("provider expectation must be valid");
    let spec = WorkerSpec::new(
        node_executable(),
        vec![OsString::from("src/worker.mjs")],
        adapter_root,
        expected,
        WorkerLimits::default(),
    )
    .expect("worker specification must be valid");
    WorkerFrontend::new(spec)
}

fn source_map(path: &str, text: &str) -> SourceMap {
    SourceMap::build(vec![SourceFileInput { path: path.to_owned(), text: text.to_owned() }])
        .expect("fixture source map must build")
}

fn byte_offset(text: &str, needle: &str) -> u32 {
    u32::try_from(text.find(needle).expect("fixture token must exist"))
        .expect("fixture offset must fit u32")
}

fn last_byte_offset(text: &str, needle: &str) -> u32 {
    u32::try_from(text.rfind(needle).expect("fixture token must exist"))
        .expect("fixture offset must fit u32")
}

fn rejection_codes(text: &str) -> (SourceMap, Vec<String>) {
    let sources = source_map("src/main.zry", text);
    let error = compile_to_verified_ir(&typescript_frontend(), &sources)
        .expect_err("fixture source must be rejected");
    let SourceToIrError::Rejected(diagnostics) = error else {
        panic!("fixture must reach a compiler rejection, not a worker failure");
    };
    let codes = diagnostics.iter().map(|diagnostic| diagnostic.code().to_owned()).collect();
    (sources, codes)
}

#[test]
fn real_universal_add_source_reaches_verified_ir_through_protocol_v2() {
    let text = include_str!("../../../examples/universal/add.zry");
    let sources = source_map("examples/universal/add.zry", text);

    let result = compile_to_verified_ir(&typescript_frontend(), &sources)
        .expect("universal add must reach verified IR");

    assert!(result.diagnostics().is_empty());
    let functions = result.program().functions().collect::<Vec<_>>();
    assert_eq!(functions.len(), 1);
    let function = functions[0];
    assert_eq!(function.export_name().as_str(), "add");
    assert_eq!(function.parameters(), &[Type::I32, Type::I32]);
    assert_eq!(function.return_type(), Type::I32);
    assert_eq!(function.body(), ExprId(2));
    assert_eq!(function.expressions().len(), 3);
    assert_eq!(function.expressions()[0].kind, ExprKind::Parameter(0));
    assert_eq!(function.expressions()[1].kind, ExprKind::Parameter(1));
    assert_eq!(function.expressions()[2].kind, ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(1) });
    for expression in function.expressions() {
        assert!(sources.resolve(expression.span).is_ok());
    }
}

#[test]
fn i32_boundaries_and_repeated_nested_additions_lower_exactly() {
    let text = concat!(
        "export function min(): i32 { return -2147483648; } ",
        "export function max(): i32 { return 2147483647; } ",
        "export function zero(): i32 { return 0; } ",
        "export function repeated(a: i32): i32 { return a + a; } ",
        "export function nested(a: i32, b: i32): i32 { return a + b + a; }",
    );
    let sources = source_map("src/boundaries.zry", text);

    let result = compile_to_verified_ir(&typescript_frontend(), &sources)
        .expect("supported i32 boundary fixture must reach verified IR");
    let functions = result.program().functions().collect::<Vec<_>>();

    assert_eq!(functions.len(), 5);
    assert_eq!(functions[0].expressions()[0].kind, ExprKind::I32Literal(i32::MIN));
    assert_eq!(functions[1].expressions()[0].kind, ExprKind::I32Literal(i32::MAX));
    assert_eq!(functions[2].expressions()[0].kind, ExprKind::I32Literal(0));
    assert_eq!(functions[3].expressions().len(), 3);
    assert_eq!(
        functions[3].expressions()[2].kind,
        ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(1) }
    );
    assert_eq!(functions[4].expressions().len(), 5);
    assert_eq!(
        functions[4].expressions()[2].kind,
        ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(1) }
    );
    assert_eq!(
        functions[4].expressions()[4].kind,
        ExprKind::I32Add { lhs: ExprId(2), rhs: ExprId(3) }
    );
}

#[test]
fn provider_warnings_remain_observable_on_verified_success() {
    let text = include_str!("../../../examples/universal/add.zry");
    let sources = source_map("examples/universal/add.zry", text);
    let mut raw = syntax_v2::decode_snapshot(include_bytes!(
        "../../../tests/fixtures/typescript-adapter-v2-result.json"
    ))
    .expect("checked adapter fixture must decode");
    raw.diagnostics.push(RawProviderDiagnostic {
        code: "TS9000".to_owned(),
        severity: Severity::Warning,
        location: RawDiagnosticLocation::Global,
        message: "provider note".to_owned(),
        guidance: "review the declaration".to_owned(),
    });
    let syntax = syntax_v2::verify_snapshot(raw, &sources)
        .expect("source-bound provider warning must verify");

    let result = lower_verified_syntax(&syntax, &sources)
        .expect("provider warning must not block verified IR");

    assert_eq!(result.program().functions().count(), 1);
    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(result.diagnostics()[0].code(), "TS9000");
    assert_eq!(result.diagnostics()[0].severity(), Severity::Warning);
}

#[test]
fn source_map_identity_precedes_provider_diagnostic_forwarding() {
    let text = "export function yes(): bool { return true; }";
    let first = source_map("src/main.zry", text);
    let second = source_map("src/main.zry", text);
    let raw = syntax_v2::decode_snapshot(include_bytes!(
        "../../../tests/fixtures/typescript-adapter-v2-error-result.json"
    ))
    .expect("provider error fixture must decode");
    let syntax = syntax_v2::verify_snapshot(raw, &first)
        .expect("provider error fixture must verify against its issuing map");

    let diagnostics = lower_verified_syntax(&syntax, &second)
        .expect_err("cross-map syntax must fail before forwarding provider diagnostics");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code(), "ZRYNA-D1001");
    assert!(diagnostics[0].primary_span().is_none());
}

#[test]
fn exact_parameter_limit_reaches_verified_ir() {
    let parameters = (0..syntax_v2::MAX_PARAMETERS_PER_FUNCTION)
        .map(|index| format!("p{index}: i32"))
        .collect::<Vec<_>>()
        .join(", ");
    let text = format!("export function exact({parameters}): i32 {{ return p0; }}");
    let sources = source_map("src/exact-parameters.zry", &text);

    let result = compile_to_verified_ir(&typescript_frontend(), &sources)
        .expect("the exact syntax, ABI, and IR parameter limit must succeed");
    let function = result.program().functions().next().expect("function must exist");

    assert_eq!(function.parameters().len(), syntax_v2::MAX_PARAMETERS_PER_FUNCTION);
    assert_eq!(function.expressions().len(), 1);
    assert_eq!(function.expressions()[0].kind, ExprKind::Parameter(0));
}

#[test]
fn semantic_rejections_are_stable_and_source_located() {
    let cases = [
        ("export function f(a): i32 { return a; }", "ZRYNA-M1003"),
        ("export function f(a: i32) { return a; }", "ZRYNA-M1003"),
        ("export function f(a: any): i32 { return 1; }", "ZRYNA-M1004"),
        ("export function f(): any { return 1; }", "ZRYNA-M1004"),
        ("export function f(a: u32): i32 { return 1; }", "ZRYNA-M1005"),
        ("export function f(a: i32, a: i32): i32 { return a; }", "ZRYNA-M1002"),
        ("export function f(a: i32): i32 { return b; }", "ZRYNA-M1006"),
        ("export function f(): i32 { return 2147483648; }", "ZRYNA-M1007"),
        ("export function f(): i32 { return -2147483649; }", "ZRYNA-M1007"),
        ("export function f(): i32 { return true + 1; }", "ZRYNA-M1008"),
        ("export function f(): i32 { return true; }", "ZRYNA-M1010"),
        ("export function f(): bool { return 1; }", "ZRYNA-M1010"),
        ("export function f(): i32 {}", "ZRYNA-M1009"),
        ("export function f(): i32 { return 1; return 2; }", "ZRYNA-M1009"),
        ("export function then(): i32 { return 1; }", "ZRYNA-M1011"),
        ("export function $value(): i32 { return 1; }", "ZRYNA-M1011"),
        (
            "export function Value(): i32 { return 1; } export function value(): i32 { return 2; }",
            "ZRYNA-M1012",
        ),
        (
            "export function value(): i32 { return 1; } export function value(): i32 { return 2; }",
            "ZRYNA-M1001",
        ),
    ];

    for (text, expected) in cases {
        let sources = source_map("src/main.zry", text);
        let error = compile_to_verified_ir(&typescript_frontend(), &sources)
            .expect_err("invalid source must not produce verified IR");
        let SourceToIrError::Rejected(diagnostics) = error else {
            panic!("invalid source must reach deterministic compiler diagnostics");
        };
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code() == expected),
            "expected {expected} for {text:?}, got {:?}",
            diagnostics.iter().map(Diagnostic::code).collect::<Vec<_>>()
        );
        let report = render_structured(&diagnostics, &sources)
            .expect("all source diagnostics must resolve through the authoritative map");
        let repeated_error = compile_to_verified_ir(&typescript_frontend(), &sources)
            .expect_err("invalid source must remain rejected on repeated analysis");
        let SourceToIrError::Rejected(repeated_diagnostics) = repeated_error else {
            panic!("repeated invalid source must reach compiler diagnostics");
        };
        let repeated_report = render_structured(&repeated_diagnostics, &sources)
            .expect("repeated source diagnostics must remain renderable");
        assert_eq!(report, repeated_report, "diagnostics changed for {text:?}");
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == expected)
            .expect("expected diagnostic must render");
        assert_eq!(diagnostic.path.as_deref(), Some("src/main.zry"));
        assert!(diagnostic.byte_start.is_some());
        assert!(diagnostic.byte_end.is_some());
    }
}

#[test]
fn multi_error_diagnostic_order_and_spans_are_exact() {
    let text = "export function f(a: any): i32 { return b + true; return 2147483648; }";
    let sources = source_map("src/exact-errors.zry", text);
    let render = || {
        let error = compile_to_verified_ir(&typescript_frontend(), &sources)
            .expect_err("multi-error fixture must be rejected");
        let SourceToIrError::Rejected(diagnostics) = error else {
            panic!("multi-error fixture must reach compiler diagnostics");
        };
        render_structured(&diagnostics, &sources).expect("diagnostics must render")
    };

    let first = render();
    let second = render();
    assert_eq!(first, second);

    let any_start = byte_offset(text, "any");
    let reference_start = byte_offset(text, "b +");
    let operator_start = byte_offset(text, "+ true");
    let second_return_start = last_byte_offset(text, "return");
    let literal_start = byte_offset(text, "2147483648");
    let second_statement_end =
        last_byte_offset(text, ";").checked_add(1).expect("fixture statement end must fit");
    let actual = first
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code.as_str(),
                diagnostic.byte_start.expect("source start must exist"),
                diagnostic.byte_end.expect("source end must exist"),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actual,
        vec![
            ("ZRYNA-M1004", any_start, any_start + 3),
            ("ZRYNA-M1006", reference_start, reference_start + 1),
            ("ZRYNA-M1008", operator_start, operator_start + 1),
            ("ZRYNA-M1009", second_return_start, second_statement_end),
            ("ZRYNA-M1007", literal_start, literal_start + 10),
        ]
    );
}

#[test]
fn entrypoint_and_provider_failures_stop_before_verified_ir() {
    let no_sources = SourceMap::build(Vec::new()).expect("empty source map must build");
    let no_source_error = compile_to_verified_ir(&typescript_frontend(), &no_sources)
        .expect_err("missing entrypoint must fail");
    let SourceToIrError::Rejected(no_source_diagnostics) = no_source_error else {
        panic!("missing entrypoint must reach semantic diagnostics");
    };
    assert_eq!(
        no_source_diagnostics.iter().map(Diagnostic::code).collect::<Vec<_>>(),
        vec!["ZRYNA-M1013"]
    );
    assert!(no_source_diagnostics[0].primary_span().is_none());

    let empty_sources = source_map("src/empty.zry", "");
    let empty_error = compile_to_verified_ir(&typescript_frontend(), &empty_sources)
        .expect_err("empty entrypoint must fail");
    assert!(empty_error.diagnostics().iter().any(|item| item.code() == "ZRYNA-M1014"));

    let first = "export function a(): i32 { return 1; }";
    let second = "export function b(): i32 { return 2; }";
    let multiple_sources = SourceMap::build(vec![
        SourceFileInput { path: "src/a.zry".to_owned(), text: first.to_owned() },
        SourceFileInput { path: "src/b.zry".to_owned(), text: second.to_owned() },
    ])
    .expect("multi-file fixture must build");
    let multiple_error = compile_to_verified_ir(&typescript_frontend(), &multiple_sources)
        .expect_err("modules must remain disabled");
    assert!(multiple_error.diagnostics().iter().any(|item| item.code() == "ZRYNA-M1013"));

    let unsupported = source_map("src/main.zry", "export class Unsupported {}");
    let provider_error = compile_to_verified_ir(&typescript_frontend(), &unsupported)
        .expect_err("unsupported provider syntax must stop compilation");
    assert!(provider_error.diagnostics().iter().any(|item| item.code() == "ZRYNA-F2002"));

    let parenthesized = source_map(
        "src/main.zry",
        "export function nested(a: i32, b: i32): i32 { return a + (b + a); }",
    );
    let parenthesized_error = compile_to_verified_ir(&typescript_frontend(), &parenthesized)
        .expect_err("parenthesized expressions are outside protocol v2");
    assert!(parenthesized_error.diagnostics().iter().any(|item| item.code() == "ZRYNA-F2002"));
}

#[test]
fn bool_source_has_a_stable_verified_profile_rejection() {
    for text in [
        "export function yes(): bool { return true; }",
        "export function no(): bool { return false; }",
        "export function identity(value: bool): bool { return value; }",
    ] {
        let (_, codes) = rejection_codes(text);
        assert!(codes.iter().any(|code| code == "ZRYNA-I1006"), "codes: {codes:?}");
    }
}

#[test]
fn real_source_publishes_imports_and_executes_deterministic_esm() {
    let text = include_str!("../../../examples/universal/add.zry");
    let sources = source_map("examples/universal/add.zry", text);
    let output_root = JavaScriptRoot::new("execute");

    let result = compile_javascript(&typescript_frontend(), &sources, output_root.output(), "main")
        .expect("real source must publish JavaScript");

    assert!(result.diagnostics().is_empty());
    assert_eq!(result.artifact().path(), output_root.path().join("main.mjs"));
    let bytes = fs::read(result.artifact().path()).expect("published module must be readable");
    assert!(bytes.ends_with(b"\n"));
    assert!(!bytes.starts_with(&[0xef, 0xbb, 0xbf]));
    let source = std::str::from_utf8(&bytes).expect("JavaScript artifact must be UTF-8");
    assert!(source.contains("export function add(p0, p1)"));
    assert!(!source.to_ascii_lowercase().contains("typescript"));
    assert!(!source.contains("import "));

    let script = r#"
import { pathToFileURL } from "node:url";
const target = await import(pathToFileURL(process.argv[1]).href);
const values = [
  target.add(2147483647, 1),
  target.add(-2147483648, -1),
  target.add(-1, -1),
  target.add(0, 0),
  target.add(20, 22),
];
const invalid = [
  () => target.add(1),
  () => target.add(1, 2, 3),
  () => target.add("1", 2),
  () => target.add(1n, 2),
  () => target.add(true, 2),
  () => target.add(undefined, 2),
  () => target.add(null, 2),
  () => target.add(new Number(1), 2),
  () => target.add({ valueOf: () => 1 }, 2),
  () => target.add(1.5, 2),
  () => target.add(Number.NaN, 2),
  () => target.add(Number.POSITIVE_INFINITY, 2),
  () => target.add(-0, 2),
  () => target.add(2147483648, 2),
];
const errors = invalid.map((invoke) => {
  try {
    invoke();
    return "missing-error";
  } catch (error) {
    return String(error.message).split(":", 1)[0];
  }
});
process.stdout.write(JSON.stringify({
  exports: Object.keys(target),
  values,
  negativeZero: values.some((value) => Object.is(value, -0)),
  errors,
}));
"#;
    let output = run_node_module(result.artifact().path(), script, &[]);

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).expect("Node output must be UTF-8"),
        concat!(
            "{\"exports\":[\"add\"],",
            "\"values\":[-2147483648,2147483647,-2,0,42],",
            "\"negativeZero\":false,",
            "\"errors\":[\"ZRYNA-B2102\",\"ZRYNA-B2102\",",
            "\"ZRYNA-B2001\",\"ZRYNA-B2001\",\"ZRYNA-B2001\",",
            "\"ZRYNA-B2001\",\"ZRYNA-B2001\",\"ZRYNA-B2001\",",
            "\"ZRYNA-B2001\",\"ZRYNA-B2002\",\"ZRYNA-B2002\",",
            "\"ZRYNA-B2002\",\"ZRYNA-B2002\",\"ZRYNA-B2002\"]}"
        )
    );
}

#[test]
fn real_source_publishes_and_executes_validated_core_webassembly() {
    let text = include_str!("../../../examples/universal/add.zry");
    let sources = source_map("examples/universal/add.zry", text);
    let output_root = JavaScriptRoot::new("webassembly-execute");

    let result =
        compile_webassembly(&typescript_frontend(), &sources, output_root.output(), "main")
            .expect("real source must publish validated WebAssembly");

    assert!(result.diagnostics().is_empty());
    assert_eq!(result.artifact().path(), output_root.path().join("main.wasm"));
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/abi/scalar-v1-fixtures.json");
    let script = r#"
import { readFile } from "node:fs/promises";
const bytes = await readFile(process.argv[1]);
if (!WebAssembly.validate(bytes)) throw new Error("module did not validate");
const module = new WebAssembly.Module(bytes);
const imports = WebAssembly.Module.imports(module);
const exports = WebAssembly.Module.exports(module);
const { instance } = await WebAssembly.instantiate(bytes, {});
const values = [
  instance.exports.add(2147483647, 1),
  instance.exports.add(-2147483648, -1),
  instance.exports.add(2147483647, 2147483647),
  instance.exports.add(-2147483648, -2147483648),
  instance.exports.add(-1, -1),
  instance.exports.add(20, 22),
];
const fixture = JSON.parse(await readFile(process.argv[2], "utf8"));
const carriers = fixture.carrierCases.filter((entry) => entry.target === "core-webassembly");
if (carriers.length !== 11) throw new Error(`expected 11 core WebAssembly carriers, got ${carriers.length}`);
for (const entry of carriers) {
  const raw = entry.raw.value;
  let actual = raw;
  let errorCode = null;
  if (entry.scalarType === "bool") {
    if (raw === 0) actual = false;
    else if (raw === 1) actual = true;
    else { actual = null; errorCode = "ZRYNA-B2003"; }
  }
  if (errorCode !== entry.errorCode) throw new Error(`carrier error mismatch for ${entry.direction}`);
  if (entry.value === null ? actual !== null : !Object.is(actual, entry.value.value)) {
    throw new Error(`carrier value mismatch for ${entry.scalarType}/${entry.direction}`);
  }
}
process.stdout.write(JSON.stringify({ imports, exports, values, carrierCount: carriers.length }));
"#;
    let output = run_node_module(result.artifact().path(), script, &[&fixture]);

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).expect("Node output must be UTF-8"),
        concat!(
            "{\"imports\":[],\"exports\":[{\"name\":\"add\",\"kind\":\"function\"}],",
            "\"values\":[-2147483648,2147483647,-2,0,-2,42],\"carrierCount\":11}"
        )
    );
}

#[test]
fn real_source_publishes_audited_native_object_create_only() {
    let text = include_str!("../../../examples/universal/add.zry");
    let sources = source_map("examples/universal/add.zry", text);
    let output_root = JavaScriptRoot::new("native-object");
    fs::write(output_root.path().join("main.mjs"), b"javascript")
        .expect("JavaScript sibling fixture");
    fs::write(output_root.path().join("main.wasm"), b"webassembly")
        .expect("WebAssembly sibling fixture");

    let result = compile_native_object(
        &typescript_frontend(),
        &sources,
        output_root.output(),
        "main",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
    )
    .expect("real source must publish audited native object");

    assert!(result.diagnostics().is_empty());
    assert_eq!(result.artifact().path(), output_root.path().join("main.o"));
    assert_eq!(&fs::read(result.artifact().path()).expect("object bytes")[..4], b"\x7fELF");
    fs::write(result.artifact().path(), b"native-sentinel")
        .expect("distinct existing native destination");
    let error = compile_native_object(
        &typescript_frontend(),
        &sources,
        output_root.output(),
        "main",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
    )
    .expect_err("create-only publication must preserve native destination");
    assert!(matches!(error, NativeObjectBuildError::Publication(_)));
    assert_eq!(error.diagnostics()[0].code(), "ZRYNA-D2007");
    assert_eq!(
        fs::read(output_root.path().join("main.o")).expect("preserved native sentinel"),
        b"native-sentinel"
    );
    assert_eq!(fs::read_dir(output_root.path()).expect("output listing").count(), 3);
}

#[test]
fn native_fixture_partition_matches_i32v1_gate() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../spec/abi/scalar-v1-fixtures.json"))
            .expect("normative scalar fixture must parse");
    let carriers = fixture["carrierCases"]
        .as_array()
        .expect("carrierCases must be an array")
        .iter()
        .filter(|entry| entry["target"] == "native-linux-x86-64")
        .collect::<Vec<_>>();
    let bool_count = carriers.iter().filter(|entry| entry["scalarType"] == "bool").count();
    let i32_count = carriers.iter().filter(|entry| entry["scalarType"] == "i32").count();
    assert_eq!(carriers.len(), 11);
    assert_eq!(i32_count, 3);
    assert_eq!(bool_count, 8);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_test_harness_observes_native_i32_values() {
    let text = concat!(
        "export function add(a: i32, b: i32): i32 { return a + b; } ",
        "export function min(): i32 { return -2147483648; } ",
        "export function max(): i32 { return 2147483647; } ",
        "export function sum7(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32): i32 { return a + b + c + d + e + f + g; }",
    );
    let sources = source_map("src/native.zry", text);
    let output_root = JavaScriptRoot::new("native-execute");
    let result = compile_native_object(
        &typescript_frontend(),
        &sources,
        output_root.output(),
        "native",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
    )
    .expect("native fixture must compile");
    let harness = output_root.path().join("harness.c");
    fs::write(
            &harness,
            concat!(
                "#include <stdint.h>\n#include <stdio.h>\n",
                "extern int32_t zryna_v1_e_add(int32_t, int32_t);\n",
                "extern int32_t zryna_v1_e_min(void);\n",
                "extern int32_t zryna_v1_e_max(void);\n",
                "extern int32_t zryna_v1_e_sum7(int32_t, int32_t, int32_t, int32_t, int32_t, int32_t, int32_t);\n",
                "int main(void) {\n",
                "  printf(\"%d,%d,%d,%d,%d,%d,%d,%d,%d\\n\",\n",
                "    zryna_v1_e_add(INT32_MAX, 1),\n",
                "    zryna_v1_e_add(INT32_MIN, -1),\n",
                "    zryna_v1_e_add(INT32_MAX, INT32_MAX),\n",
                "    zryna_v1_e_add(INT32_MIN, INT32_MIN),\n",
                "    zryna_v1_e_add(-1, -1), zryna_v1_e_add(20, 22),\n",
                "    zryna_v1_e_min(), zryna_v1_e_max(),\n",
                "    zryna_v1_e_sum7(1, 2, 3, 4, 5, 6, 7));\n",
                "  return 0;\n}\n",
            ),
        )
        .expect("test-only C harness");
    let executable = output_root.path().join("harness");
    let linked = crate::process_spawn::output(
        Command::new("cc")
            .arg("-std=c11")
            .arg("-o")
            .arg(&executable)
            .arg(&harness)
            .arg(result.artifact().path()),
    )
    .expect("test-only C linker must start");
    assert!(linked.status.success(), "link stderr: {}", String::from_utf8_lossy(&linked.stderr));
    let executed = crate::process_spawn::output(&mut Command::new(&executable))
        .expect("test harness must start");
    assert!(executed.status.success());
    assert_eq!(
        String::from_utf8(executed.stdout).expect("harness UTF-8"),
        "-2147483648,2147483647,-2,0,-2,42,-2147483648,2147483647,28\n"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_native_i32_fixture_drives_object_observation() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../spec/abi/scalar-v1-fixtures.json"))
            .expect("normative scalar fixture must parse");
    let values = fixture["carrierCases"]
        .as_array()
        .expect("carrierCases must be an array")
        .iter()
        .filter(|entry| entry["target"] == "native-linux-x86-64" && entry["scalarType"] == "i32")
        .map(|entry| {
            i32::try_from(entry["raw"]["value"].as_i64().expect("native i32 raw value"))
                .expect("native fixture i32 range")
        })
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 3);

    let sources =
        source_map("src/probe.zry", "export function probe(value: i32): i32 { return value; }");
    let output_root = JavaScriptRoot::new("native-fixture");
    let result = compile_native_object(
        &typescript_frontend(),
        &sources,
        output_root.output(),
        "fixture",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
    )
    .expect("native fixture probe must compile");
    let calls = values
        .iter()
        .map(|value| format!("zryna_v1_e_probe(INT32_C({value}))"))
        .collect::<Vec<_>>()
        .join(", ");
    let expected = values.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
    let harness_source = format!(
        concat!(
            "#include <stdint.h>\n#include <stdio.h>\n",
            "extern int32_t zryna_v1_e_probe(int32_t);\n",
            "int main(void) {{ printf(\"%d,%d,%d\\n\", {calls}); return 0; }}\n",
        ),
        calls = calls
    );
    let harness = output_root.path().join("fixture.c");
    fs::write(&harness, harness_source).expect("test-only fixture harness");
    let executable = output_root.path().join("fixture-harness");
    let linked = crate::process_spawn::output(
        Command::new("cc")
            .arg("-std=c11")
            .arg("-o")
            .arg(&executable)
            .arg(&harness)
            .arg(result.artifact().path()),
    )
    .expect("test-only C linker must start");
    assert!(linked.status.success(), "link stderr: {}", String::from_utf8_lossy(&linked.stderr));
    let executed = crate::process_spawn::output(&mut Command::new(&executable))
        .expect("fixture harness must start");
    assert!(executed.status.success());
    assert_eq!(
        String::from_utf8(executed.stdout).expect("fixture harness UTF-8"),
        format!("{expected}\n")
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn assert_native_i32_invocation(
    sources: &SourceMap,
    output_root: &JavaScriptRoot,
    toolchain: &super::LinuxX8664LinkToolchain,
    stem: &str,
    export: &str,
    arguments: Vec<zryna_abi::ScalarValue>,
    expected: i32,
) {
    use std::os::unix::fs::PermissionsExt;

    let build = compile_native_invocation(
        &typescript_frontend(),
        sources,
        output_root.output(),
        stem,
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        toolchain,
        zryna_abi::Invocation::new(export.to_owned(), arguments),
        super::NativeProcessLimits::default(),
    )
    .expect("real source invocation must link");
    assert_eq!(build.artifact().path(), output_root.path().join(format!("{stem}.elf")));
    assert_eq!(
        fs::metadata(build.artifact().path())
            .expect("published executable metadata")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        run_native_invocation(build.artifact(), super::NativeProcessLimits::default())
            .expect("typed native invocation"),
        zryna_abi::ScalarOutcome::Returned { value: zryna_abi::ScalarValue::I32(expected) }
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn production_native_invocations_link_publish_and_preserve_typed_i32() {
    let text = include_str!("../../../examples/universal/add.zry");
    let sources = source_map("examples/universal/add.zry", text);
    let output_root = JavaScriptRoot::new("native path ; touch-zryna-marker ; $(false) ' quoted");
    for (name, bytes) in [
        ("same.mjs", b"javascript".as_slice()),
        ("same.wasm", b"webassembly".as_slice()),
        ("same.o", b"native-object".as_slice()),
    ] {
        fs::write(output_root.path().join(name), bytes).expect("sibling fixture");
    }
    let toolchain = discover_linux_native_toolchain(super::NativeProcessLimits::default())
        .expect("documented Linux native toolchain");
    let cases = [
        ("wrap_max", i32::MAX, 1, i32::MIN),
        ("wrap_min", i32::MIN, -1, i32::MAX),
        ("double_max", i32::MAX, i32::MAX, -2),
        ("double_min", i32::MIN, i32::MIN, 0),
        ("negative", -1, -1, -2),
        ("answer", 20, 22, 42),
    ];
    for (stem, left, right, expected) in cases {
        assert_native_i32_invocation(
            &sources,
            &output_root,
            &toolchain,
            stem,
            "add",
            vec![zryna_abi::ScalarValue::I32(left), zryna_abi::ScalarValue::I32(right)],
            expected,
        );
    }

    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../spec/abi/scalar-v1-fixtures.json"))
            .expect("normative scalar fixture must parse");
    let native_i32 = fixture["carrierCases"]
        .as_array()
        .expect("carrierCases must be an array")
        .iter()
        .filter(|entry| entry["target"] == "native-linux-x86-64" && entry["scalarType"] == "i32")
        .map(|entry| {
            i32::try_from(entry["raw"]["value"].as_i64().expect("native i32 fixture value"))
                .expect("native i32 fixture range")
        })
        .collect::<Vec<_>>();
    assert_eq!(native_i32.len(), 3);
    let identity_sources = source_map(
        "src/identity.zry",
        "export function identity(value: i32): i32 { return value; }",
    );
    for (index, value) in native_i32.into_iter().enumerate() {
        assert_native_i32_invocation(
            &identity_sources,
            &output_root,
            &toolchain,
            &format!("fixture_{index}"),
            "identity",
            vec![zryna_abi::ScalarValue::I32(value)],
            value,
        );
    }
    assert_eq!(fs::read(output_root.path().join("same.mjs")).expect("mjs"), b"javascript");
    assert_eq!(fs::read(output_root.path().join("same.wasm")).expect("wasm"), b"webassembly");
    assert_eq!(fs::read(output_root.path().join("same.o")).expect("object"), b"native-object");
    assert!(fs::read_dir(output_root.path()).expect("output listing").all(|entry| {
        !entry.expect("output entry").file_name().to_string_lossy().starts_with(".zryna-link-")
    }));

    let collision = compile_native_invocation(
        &typescript_frontend(),
        &sources,
        output_root.output(),
        "answer",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        zryna_abi::Invocation::new(
            "add".to_owned(),
            vec![zryna_abi::ScalarValue::I32(1), zryna_abi::ScalarValue::I32(2)],
        ),
        super::NativeProcessLimits::default(),
    )
    .expect_err("create-only executable must reject a collision");
    assert_eq!(collision.diagnostics()[0].code(), "ZRYNA-D2007");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_run_uses_the_retained_audited_snapshot() {
    let sources =
        source_map("src/main.zry", "export function add(a: i32, b: i32): i32 { return a + b; }");
    let output_root = JavaScriptRoot::new("native-sealed-snapshot");
    let toolchain = discover_linux_native_toolchain(super::NativeProcessLimits::default())
        .expect("documented Linux native toolchain");
    let sealed = compile_native_invocation(
        &typescript_frontend(),
        &sources,
        output_root.output(),
        "sealed_snapshot",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        zryna_abi::Invocation::new(
            "add".to_owned(),
            vec![zryna_abi::ScalarValue::I32(19), zryna_abi::ScalarValue::I32(23)],
        ),
        super::NativeProcessLimits::default(),
    )
    .expect("sealed executable snapshot must build");
    fs::write(sealed.artifact().path(), b"replaced public path")
        .expect("public path replacement fixture");
    assert_eq!(
        run_native_invocation(sealed.artifact(), super::NativeProcessLimits::default())
            .expect("run must use retained audited bytes"),
        zryna_abi::ScalarOutcome::Returned { value: zryna_abi::ScalarValue::I32(42) }
    );
    assert_eq!(
        fs::read(sealed.artifact().path()).expect("replacement remains public"),
        b"replaced public path"
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_invocation_rejects_bad_abi_and_boolean_source_before_staging() {
    let output_root = JavaScriptRoot::new("native-invocation-rejections");
    let toolchain = discover_linux_native_toolchain(super::NativeProcessLimits::default())
        .expect("documented Linux native toolchain");
    let i32_sources =
        source_map("src/main.zry", "export function value(input: i32): i32 { return input; }");
    let invalid = compile_native_invocation(
        &typescript_frontend(),
        &i32_sources,
        output_root.output(),
        "invalid",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        zryna_abi::Invocation::new("missing".to_owned(), Vec::new()),
        super::NativeProcessLimits::default(),
    )
    .expect_err("unknown export must fail before staging");
    assert_eq!(invalid.diagnostics()[0].code(), "ZRYNA-B2101");

    let wrong_arity = compile_native_invocation(
        &typescript_frontend(),
        &i32_sources,
        output_root.output(),
        "wrong_arity",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        zryna_abi::Invocation::new("value".to_owned(), Vec::new()),
        super::NativeProcessLimits::default(),
    )
    .expect_err("wrong arity must fail before staging");
    assert_eq!(wrong_arity.diagnostics()[0].code(), "ZRYNA-B2102");

    let wrong_type = compile_native_invocation(
        &typescript_frontend(),
        &i32_sources,
        output_root.output(),
        "wrong_type",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        zryna_abi::Invocation::new("value".to_owned(), vec![zryna_abi::ScalarValue::Bool(true)]),
        super::NativeProcessLimits::default(),
    )
    .expect_err("wrong scalar type must fail before staging");
    assert_eq!(wrong_type.diagnostics()[0].code(), "ZRYNA-B2103");

    let bool_sources =
        source_map("src/bool.zry", "export function identity(value: bool): bool { return value; }");
    let rejected = compile_native_invocation(
        &typescript_frontend(),
        &bool_sources,
        output_root.output(),
        "boolean",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        zryna_abi::Invocation::new("identity".to_owned(), vec![zryna_abi::ScalarValue::Bool(true)]),
        super::NativeProcessLimits::default(),
    )
    .expect_err("Boolean source remains gated by I32V1");
    assert!(rejected.diagnostics().iter().any(|item| item.code() == "ZRYNA-I1006"));
    assert_eq!(fs::read_dir(output_root.path()).expect("empty output").count(), 0);
}

#[test]
fn javascript_carriers_consume_the_shared_scalar_abi_fixture() {
    let sources =
        source_map("src/probe.zry", "export function probe(value: i32): i32 { return value; }");
    let output_root = JavaScriptRoot::new("bool-carriers");
    let result =
        compile_javascript(&typescript_frontend(), &sources, output_root.output(), "probe")
            .expect("i32 probe must publish");
    let mut probe_source =
        fs::read_to_string(result.artifact().path()).expect("probe module must be readable");
    probe_source.push_str("export { $zryna$bool as boolProbe, $zryna$i32 as i32Probe };\n");
    let probe_path = output_root.path().join("scalar-probe.mjs");
    fs::write(&probe_path, probe_source).expect("test-only scalar probe must be written");
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/abi/scalar-v1-fixtures.json");
    let script = r#"
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
const target = await import(pathToFileURL(process.argv[1]).href);
const fixture = JSON.parse(await readFile(process.argv[2], "utf8"));
const cases = fixture.carrierCases.filter((entry) => entry.target === "javascript");
for (const entry of cases) {
    let raw;
    if (entry.raw.kind === "javascript-bool") raw = entry.raw.value;
    else if (entry.raw.number.kind === "finite") raw = entry.raw.number.value;
    else if (entry.raw.number.kind === "nan") raw = Number.NaN;
    else if (entry.raw.number.kind === "positive-infinity") raw = Number.POSITIVE_INFINITY;
    else if (entry.raw.number.kind === "negative-infinity") raw = Number.NEGATIVE_INFINITY;
    else throw new Error("unexpected JavaScript carrier fixture");
    const validate = entry.scalarType === "bool" ? target.boolProbe : target.i32Probe;
    try {
      const value = validate(raw);
      if (entry.errorCode !== null || entry.value === null || !Object.is(value, entry.value.value)) {
        throw new Error(`fixture expected rejection for ${entry.scalarType}/${entry.direction}`);
      }
    } catch (error) {
      const code = String(error.message).split(":", 1)[0];
      if (code !== entry.errorCode) {
        throw new Error(`fixture mismatch for ${entry.scalarType}/${entry.direction}: ${code}`);
      }
    }
}
process.stdout.write(JSON.stringify({
  count: cases.length,
  bool: cases.filter((entry) => entry.scalarType === "bool").length,
  i32: cases.filter((entry) => entry.scalarType === "i32").length,
}));
"#;
    let output = run_node_module(&probe_path, script, &[&fixture]);

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).expect("Node output must be UTF-8"),
        "{\"count\":20,\"bool\":5,\"i32\":15}"
    );
}

#[test]
fn rejected_boolean_source_and_existing_outputs_never_report_a_new_artifact() {
    let output_root = JavaScriptRoot::new("fail-closed");
    let bool_sources = source_map("src/main.zry", "export function yes(): bool { return true; }");
    let wasm_error =
        compile_webassembly(&typescript_frontend(), &bool_sources, output_root.output(), "main")
            .expect_err("Boolean source must not produce WebAssembly under I32V1");
    assert!(matches!(wasm_error, WebAssemblyBuildError::Source(_)));
    assert!(wasm_error.diagnostics().iter().any(|item| item.code() == "ZRYNA-I1006"));
    assert!(fs::read_dir(output_root.path()).expect("fixture listing").next().is_none());

    let native_error = compile_native_object(
        &typescript_frontend(),
        &bool_sources,
        output_root.output(),
        "main",
        zryna_backend_native::NATIVE_OBJECT_TARGET,
    )
    .expect_err("Boolean source must not produce a native object under I32V1");
    assert!(matches!(native_error, NativeObjectBuildError::Source(_)));
    assert!(native_error.diagnostics().iter().any(|item| item.code() == "ZRYNA-I1006"));
    assert!(fs::read_dir(output_root.path()).expect("fixture listing").next().is_none());

    let unsupported_error = compile_native_object(
        &typescript_frontend(),
        &bool_sources,
        output_root.output(),
        "main",
        "x86_64-pc-windows-msvc",
    )
    .expect_err("unsupported target must fail before source compilation");
    assert!(matches!(unsupported_error, NativeObjectBuildError::Backend(_)));
    assert_eq!(unsupported_error.diagnostics()[0].code(), "ZRYNA-N3001");
    assert!(fs::read_dir(output_root.path()).expect("fixture listing").next().is_none());

    let error =
        compile_javascript(&typescript_frontend(), &bool_sources, output_root.output(), "main")
            .expect_err("Boolean source must remain outside I32V1");
    assert!(matches!(error, JavaScriptBuildError::Source(_)));
    assert!(error.diagnostics().iter().any(|item| item.code() == "ZRYNA-I1006"));
    assert!(fs::read_dir(output_root.path()).expect("fixture listing").next().is_none());

    let destination = output_root.path().join("main.mjs");
    fs::write(&destination, b"sentinel").expect("sentinel must be written");
    let i32_sources = source_map("src/main.zry", "export function value(): i32 { return 1; }");
    let error =
        compile_javascript(&typescript_frontend(), &i32_sources, output_root.output(), "main")
            .expect_err("create-only publication must preserve the destination");
    assert!(matches!(error, JavaScriptBuildError::Publication(_)));
    assert_eq!(error.diagnostics()[0].code(), "ZRYNA-D2007");
    assert_eq!(fs::read(destination).expect("sentinel must remain"), b"sentinel");
}

#[test]
fn one_verified_program_drives_both_backend_boundaries() {
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/add.zry".to_owned(),
        text: "x".to_owned(),
    }])
    .expect("fixture source map must be valid");
    let path = NormalizedSourcePath::new("src/add.zry").expect("fixture path must be valid");
    let file = sources.file_id(&path).expect("fixture file must exist");
    let span = sources.span(file, 0, 1).expect("fixture span must be valid");
    let program = Program {
        functions: vec![
            Function {
                name: "add".to_owned(),
                parameters: vec![Type::I32, Type::I32],
                return_type: Type::I32,
                expressions: vec![
                    Expr { ty: Type::I32, span, kind: ExprKind::Parameter(0) },
                    Expr { ty: Type::I32, span, kind: ExprKind::Parameter(1) },
                    Expr {
                        ty: Type::I32,
                        span,
                        kind: ExprKind::I32Add { lhs: ExprId(0), rhs: ExprId(1) },
                    },
                ],
                body: ExprId(2),
            },
            Function {
                name: "answer".to_owned(),
                parameters: Vec::new(),
                return_type: Type::I32,
                expressions: vec![Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(42) }],
                body: ExprId(0),
            },
        ],
    };
    let verified = verify(program, &sources).expect("fixture IR must verify");

    let artifacts = super::emit_verified(&verified).expect("both backends must emit");

    assert!(artifacts.javascript.source.contains("export function add(p0, p1)"));
    assert!(artifacts.javascript.source.contains("export function answer()"));
    assert!(artifacts.javascript.source.contains("(v0 + v1) | 0"));
    assert!(artifacts.javascript.source.contains("$zryna$i32"));
    assert_eq!(
        artifacts.llvm_ir.source,
        "define i32 @zryna_v1_e_add(i32 %p0, i32 %p1) {\nentry:\n  %v2 = add i32 %p0, %p1\n  ret i32 %v2\n}\ndefine i32 @zryna_v1_e_answer() {\nentry:\n  %v0 = add i32 0, 42\n  ret i32 %v0\n}\n"
    );
}

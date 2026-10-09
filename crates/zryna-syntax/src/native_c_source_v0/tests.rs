use super::{authenticate_sources, parser, raw::ExpressionKind};
use crate::native_c_v0::raw::Primitive;
use std::fmt::Write as _;
use zryna_source::{SourceFileInput, SourceMap};

fn map(text: &str) -> SourceMap {
    SourceMap::build(vec![SourceFileInput { path: "source.zry".into(), text: text.into() }])
        .expect("independent source capture")
}

fn reference() -> SourceMap {
    SourceMap::build(
        [
            ("buffer", include_str!("../../../../tests/native-c-abi-v0/source-buffer.zry")),
            ("handle", include_str!("../../../../tests/native-c-abi-v0/source-handle.zry")),
            ("scalar", include_str!("../../../../tests/native-c-abi-v0/source-scalar.zry")),
        ]
        .into_iter()
        .map(|(name, text)| SourceFileInput {
            path: format!("tests/native-c-abi-v0/source-{name}.zry"),
            text: text.into(),
        })
        .collect(),
    )
    .expect("complete independent reference sources")
}

#[test]
fn native_c_source_v0_complete_reference_has_exact_sites_and_original_map_identity() {
    let sources = reference();
    let syntax = authenticate_sources(&sources).expect("complete restricted source grammar");
    assert!(syntax.belongs_to(&sources));
    assert!(!syntax.belongs_to(&reference()), "byte-identical rebuilt maps cannot share authority");
    assert_eq!(syntax.files().iter().map(|file| file.functions().len()).sum::<usize>(), 6);
    assert_eq!(syntax.files().iter().map(|file| file.sites().len()).sum::<usize>(), 31);
    let claims: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../tests/native-c-abi-v0/declarations.ffi.json"
    ))
    .expect("independent fixture JSON");
    let actual: Vec<_> = syntax
        .files()
        .iter()
        .flat_map(|file| file.sites().iter().map(move |site| (file, site)))
        .collect();
    for (file, claim) in
        syntax.files().iter().zip(claims["sources"].as_array().expect("source claims"))
    {
        let digest = file.sha256().iter().fold(String::new(), |mut text, byte| {
            write!(text, "{byte:02x}").expect("digest formatting");
            text
        });
        assert_eq!(digest, claim["sha256"].as_str().expect("independent fixed source digest"));
    }
    for ((file, site), claim) in actual.iter().zip(claims["sites"].as_array().expect("site claims"))
    {
        assert_eq!(file.path(), claim["path"].as_str().expect("path"));
        assert_eq!(u64::from(site.span().start()), claim["start"].as_u64().expect("start"));
        assert_eq!(u64::from(site.span().end()), claim["end"].as_u64().expect("end"));
        assert_eq!(serde_json::to_value(site.primitive()).expect("primitive"), claim["primitive"]);
        assert_eq!(site.operation(), claim["operation"].as_str());
        assert!(sources.resolve(site.span()).is_ok());
    }
}

#[test]
fn native_c_source_v0_token_and_expression_arena_bounds_are_exact() {
    assert!(super::lexer::lex(&"0;".repeat(131_072)).is_ok());
    assert_eq!(
        super::lexer::lex(&format!("{}0", "0;".repeat(131_072)))
            .expect_err("first extra token")
            .detail(),
        "tokens"
    );
    let addition = format!("{};", vec!["0"; 128].join("+"));
    let body = format!("{}{}", addition.repeat(64), "0;".repeat(64));
    let exact =
        parser::parse(&format!("function f(): i32 {{ {body} }}")).expect("exact expression arena");
    assert_eq!(exact[0].expressions.len(), 16384);
    assert_eq!(
        parser::parse(&format!("function f(): i32 {{ {body} 0; }}"))
            .expect_err("first extra expression")
            .detail(),
        "expressions"
    );
}

#[test]
fn native_c_source_v0_intrinsics_in_comments_are_not_nodes_and_offsets_count_utf8_bytes() {
    let text = "// é\nfunction f(): i32 { /* Ffi.rawCall(\"x@0/a\", 0) */ return Ffi.rawCall(\"x@0/a\", 1); }";
    let sources = map(text);
    let syntax = authenticate_sources(&sources).expect("UTF-8 comment and actual intrinsic");
    let sites = syntax.files()[0].sites();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].span().start() as usize, text.rfind("Ffi.rawCall").expect("actual call"));
    assert_eq!(sites[0].primitive(), Primitive::RawCall);
    assert_eq!(sites[0].operation(), Some("x@0/a"));
    assert_eq!(syntax.files()[0].sha256().len(), 32);
}

#[test]
fn native_c_source_v0_complete_input_rejects_aliases_namespace_shadowing_and_hidden_tail() {
    for text in [
        "function Ffi(): i32 { return 0; }",
        "function f(Ffi: i32): i32 { return Ffi; }",
        "function f(): i32 { const Ffi: i32 = 0; return Ffi; }",
        "import { Ffi } from \"x\"; function f(): i32 { return 0; }",
        "function f(): i32 { return Ffi[\"rawCall\"](); }",
        "function f(): i32 { return Ffi?.rawCall(); }",
        "function f(): i32 { return Ffi.rawCall<i32>(\"x@0/a\"); }",
        "function f(): i32 { return Ffi.rawCall(\"x@0/a\", ...values); }",
        "function f(): i32 { return Ffi.rawCall(\"x@0/\\u0061\", 0); }",
        "function f(): i32 { return Ffi.rawCa11(\"x@0/a\", 0); }",
        "function f(): i32 { return plain(0); }",
        "function f(): i32 { return 0; } ignored",
        "function f(): i32 { return 0;",
        "function f(): i32 { /* never ends",
        "function f(): i32 { return Ffi.outI32(1); }",
        "function f(): i32 { return Ffi.rawCall(key, 0); }",
        "function f(): i32 { if (status === 0) { return Ffi.foreignError(\"x@0/a\", status); } return 0; }",
    ] {
        assert!(authenticate_sources(&map(text)).is_err(), "{text}");
    }
    assert!(authenticate_sources(&reference()).is_ok(), "recovery does not retain rejected syntax");
}

#[test]
fn native_c_source_v0_scalar_limits_and_flat_arenas_are_exact() {
    assert!(parser::parse("function f(): i32 { return -2147483648 + 2147483647; }").is_ok());
    for literal in ["2147483648", "-2147483649", "01"] {
        assert!(parser::parse(&format!("function f(): i32 {{ return {literal}; }}")).is_err());
    }
    let addition =
        |count| format!("function f(): i32 {{ return {}; }}", vec!["0"; count].join("+"));
    let exact = parser::parse(&addition(128)).expect("exact expression depth");
    assert_eq!(exact[0].expressions.len(), 255);
    assert!(matches!(exact[0].expressions.last().expect("root").kind, ExpressionKind::Add(_, _)));
    assert_eq!(
        parser::parse(&addition(129)).expect_err("first extra").detail(),
        "expression-depth"
    );
    let nested = |count| {
        format!(
            "function f(): i32 {{ return {}0{}; }}",
            "Ffi.byteLength(".repeat(count),
            ")".repeat(count)
        )
    };
    assert!(parser::parse(&nested(127)).is_ok());
    assert_eq!(
        parser::parse(&nested(128)).expect_err("first extra nested expression").code(),
        "ZRYNA-C4107"
    );
    let parameters =
        |count| (0..count).map(|index| format!("p{index}: i32")).collect::<Vec<_>>().join(",");
    assert!(parser::parse(&format!("function f({}): i32 {{ return 0; }}", parameters(16))).is_ok());
    assert_eq!(
        parser::parse(&format!("function f({}): i32 {{ return 0; }}", parameters(17)))
            .expect_err("first extra parameter")
            .detail(),
        "parameters"
    );
}

#[test]
fn native_c_source_v0_function_statement_and_site_budgets_are_exact() {
    let functions = |count| {
        (0..count).fold(String::new(), |mut text, index| {
            write!(text, "function f{index}(): i32 {{ return 0; }}").expect("fixture formatting");
            text
        })
    };
    assert_eq!(parser::parse(&functions(256)).expect("exact functions").len(), 256);
    assert_eq!(
        parser::parse(&functions(257)).expect_err("first extra function").detail(),
        "functions"
    );
    let statements = |count| format!("function f(): i32 {{ {} }}", "0;".repeat(count));
    assert!(parser::parse(&statements(4096)).is_ok());
    assert_eq!(
        parser::parse(&statements(4097)).expect_err("first extra statement").detail(),
        "statements"
    );
    let sites = |count: usize| {
        format!(
            "function a(): i32 {{ {} }} function b(): i32 {{ {} }}",
            "Ffi.outI32();".repeat(2048),
            "Ffi.outI32();".repeat(count - 2048)
        )
    };
    assert!(authenticate_sources(&map(&sites(4096))).is_ok());
    assert_eq!(
        authenticate_sources(&map(&sites(4097))).expect_err("first extra site").detail(),
        "sites"
    );
}

#[test]
fn native_c_source_v0_file_and_aggregate_source_budgets_are_exact() {
    let inputs = |count| {
        (0..count)
            .map(|index| SourceFileInput { path: format!("f{index:03}.zry"), text: String::new() })
            .collect()
    };
    assert!(
        authenticate_sources(&SourceMap::build(inputs(256)).expect("exact file capture")).is_ok()
    );
    assert_eq!(
        authenticate_sources(&SourceMap::build(inputs(257)).expect("first-extra capture"))
            .expect_err("foreign file bound")
            .detail(),
        "sources"
    );
    let chunk = " ".repeat(2 * 1024 * 1024);
    let mut files: Vec<_> = (0..4)
        .map(|index| SourceFileInput { path: format!("f{index}.zry"), text: chunk.clone() })
        .collect();
    assert!(
        authenticate_sources(&SourceMap::build(files.clone()).expect("exact byte capture")).is_ok()
    );
    files.push(SourceFileInput { path: "extra.zry".into(), text: " ".into() });
    assert_eq!(
        authenticate_sources(&SourceMap::build(files).expect("first-extra byte capture"))
            .expect_err("aggregate byte bound")
            .detail(),
        "aggregate-source-bytes"
    );
}

#[test]
fn native_c_source_v0_project_function_statement_and_expression_budgets_are_exact() {
    let check = |texts: Vec<String>, metric| {
        let mut files: Vec<_> = texts
            .into_iter()
            .enumerate()
            .map(|(index, text)| SourceFileInput { path: format!("f{index:03}.zry"), text })
            .collect();
        assert!(
            authenticate_sources(&SourceMap::build(files.clone()).expect("exact capture")).is_ok(),
            "{metric}"
        );
        files.push(SourceFileInput {
            path: "z-extra.zry".into(),
            text: "function extra(): i32 { return 0; }".into(),
        });
        assert_eq!(
            authenticate_sources(&SourceMap::build(files).expect("first extra capture"))
                .expect_err("first extra project occupant")
                .detail(),
            metric
        );
    };
    let functions = (0..256).fold(String::new(), |mut text, index| {
        write!(text, "function f{index}(): i32 {{ return 0; }}").expect("fixture formatting");
        text
    });
    check(vec![functions; 16], "project-functions");
    let statements = format!("function f(): i32 {{ {} }}", "0;".repeat(4096));
    check(vec![statements; 16], "project-statements");
    let addition = format!("{};", vec!["0"; 128].join("+"));
    let expressions = format!("function f(): i32 {{ {}{} }}", addition.repeat(64), "0;".repeat(64));
    check(vec![expressions; 16], "project-expressions");
}

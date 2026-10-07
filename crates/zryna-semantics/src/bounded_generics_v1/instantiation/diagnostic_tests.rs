//! Source rejection and synthetic ordering/overflow proofs are separate evidence.

use std::fmt::Write as _;

use super::diagnostics::Errors;
use super::{BodyTypeContext, InstantiationFailure, keys, tests::check};
use crate::bounded_generics_v1::tests::body_fixtures::project;
use crate::bounded_generics_v1::{SemanticInput, body_types, resolve_declarations};
use zryna_source::{NormalizedSourcePath, UntrustedSpan};

fn bodies(test: impl FnOnce(&BodyTypeContext<'_, '_>)) {
    let input = project(&[("main.zry", "function score():i32 {return 0;}")]);
    let entry = input
        .sources
        .file_id(&NormalizedSourcePath::new("main.zry").expect("path"))
        .expect("source entry");
    let declarations = resolve_declarations(
        SemanticInput::try_new(&input.syntax, &input.sources, entry).expect("original authority"),
    )
    .expect("declarations");
    let context = body_types::check_body_types(&declarations).expect("original bodies");
    test(&context);
}

#[test]
fn independent_source_cycles_collect_closing_calls_in_utf8_span_order() {
    let source = "function text():String {return \"é😀\";}\r\nfunction z():i32 {return z();}\r\nfunction a():i32 {return a();}";
    let InstantiationFailure::Diagnostics(errors) =
        check(&[("main.zry", source)]).expect_err("both independent cycles must reject")
    else {
        panic!("source diagnostics");
    };
    assert_eq!(errors.len(), 2);
    for (error, call) in errors.iter().zip(["z();", "a();"]) {
        assert_eq!(error.code(), "ZRYNA-M7003");
        let start = u32::try_from(source.find(call).expect("closing call")).expect("byte span");
        let span = error.primary_span().expect("authenticated span");
        assert_eq!((span.start(), span.end()), (start, start + 1));
        assert!(error.message().contains("original declaration path"));
    }
    assert_eq!(
        errors.iter().map(zryna_diagnostics::Diagnostic::message).collect::<Vec<_>>(),
        match check(&[("main.zry", source)]).expect_err("independent replay") {
            InstantiationFailure::Diagnostics(replay) =>
                replay.iter().map(|e| e.message().to_owned()).collect::<Vec<_>>(),
            _ => panic!("source diagnostics"),
        }
    );
}

#[test]
fn independent_invalid_nominal_arguments_collect_before_generated_expansion() {
    let source = "interface BadZ extends ZrynaStruct {loan:Borrow<i32>;} interface BadA extends ZrynaStruct {loan:Borrow<i32>;} interface Box<T extends ZrynaValue> extends ZrynaStruct {value:T;} interface Nest<T extends ZrynaValue> extends ZrynaStruct {next:Vec<Nest<Vec<T>>>;} function read(x:Box<BadZ>,y:Box<BadA>,z:Nest<i32>):i32 {return 0;}";
    let InstantiationFailure::Diagnostics(errors) = check(&[("main.zry", source)])
        .expect_err("both original arguments reject before pending expansion")
    else {
        panic!("source diagnostics");
    };
    assert_eq!(errors.len(), 2);
    for (error, argument) in errors.iter().zip(["BadZ>", "BadA>"]) {
        assert_eq!(error.code(), "ZRYNA-M7001");
        let start =
            u32::try_from(source.find(argument).expect("argument text")).expect("byte span");
        let span = error.primary_span().expect("original argument span");
        assert_eq!((span.start(), span.end()), (start, start + 4));
    }
}

#[test]
fn source_cycles_accept_255_diagnostics_and_reserve_the_256th_terminal() {
    for count in [255, 256, 257] {
        let mut source = String::new();
        for i in 0..count {
            writeln!(source, "function r{i}():i32 {{return r{i}();}}").expect("fixture storage");
        }
        let InstantiationFailure::Diagnostics(errors) =
            check(&[("main.zry", &source)]).expect_err("source recursion")
        else {
            panic!("source diagnostics");
        };
        assert_eq!(errors.len(), count.min(256));
        assert!(errors[..count.min(255)].iter().all(|e| e.code() == "ZRYNA-M7003"));
        if count >= 256 {
            assert_eq!(errors[255].code(), "ZRYNA-M7201");
            assert!(errors[255].message().contains("rejected count 256"));
            let start = u32::try_from(source.find("r255();").expect("first extra call"))
                .expect("byte span");
            assert_eq!(errors[255].primary_span().expect("terminal span").start(), start);
        }
    }
    assert!(check(&[("main.zry", "function score():i32 {return 7;}")]).is_ok());
}

#[test]
fn synthetic_candidates_sort_code_unsigned_key_numeric_witness_and_deduplicate() {
    bodies(|bodies| {
        let mut errors = Errors::default();
        for (code, key, witness, message) in [
            ("ZRYNA-M7003", 0, 0, "code last"),
            ("ZRYNA-M7001", 128, 0, "unsigned key last"),
            ("ZRYNA-M7001", 1, 2, "witness last"),
            ("ZRYNA-M7001", 1, 1, "first"),
            ("ZRYNA-M7001", 1, 1, "duplicate tuple"),
        ] {
            errors
                .at(bodies, code, None, &[key], &[witness], message.into())
                .expect("candidate storage");
        }
        assert_eq!(
            errors
                .finish()
                .expect("bounded output")
                .iter()
                .map(zryna_diagnostics::Diagnostic::message)
                .collect::<Vec<_>>(),
            ["first", "witness last", "unsigned key last", "code last"]
        );
    });
}

#[test]
fn authenticated_locations_sort_before_global_and_invalid_spans_fail_internal() {
    bodies(|bodies| {
        let at = bodies.declarations().syntax().files()[0].functions[0].name.span;
        let mut errors = Errors::default();
        errors
            .at(bodies, "ZRYNA-M7001", None, &[], &[], "global".into())
            .expect("global candidate");
        errors
            .at(bodies, "ZRYNA-M7003", Some(at), &[], &[], "source".into())
            .expect("source candidate");
        let invalid = UntrustedSpan { file: 999, start: 0, end: 1 };
        assert!(matches!(
            errors.at(bodies, "ZRYNA-M7001", Some(invalid), &[], &[], "forged".into()),
            Err(InstantiationFailure::InternalFailure)
        ));
        assert!(matches!(
            super::failure(bodies, "ZRYNA-M7001", Some(invalid), "forged".into()),
            InstantiationFailure::InternalFailure
        ));
        let result = errors.finish().expect("bounded output");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message(), "source");
        assert_eq!(result[1].message(), "global");
    });
}

#[test]
fn synthetic_key_and_inventory_arithmetic_overflow_never_wraps() {
    assert_eq!(keys::encoded_size(2, [1, 3].into_iter()).expect("actual key size"), 21);
    for (lanes, lengths) in
        [(usize::MAX, vec![]), (0, vec![usize::MAX]), (0, vec![usize::MAX - 5, 1])]
    {
        assert!(matches!(
            keys::encoded_size(lanes, lengths.into_iter()),
            Err(InstantiationFailure::InternalFailure)
        ));
    }
    assert!(matches!(
        super::checked_count([usize::MAX, 1].into_iter()),
        Err(InstantiationFailure::InternalFailure)
    ));
    assert_eq!(
        super::checked_count([usize::MAX - 1, 1].into_iter()).expect("exact arithmetic"),
        usize::MAX
    );
}

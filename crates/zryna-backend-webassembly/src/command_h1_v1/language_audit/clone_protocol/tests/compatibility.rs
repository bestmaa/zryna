use super::Candidate;
use wasm_encoder::{
    ConstExpr, GlobalSection, GlobalType, Module, RawSection, Section as _, ValType,
};
use wasmparser::{Parser, Payload, TypeRef};

#[test]
fn commands_without_sealed_prefix_keep_six_imported_globals_and_no_defined_globals() {
    for name in ["pure-entry", "control-flow", "weak-upgrade", "environment-consume"] {
        let candidate = Candidate::new(name);
        let shape = super::super::super::declarations::Shape::derive(&candidate.program)
            .expect("whole source declaration shape");
        let protocol = super::super::Protocol::derive(&candidate.program, &shape)
            .expect("independently derived protocol mode");
        assert!(!protocol.enabled(), "fixture must have no sealed clone prefix");
        let mut imported = 0;
        for payload in Parser::new(0).parse_all(&candidate.bytes) {
            match payload.expect("candidate section") {
                Payload::ImportSection(imports) => {
                    imported += imports
                        .into_imports()
                        .filter(|import| {
                            matches!(import.as_ref().expect("import").ty, TypeRef::Global(_))
                        })
                        .count();
                }
                Payload::GlobalSection(_) => panic!("disabled protocol must preserve no section"),
                _ => {}
            }
        }
        assert_eq!(imported, 6);
        super::super::super::audit(&candidate.bytes, &candidate.program)
            .expect("original closed command form accepted");
        candidate.reject(&with_private_globals(&candidate.bytes));
    }
}

fn with_private_globals(original: &[u8]) -> Vec<u8> {
    let mut globals = GlobalSection::new();
    for _ in 0..4 {
        globals.global(
            GlobalType { val_type: ValType::I32, mutable: true, shared: false },
            &ConstExpr::i32_const(0),
        );
    }
    let mut module = Module::new();
    let mut inserted = false;
    for payload in Parser::new(0).parse_all(original) {
        if let Some((id, range)) = payload.expect("original section").as_section() {
            if !inserted && id > globals.id() {
                module.section(&globals);
                inserted = true;
            }
            let start = usize::try_from(range.start).expect("bounded section start");
            let end = usize::try_from(range.end).expect("bounded section end");
            module.section(&RawSection { id, data: &original[start..end] });
        }
    }
    assert!(inserted, "private globals inserted before original exports");
    module.finish()
}

mod bindings;
mod compatibility;
mod helper_roles;
mod mutations;
mod nested_types;

use wasm_encoder::{CodeSection, Encode as _, Instruction, Module, RawSection, Section as _};
use wasmparser::{FunctionBody, Operator, Parser, Payload, Validator, WasmFeatures};
use zryna_ir::command_h1_v1::VerifiedProgram;
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::{command_h1_v1, v4};

struct Candidate {
    program: VerifiedProgram,
    bytes: Vec<u8>,
}

struct Operation<'a> {
    start: usize,
    end: usize,
    operator: Operator<'a>,
}

impl Candidate {
    fn new(name: &str) -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/wasi-command-source-fixtures");
        let text =
            std::fs::read_to_string(root.join(format!("{name}.zry"))).expect("source fixture");
        let bytes = std::fs::read(root.join(format!("{name}.json"))).expect("provider fixture");
        let sources = SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text }])
            .expect("source authority");
        let syntax =
            v4::verify_snapshot(v4::decode_snapshot(&bytes).expect("provider decode"), &sources)
                .expect("authenticated provider source");
        let source = command_h1_v1::admit(&syntax, &sources).expect("whole command syntax");
        let compiled = zryna_semantics::command_h1_v1::lower(&source, &sources)
            .expect("real mandatory command verification");
        let program = compiled.verified_ir().clone();
        let bytes = crate::data_ownership_v1::encode::command(&program)
            .expect("candidate only, never an audit reference");
        Self { program, bytes }
    }

    fn reject(&self, bytes: &[u8]) {
        Validator::new_with_features(WasmFeatures::WASM1)
            .validate_all(bytes)
            .expect("independent mutant remains valid Wasm");
        let error = super::super::audit(bytes, &self.program)
            .expect_err("private protocol mutant rejected");
        assert_eq!(error.code(), "ZRYNA-W4103");
        super::super::audit(&self.bytes, &self.program).expect("original next-valid recovery");
    }

    fn mutate(
        &self,
        select: impl Fn(&FunctionBody<'_>) -> bool,
        edit: impl FnOnce(&mut Vec<u8>, &[Operation<'_>]),
    ) -> Vec<u8> {
        let body = Parser::new(0)
            .parse_all(&self.bytes)
            .find_map(|payload| match payload.expect("candidate syntax") {
                Payload::CodeSectionEntry(body) if select(&body) => Some(body),
                _ => None,
            })
            .expect("independently selected body context");
        let origin = body.range().start;
        let mut reader = body.get_operators_reader().expect("candidate operators");
        let mut operations = Vec::new();
        while !reader.eof() {
            let (operator, start) = reader.read_with_offset().expect("operator");
            operations.push(Operation {
                start: usize::try_from(start - origin).expect("bounded instruction start"),
                end: usize::try_from(reader.original_position() - origin)
                    .expect("bounded instruction end"),
                operator,
            });
        }
        let mut replacement = body.as_bytes().to_vec();
        edit(&mut replacement, &operations);
        let mut code = CodeSection::new();
        for payload in Parser::new(0).parse_all(&self.bytes) {
            if let Payload::CodeSectionEntry(original) = payload.expect("original code") {
                code.raw(if original.range() == body.range() {
                    &replacement
                } else {
                    original.as_bytes()
                });
            }
        }
        self.section(code.id(), &code)
    }

    fn section(&self, target: u8, replacement: &impl wasm_encoder::Section) -> Vec<u8> {
        let mut module = Module::new();
        for payload in Parser::new(0).parse_all(&self.bytes) {
            if let Some((id, range)) = payload.expect("original section").as_section() {
                if id == target {
                    module.section(replacement);
                } else {
                    let start = usize::try_from(range.start).expect("bounded start");
                    let end = usize::try_from(range.end).expect("bounded end");
                    module.section(&RawSection { id, data: &self.bytes[start..end] });
                }
            }
        }
        module.finish()
    }
}

fn replace(body: &mut Vec<u8>, operation: &Operation<'_>, instruction: &Instruction<'_>) {
    let mut bytes = Vec::new();
    instruction.encode(&mut bytes);
    body.splice(operation.start..operation.end, bytes);
}

fn has_acquisition(body: &FunctionBody<'_>) -> bool {
    let mut reader = body.get_operators_reader().expect("candidate helper");
    let mut previous = false;
    while !reader.eof() {
        let operator = reader.read().expect("candidate helper instruction");
        if previous && matches!(operator, Operator::I32Const { value: 1 }) {
            return true;
        }
        previous = matches!(operator, Operator::GlobalGet { global_index: 9 });
    }
    false
}

#[test]
fn complete_private_protocol_admits_real_verified_candidates() {
    for name in [
        "pure-entry",
        "owned-aggregates",
        "weak-upgrade",
        "environment-consume",
        "clone-aggregate",
        "clone-vector",
        "clone-generic",
    ] {
        let candidate = Candidate::new(name);
        super::super::audit(&candidate.bytes, &candidate.program)
            .expect("independent complete language protocol audit");
    }
}

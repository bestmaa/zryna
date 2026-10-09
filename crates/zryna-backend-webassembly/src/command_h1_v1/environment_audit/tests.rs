mod callsites;
mod mutations;

use wasm_encoder::{CodeSection, Encode as _, Instruction, Module, RawSection, Section as _};
use wasmparser::{FunctionBody, Parser, Payload, Validator, WasmFeatures};
use zryna_ir::command_h1_v1::VerifiedProgram;
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::{command_h1_v1, v4};

struct Candidate {
    core: Vec<u8>,
    program: VerifiedProgram,
}

impl Candidate {
    fn new(name: &str) -> Self {
        Self::with_key(name, None)
    }

    fn with_key(name: &str, key: Option<&str>) -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/wasi-command-source-fixtures");
        let mut text =
            std::fs::read_to_string(root.join(format!("{name}.zry"))).expect("source fixture");
        let mut provider =
            std::fs::read(root.join(format!("{name}.json"))).expect("provider fixture");
        if let Some(key) = key {
            assert!(key.is_ascii() && key.len() == 4);
            text = text.replace("MODE", key);
            provider = String::from_utf8(provider)
                .expect("UTF-8 fixture")
                .replace("MODE", key)
                .into_bytes();
        }
        let sources = SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text }])
            .expect("source authority");
        let syntax =
            v4::verify_snapshot(v4::decode_snapshot(&provider).expect("provider decode"), &sources)
                .expect("authenticated syntax");
        let source = command_h1_v1::admit(&syntax, &sources).expect("whole command source");
        let result =
            zryna_semantics::command_h1_v1::lower(&source, &sources).expect("verified command");
        let program = result.verified_ir().clone();
        // The real encoder supplies only the candidate, never the independent reference.
        let core = crate::data_ownership_v1::encode::command(&program).expect("candidate core");
        Self { core, program }
    }

    fn body<'a>(&self, bytes: &'a [u8]) -> FunctionBody<'a> {
        let index = self.program.linear32_layouts().types().len() * 2;
        Self::body_at(bytes, index)
    }

    fn body_at(bytes: &[u8], index: usize) -> FunctionBody<'_> {
        Parser::new(0)
            .parse_all(bytes)
            .filter_map(|payload| match payload.expect("core syntax") {
                Payload::CodeSectionEntry(body) => Some(body),
                _ => None,
            })
            .nth(index)
            .expect("environment helper")
    }

    fn mutate(&self, changes: impl FnOnce(&mut Vec<u8>, &[Operation])) -> Vec<u8> {
        self.mutate_body(self.program.linear32_layouts().types().len() * 2, changes)
    }

    fn mutate_body(
        &self,
        index: usize,
        changes: impl FnOnce(&mut Vec<u8>, &[Operation]),
    ) -> Vec<u8> {
        let target = Self::body_at(&self.core, index);
        let origin = target.range().start;
        let mut reader = target.get_operators_reader().expect("candidate operators");
        let mut operations = Vec::new();
        while !reader.eof() {
            let (operator, start) = reader.read_with_offset().expect("candidate instruction");
            operations.push(Operation {
                // Mutation selection also visits ordinary language/clone bodies. Their
                // other operations stay opaque; production reference matching stays closed.
                text: super::normalize::operator(&operator)
                    .unwrap_or_else(|_| "other language instruction".to_owned()),
                start: usize::try_from(start - origin).expect("bounded instruction start"),
                end: usize::try_from(reader.original_position() - origin)
                    .expect("bounded instruction end"),
            });
        }
        let mut body = target.as_bytes().to_vec();
        changes(&mut body, &operations);
        let mut code = CodeSection::new();
        for payload in Parser::new(0).parse_all(&self.core) {
            if let Payload::CodeSectionEntry(original) = payload.expect("candidate syntax") {
                code.raw(if original.range() == target.range() {
                    &body
                } else {
                    original.as_bytes()
                });
            }
        }
        let mut module = Module::new();
        for payload in Parser::new(0).parse_all(&self.core) {
            if let Some((id, range)) = payload.expect("candidate section").as_section() {
                if id == code.id() {
                    module.section(&code);
                } else {
                    let start = usize::try_from(range.start).expect("bounded section start");
                    let end = usize::try_from(range.end).expect("bounded section end");
                    module.section(&RawSection { id, data: &self.core[start..end] });
                }
            }
        }
        module.finish()
    }

    fn reject(&self, bytes: &[u8]) {
        Validator::new_with_features(WasmFeatures::WASM1)
            .validate_all(bytes)
            .expect("independent mutation remains valid WebAssembly 1.0");
        let diagnostic = super::audit(&self.body(bytes), &self.program)
            .expect_err("independent hostile environment helper rejected");
        assert_eq!(diagnostic.code(), "ZRYNA-W4103");
        super::audit(&self.body(&self.core), &self.program).expect("next valid candidate recovers");
    }
}

struct Operation {
    text: String,
    start: usize,
    end: usize,
}

fn locate(operations: &[Operation], pattern: &[&str], after: usize) -> usize {
    operations
        .windows(pattern.len())
        .enumerate()
        .find_map(|(index, actual)| {
            (index >= after
                && actual.iter().zip(pattern).all(|(actual, expected)| actual.text == *expected))
            .then_some(index)
        })
        .expect("independently named mutation context exists")
}

fn replacement(body: &mut Vec<u8>, operation: &Operation, instruction: &Instruction<'_>) {
    let mut encoded = Vec::new();
    instruction.encode(&mut encoded);
    body.splice(operation.start..operation.end, encoded);
}

#[test]
fn complete_independent_environment_reference_admits_whole_verified_candidates() {
    for name in ["environment-match", "environment-helper", "environment-live-prefix"] {
        let candidate = Candidate::new(name);
        Validator::new_with_features(WasmFeatures::WASM1)
            .validate_all(&candidate.core)
            .expect("candidate WebAssembly 1.0");
        super::audit(&candidate.body(&candidate.core), &candidate.program)
            .expect("complete independently authored reference");
    }
}

#[test]
fn environment_helper_cannot_borrow_authority_from_a_pure_command() {
    let candidate = Candidate::new("environment-match");
    let pure = Candidate::new("pure-entry");
    assert!(super::audit(&candidate.body(&candidate.core), &pure.program).is_err());
}

#[test]
fn environment_helper_rejects_a_different_whole_verified_source_key() {
    let original = Candidate::new("environment-match");
    let changed = Candidate::with_key("environment-match", Some("MORE"));
    assert!(super::audit(&original.body(&original.core), &changed.program).is_err());
    super::audit(&changed.body(&changed.core), &changed.program)
        .expect("fresh source-derived literal reference");
}

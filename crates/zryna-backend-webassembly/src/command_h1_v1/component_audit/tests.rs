mod binding;
mod graph;
mod process;
mod scanner;
mod types;

use std::ops::Range;

use wasm_encoder::{Component, ComponentSection, RawSection};
use wasmparser::{Parser, Payload, Validator, WasmFeatures};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::{command_h1_v1, v4};

use super::super::artifact::Artifact;
use crate::wit_world_audit::AuthenticatedCommandWorld;

struct Candidate {
    artifact: Artifact,
    world: AuthenticatedCommandWorld,
    sources: SourceMap,
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
                .expect("bound syntax");
        let source = command_h1_v1::admit(&syntax, &sources).expect("whole command admission");
        let program = zryna_semantics::command_h1_v1::lower(&source, &sources)
            .expect("paired verified program and runtime");
        let wit = crate::pinned_wit_sources();
        let world =
            AuthenticatedCommandWorld::new(&wit).expect("complete authenticated WIT closure");
        let artifact = Artifact::emit(program.verified_ir(), program.runtime_abi(), &sources, &wit)
            .expect("actual sealed artifact factory");
        Self { artifact, world, sources }
    }

    fn audit(&self, bytes: &[u8]) -> Result<u64, zryna_diagnostics::Diagnostic> {
        assert!(self.artifact.program().source().is_bound_to(&self.sources));
        super::audit(bytes, self.artifact.storage(), self.artifact.language(), &self.world)
    }

    fn reject_valid(&self, bytes: &[u8], code: &str) {
        valid(bytes);
        let error = self.audit(bytes).expect_err("independent final-component mutation rejected");
        assert_eq!(error.code(), code);
        self.audit(self.artifact.bytes()).expect("next retained component recovers");
    }
}

fn valid(bytes: &[u8]) {
    Validator::new_with_features(WasmFeatures::WASM1.union(WasmFeatures::COMPONENT_MODEL))
        .validate_all(bytes)
        .expect("mutation remains an independently valid component");
}

struct Chunk<'a> {
    id: u8,
    range: Range<usize>,
    data: &'a [u8],
}

fn range(range: Range<u64>) -> Range<usize> {
    usize::try_from(range.start).expect("bounded section start")
        ..usize::try_from(range.end).expect("bounded section end")
}

fn chunks(bytes: &[u8]) -> Vec<Chunk<'_>> {
    let mut depth = 0;
    let mut sections = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.expect("component candidate syntax");
        if depth == 0
            && let Some((id, at)) = payload.as_section()
        {
            let at = range(at);
            sections.push(Chunk { id, data: &bytes[at.clone()], range: at });
        }
        match payload {
            Payload::ModuleSection { .. } | Payload::ComponentSection { .. } => depth += 1,
            Payload::End(_) if depth > 0 => depth -= 1,
            _ => {}
        }
    }
    sections
}

fn rewrite(bytes: &[u8], mut edit: impl FnMut(&Chunk<'_>, &mut Component) -> bool) -> Vec<u8> {
    let mut component = Component::new();
    for chunk in chunks(bytes) {
        if !edit(&chunk, &mut component) {
            component.section(&RawSection { id: chunk.id, data: chunk.data });
        }
    }
    component.finish()
}

fn replace(bytes: &[u8], at: Range<u64>, replacement: &impl ComponentSection) -> Vec<u8> {
    let at = range(at);
    let mut changed = false;
    let result = rewrite(bytes, |chunk, component| {
        if chunk.range == at {
            assert!(!changed);
            component.section(replacement);
            changed = true;
            true
        } else {
            false
        }
    });
    assert!(changed, "independently selected original section exists");
    result
}

fn payload(section: &impl ComponentSection) -> Vec<u8> {
    let mut component = Component::new();
    component.section(section);
    let bytes = component.finish();
    let sections = chunks(&bytes);
    assert_eq!(sections.len(), 1);
    sections[0].data.to_vec()
}

#[test]
fn sealed_factory_retains_exact_component_graph_and_source_authority() {
    for name in ["pure-entry", "environment-match", "environment-helper"] {
        let candidate = Candidate::new(name);
        valid(candidate.artifact.bytes());
        let original = rewrite(candidate.artifact.bytes(), |_, _| false);
        assert_eq!(original, candidate.artifact.bytes());
        candidate.audit(&original).expect("whole retained graph");
    }
}

use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, MemorySection, MemoryType, Module, RawSection, Section,
};
use wasmparser::{Operator, Parser, Payload, Validator, WasmFeatures};

use super::super::{audit, encode};
mod boundaries;

#[test]
fn command_storage_independent_memory_and_private_export_mutations_are_rejected() {
    let bytes = encode();
    for pages in [255, 257] {
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: pages,
            maximum: Some(pages),
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        reject_valid(&replace_section(&bytes, &memory));
    }
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    for (index, name) in
        ["arena", "status", "drops", "live", "peak", "references"].into_iter().enumerate()
    {
        exports.export(
            name,
            ExportKind::Global,
            if index == 5 { 9 } else { u32::try_from(index).expect("six language globals") },
        );
    }
    for (index, name) in [
        (0, "allocate"),
        (1, "copy"),
        (2, "realloc"),
        (3, "validate"),
        (4, "drain"),
        (8, "canonical-state"),
    ] {
        exports.export(name, ExportKind::Func, index);
    }
    reject_valid(&replace_section(&bytes, &exports));
}

#[test]
fn command_storage_independent_memory_growth_and_language_state_writes_are_rejected() {
    let bytes = encode();
    let mut code = CodeSection::new();
    let mut count = 0;
    for payload in Parser::new(0).parse_all(&bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("fixed source core") {
            let mut instructions = body.as_bytes().to_vec();
            if count == 0 {
                let end = instructions.pop().expect("final end");
                assert_eq!(end, 0x0b);
                instructions.extend_from_slice(&[0x41, 0, 0x40, 0, 0x1a, 0x0b]);
            }
            code.raw(&instructions);
            count += 1;
        }
    }
    assert_eq!(count, 9);
    reject_valid(&replace_section(&bytes, &code));

    let mut altered = bytes.clone();
    let first = Parser::new(0)
        .parse_all(&bytes)
        .find_map(|payload| match payload.expect("fixed core") {
            Payload::CodeSectionEntry(body) => Some(body),
            _ => None,
        })
        .expect("language allocator");
    let mut operators = first.get_operators_reader().expect("language allocator operators");
    let position = loop {
        let (operator, position) = operators.read_with_offset().expect("global assignment exists");
        if matches!(operator, Operator::GlobalSet { global_index: 0 }) {
            break position;
        }
    };
    let position = usize::try_from(position).expect("bounded operator position");
    assert_eq!(altered[position + 1], 0);
    altered[position + 1] = 6;
    reject_valid(&altered);
}

fn replace_section(bytes: &[u8], replacement: &impl Section) -> Vec<u8> {
    let mut module = Module::new();
    let mut replaced = false;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Some((id, range)) = payload.expect("source core syntax").as_section() {
            if id == replacement.id() {
                assert!(!replaced);
                module.section(replacement);
                replaced = true;
            } else {
                let start = usize::try_from(range.start).expect("bounded section start");
                let end = usize::try_from(range.end).expect("bounded section end");
                module.section(&RawSection { id, data: &bytes[start..end] });
            }
        }
    }
    assert!(replaced);
    module.finish()
}

pub(super) fn reject_valid(bytes: &[u8]) {
    Validator::new_with_features(WasmFeatures::WASM1)
        .validate_all(bytes)
        .expect("mutation remains independently valid WebAssembly 1.0");
    let failure = audit::audit(bytes).expect_err("mutation exceeds command storage authority");
    assert_eq!(failure.code(), "ZRYNA-W4101");
}

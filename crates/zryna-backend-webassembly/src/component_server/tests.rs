use sha2::{Digest, Sha256};
use wasm_encoder::ComponentSection;
use wasmparser::{Parser, Payload};
use zryna_ir::{Expr, ExprId, ExprKind, Function, Program, Type, VerifiedProgram};
use zryna_source::{SourceFileInput, SourceMap};

use super::{ServerOperation, ValidatedServerComponent, audit, emit_server_response, memory};
use crate::{WitSource, pinned_wit_sources};

fn program(status: i32, arity: usize) -> VerifiedProgram {
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "status.zry".into(),
        text: "status".into(),
    }])
    .expect("source map");
    let file = sources.verify_file_id(0).expect("file identity");
    let span = sources.span(file, 0, 6).expect("source span");
    zryna_ir::verify(
        Program {
            functions: vec![Function {
                name: "status".into(),
                parameters: vec![Type::I32; arity],
                return_type: Type::I32,
                expressions: vec![Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(status) }],
                body: ExprId(0),
            }],
        },
        &sources,
    )
    .expect("verified scalar status")
}

fn component(operation: ServerOperation) -> ValidatedServerComponent {
    emit_server_response(&program(201, 0), &pinned_wit_sources(), "status", operation)
        .expect("authenticated executable server component")
}

fn check(
    component: &ValidatedServerComponent,
    bytes: &[u8],
) -> Result<(), zryna_diagnostics::Diagnostic> {
    audit::audit(
        bytes,
        &component.core,
        &component.world,
        &component.core_export,
        component.operation,
    )
}

#[test]
fn exact_world_empty_response_and_clock_arrangements_validate_and_replay() {
    for operation in [ServerOperation::Reply, ServerOperation::ClockRead] {
        let first = component(operation);
        let second = component(operation);
        assert_eq!(first.bytes(), second.bytes());
        assert_eq!(*first.digest(), <[u8; 32]>::from(Sha256::digest(first.bytes())));
        assert_eq!(first.core().bytes(), crate::emit(&program(201, 0)).expect("core").bytes());
        first.revalidate(&program(201, 0)).expect("source revalidation");
        let server = &first.world_audit().worlds()[2];
        assert_eq!(server.resolved_imports().len(), 8);
        assert_eq!(server.exports(), ["wasi:http/incoming-handler@0.2.12"]);
        assert!(!server.resolved_imports().iter().any(|id| id.contains("filesystem")
            || id.contains("environment")
            || id.contains("sockets")));
    }
    assert_ne!(
        component(ServerOperation::Reply).digest(),
        component(ServerOperation::ClockRead).digest()
    );
}

#[test]
fn stale_program_and_wrong_scalar_signature_cannot_enter_server() {
    let first = component(ServerOperation::Reply);
    assert!(first.revalidate(&program(202, 0)).is_err());
    assert!(
        emit_server_response(
            &program(200, 1),
            &pinned_wit_sources(),
            "status",
            ServerOperation::Reply
        )
        .is_err()
    );
    assert!(
        emit_server_response(
            &program(200, 0),
            &pinned_wit_sources(),
            "missing",
            ServerOperation::Reply
        )
        .is_err()
    );
}

#[test]
fn substituted_wit_version_and_environment_world_are_rejected_before_emission() {
    for replacement in ["@0.2.13", "@0.2.11"] {
        let mut sources = pinned_wit_sources();
        let index = sources
            .iter()
            .position(|source| source.path().ends_with("worlds.wit"))
            .expect("root source");
        let original = &sources[index];
        let text = String::from_utf8(original.bytes().to_vec())
            .expect("WIT UTF8")
            .replace("@0.2.12", replacement);
        sources[index] = WitSource::new(original.path(), text.into_bytes());
        assert!(
            emit_server_response(&program(200, 0), &sources, "status", ServerOperation::Reply)
                .is_err()
        );
    }
    let mut sources = pinned_wit_sources();
    let root = &sources[0];
    let text = String::from_utf8(root.bytes().to_vec())
        .expect("WIT UTF8")
        .replace("world server {", "world server { import wasi:cli/environment@0.2.12;");
    sources[0] = WitSource::new(root.path(), text.into_bytes());
    assert!(
        emit_server_response(&program(200, 0), &sources, "status", ServerOperation::Reply).is_err()
    );
}

#[test]
fn mutated_bridge_core_and_allocator_are_rejected_even_with_a_new_digest() {
    let mut component = component(ServerOperation::Reply);
    let ranges = Parser::new(0)
        .parse_all(component.bytes())
        .filter_map(|payload| match payload {
            Ok(Payload::ModuleSection { unchecked_range, .. }) => Some(unchecked_range),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ranges.len(), 3);
    for range in ranges {
        let mut bytes = component.bytes().to_vec();
        bytes[usize::try_from(range.end).expect("bounded module offset") - 2] ^= 1;
        assert!(check(&component, &bytes).is_err());
    }
    component.bytes[0] ^= 1;
    component.digest = Sha256::digest(&component.bytes).into();
    assert!(component.revalidate(&program(201, 0)).is_err(), "digest is not sufficient authority");
}

#[test]
fn malformed_oversized_and_extra_sections_do_not_reach_world_decoder() {
    let component = component(ServerOperation::Reply);
    for bytes in [vec![], vec![0; 32], vec![0; 1024 * 1024 + 1]] {
        assert!(check(&component, &bytes).is_err());
    }
    let mut extra = component.bytes().to_vec();
    wasm_encoder::CustomSection { name: "forged".into(), data: [1, 2, 3].as_slice().into() }
        .append_to_component(&mut extra);
    assert!(check(&component, &extra).is_err());
}

#[test]
fn binary_valid_wrong_wasi_version_cannot_reseal_as_the_server_world() {
    let component = component(ServerOperation::Reply);
    let mut bytes = component.bytes().to_vec();
    let mut changes = 0;
    for start in 0..bytes.len().saturating_sub(7) {
        if &bytes[start..start + 7] == b"@0.2.12" {
            bytes[start + 6] = b'3';
            changes += 1;
        }
    }
    assert!(changes >= 8, "mutate the actual resolved interface versions");
    wasmparser::Validator::new_with_features(
        wasmparser::WasmFeatures::WASM1.union(wasmparser::WasmFeatures::COMPONENT_MODEL),
    )
    .validate_all(&bytes)
    .expect("version substitution preserves a well-typed executable component");
    assert_eq!(
        check(&component, &bytes)
            .expect_err("binary validity cannot replace world authority")
            .code(),
        "ZRYNA-W4030",
    );
}

#[test]
fn allocator_has_one_fixed_page_and_no_memory_growth_or_start() {
    let bytes = memory::encode();
    let mut memories = 0;
    for payload in Parser::new(0).parse_all(&bytes) {
        match payload.expect("allocator payload") {
            Payload::MemorySection(section) => {
                for memory in section {
                    let memory = memory.expect("allocator memory");
                    memories += 1;
                    assert_eq!((memory.initial, memory.maximum), (1, Some(1)));
                }
            }
            Payload::StartSection { .. } => panic!("allocator must not execute during startup"),
            Payload::CodeSectionEntry(body) => {
                let mut operators = body.get_operators_reader().expect("allocator operators");
                while !operators.eof() {
                    assert!(!matches!(
                        operators.read().expect("operator"),
                        wasmparser::Operator::MemoryGrow { .. }
                    ));
                }
            }
            _ => {}
        }
    }
    assert_eq!(memories, 1);
}

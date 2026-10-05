//! Actual source/material inputs, rejected before the genuine native emitter dispatch.

use super::*;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use object::{Object, ObjectSymbol};
use zryna_source::SourceFileInput;

const BUFFER: &str = include_str!("../../../../../../tests/native-c-abi-v0/source-buffer.zry");
const HANDLE: &str = include_str!("../../../../../../tests/native-c-abi-v0/source-handle.zry");
const SCALAR: &str = include_str!("../../../../../../tests/native-c-abi-v0/source-scalar.zry");
const DECLARATIONS: &[u8] =
    include_bytes!("../../../../../../tests/native-c-abi-v0/declarations.ffi.json");
const HEADER: &[u8] = include_bytes!("../../../../../../tests/native-c-abi-v0/candidate.h");
const POLICY: &[u8] =
    include_bytes!("../../../../../../tests/native-c-auth-v0/library-policy.json");

mod report;

fn sources() -> SourceMap {
    SourceMap::build(
        [("buffer", BUFFER), ("handle", HANDLE), ("scalar", SCALAR)]
            .into_iter()
            .map(|(name, text)| SourceFileInput {
                path: format!("tests/native-c-abi-v0/source-{name}.zry"),
                text: text.into(),
            })
            .collect(),
    )
    .expect("existing immutable fixture source")
}

fn emit(sources: &SourceMap, target: &str) -> Result<ValidatedHandleEntries, Vec<Diagnostic>> {
    emit_handle_entry(
        sources,
        DECLARATIONS,
        &[LibraryMaterial {
            library_id: "fixture-c-v0@0",
            header_bytes: HEADER,
            policy_bytes: POLICY,
        }],
        target,
        "imported",
    )
}

fn rejects_before_emitter(target: &str) {
    EMITTER_ENTRIES.with(|entries| entries.set(0));
    let errors = emit(&sources(), target).expect_err("actual foreign requirement rejects");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "ZRYNA-C4103");
    EMITTER_ENTRIES.with(|entries| assert_eq!(entries.get(), 0));
    eprintln!(
        "417-selection-evidence {}",
        serde_json::json!({"target":target,"pre_emitter_entries":0,"diagnostic":"ZRYNA-C4103"})
    );
}

#[test]
fn all_rejects_actual_foreign_requirement_before_emitter() {
    rejects_before_emitter("all");
}

#[test]
fn universal_rejects_actual_foreign_requirement_before_emitter() {
    rejects_before_emitter("universal");
}

#[test]
fn javascript_rejects_actual_foreign_requirement_before_emitter() {
    rejects_before_emitter("javascript");
}

#[test]
fn webassembly_rejects_actual_foreign_requirement_before_emitter() {
    rejects_before_emitter("webassembly");
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn native_only_source_reaches_real_audited_emitter_with_original_issuers() {
    let sources = sources();
    EMITTER_ENTRIES.with(|entries| entries.set(0));
    let object = emit(&sources, zryna_syntax::native_c_v0::TARGET)
        .expect("actual semantic/IR/MIR/native object admission");
    EMITTER_ENTRIES.with(|entries| assert_eq!(entries.get(), 1));
    assert!(object.program().source().belongs_to(&sources));
    assert_eq!(object.program().source().target(), zryna_syntax::native_c_v0::TARGET);
    assert_eq!(object.entries().len(), 1);
    let authority = object.program().source().private_authority().body_authority();
    assert_eq!(authority.declaration_authority().declaration_bytes(), DECLARATIONS);
    assert_eq!(authority.declaration_authority().header_bytes("fixture-c-v0@0"), Some(HEADER));
    assert_eq!(authority.declaration_authority().policy_bytes("fixture-c-v0@0"), Some(POLICY));
    let elf = object::File::parse(object.bytes()).expect("real audited ELF object");
    assert_eq!(elf.format(), object::BinaryFormat::Elf);
    assert_eq!(elf.architecture(), object::Architecture::X86_64);
    assert!(elf.symbols().any(|symbol| { symbol.is_undefined() && symbol.name() == Ok("add") }));
    assert_eq!(object.imported_operations().count(), 1);
    eprintln!(
        "417-selection-evidence {}",
        serde_json::json!({"target":zryna_syntax::native_c_v0::TARGET,"pre_emitter_entries":1,
            "original_source_issuer":true,"format":"ELF","architecture":"x86_64",
            "imported_symbol":"add","object_sha256":format!("{:x}",<sha2::Sha256 as sha2::Digest>::digest(object.bytes()))})
    );
}

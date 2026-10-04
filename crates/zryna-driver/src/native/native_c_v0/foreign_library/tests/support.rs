use super::*;

pub(super) const DEFINITIONS: [&str; 7] = [
    "add",
    "fixture_close",
    "fixture_copy_bytes",
    "fixture_open",
    "fixture_read",
    "fixture_release_bytes",
    "sum_bytes",
];

pub(super) fn linked_requirements(
    capture: &capture::Capture,
    name: &str,
) -> HandleLinkRequirements {
    let ir = zryna_native_c_ir::lower(&capture.sources, &capture.authority)
        .expect("original source-bound IR");
    let mir = zryna_native_mir::native_c_v0::lower(&ir).expect("independent machine seal");
    let symbol = mir
        .functions()
        .find(|function| function.name() == name)
        .expect("original function")
        .entry()
        .symbol
        .clone();
    let object = zryna_backend_native::native_c_v0::resources::emit_handle_entries(
        &mir,
        &[&symbol],
        zryna_backend_native::select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET)
            .expect("exact target"),
    )
    .expect("audited object");
    super::super::super::resource_identity::handle_link_requirements(&object)
        .expect("retained requirements")
}
pub(super) fn synthetic(edit: impl FnOnce(&mut Object<'_>)) -> Vec<u8> {
    let mut object = Object::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
    let text = object.section_id(object::write::StandardSection::Text);
    object.append_section_data(text, &[0xc3; 64], 1);
    object.add_section(Vec::new(), b".note.GNU-stack".to_vec(), SectionKind::Other);
    for (index, name) in DEFINITIONS.iter().enumerate() {
        object.add_symbol(Symbol {
            name: name.as_bytes().to_vec(),
            value: index as u64,
            size: 1,
            kind: SymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: SymbolSection::Section(text),
            flags: SymbolFlags::None,
        });
    }
    edit(&mut object);
    object.write().expect("independent ELF producer")
}
pub(super) fn undefined(object: &mut Object<'_>, name: &[u8]) -> object::write::SymbolId {
    object.add_symbol(Symbol {
        name: name.to_vec(),
        value: 0,
        size: 0,
        kind: SymbolKind::Unknown,
        scope: SymbolScope::Dynamic,
        weak: false,
        section: SymbolSection::Undefined,
        flags: SymbolFlags::None,
    })
}
pub(super) fn sha(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub(super) fn material_input<'a>(
    requirements: &'a HandleLinkRequirements,
    bytes: &'a [u8],
    digest: &'a [u8; 32],
    dependencies: &'a [&'a str],
) -> ForeignLibraryInput<'a> {
    let authority = requirements
        .object()
        .program()
        .source()
        .private_authority()
        .body_authority()
        .declaration_authority();
    ForeignLibraryInput {
        library_id: "fixture-c-v0@0",
        header_bytes: authority.header_bytes("fixture-c-v0@0").expect("original header"),
        policy_bytes: authority.policy_bytes("fixture-c-v0@0").expect("original policy"),
        object_bytes: bytes,
        object_size: bytes.len(),
        object_sha256: digest,
        dependency_symbols: dependencies,
    }
}
pub(super) fn accept(
    requirements: &HandleLinkRequirements,
    bytes: &[u8],
    dependencies: &[&str],
) -> CapturedForeignLibrary {
    let captured = capture_foreign_library(
        requirements,
        &material_input(requirements, bytes, &sha(bytes), dependencies),
    );
    match captured {
        Ok(captured) => captured,
        Err(error) => {
            use object::{Object as _, ObjectSection as _, ObjectSymbol as _};
            let file = object::File::parse(bytes).expect("independent fixture ELF");
            let sections = file
                .sections()
                .take(16)
                .map(|s| (s.name(), s.size(), s.flags()))
                .collect::<Vec<_>>();
            let symbols = file
                .symbols()
                .take(24)
                .map(|s| (s.name(), s.kind(), s.flags()))
                .collect::<Vec<_>>();
            panic!(
                "independent accepted ELF: {error:?}; sections={sections:?}; symbols={symbols:?}"
            );
        }
    }
}
pub(super) fn reject(requirements: &HandleLinkRequirements, bytes: &[u8], dependencies: &[&str]) {
    assert!(
        capture_foreign_library(
            requirements,
            &material_input(requirements, bytes, &sha(bytes), dependencies)
        )
        .is_err(),
        "hostile actual bytes must reject"
    );
}

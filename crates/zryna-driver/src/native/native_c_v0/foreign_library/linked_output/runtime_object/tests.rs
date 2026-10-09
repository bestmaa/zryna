use super::super::CapturedForeignLibrary;
use super::*;
mod fixture;
mod mutations;
use fixture::Fixture;
use mutations::*;

fn rejects(fixture: &Fixture, bytes: &[u8]) {
    assert_eq!(
        check(&fixture.requirements, bytes).expect_err("hostile runtime rejects").code(),
        "ZRYNA-C4104"
    );
    check(&fixture.requirements, fixture.runtime.bytes()).expect("original runtime remains valid");
    fixture.empty();
}

#[test]
fn runtime_object_genuine_original_abi_source_and_object_pass() {
    let fixture = Fixture::new();
    check(&fixture.requirements, fixture.runtime.bytes()).expect("genuine runtime object");
    fixture.empty();
}

#[test]
fn runtime_object_stack_protected_compiler_control_rejects() {
    use object::{Object as _, ObjectSymbol as _};
    let fixture = Fixture::new();
    let protected = super::super::producer::compile_stack_protected_object(
        &fixture.root,
        fixture.requirements.private_runtime_source().expect("original rendered source"),
        fixture.runtime.tools(),
    )
    .expect("independently instrumented original runtime");
    let file = object::File::parse(protected.bytes()).expect("actual compiler ELF");
    assert!(
        file.symbols()
            .any(|symbol| { symbol.is_undefined() && symbol.name() == Ok("__stack_chk_fail") })
    );
    rejects(&fixture, protected.bytes());
}

#[test]
fn runtime_object_missing_extra_duplicate_and_weak_definitions_reject() {
    let fixture = Fixture::new();
    let original = fixture.runtime.bytes();
    let definition = symbol(original, "zryna_rt_o1_allocate");
    rejects(&fixture, &remove_definition(original, "zryna_rt_o1_weak_upgrade"));
    rejects(&fixture, &append_symbol(original, definition, Some("extra_runtime_function")));
    rejects(&fixture, &append_symbol(original, definition, None));
    let mut weak = original.to_vec();
    weak[definition + 4] = 0x22;
    rejects(&fixture, &weak);
    for (field, width, value) in [(4, 1, 0x1a), (5, 1, 2), (6, 2, 0xfff2)] {
        let mut bytes = original.to_vec();
        put(&mut bytes, definition + field, width, value);
        rejects(&fixture, &bytes);
    }
}

#[test]
fn runtime_object_local_definitions_cannot_enter_reserved_abi_namespace() {
    let fixture = Fixture::new();
    let original = fixture.runtime.bytes();
    for kind in [object::SymbolKind::Text, object::SymbolKind::Data] {
        let local = local_definition(original, kind);
        rejects(&fixture, &rename_symbol(original, local, "zryna_rt_o1_unknown"));
    }
}

#[test]
fn runtime_object_undeclared_and_duplicate_imports_reject() {
    let fixture = Fixture::new();
    let original = fixture.runtime.bytes();
    let imported = symbol(original, "malloc");
    rejects(&fixture, &append_symbol(original, imported, Some("getenv")));
    rejects(&fixture, &append_symbol(original, imported, None));
    let mut weak = original.to_vec();
    weak[imported + 4] = 0x20;
    rejects(&fixture, &weak);
}

#[test]
fn runtime_object_constructor_tls_and_executable_data_sections_reject() {
    let fixture = Fixture::new();
    let original = fixture.runtime.bytes();
    rejects(&fixture, &rename_section(original, ".data", ".init_array"));
    rejects(&fixture, &rename_section(original, ".bss", ".tbss"));
    for (name, flags) in [(".bss", 0x403), (".data", 7)] {
        let mut bytes = original.to_vec();
        let header = section(original, name);
        put(&mut bytes, header + 8, 8, flags);
        rejects(&fixture, &bytes);
    }
}

#[test]
fn runtime_object_malformed_header_tables_and_strings_reject() {
    let fixture = Fixture::new();
    let original = fixture.runtime.bytes();
    let symbols = section(original, ".symtab");
    let relocation = section(original, ".rela.text");
    for (offset, width, value) in [
        (40, 8, u64::MAX),
        (32, 8, 64),
        (62, 2, 65535),
        (symbols + 56, 8, 23),
        (symbols + 24, 8, data(original, ".text").0 as u64),
        (symbol(original, "zryna_rt_o1_allocate"), 4, u64::from(u32::MAX)),
        (symbols + 40, 4, 65535),
        (symbols + 44, 4, 0),
        (relocation + 32, 8, 1),
        (relocation + 40, 4, 0),
        (relocation + 44, 4, 0),
    ] {
        let mut bytes = original.to_vec();
        put(&mut bytes, offset, width, value);
        rejects(&fixture, &bytes);
    }
    rejects(&fixture, &original[..original.len() - 1]);
    for length in 0..64 {
        rejects(&fixture, &original[..length]);
    }
    let mut bytes = original.to_vec();
    let (start, size) = data(original, ".strtab");
    bytes[start + size - 1] = b'x';
    rejects(&fixture, &bytes);
}

#[test]
fn runtime_object_invalid_relocation_targets_fields_types_and_overlaps_reject() {
    let fixture = Fixture::new();
    let original = fixture.runtime.bytes();
    let (relocation, size) = data(original, ".rela.text");
    assert!(size >= 48);
    let info = word(original, relocation + 8, 8);
    for (offset, value) in [
        (relocation, u64::MAX),
        (relocation + 8, 4),
        (relocation + 8, (u64::from(u32::MAX) << 32) | 4),
        (relocation + 8, (info & !0xffff_ffff) | 0xffff),
        (relocation + 16, i64::MIN.cast_unsigned()),
    ] {
        let mut bytes = original.to_vec();
        put(&mut bytes, offset, 8, value);
        rejects(&fixture, &bytes);
    }
    let table = tables::read(original).expect("original tables");
    let expected = fixture
        .requirements
        .object()
        .program()
        .source()
        .runtime_abi()
        .native_linux_x86_64_functions()
        .map(zryna_ownership_runtime_abi::VerifiedNativeFunction::symbol)
        .collect();
    let symbols = inventory::check(&table, &expected).expect("original symbols");
    let data_record = table
        .relocations
        .chunks_exact(24)
        .position(|row| {
            let ordinal = usize::try_from(word(row, 8, 8) >> 32).expect("original ordinal");
            matches!(symbols[ordinal].kind, 1 | 3)
        })
        .expect("original data relocation");
    let mut bytes = original.to_vec();
    put(&mut bytes, relocation + data_record * 24 + 16, 8, i64::MAX.cast_unsigned());
    rejects(&fixture, &bytes);
    let mut bytes = original.to_vec();
    bytes[relocation + 24..relocation + 48].copy_from_slice(&original[relocation..relocation + 24]);
    rejects(&fixture, &bytes);
}

#[test]
fn runtime_object_inventory_and_object_bytes_are_bounded() {
    let fixture = Fixture::new();
    let mut bytes = fixture.runtime.bytes().to_vec();
    put(&mut bytes, 60, 2, (MAX_SECTIONS + 2) as u64);
    assert_eq!(
        check(&fixture.requirements, &bytes).expect_err("section budget").code(),
        "ZRYNA-C4105"
    );
    assert_eq!(
        check(&fixture.requirements, &vec![0; zryna_backend_native::MAX_NATIVE_OBJECT_BYTES + 1])
            .expect_err("byte budget")
            .code(),
        "ZRYNA-C4105"
    );
    for (name, size) in
        [(".symtab", (MAX_SYMBOLS + 2) * 24), (".rela.text", (MAX_RELOCATIONS + 1) * 24)]
    {
        let bytes = resized_table(fixture.runtime.bytes(), name, size);
        assert_eq!(
            check(&fixture.requirements, &bytes).expect_err("record budget").code(),
            "ZRYNA-C4105"
        );
    }
    check(&fixture.requirements, fixture.runtime.bytes()).expect("pristine recovery");
}

#[test]
fn runtime_object_rejects_before_client_compile_or_link_and_preserves_denial_flags() {
    let fixture = Fixture::new();
    let (library, foreign) = fixture.foreign();
    let mut runtime = fixture.runtime.clone();
    let definition = symbol(&runtime.bytes, "zryna_rt_o1_allocate");
    runtime.bytes[definition + 4] = 0x22;
    let failure = super::super::observe(
        &fixture.root,
        &fixture.requirements,
        &library,
        &foreign,
        b"not C",
        Some(&runtime),
        false,
    )
    .expect_err("runtime audit precedes client compilation");
    assert_eq!(failure.diagnostics[0].code(), "ZRYNA-C4104");
    assert!(failure.invocations.is_empty());
    fixture.empty();
    runtime = fixture.runtime.clone();
    runtime.source[0] ^= 1;
    let failure = super::super::observe(
        &fixture.root,
        &fixture.requirements,
        &library,
        &foreign,
        b"not C",
        Some(&runtime),
        false,
    )
    .expect_err("source binding precedes structural audit");
    assert_eq!(failure.diagnostics[0].code(), "ZRYNA-C4102");
    assert!(failure.invocations.is_empty());
    fixture.empty();
    let observed = super::super::observe(
        &fixture.root,
        &fixture.requirements,
        &library,
        &foreign,
        b"int main(void){return 0;}",
        Some(&fixture.runtime),
        false,
    )
    .expect("original inputs compile and link without executing");
    let report = observed.report();
    for flag in [
        "execution_authorized",
        "target_executed",
        "driver_executable_mode_applied",
        "complete_input_use_attestation",
        "loader_or_provider_lookup_performed",
    ] {
        assert_eq!(report[flag], false);
    }
    assert_eq!(report["missing_prerequisites"], serde_json::json!(super::super::MISSING));
    fixture.empty();
}

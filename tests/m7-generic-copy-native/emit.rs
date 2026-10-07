//! Real source -> same sealed program -> JavaScript, core Wasm and native ELF conformance.
use zryna_ir::generic_v1::{copy_v1, wire};
use zryna_layout::StorageTarget;
use zryna_ownership_runtime_abi::generic_v1 as runtime;
use zryna_semantics::bounded_generics_v1::{
    SemanticInput,
    body_types::check_body_types,
    instantiation::{copy_v1::produce_claim, discover, layouts::verify_layouts},
    resolve_declarations,
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5::{decode_snapshot, verify_snapshot};

fn with_program(
    output: &std::path::Path,
    cross: bool,
    test: impl FnOnce(&copy_v1::VerifiedCopyProgram<'_>),
) {
    let mut files = vec![SourceFileInput {
        path: "main.zry".into(),
        text: if cross {
            std::fs::read_to_string("tests/m7-generic-copy-fixtures/cross-main.zry")
                .expect("source fixture")
        } else {
            std::fs::read_to_string("tests/m7-generic-copy-fixtures/main.zry")
                .expect("source fixture")
        }
        .into(),
    }];
    if cross {
        files.push(SourceFileInput {
            path: "values.zry".into(),
            text: std::fs::read_to_string("tests/m7-generic-copy-fixtures/cross-values.zry")
                .expect("source fixture")
                .into(),
        });
    }
    let snapshot: Vec<u8> = if cross {
        std::fs::read("tests/m7-generic-copy-fixtures/cross-reference.json").expect("frozen DTO")
    } else {
        std::fs::read("tests/m7-generic-copy-fixtures/reference.json").expect("frozen DTO")
    };
    let sources = SourceMap::build(files).expect("genuine source map");
    let syntax = verify_snapshot(decode_snapshot(&snapshot).expect("frozen DTO"), &sources)
        .expect("exact full source authentication");
    let entry = sources.verify_file_id(0).expect("entry");
    let declarations = resolve_declarations(
        SemanticInput::try_new(&syntax, &sources, entry).expect("exact semantic input"),
    )
    .expect("declarations");
    let bodies = check_body_types(&declarations).expect("opaque symbolic original bodies");
    let instances = discover(&bodies).expect("bounded complete demand");
    assert_eq!(instances.function_keys().len(), 5);
    let linear = verify_layouts(&instances, StorageTarget::Linear32V1).expect("linear layouts");
    let linux = verify_layouts(&instances, StorageTarget::LinuxX8664V1).expect("linux layouts");
    let runtime = runtime::verify_v1(
        runtime::raw_v1(&linear, &linux).expect("raw contract"),
        &linear,
        &linux,
    )
    .expect("separate runtime issuer");
    let raw = produce_claim(&instances, &linear, &linux).expect("untrusted semantic claim");
    let encoded = wire::encode(&raw).expect("bounded wire");
    let program = copy_v1::verify(
        wire::decode(&encoded).expect("wire decode"),
        &syntax,
        &sources,
        entry,
        &linear,
        &linux,
        &runtime,
    )
    .expect("independent source/body/demand/Copy seal");
    assert_eq!(program.ownership_effects(), (0, 0));
    std::fs::write(output.join(format!("{cross}.zir")), &encoded).expect("wire proof");
    test(&program);
}

fn hostile_inventory(
    bytes: &[u8],
    mir: &zryna_native_mir::generic_copy_v1::VerifiedProgram<'_, '_>,
) {
    use zryna_backend_native::generic_copy_v1::validate_object_inventory;
    validate_object_inventory(bytes, mir).expect("pristine exact inventory");
    let mut name = bytes.to_vec();
    let offset = name
        .windows(b"zryna_v1_e_score".len())
        .position(|b| b == b"zryna_v1_e_score")
        .expect("scalar symbol");
    name[offset] = b'x';
    let mut executable = bytes.to_vec();
    executable[16] = 2; // Valid ELF type becomes ET_EXEC rather than relocatable.
    let mut architecture = bytes.to_vec();
    architecture[18] = 3; // EM_386 rather than x86_64.
    let mut truncated = bytes.to_vec();
    truncated.truncate(40);
    let section_table =
        u64::from_le_bytes(bytes[40..48].try_into().expect("ELF section offset")) as usize;
    let section_size =
        u16::from_le_bytes(bytes[58..60].try_into().expect("ELF section size")) as usize;
    let section_count =
        u16::from_le_bytes(bytes[60..62].try_into().expect("ELF section count")) as usize;
    let relocation_header = (0..section_count)
        .map(|i| section_table + i * section_size)
        .find(|h| u32::from_le_bytes(bytes[h + 4..h + 8].try_into().expect("section type")) == 4)
        .expect("RELA");
    let relocation = u64::from_le_bytes(
        bytes[relocation_header + 24..relocation_header + 32].try_into().expect("RELA offset"),
    ) as usize;
    let mut addend = bytes.to_vec();
    addend[relocation + 16..relocation + 24].copy_from_slice(&(-3i64).to_le_bytes());
    let mut unknown_target = bytes.to_vec();
    unknown_target[relocation + 12..relocation + 16].copy_from_slice(&0u32.to_le_bytes());
    let mut wrong_relocation = bytes.to_vec();
    wrong_relocation[relocation + 8..relocation + 12].copy_from_slice(&2u32.to_le_bytes()); // PC32, not PLT32.
    let mut executable_stack = bytes.to_vec();
    let note = (0..section_count)
        .map(|i| section_table + i * section_size)
        .find(|h| {
            u32::from_le_bytes(bytes[h + 4..h + 8].try_into().expect("section type")) == 1
                && u64::from_le_bytes(bytes[h + 32..h + 40].try_into().expect("section extent"))
                    == 0
        })
        .expect("empty GNU stack section");
    executable_stack[note + 8..note + 16].copy_from_slice(&4u64.to_le_bytes());
    for attack in [
        name,
        executable,
        architecture,
        truncated,
        addend,
        unknown_target,
        wrong_relocation,
        executable_stack,
    ] {
        assert_eq!(
            validate_object_inventory(&attack, mir).expect_err("independent ELF mutation").code,
            "ZRYNA-N7103"
        );
    }
    assert_eq!(
        validate_object_inventory(&vec![0; 8 * 1024 * 1024 + 1], mir)
            .expect_err("first extra final object byte")
            .code,
        "ZRYNA-N7103"
    );
    validate_object_inventory(bytes, mir).expect("pristine replay after every rejection");
}

fn main() {
    let output =
        std::path::PathBuf::from(std::env::args_os().nth(1).expect("explicit evidence directory"));
    std::fs::create_dir_all(&output).expect("proof directory");
    for cross in [false, true] {
        with_program(&output, cross, |program| {
            let mir = zryna_native_mir::generic_copy_v1::lower(program)
                .expect("native MIR from exact same Copy object");
            let target = zryna_backend_native::select_object_target(
                zryna_backend_native::NATIVE_OBJECT_TARGET,
            )
            .expect("exact target");
            let native = zryna_backend_native::generic_copy_v1::emit_object(&mir, target)
                .expect("closed generic native object");
            hostile_inventory(native.bytes(), &mir);
            assert_eq!(
                native,
                zryna_backend_native::generic_copy_v1::emit_object(&mir, target)
                    .expect("deterministic replay")
            );
            std::fs::write(output.join(format!("{cross}.o")), native.bytes())
                .expect("native object");
            for (request, expected_code) in [
                (
                    zryna_abi::Invocation::new("flag".into(), vec![zryna_abi::ScalarValue::I32(1)]),
                    "ZRYNA-B2103",
                ),
                (
                    zryna_abi::Invocation::new(
                        "score".into(),
                        vec![zryna_abi::ScalarValue::Bool(true)],
                    ),
                    "ZRYNA-B2103",
                ),
                (zryna_abi::Invocation::new("score".into(), vec![]), "ZRYNA-B2102"),
                (
                    zryna_abi::Invocation::new(
                        "score".into(),
                        vec![zryna_abi::ScalarValue::I32(0), zryna_abi::ScalarValue::I32(1)],
                    ),
                    "ZRYNA-B2102",
                ),
                (zryna_abi::Invocation::new("unknown".into(), vec![]), "ZRYNA-B2101"),
            ] {
                assert_eq!(
                    program
                        .scalar_abi()
                        .prepare_invocation(request)
                        .expect_err("typed input rejection")
                        .code(),
                    expected_code
                );
            }
            let js = zryna_backend_javascript::generic_copy_v1::emit(program).expect("sealed JS");
            let wasm =
                zryna_backend_webassembly::generic_copy_v1::emit(program).expect("sealed Wasm");
            std::fs::write(output.join(format!("{cross}.mjs")), js.source).expect("JS bytes");
            std::fs::write(output.join(format!("{cross}.wasm")), wasm.bytes()).expect("Wasm bytes");
            println!(
                "same sealed program -> JS, Wasm, native: {cross}; functions={}, exports={}, ownership={:?}",
                program.functions().len(),
                program.scalar_abi().exports().len(),
                program.ownership_effects()
            );
        });
    }
}

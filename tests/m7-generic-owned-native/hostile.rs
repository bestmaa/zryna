pub(super) fn hostile_inventory(
    bytes: &[u8],
    mir: &zryna_native_mir::generic_owned_v2::VerifiedProgram<'_, '_>,
    scalar_symbol: &[u8],
) {
    use zryna_backend_native::generic_owned_v2::validate_object_inventory;
    validate_object_inventory(bytes, mir).expect("pristine exact inventory");
    let mut name = bytes.to_vec();
    let offset = name
        .windows(scalar_symbol.len())
        .position(|b| b == scalar_symbol)
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

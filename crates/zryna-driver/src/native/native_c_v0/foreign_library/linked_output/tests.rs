use super::*;

#[test]
fn linker_trace_preserves_order_repetition_and_archive_members() {
    let text = b"startup.o\nlib.a\n(lib.a)member.o\nlib.a\nfinish.o\n";
    assert_eq!(
        process::trace(text).expect("independent observed fixture prerequisite"),
        ["startup.o", "lib.a", "(lib.a)member.o", "lib.a", "finish.o"]
    );
    for text in [b"".as_slice(), b"unterminated", b"a\n\n", b"a\r\n", b"a\0\n", b"\xff\n"] {
        assert!(process::trace(text).is_err());
    }
    assert!(process::trace(&vec![b'x'; native::MAX_NATIVE_TOOL_OUTPUT_BYTES + 1]).is_err());
}

#[test]
fn elf_observation_rejects_truncation_wrong_target_and_oversized_tables() {
    for bytes in
        [vec![], vec![0; 64], b"not ELF".to_vec(), vec![0; native::MAX_NATIVE_EXECUTABLE_BYTES + 1]]
    {
        assert!(elf::read(&bytes).is_err());
    }
}

impl Observation {
    pub(crate) fn check_elf_rejection_controls(&self) {
        assert!(elf::read(&self.final_elf).is_ok());
        let word = |offset: usize, size: usize| {
            usize::try_from(elf_word(&self.final_elf, offset, size))
                .expect("independent observed fixture prerequisite")
        };
        let ph = word(32, 8);
        let rows = (0..word(56, 2)).map(|i| ph + i * 56).collect::<Vec<_>>();
        let interpreter = *rows
            .iter()
            .find(|row| word(**row, 4) == 3)
            .expect("independent observed fixture prerequisite");
        let dynamic = *rows
            .iter()
            .find(|row| word(**row, 4) == 2)
            .expect("independent observed fixture prerequisite");
        let dynamic_start = word(dynamic + 8, 8);
        let dynamic_size = word(dynamic + 32, 8);
        let needed = (dynamic_start..dynamic_start + dynamic_size)
            .step_by(16)
            .find(|row| word(*row, 8) == 1)
            .expect("independent observed fixture prerequisite");
        let strings = (dynamic_start..dynamic_start + dynamic_size)
            .step_by(16)
            .find(|row| word(*row, 8) == 5)
            .expect("independent observed fixture prerequisite");
        for (offset, data) in [
            (18, 0_u64.to_le_bytes().to_vec()),
            (32, u64::MAX.to_le_bytes().to_vec()),
            (interpreter + 32, u64::MAX.to_le_bytes().to_vec()),
            (needed + 8, u64::MAX.to_le_bytes().to_vec()),
            (strings + 8, u64::MAX.to_le_bytes().to_vec()),
            (dynamic + 32, 17_u64.to_le_bytes().to_vec()),
        ] {
            let mut bytes = self.final_elf.clone();
            bytes[offset..offset + data.len()].copy_from_slice(&data);
            assert!(elf::read(&bytes).is_err(), "mutation at {offset}");
        }
        let mut bytes = self.final_elf.clone();
        let last = word(interpreter + 8, 8) + word(interpreter + 32, 8) - 1;
        bytes[last] = b'x';
        assert!(elf::read(&bytes).is_err(), "unterminated interpreter");
        let mut bytes = self.final_elf.clone();
        for row in (dynamic_start..dynamic_start + dynamic_size).step_by(16) {
            if word(row, 8) == 0 {
                bytes[row..row + 8].copy_from_slice(&1_u64.to_le_bytes());
            }
        }
        assert!(elf::read(&bytes).is_err(), "unterminated dynamic array");
    }
}

fn elf_word(bytes: &[u8], offset: usize, size: usize) -> u64 {
    bytes[offset..offset + size]
        .iter()
        .enumerate()
        .fold(0, |value, (i, byte)| value | (u64::from(*byte) << (i * 8)))
}

impl CompiledObject {
    pub(crate) fn check_failure_retention_controls(&self, root: &ArtifactOutputRoot) {
        let stages = staging::stages(root, 1).expect("independent observed fixture prerequisite");
        let residue = stages[0].directory.join("owned-cleanup-control");
        std::fs::write(&residue, b"owned test residue")
            .expect("independent observed fixture prerequisite");
        let failure = staging::finish(&stages, Ok(()), std::slice::from_ref(&self.invocation))
            .expect_err("expected observational rejection");
        assert_eq!(failure.diagnostics[0].code(), "ZRYNA-N4016");
        assert_eq!(failure.invocations.len(), 1);
        assert_eq!(failure.report()["invocations"][0]["success"], true);
        std::fs::remove_file(residue).expect("independent observed fixture prerequisite");
        assert!(stages[0].cleanup().is_empty());
        let stages = staging::stages(root, 1).expect("independent observed fixture prerequisite");
        std::fs::remove_dir(&stages[0].directory)
            .expect("independent observed fixture prerequisite");
        let failure = process::validate(&stages[0], self.invocation.clone())
            .expect_err("expected observational rejection");
        assert_eq!(failure.invocations.len(), 1);
        assert_eq!(failure.report()["invocations"][0]["success"], true);
    }
}

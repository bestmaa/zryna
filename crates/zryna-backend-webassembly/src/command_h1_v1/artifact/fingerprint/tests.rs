use super::{Inputs, program, program_bytes};
use std::fmt::Write as _;

fn inputs(key: Option<&str>) -> Inputs<'_> {
    Inputs {
        source: [1; 32],
        language: [2; 32],
        storage: [3; 32],
        linear: [4; 32],
        linux: [5; 32],
        world: [6; 32],
        key,
    }
}

#[test]
fn program_binding_matches_independent_sha256_reference_vectors() {
    // Independently encoded with a separate implementation of SHA-256 and
    // explicit little-endian field framing; no compiler output supplies these vectors.
    for (key, length, expected) in [
        (None, 271, "1e3efdabce7cf4d3e6931f43653c6e8dd1f75f471dde1090c631c11b3905c756"),
        (Some(""), 275, "07612e9b791d531524f7a752f79b3cd77f321202164d4f64dc125943e7be3f76"),
        (Some("MODE"), 279, "3b1f226e8778d28c6522773f5ed6ef2fad5935833165723bcc0d93c543982424"),
        (Some("é🙂"), 281, "e0f77089cd98b366a0777e258f6494305bf5843f4243a123cbc7a02be2e62e1a"),
    ] {
        let input = inputs(key);
        let bytes = program_bytes(&input).expect("closed encoding");
        assert_eq!(bytes.len(), length);
        let actual = program(&input).expect("closed binding");
        let actual = actual.iter().fold(String::with_capacity(64), |mut text, byte| {
            write!(&mut text, "{byte:02x}").expect("bounded digest formatting");
            text
        });
        assert_eq!(actual, expected);
    }
}

#[test]
fn every_digest_and_exact_key_byte_changes_the_program_binding() {
    let original = program(&inputs(Some("MODE"))).expect("original");
    for field in 0..6 {
        let mut input = inputs(Some("MODE"));
        match field {
            0 => input.source[0] ^= 1,
            1 => input.language[0] ^= 1,
            2 => input.storage[0] ^= 1,
            3 => input.linear[0] ^= 1,
            4 => input.linux[0] ^= 1,
            _ => input.world[0] ^= 1,
        }
        assert_ne!(program(&input).expect("mutated binding"), original);
    }
    for key in [None, Some(""), Some("mode"), Some("MODE\0"), Some("MODÉ"), Some("MODE🙂")] {
        assert_ne!(program(&inputs(key)).expect("different source key"), original);
    }
}

#[test]
fn profile_partition_and_optional_key_have_explicit_unambiguous_frames() {
    let none = program_bytes(&inputs(None)).expect("none encoding");
    assert_eq!(&none[..33], b"zryna.command-program-binding.v1\0");
    assert_eq!(&none[33..37], &13_u32.to_le_bytes());
    assert_eq!(&none[37..50], b"command-h1-v1");
    let partition = &none[242..270];
    let expected = [256_u32, 0, 65_536, 65_536, 15_728_640, 15_728_640, 16_777_216]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(partition, expected);
    assert_eq!(&none[270..], &[0]);
    let some = program_bytes(&inputs(Some("MODE"))).expect("some encoding");
    assert_eq!(&some[270..], &[1, 4, 0, 0, 0, b'M', b'O', b'D', b'E']);
    assert_ne!(program(&inputs(None)).expect("none"), program(&inputs(Some(""))).expect("empty"));
}

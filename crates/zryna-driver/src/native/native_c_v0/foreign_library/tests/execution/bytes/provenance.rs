//! Reference source provenance is explicit; no production or real-library approval is created.

use super::*;

#[test]
fn separate_byte_reference_material_matches_the_preserved_review_checkpoint() {
    let document: serde_json::Value =
        serde_json::from_str(include_str!("reference-provenance.json"))
            .expect("independent byte fixture prerequisite");
    assert_eq!(document["reference_checkpoint"], "42f8e8000d0a2afd3675fcefddadd526adc817e3");
    assert_eq!(document["library_id"], "fixture-c-v0@0");
    assert_eq!(document["production_approval"], "not-granted");
    for (key, bytes) in [
        ("library_body", include_bytes!("../../library.c").as_slice()),
        ("header", capture::HEADER),
        (
            "policy",
            include_bytes!("../../../../../../../../../tests/native-c-auth-v0/library-policy.json")
                .as_slice(),
        ),
        (
            "private_runtime_template",
            include_bytes!("../../../../../../../../../runtime/native/ownership_runtime_v1.c")
                .as_slice(),
        ),
    ] {
        assert_eq!(document["materials"][key]["bytes"], bytes.len());
        assert_eq!(document["materials"][key]["sha256"], format!("{:x}", Sha256::digest(bytes)));
    }
    let captured = capture::reference();
    let requirements = requirements(&captured, "copied");
    let library = &requirements.libraries()[0];
    assert_eq!(
        format!("{:x}", Sha256::digest(capture::HEADER)),
        document["materials"]["header"]["sha256"]
    );
    assert_eq!(hex(library.header_sha256()), document["materials"]["header"]["sha256"]);
    assert_eq!(hex(library.policy_sha256()), document["materials"]["policy"]["sha256"]);
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().fold(String::with_capacity(64), |mut text, byte| {
        write!(text, "{byte:02x}").expect("hex digest write");
        text
    })
}

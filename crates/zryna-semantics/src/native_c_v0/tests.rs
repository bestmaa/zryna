use super::{LibraryMaterial, verify};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::{native_c_source_v0::authenticate_sources, native_c_v0::raw::AbiType};

const DECLARATIONS: &[u8] =
    include_bytes!("../../../../tests/native-c-abi-v0/declarations.ffi.json");
const HEADER: &[u8] = include_bytes!("../../../../tests/native-c-abi-v0/candidate.h");
const POLICY: &[u8] = include_bytes!("../../../../tests/native-c-auth-v0/library-policy.json");

fn source_inputs() -> Vec<SourceFileInput> {
    [
        ("buffer", include_str!("../../../../tests/native-c-abi-v0/source-buffer.zry")),
        ("handle", include_str!("../../../../tests/native-c-abi-v0/source-handle.zry")),
        ("scalar", include_str!("../../../../tests/native-c-abi-v0/source-scalar.zry")),
    ]
    .into_iter()
    .map(|(name, text)| SourceFileInput {
        path: format!("tests/native-c-abi-v0/source-{name}.zry"),
        text: text.into(),
    })
    .collect()
}
fn sources() -> SourceMap {
    SourceMap::build(source_inputs()).expect("independent captured sources")
}
fn document() -> Value {
    serde_json::from_slice(DECLARATIONS).expect("independent declaration fixture")
}
fn wire(mut value: Value) -> Vec<u8> {
    value.sort_all_objects();
    let mut bytes = serde_json::to_vec(&value).expect("independent test serialization");
    bytes.push(b'\n');
    bytes
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn material(policy: &[u8]) -> [LibraryMaterial<'_>; 1] {
    [LibraryMaterial { library_id: "fixture-c-v0@0", header_bytes: HEADER, policy_bytes: policy }]
}

// Independent hostile test capture builder, not the production authority's policy projector.
fn capture_changed_policy(document: &mut Value) -> Vec<u8> {
    let library = &document["libraries"][0];
    let id = library["id"].as_str().expect("library id");
    let operations: Vec<_> = document["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .filter(|operation| operation["library"].as_str() == Some(id))
        .map(|operation| {
            let mut policy = operation.clone();
            policy.as_object_mut().expect("operation object").remove("sourceBinding");
            policy
        })
        .collect();
    let bytes = wire(
        serde_json::json!({ "allocators": library["allocators"], "kinds": library["kinds"], "operations": operations }),
    );
    document["libraries"][0]["policySha256"] = sha256(&bytes).into();
    bytes
}

#[test]
fn native_c_v0_declaration_authority_authenticates_fixed_independent_captures() {
    let sources = sources();
    let syntax = authenticate_sources(&sources).expect("independent complete source grammar");
    assert_eq!(sha256(POLICY), "918edc899c9fff69bdc4b79d0cc8332af01f0e4b6b15f4778bf643809ce0dba8");
    let verified =
        verify(DECLARATIONS, &sources, &syntax, &material(POLICY), "x86_64-unknown-linux-gnu")
            .expect("declaration authority only");
    assert_eq!(verified.operation_count(), 8);
    assert_eq!(verified.declaration_bytes(), DECLARATIONS);
    assert_eq!(verified.header_bytes("fixture-c-v0@0"), Some(HEADER));
    assert_eq!(verified.policy_bytes("fixture-c-v0@0"), Some(POLICY));
    let digest = verified.declaration_sha256().iter().fold(String::new(), |mut digest, byte| {
        write!(digest, "{byte:02x}").expect("string formatting");
        digest
    });
    assert_eq!(digest, "7eb6d630326d1dac6542a3579e9e1db42768f923358ffa4ed33d6dbcd556aa1e");
    assert!(verified.belongs_to(&sources));
    assert!(!verified.belongs_to(&self::sources()));
    let add = verified.operation("fixture-c-v0@0/add").expect("verified import");
    assert_eq!(add.result_carrier(), AbiType::CI32);
    assert_eq!(add.parameter_carriers().collect::<Vec<_>>(), [AbiType::CI32, AbiType::CI32]);
    assert!(sources.resolve(add.source_span()).is_ok());
    assert!(verified.operation("missing@0/add").is_none());
}

#[test]
fn native_c_v0_declaration_authority_rejects_all_independent_malformed_records() {
    let sources = sources();
    let syntax = authenticate_sources(&sources).expect("independent grammar");
    let rows: Vec<Value> = serde_json::from_slice(include_bytes!(
        "../../../../tests/native-c-abi-v0/malformed-declarations.json"
    ))
    .expect("independent hostile rows");
    assert_eq!(rows.len(), 38);
    for row in rows {
        let mut document = document();
        let target = row["target"].as_str().expect("target");
        let mut selected = match target {
            "root" => &mut document,
            "library" => &mut document["libraries"][0],
            "raw-site" => document["sites"]
                .as_array_mut()
                .expect("sites")
                .iter_mut()
                .find(|site| site["primitive"] == "rawCall")
                .expect("raw site"),
            _ => document["operations"]
                .as_array_mut()
                .expect("operations")
                .iter_mut()
                .find(|operation| operation["symbol"] == row["symbol"])
                .expect("operation"),
        };
        let fields = row["field"].as_array().expect("field path");
        for field in &fields[..fields.len() - 1] {
            selected = if let Some(key) = field.as_str() {
                &mut selected[key]
            } else {
                &mut selected
                    [usize::try_from(field.as_u64().expect("array index")).expect("bounded index")]
            };
        }
        let last = fields.last().expect("field key");
        if row["delete"] == true {
            selected
                .as_object_mut()
                .expect("delete object")
                .remove(last.as_str().expect("delete key"));
        } else if let Some(key) = last.as_str() {
            selected[key] = row["value"].clone();
        } else {
            selected
                [usize::try_from(last.as_u64().expect("array index")).expect("bounded index")] =
                row["value"].clone();
        }
        let policy = if row["reseal"] == true {
            capture_changed_policy(&mut document)
        } else {
            POLICY.to_vec()
        };
        let expected = if row["id"] == "wrong-target" {
            "ZRYNA-C4103"
        } else {
            row["code"].as_str().expect("fixed code")
        };
        let failure = verify(
            &wire(document),
            &sources,
            &syntax,
            &material(&policy),
            "x86_64-unknown-linux-gnu",
        )
        .expect_err("hostile declaration must fail");
        assert_eq!(failure.code(), expected, "{}: {}", row["id"], failure);
    }
    assert!(
        verify(DECLARATIONS, &sources, &syntax, &material(POLICY), "x86_64-unknown-linux-gnu")
            .is_ok()
    );
}

#[test]
fn native_c_v0_declaration_authority_requires_original_map_and_exact_material_inventory() {
    let sources = sources();
    let syntax = authenticate_sources(&sources).expect("independent grammar");
    let failure = verify(
        DECLARATIONS,
        &self::sources(),
        &syntax,
        &material(POLICY),
        "x86_64-unknown-linux-gnu",
    )
    .expect_err("rebuilt source map");
    assert_eq!(failure.detail(), "source-map-identity");
    for materials in [
        Vec::new(),
        vec![material(POLICY)[0], material(POLICY)[0]],
        vec![LibraryMaterial {
            library_id: "unknown@0",
            header_bytes: HEADER,
            policy_bytes: POLICY,
        }],
    ] {
        assert_eq!(
            verify(DECLARATIONS, &sources, &syntax, &materials, "x86_64-unknown-linux-gnu")
                .expect_err("wrong material set")
                .code(),
            "ZRYNA-C4102"
        );
    }
    for target in [
        "all",
        "universal",
        "javascript",
        "webassembly",
        "component",
        "x86_64-pc-windows-msvc",
        "aarch64-unknown-linux-gnu",
    ] {
        assert_eq!(
            verify(DECLARATIONS, &sources, &syntax, &material(POLICY), target)
                .expect_err("independent target selection")
                .code(),
            "ZRYNA-C4103"
        );
    }
}

#[test]
fn native_c_v0_declaration_authority_compares_actual_material_bytes_not_digest_assertions() {
    let sources = sources();
    let syntax = authenticate_sources(&sources).expect("independent grammar");
    let mut changed_header = HEADER.to_vec();
    changed_header[0] ^= 1;
    let materials = [LibraryMaterial {
        library_id: "fixture-c-v0@0",
        header_bytes: &changed_header,
        policy_bytes: POLICY,
    }];
    assert_eq!(
        verify(DECLARATIONS, &sources, &syntax, &materials, "x86_64-unknown-linux-gnu")
            .expect_err("changed captured header")
            .detail(),
        "header-digest"
    );
    let policy = [POLICY, b"\n"].concat();
    assert_eq!(
        verify(DECLARATIONS, &sources, &syntax, &material(&policy), "x86_64-unknown-linux-gnu")
            .expect_err("noncanonical captured policy")
            .detail(),
        "policy-material-or-digest"
    );
    let mut changed = document();
    changed["libraries"][0]["policySha256"] = sha256(&policy).into();
    assert_eq!(
        verify(&wire(changed), &sources, &syntax, &material(&policy), "x86_64-unknown-linux-gnu")
            .expect_err("matching policy digest cannot replace exact projection")
            .detail(),
        "policy-material-or-digest"
    );
    let header = vec![0; super::MAX_MATERIAL_BYTES];
    let mut changed = document();
    changed["libraries"][0]["headerSha256"] = sha256(&header).into();
    let materials = [LibraryMaterial {
        library_id: "fixture-c-v0@0",
        header_bytes: &header,
        policy_bytes: POLICY,
    }];
    assert!(
        verify(&wire(changed), &sources, &syntax, &materials, "x86_64-unknown-linux-gnu").is_ok(),
        "exact header byte cap authenticates bytes, not C behavior"
    );
    let header = vec![0; super::MAX_MATERIAL_BYTES + 1];
    let materials = [LibraryMaterial {
        library_id: "fixture-c-v0@0",
        header_bytes: &header,
        policy_bytes: POLICY,
    }];
    assert_eq!(
        verify(DECLARATIONS, &sources, &syntax, &materials, "x86_64-unknown-linux-gnu")
            .expect_err("first extra header byte")
            .code(),
        "ZRYNA-C4107"
    );
}

#[test]
fn native_c_v0_declaration_authority_replays_complete_parsed_sites_and_exact_bindings() {
    let sources = sources();
    let syntax = authenticate_sources(&sources).expect("independent grammar");
    let mut missing = document();
    missing["sites"].as_array_mut().expect("sites").remove(0);
    assert_eq!(
        verify(&wire(missing), &sources, &syntax, &material(POLICY), "x86_64-unknown-linux-gnu")
            .expect_err("missing site")
            .detail(),
        "complete-primitive-site-set"
    );
    let mut shifted = document();
    shifted["sites"][0]["start"] =
        (shifted["sites"][0]["start"].as_u64().expect("start") + 1).into();
    assert_eq!(
        verify(&wire(shifted), &sources, &syntax, &material(POLICY), "x86_64-unknown-linux-gnu")
            .expect_err("shifted exact span")
            .detail(),
        "parsed-primitive-site"
    );
    let mut inputs = source_inputs();
    inputs
        .iter_mut()
        .find(|input| input.path.ends_with("source-scalar.zry"))
        .expect("scalar capture")
        .text
        .push_str(
            "\nfunction unreported(): i32 { return Ffi.rawCall(\"fixture-c-v0@0/add\", 1, 2); }\n",
        );
    let changed_sources = SourceMap::build(inputs).expect("independent changed capture");
    let changed_syntax =
        authenticate_sources(&changed_sources).expect("complete extra call is parsed");
    let mut changed = document();
    let path = "tests/native-c-abi-v0/source-scalar.zry";
    let file =
        changed_syntax.files().iter().find(|file| file.path() == path).expect("changed file");
    let digest =
        sha256(changed_syntax.source_text(file.file_id()).expect("changed text").as_bytes());
    for source in changed["sources"].as_array_mut().expect("sources") {
        if source["path"] == path {
            source["sha256"] = digest.clone().into();
        }
    }
    for operation in changed["operations"].as_array_mut().expect("operations") {
        if operation["sourceBinding"]["path"] == path {
            operation["sourceBinding"]["sha256"] = digest.clone().into();
        }
    }
    for site in changed["sites"].as_array_mut().expect("sites") {
        if site["path"] == path {
            site["sourceSha256"] = digest.clone().into();
        }
    }
    assert_eq!(
        verify(
            &wire(changed),
            &changed_sources,
            &changed_syntax,
            &material(POLICY),
            "x86_64-unknown-linux-gnu"
        )
        .expect_err("fresh source hashes cannot conceal an omitted parsed call")
        .detail(),
        "complete-primitive-site-set"
    );
}

#[test]
fn native_c_v0_declaration_authority_keeps_c_int_distinct_and_malformed_release_promises_explicit()
{
    let sources = sources();
    let syntax = authenticate_sources(&sources).expect("independent grammar");
    let mut changed = document();
    changed["operations"][0]["parameters"][0]["abi"] = "c-int".into();
    let policy = capture_changed_policy(&mut changed);
    let verified =
        verify(&wire(changed), &sources, &syntax, &material(&policy), "x86_64-unknown-linux-gnu")
            .expect("explicit declared C-int bridge");
    assert_eq!(
        verified
            .operation("fixture-c-v0@0/add")
            .expect("add")
            .parameter_carriers()
            .collect::<Vec<_>>(),
        [AbiType::CInt, AbiType::CI32]
    );
    let mut changed = document();
    changed["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["symbol"] == "fixture_open")
        .expect("open")["resources"][0]["releasableOnMalformed"] = false.into();
    let policy = capture_changed_policy(&mut changed);
    assert!(
        verify(&wire(changed), &sources, &syntax, &material(&policy), "x86_64-unknown-linux-gnu")
            .is_ok(),
        "a false release promise must remain false for later runtime failure replay"
    );
}

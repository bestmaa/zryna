//! Genuine fixture capture shared by the independent IR tests.
#![allow(missing_docs)]

use serde_json::Value;
use zryna_semantics::native_c_v0::{
    LibraryMaterial,
    body::{VerifiedPrivateBoundaries, compose_private_boundaries, verify_bodies},
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::native_c_source_v0::{AuthenticatedForeignSources, authenticate_sources};

pub const BUFFER: &str = include_str!("../../../tests/native-c-abi-v0/source-buffer.zry");
pub const HANDLE: &str = include_str!("../../../tests/native-c-abi-v0/source-handle.zry");
pub const SCALAR: &str = include_str!("../../../tests/native-c-abi-v0/source-scalar.zry");
pub const HEADER: &[u8] = include_bytes!("../../../tests/native-c-abi-v0/candidate.h");
const POLICY: &[u8] = include_bytes!("../../../tests/native-c-auth-v0/library-policy.json");
const DECLARATIONS: &[u8] = include_bytes!("../../../tests/native-c-abi-v0/declarations.ffi.json");

#[derive(Debug)]
pub struct Capture {
    pub sources: SourceMap,
    pub authority: VerifiedPrivateBoundaries,
}

/// # Panics
/// Panics if independently recaptured fixture source or declaration material is invalid.
#[must_use]
pub fn reference() -> Capture {
    let sources = map(BUFFER, HANDLE, SCALAR, false);
    let syntax = authenticate_sources(&sources).expect("actual original syntax");
    let declarations = zryna_semantics::native_c_v0::verify(
        DECLARATIONS,
        &sources,
        &syntax,
        &[LibraryMaterial {
            library_id: "fixture-c-v0@0",
            header_bytes: HEADER,
            policy_bytes: POLICY,
        }],
        zryna_syntax::native_c_v0::TARGET,
    )
    .expect("actual retained declaration/material authority");
    let bodies = verify_bodies(&sources, &declarations).expect("actual bodies");
    let authority = compose_private_boundaries(&sources, &bodies)
        .expect("actual dual-layout/runtime private issuer");
    Capture { sources, authority }
}

/// # Panics
/// Panics if independently recaptured fixture source or declaration material is invalid.
#[must_use]
pub fn edited(buffer: &str, handle: &str, scalar: &str) -> Capture {
    recaptured(
        buffer,
        handle,
        scalar,
        serde_json::from_slice(DECLARATIONS).expect("independent fixed declaration bytes"),
        false,
    )
}

/// # Panics
/// Panics if independently recaptured fixture source or declaration material is invalid.
#[must_use]
pub fn compact_handle(extra: &str) -> Capture {
    let text =
        std::str::from_utf8(DECLARATIONS).expect("fixture UTF-8").replace("fixture-c-v0@0", "l@0");
    recaptured(
        &BUFFER.replace("fixture-c-v0@0", "l@0"),
        &format!("{HANDLE}{extra}").replace("fixture-c-v0@0", "l@0"),
        &SCALAR.replace("fixture-c-v0@0", "l@0"),
        serde_json::from_str(&text).expect("independent short-id declaration document"),
        true,
    )
}

fn map(buffer: &str, handle: &str, scalar: &str, compact: bool) -> SourceMap {
    SourceMap::build(
        [("buffer", buffer), ("handle", handle), ("scalar", scalar)]
            .into_iter()
            .map(|(name, text)| SourceFileInput {
                path: if compact {
                    format!("{name}.zry")
                } else {
                    format!("tests/native-c-abi-v0/source-{name}.zry")
                },
                text: text.into(),
            })
            .collect(),
    )
    .expect("independently captured original source map")
}

fn sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn wire(mut document: Value) -> Vec<u8> {
    document.sort_all_objects();
    let mut bytes = serde_json::to_vec(&document).expect("canonical test capture wire");
    bytes.push(b'\n');
    bytes
}

fn recaptured(
    buffer: &str,
    handle: &str,
    scalar: &str,
    mut document: Value,
    compact: bool,
) -> Capture {
    let sources = map(buffer, handle, scalar, compact);
    let syntax =
        authenticate_sources(&sources).expect("complete edited source independently authenticated");
    document["sources"] = syntax.files().iter().map(|file| serde_json::json!({
        "path":file.path(), "sha256":sha(syntax.source_text(file.file_id()).expect("source bytes").as_bytes())
    })).collect::<Vec<_>>().into();
    document["sites"] = sites(&syntax).into();
    let operations = document["operations"].as_array_mut().expect("complete operations");
    operations.sort_by(|a, b| a["key"].as_str().cmp(&b["key"].as_str()));
    for (ordinal, operation) in operations.iter_mut().enumerate() {
        let key = operation["key"].as_str().expect("key");
        let (file, start, end) = if operation["direction"] == "import" {
            let (file, site) = syntax
                .files()
                .iter()
                .find_map(|file| {
                    file.sites()
                        .iter()
                        .find(|site| {
                            site.operation() == Some(key)
                                && matches!(
                                    site.primitive(),
                                    zryna_syntax::native_c_v0::raw::Primitive::RawCall
                                        | zryna_syntax::native_c_v0::raw::Primitive::Release
                                )
                        })
                        .map(|site| (file, site))
                })
                .expect("original import binding still present");
            (file, site.span().start(), site.span().end())
        } else {
            let name = operation["logicalName"].as_str().expect("export name");
            let (file, function) = syntax
                .files()
                .iter()
                .find_map(|file| {
                    file.functions()
                        .iter()
                        .find(|function| function.exported && function.name == name)
                        .map(|function| (file, function))
                })
                .expect("original total scalar export still present");
            (file, function.range.start, function.range.end)
        };
        operation["sourceBinding"] = serde_json::json!({"path":file.path(), "sha256":sha(syntax.source_text(file.file_id()).expect("bytes").as_bytes()),
            "start":start,"end":end,"ordinal":ordinal});
    }
    let library = &document["libraries"][0];
    let id = library["id"].as_str().expect("library id").to_owned();
    let operations = document["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .filter(|op| op["library"] == id)
        .map(|op| {
            let mut policy = op.clone();
            policy.as_object_mut().expect("operation object").remove("sourceBinding");
            policy
        })
        .collect::<Vec<_>>();
    let policy = wire(
        serde_json::json!({"allocators":library["allocators"],"kinds":library["kinds"],"operations":operations}),
    );
    document["libraries"][0]["policySha256"] = sha(&policy).into();
    document["libraries"][0]["headerSha256"] = sha(HEADER).into();
    let declarations = zryna_semantics::native_c_v0::verify(
        &wire(document),
        &sources,
        &syntax,
        &[LibraryMaterial { library_id: &id, header_bytes: HEADER, policy_bytes: &policy }],
        zryna_syntax::native_c_v0::TARGET,
    )
    .expect("edited inputs passed genuine declaration/material authentication");
    let bodies = verify_bodies(&sources, &declarations).expect("complete edited body issuer");
    let authority =
        compose_private_boundaries(&sources, &bodies).expect("complete edited private issuer");
    Capture { sources, authority }
}

fn sites(syntax: &AuthenticatedForeignSources) -> Vec<Value> {
    let mut sites = Vec::new();
    for file in syntax.files() {
        let text = syntax.source_text(file.file_id()).expect("actual source");
        for site in file.sites() {
            sites.push(serde_json::json!({"primitive":site.primitive(),"safety":site.safety(),"operation":site.operation(),
                "path":file.path(),"sourceSha256":sha(text.as_bytes()),"start":site.span().start(),"end":site.span().end(),
                "spelling":text.get(site.span().start() as usize..site.span().end() as usize).expect("authenticated exact site") }));
        }
    }
    sites.sort_by(|a, b| {
        (a["path"].as_str(), a["start"].as_u64(), a["end"].as_u64()).cmp(&(
            b["path"].as_str(),
            b["start"].as_u64(),
            b["end"].as_u64(),
        ))
    });
    sites
}

#[test]
fn captured_materials_and_source_issuers_survive_ir_admission() {
    let capture = reference();
    let program =
        zryna_native_c_ir::lower(&capture.sources, &capture.authority).expect("independent IR");
    assert_eq!(
        program
            .private_authority()
            .body_authority()
            .declaration_authority()
            .header_bytes("fixture-c-v0@0"),
        Some(HEADER)
    );
    assert_eq!(
        program
            .private_authority()
            .body_authority()
            .declaration_authority()
            .policy_bytes("fixture-c-v0@0"),
        Some(POLICY)
    );
    assert!(program.belongs_to(&capture.sources));
    assert_eq!(program.source_map_identity(), capture.sources.identity());
    assert_eq!(program.native_layouts().source_map_identity(), capture.sources.identity());
    assert_eq!(program.linear_layouts().source_map_identity(), capture.sources.identity());
}

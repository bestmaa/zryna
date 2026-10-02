//! Independent source edits are freshly captured; no body, ownership or IR input is fabricated.

use super::super::super::{DeclarationError, LibraryMaterial, VerifiedDeclarationSet, verify};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::native_c_source_v0::{AuthenticatedForeignSources, authenticate_sources};

const DECLARATIONS: &[u8] =
    include_bytes!("../../../../../../tests/native-c-abi-v0/declarations.ffi.json");
pub(super) const HEADER: &[u8] =
    include_bytes!("../../../../../../tests/native-c-abi-v0/candidate.h");
const POLICY: &[u8] =
    include_bytes!("../../../../../../tests/native-c-auth-v0/library-policy.json");
pub(super) const BUFFER: &str =
    include_str!("../../../../../../tests/native-c-abi-v0/source-buffer.zry");
pub(super) const HANDLE: &str =
    include_str!("../../../../../../tests/native-c-abi-v0/source-handle.zry");
pub(super) const SCALAR: &str =
    include_str!("../../../../../../tests/native-c-abi-v0/source-scalar.zry");

#[derive(Debug)]
pub(super) struct Capture {
    pub sources: SourceMap,
    pub declarations: VerifiedDeclarationSet,
}

pub(super) fn reference() -> Capture {
    let sources = source_map(&[("buffer", BUFFER), ("handle", HANDLE), ("scalar", SCALAR)], false);
    let syntax = authenticate_sources(&sources).expect("complete original reference syntax");
    let declarations = verify(
        DECLARATIONS,
        &sources,
        &syntax,
        &[LibraryMaterial {
            library_id: "fixture-c-v0@0",
            header_bytes: HEADER,
            policy_bytes: POLICY,
        }],
        "x86_64-unknown-linux-gnu",
    )
    .expect("independent fixed declaration/material authority");
    Capture { sources, declarations }
}

pub(super) fn edited(buffer: &str, handle: &str, scalar: &str) -> Capture {
    try_edited(buffer, handle, scalar)
        .expect("changed inputs passed genuine declaration/material boundary before body test")
}

pub(super) fn try_edited(
    buffer: &str,
    handle: &str,
    scalar: &str,
) -> Result<Capture, DeclarationError> {
    try_captured_headers(
        &[("buffer", buffer), ("handle", handle), ("scalar", scalar)],
        false,
        document(),
        &[("fixture-c-v0@0", HEADER)],
    )
}

pub(super) fn append_handle(extra: &str) -> Capture {
    edited(BUFFER, &format!("{HANDLE}{extra}"), SCALAR)
}

pub(super) fn compact(extra: &str) -> Capture {
    let mut document = document();
    let serialized = serde_json::to_string(&document).expect("independent raw document");
    document = serde_json::from_str(&serialized.replace("fixture-c-v0@0", "l@0"))
        .expect("short logical library identity");
    captured(
        &[
            ("buffer", &BUFFER.replace("fixture-c-v0@0", "l@0")),
            ("handle", &format!("{HANDLE}{extra}").replace("fixture-c-v0@0", "l@0")),
            ("scalar", &SCALAR.replace("fixture-c-v0@0", "l@0")),
        ],
        true,
        document,
    )
}

pub(super) fn document() -> Value {
    serde_json::from_slice(DECLARATIONS).expect("independent declaration fixture")
}

fn source_map(inputs: &[(&str, &str)], compact: bool) -> SourceMap {
    SourceMap::build(
        inputs
            .iter()
            .map(|(name, text)| SourceFileInput {
                path: if compact {
                    format!("{name}.zry")
                } else {
                    format!("tests/native-c-abi-v0/source-{name}.zry")
                },
                text: (*text).into(),
            })
            .collect(),
    )
    .expect("independent original source capture")
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn wire(mut value: Value) -> Vec<u8> {
    value.sort_all_objects();
    let mut bytes = serde_json::to_vec(&value).expect("closed independent wire serialization");
    bytes.push(b'\n');
    bytes
}

pub(super) fn captured(inputs: &[(&str, &str)], compact: bool, document: Value) -> Capture {
    let id = document["libraries"][0]["id"].as_str().expect("one original library").to_owned();
    captured_headers(inputs, compact, document, &[(id.as_str(), HEADER)])
}

pub(super) fn captured_headers(
    inputs: &[(&str, &str)],
    compact: bool,
    document: Value,
    headers: &[(&str, &[u8])],
) -> Capture {
    try_captured_headers(inputs, compact, document, headers)
        .expect("changed inputs passed genuine declaration/material boundary before body test")
}

fn try_captured_headers(
    inputs: &[(&str, &str)],
    compact: bool,
    mut document: Value,
    headers: &[(&str, &[u8])],
) -> Result<Capture, DeclarationError> {
    let sources = source_map(inputs, compact);
    let syntax =
        authenticate_sources(&sources).expect("independently parsed complete changed source");
    capture_sites(&mut document, &syntax);
    capture_bindings(&mut document, &syntax);
    let policies = capture_policies(&mut document, headers);
    let materials: Vec<_> = policies
        .iter()
        .map(|(id, policy, header)| LibraryMaterial {
            library_id: id,
            header_bytes: header,
            policy_bytes: policy,
        })
        .collect();
    let declarations =
        verify(&wire(document), &sources, &syntax, &materials, "x86_64-unknown-linux-gnu")?;
    Ok(Capture { sources, declarations })
}

fn capture_sites(document: &mut Value, syntax: &AuthenticatedForeignSources) {
    document["sources"] = syntax
        .files()
        .iter()
        .map(|file| {
            let bytes = syntax.source_text(file.file_id()).expect("captured text").as_bytes();
            serde_json::json!({"path":file.path(),"sha256":sha256(bytes)})
        })
        .collect::<Vec<_>>()
        .into();
    let mut sites = Vec::new();
    for file in syntax.files() {
        let hash = sha256(syntax.source_text(file.file_id()).expect("complete source").as_bytes());
        for site in file.sites() {
            let spelling = syntax
                .source_text(file.file_id())
                .expect("captured source bytes")
                .get(site.span().start() as usize..site.span().end() as usize)
                .expect("exact authenticated call range");
            sites.push(serde_json::json!({
                "primitive": site.primitive(), "safety":site.safety(), "operation":site.operation(),
                "path":file.path(), "sourceSha256":hash, "start":site.span().start(), "end":site.span().end(),
                "spelling":spelling,
            }));
        }
    }
    sites.sort_by(|left, right| {
        (left["path"].as_str(), left["start"].as_u64(), left["end"].as_u64()).cmp(&(
            right["path"].as_str(),
            right["start"].as_u64(),
            right["end"].as_u64(),
        ))
    });
    document["sites"] = sites.into();
}

fn capture_bindings(document: &mut Value, syntax: &AuthenticatedForeignSources) {
    let operations = document["operations"].as_array_mut().expect("operation inventory");
    operations.sort_by(|left, right| left["key"].as_str().cmp(&right["key"].as_str()));
    for (ordinal, operation) in operations.iter_mut().enumerate() {
        let key = operation["key"].as_str().expect("operation key");
        let (path, file, start, end) = if operation["direction"] == "import" {
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
                .expect("complete import binding remains independently present");
            (file.path(), file.file_id(), site.span().start(), site.span().end())
        } else {
            let name = operation["logicalName"].as_str().expect("export source identifier");
            let (file, function) = syntax
                .files()
                .iter()
                .find_map(|file| {
                    file.functions()
                        .iter()
                        .find(|function| function.exported && function.name == name)
                        .map(|function| (file, function))
                })
                .expect("complete scalar export remains present");
            (file.path(), file.file_id(), function.range.start, function.range.end)
        };
        operation["sourceBinding"] = serde_json::json!({
            "path":path,"sha256":sha256(syntax.source_text(file).expect("captured function source").as_bytes()),
            "start":start,"end":end,"ordinal":ordinal,
        });
    }
}

fn capture_policies<'a>(
    document: &mut Value,
    headers: &[(&str, &'a [u8])],
) -> Vec<(String, Vec<u8>, &'a [u8])> {
    let mut policies = Vec::new();
    for index in 0..document["libraries"].as_array().expect("libraries").len() {
        let library = &document["libraries"][index];
        let id = library["id"].as_str().expect("library id").to_owned();
        let header = headers
            .iter()
            .find(|(library, _)| *library == id.as_str())
            .map(|(_, bytes)| *bytes)
            .expect("complete independently captured header set");
        let operations: Vec<_> = document["operations"]
            .as_array()
            .expect("operations")
            .iter()
            .filter(|operation| operation["library"].as_str() == Some(id.as_str()))
            .map(|operation| {
                let mut policy = operation.clone();
                policy.as_object_mut().expect("operation object").remove("sourceBinding");
                policy
            })
            .collect();
        let policy = wire(
            serde_json::json!({"allocators":library["allocators"],"kinds":library["kinds"],"operations":operations}),
        );
        document["libraries"][index]["policySha256"] = sha256(&policy).into();
        document["libraries"][index]["headerSha256"] = sha256(header).into();
        policies.push((id, policy, header));
    }
    policies
}

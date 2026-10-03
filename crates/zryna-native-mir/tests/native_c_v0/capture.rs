//! Original fixture recapture; every issuer is obtained through production admission.

use serde_json::Value;
use sha2::{Digest, Sha256};
use zryna_native_c_ir::VerifiedNativeCProgram;
use zryna_semantics::native_c_v0::{
    LibraryMaterial,
    body::{compose_private_boundaries, verify_bodies},
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::native_c_source_v0::{AuthenticatedForeignSources, authenticate_sources};

const BUFFER: &str = include_str!("../../../../tests/native-c-abi-v0/source-buffer.zry");
const HANDLE: &str = include_str!("../../../../tests/native-c-abi-v0/source-handle.zry");
const SCALAR: &str = include_str!("../../../../tests/native-c-abi-v0/source-scalar.zry");
const HEADER: &[u8] = include_bytes!("../../../../tests/native-c-abi-v0/candidate.h");
const POLICY: &[u8] = include_bytes!("../../../../tests/native-c-auth-v0/library-policy.json");
const DECLARATIONS: &[u8] =
    include_bytes!("../../../../tests/native-c-abi-v0/declarations.ffi.json");

pub(super) fn reference() -> (SourceMap, VerifiedNativeCProgram) {
    let sources = map(SCALAR);
    seal(sources, DECLARATIONS, HEADER, POLICY)
}
fn map(scalar: &str) -> SourceMap {
    SourceMap::build(
        [("buffer", BUFFER), ("handle", HANDLE), ("scalar", scalar)]
            .into_iter()
            .map(|(name, text)| SourceFileInput {
                path: format!("tests/native-c-abi-v0/source-{name}.zry"),
                text: text.into(),
            })
            .collect(),
    )
    .expect("genuine original source map")
}
fn seal(
    sources: SourceMap,
    declarations: &[u8],
    header: &[u8],
    policy: &[u8],
) -> (SourceMap, VerifiedNativeCProgram) {
    let syntax = authenticate_sources(&sources).expect("genuine original syntax");
    let declarations = zryna_semantics::native_c_v0::verify(
        declarations,
        &sources,
        &syntax,
        &[LibraryMaterial {
            library_id: "fixture-c-v0@0",
            header_bytes: header,
            policy_bytes: policy,
        }],
        zryna_syntax::native_c_v0::TARGET,
    )
    .expect("original declarations and captured materials");
    let bodies = verify_bodies(&sources, &declarations).expect("complete original body issuer");
    let private = compose_private_boundaries(&sources, &bodies).expect("genuine private issuers");
    let ir =
        zryna_native_c_ir::lower(&sources, &private).expect("independent original IR admission");
    (sources, ir)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn wire(mut document: Value) -> Vec<u8> {
    document.sort_all_objects();
    let mut bytes = serde_json::to_vec(&document).expect("canonical exact fixture JSON");
    bytes.push(b'\n');
    bytes
}
fn recapture(scalar: &str, mut document: Value, header: &[u8]) -> VerifiedNativeCProgram {
    let sources = map(scalar);
    let syntax = authenticate_sources(&sources).expect("independently parsed complete source");
    document["sources"] = syntax.files().iter().map(|file| serde_json::json!({
        "path": file.path(), "sha256": digest(syntax.source_text(file.file_id()).expect("source").as_bytes())
    })).collect::<Vec<_>>().into();
    document["sites"] = sites(&syntax).into();
    let operations = document["operations"].as_array_mut().expect("operations");
    operations.sort_by(|a, b| a["key"].as_str().cmp(&b["key"].as_str()));
    for (ordinal, operation) in operations.iter_mut().enumerate() {
        let key = operation["key"].as_str().expect("exact key");
        let (file, start, end) =
            if operation["direction"] == "import" {
                let (file, site) =
                    syntax
                        .files()
                        .iter()
                        .find_map(|file| {
                            file.sites()
                                .iter()
                                .find(|site| {
                                    site.operation() == Some(key)
                    && matches!(site.primitive(), zryna_syntax::native_c_v0::raw::Primitive::RawCall
                        | zryna_syntax::native_c_v0::raw::Primitive::Release)
                                })
                                .map(|site| (file, site))
                        })
                        .expect("complete original import site");
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
                    .expect("original exported source function");
                (file, function.range.start, function.range.end)
            };
        operation["sourceBinding"] = serde_json::json!({
            "path":file.path(), "sha256":digest(syntax.source_text(file.file_id()).expect("bytes").as_bytes()),
            "start":start, "end":end, "ordinal":ordinal
        });
    }
    let library = &document["libraries"][0];
    let operations = document["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .filter(|operation| operation["library"] == "fixture-c-v0@0")
        .map(|operation| {
            let mut policy = operation.clone();
            policy.as_object_mut().expect("operation").remove("sourceBinding");
            policy
        })
        .collect::<Vec<_>>();
    let policy = wire(serde_json::json!({
        "allocators":library["allocators"],"kinds":library["kinds"],"operations":operations
    }));
    document["libraries"][0]["policySha256"] = digest(&policy).into();
    document["libraries"][0]["headerSha256"] = digest(header).into();
    seal(sources, &wire(document), header, &policy).1
}
fn sites(syntax: &AuthenticatedForeignSources) -> Vec<Value> {
    let mut sites = Vec::new();
    for file in syntax.files() {
        let text = syntax.source_text(file.file_id()).expect("original bytes");
        for site in file.sites() {
            sites.push(serde_json::json!({
                "primitive":site.primitive(),"safety":site.safety(),"operation":site.operation(),
                "path":file.path(),"sourceSha256":digest(text.as_bytes()),
                "start":site.span().start(),"end":site.span().end(),
                "spelling":text.get(site.span().start() as usize..site.span().end() as usize).expect("exact site")
            }));
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
pub(super) fn stack_export(parameters: usize) -> VerifiedNativeCProgram {
    let names = (0..parameters).map(|index| format!("a{index}")).collect::<Vec<_>>();
    let signature = names.iter().map(|name| format!("{name}: i32")).collect::<Vec<_>>().join(", ");
    let scalar = format!(
        "{}export function add({signature}): i32 {{ return {}; }}\n",
        SCALAR.split("export function").next().expect("imported source"),
        names.join(" + ")
    );
    let mut document: Value =
        serde_json::from_slice(DECLARATIONS).expect("fixed declaration bytes");
    let export = document["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["direction"] == "export")
        .expect("reverse export");
    export["parameters"] = names
        .iter()
        .map(|name| {
            serde_json::json!({
                "name":name, "abi":"c-i32", "resource":null
            })
        })
        .collect::<Vec<_>>()
        .into();
    let c_parameters =
        names.iter().map(|name| format!("int32_t {name}")).collect::<Vec<_>>().join(", ");
    let header = std::str::from_utf8(HEADER).expect("header").replace(
        "int32_t zryna_c_v0_e_add(int32_t left, int32_t right);",
        &format!("int32_t zryna_c_v0_e_add({c_parameters});"),
    );
    recapture(&scalar, document, header.as_bytes())
}
pub(super) fn constant_export() -> VerifiedNativeCProgram {
    let scalar = SCALAR.replace(
        "export function add(left: i32, right: i32): i32 {\n  return left + right;\n}",
        "export function add(): i32 {\n  return -2147483648;\n}",
    );
    let mut document: Value = serde_json::from_slice(DECLARATIONS).expect("declarations");
    let export = document["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["direction"] == "export")
        .expect("export");
    export["parameters"] = serde_json::json!([]);
    let header = std::str::from_utf8(HEADER).expect("header").replace(
        "int32_t zryna_c_v0_e_add(int32_t left, int32_t right);",
        "int32_t zryna_c_v0_e_add(void);",
    );
    recapture(&scalar, document, header.as_bytes())
}

pub(super) fn boolean_export() -> VerifiedNativeCProgram {
    let scalar = SCALAR.replace(
        "export function add(left: i32, right: i32): i32 {\n  return left + right;\n}",
        "export function add(flag: bool): bool {\n  return flag;\n}",
    );
    let mut document: Value = serde_json::from_slice(DECLARATIONS).expect("declarations");
    let export = document["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["direction"] == "export")
        .expect("export");
    export["parameters"] = serde_json::json!([{"name":"flag","abi":"bool32","resource":null}]);
    export["result"] = "bool32".into();
    let header = std::str::from_utf8(HEADER).expect("header").replace(
        "int32_t zryna_c_v0_e_add(int32_t left, int32_t right);",
        "uint32_t zryna_c_v0_e_add(uint32_t flag);",
    );
    recapture(&scalar, document, header.as_bytes())
}

pub(super) fn c_int_export() -> VerifiedNativeCProgram {
    let mut document: Value = serde_json::from_slice(DECLARATIONS).expect("declarations");
    let export = document["operations"]
        .as_array_mut()
        .expect("operations")
        .iter_mut()
        .find(|operation| operation["direction"] == "export")
        .expect("export");
    for parameter in export["parameters"].as_array_mut().expect("parameters") {
        parameter["abi"] = "c-int".into();
    }
    export["result"] = "c-int".into();
    let header = std::str::from_utf8(HEADER).expect("header").replace(
        "int32_t zryna_c_v0_e_add(int32_t left, int32_t right);",
        "int zryna_c_v0_e_add(int left, int right);",
    );
    recapture(SCALAR, document, header.as_bytes())
}

pub(super) fn boolean_import() -> VerifiedNativeCProgram {
    let mut document: Value =
        serde_json::from_slice(DECLARATIONS).expect("fixed declaration bytes");
    let mut operation = document["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .find(|operation| operation["symbol"] == "add")
        .expect("scalar import")
        .clone();
    operation["key"] = "fixture-c-v0@0/fixture_boolean_shim".into();
    operation["logicalName"] = "fixture_boolean_shim".into();
    operation["symbol"] = "fixture_boolean_shim".into();
    operation["parameters"] = serde_json::json!([{"name":"arg0","abi":"bool32","resource":null}]);
    operation["result"] = "bool32".into();
    document["operations"].as_array_mut().expect("operations").push(operation);
    let scalar = format!(
        "{SCALAR}\nfunction booleanBridge(flag: bool): bool {{ return Ffi.rawCall(\"fixture-c-v0@0/fixture_boolean_shim\", flag); }}\n"
    );
    let header = [HEADER, b"\nuint32_t fixture_boolean_shim(uint32_t flag);\n"].concat();
    recapture(&scalar, document, &header)
}

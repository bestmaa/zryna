use super::*;

// Independent source/wire/policy captures. These verify declaration authority, not C execution.
fn import_capture(
    name: &str,
    parameter: &str,
    library: &str,
    symbol: &str,
    path: &str,
    spelling_bytes: Option<usize>,
) -> (SourceMap, Value, Vec<u8>, Vec<u8>) {
    let key = format!("{library}/{symbol}");
    let call = format!("Ffi.rawCall(\"{key}\", {parameter}, right)");
    let call = spelling_bytes.map_or_else(
        || call.clone(),
        |length| format!("{}{})", &call[..call.len() - 1], " ".repeat(length - call.len())),
    );
    let text =
        format!("function imported({parameter}: i32, right: i32): i32 {{ return {call}; }}\n");
    let start = text.find("Ffi.rawCall(").expect("call bytes");
    let end = start + call.len();
    let digest = sha256(text.as_bytes());
    let sources = SourceMap::build(vec![SourceFileInput { path: path.into(), text }])
        .expect("actual source map issuer");
    let mut document = document();
    let mut operation = document["operations"][0].clone();
    operation["key"] = key.clone().into();
    operation["library"] = library.into();
    operation["logicalName"] = name.into();
    operation["symbol"] = symbol.into();
    operation["parameters"][0]["name"] = parameter.into();
    operation["sourceBinding"] =
        serde_json::json!({"path":path,"sha256":digest,"start":start,"end":end,"ordinal":0});
    document["operations"] = vec![operation].into();
    document["libraries"][0]["id"] = library.into();
    document["libraries"][0]["version"] =
        library.split_once('@').expect("library identity").1.into();
    document["libraries"][0]["kinds"] = Vec::<Value>::new().into();
    document["libraries"][0]["allocators"] = Vec::<Value>::new().into();
    let header =
        format!("#include <stdint.h>\nint32_t {symbol}(int32_t {parameter}, int32_t right);\n")
            .into_bytes();
    document["libraries"][0]["headerSha256"] = sha256(&header).into();
    document["sources"] = serde_json::json!([{"path":path,"sha256":digest}]);
    document["sites"] = serde_json::json!([{"path":path,"sourceSha256":digest,"start":start,"end":end,
        "primitive":"rawCall","safety":"unsafe-raw","operation":key,"spelling":call}]);
    let policy = capture_changed_policy(&mut document);
    (sources, document, header, policy)
}

#[test]
fn native_c_v0_declaration_exact_field_bounds_have_actual_source_material_issuers() {
    let rows = [
        ("logicalName", "/operations/0/logicalName", "name-bytes"),
        ("parameter", "/operations/0/parameters/0/name", "name-bytes"),
        ("symbol", "/operations/0/symbol", "name-bytes"),
        ("library", "/libraries/0/id", "library-id-bytes"),
        ("key", "/operations/0/key", "key-bytes"),
        ("version-intersection", "/libraries/0/id", "library-id-bytes"),
        ("path", "/sources/0/path", "source-path-bytes"),
        ("spelling", "/sites/0/spelling", "primitive-spelling-bytes"),
    ];
    for (row, pointer, metric) in rows {
        let name = if row == "logicalName" { "n".repeat(128) } else { "add".into() };
        let parameter = if row == "parameter" { "p".repeat(128) } else { "left".into() };
        let library = match row {
            "library" | "key" => format!("{}@0", "l".repeat(126)),
            "version-intersection" => format!("l@{}", "1".repeat(126)),
            _ => "fixture-c-v0@0".into(),
        };
        let symbol = if matches!(row, "symbol" | "key") { "a".repeat(128) } else { "add".into() };
        let path =
            if row == "path" { format!("d/{}.zry", "s".repeat(250)) } else { "limits.zry".into() };
        let spelling = (row == "spelling").then_some(4096);
        let (sources, mut document, header, policy) =
            import_capture(&name, &parameter, &library, &symbol, &path, spelling);
        let syntax = authenticate_sources(&sources).expect("actual independently parsed source");
        let materials = [LibraryMaterial {
            library_id: &library,
            header_bytes: &header,
            policy_bytes: &policy,
        }];
        let accepted = verify(
            &wire(document.clone()),
            &sources,
            &syntax,
            &materials,
            zryna_syntax::native_c_v0::TARGET,
        )
        .unwrap_or_else(|error| panic!("exact {row}: {error}"));
        assert!(accepted.belongs_to(&sources));
        assert_eq!(accepted.header_bytes(&library), Some(header.as_slice()));
        assert_eq!(accepted.policy_bytes(&library), Some(policy.as_slice()));
        let value = document.pointer_mut(pointer).expect("exact field");
        let exact_bytes = value.as_str().expect("string").len();
        let mut extra = value.as_str().expect("string").to_owned();
        if row == "path" {
            extra.insert(0, 's');
        } else {
            extra.push('a');
        }
        *value = extra.into();
        let error = verify(
            &wire(document),
            &sources,
            &syntax,
            &materials,
            zryna_syntax::native_c_v0::TARGET,
        )
        .expect_err("first extra rejects before constructing a declaration seal");
        assert_eq!((error.code(), error.detail()), ("ZRYNA-C4107", metric), "{row}");
        eprintln!(
            "417-declaration-boundary-evidence {}",
            serde_json::json!({"row":row,
            "field":pointer,"exact_bytes":exact_bytes,"first_extra_bytes":exact_bytes+1,
            "original_source_issuer":true,"actual_policy_bytes":true,"diagnostic":error.code(),"metric":metric})
        );
    }
}

#[test]
fn native_c_v0_declaration_export_name_115_has_actual_source_issuer_and_116_rejects() {
    let name = "e".repeat(115);
    let text =
        format!("export function {name}(left: i32, right: i32): i32 {{ return left + right; }}\n");
    let digest = sha256(text.as_bytes());
    let end = text.len() - 1;
    let sources = SourceMap::build(vec![SourceFileInput { path: "export.zry".into(), text }])
        .expect("export issuer");
    let syntax = authenticate_sources(&sources).expect("exact exported function");
    let mut document = document();
    let mut operation = document["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .find(|operation| operation["direction"] == "export")
        .expect("scalar export")
        .clone();
    operation["logicalName"] = name.clone().into();
    operation["key"] =
        format!("{}/{name}", operation["library"].as_str().expect("output module")).into();
    operation["symbol"] = format!("zryna_c_v0_e_{name}").into();
    operation["sourceBinding"] =
        serde_json::json!({"path":"export.zry","sha256":digest,"start":0,"end":end,"ordinal":0});
    document["operations"] = vec![operation].into();
    document["libraries"] = Vec::<Value>::new().into();
    document["sites"] = Vec::<Value>::new().into();
    document["sources"] = serde_json::json!([{"path":"export.zry","sha256":digest}]);
    let accepted =
        verify(&wire(document.clone()), &sources, &syntax, &[], zryna_syntax::native_c_v0::TARGET)
            .expect("exact 115 export authority");
    assert!(accepted.belongs_to(&sources));
    document["operations"][0]["logicalName"] = "e".repeat(116).into();
    document["operations"][0]["symbol"] = format!("zryna_c_v0_e_{}", "e".repeat(116)).into();
    assert_eq!(
        verify(&wire(document), &sources, &syntax, &[], zryna_syntax::native_c_v0::TARGET)
            .expect_err("116 export refuses")
            .code(),
        "ZRYNA-C4107"
    );
}

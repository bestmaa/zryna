//! One genuine source-bound owned program enters all three target emitters unchanged.
use zryna_ir::generic_v1::owned_v2;
use zryna_layout::StorageTarget;
use zryna_ownership_runtime_abi::generic_v1 as runtime;
use zryna_semantics::bounded_generics_v1::{
    SemanticInput,
    body_types::check_body_types,
    instantiation::{
        layouts::verify_layouts,
        owned_v2::{discover, produce_claim},
    },
    resolve_declarations,
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5::{decode_snapshot, verify_snapshot};
#[path = "../m7-generic-owned-native/hostile.rs"]
mod hostile;
fn main() {
    let out = std::path::PathBuf::from(std::env::args_os().nth(1).expect("explicit output"));
    std::fs::create_dir_all(&out).expect("output");
    let values =
        std::fs::read_to_string("tests/m7-generic-owned-clone/values.zry").expect("values");
    let main = std::fs::read_to_string("tests/m7-generic-owned-clone/main.zry").expect("main");
    for cross in [false, true] {
        let joined = format!("{values}\n{main}");
        let imported = format!("import {{option,result,makeOption,makeOk,makeErr,discardOption,discardResult}} from \"./values.zry\";\n{main}");
        let files = if cross {
            vec![("main.zry", imported.as_str()), ("values.zry", values.as_str())]
        } else {
            vec![("main.zry", joined.as_str())]
        };
        let dto = std::fs::read(format!(
            "tests/m7-generic-owned-clone/{}-reference.json",
            files.len()
        ))
        .expect("frozen DTO");
        let sources = SourceMap::build(
            files
                .iter()
                .map(|(path, text)| SourceFileInput { path: (*path).into(), text: (*text).into() })
                .collect(),
        )
        .expect("source");
        let syntax = verify_snapshot(decode_snapshot(&dto).expect("DTO"), &sources)
            .expect("exact source syntax");
        let entry = sources.verify_file_id(0).expect("entry");
        let d =
            resolve_declarations(SemanticInput::try_new(&syntax, &sources, entry).expect("input"))
                .expect("declarations");
        let b = check_body_types(&d).expect("opaque bodies");
        let i = discover(&b).expect("demand");
        let linear = verify_layouts(i.instances(), StorageTarget::Linear32V1).expect("linear");
        let linux = verify_layouts(i.instances(), StorageTarget::LinuxX8664V1).expect("linux");
        let abi = runtime::verify_v1(
            runtime::raw_v1(&linear, &linux).expect("declarations"),
            &linear,
            &linux,
        )
        .expect("runtime");
        let claim = produce_claim(&i, &linear, &linux).expect("owned claims");
        let wire = owned_v2::wire::v3::encode(&claim).expect("wire");
        let p = owned_v2::verify(
            owned_v2::wire::v3::decode(&wire).expect("decode"),
            &syntax,
            &sources,
            entry,
            &linear,
            &linux,
            &abi,
        )
        .expect("seal");
        let mir = zryna_native_mir::generic_owned_v2::lower(&p).expect("native MIR");
        let target =
            zryna_backend_native::select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET)
                .expect("target");
        let native = zryna_backend_native::generic_owned_v2::emit_object(&mir, target)
            .expect("closed owned ELF");
        hostile::hostile_inventory(native.bytes(), &mir, b"zryna_v1_e_optionNone");
        assert_eq!(
            native,
            zryna_backend_native::generic_owned_v2::emit_object(&mir, target)
                .expect("deterministic ELF")
        );
        let js = zryna_backend_javascript::generic_owned_v2::emit(&p).expect("owned JS");
        let wasm = zryna_backend_webassembly::generic_owned_v2::emit(&p).expect("owned Wasm");
        std::fs::write(out.join(format!("{cross}.o")), native.bytes()).expect("ELF evidence");
        std::fs::write(out.join(format!("{cross}.mjs")), js.source).expect("JS evidence");
        std::fs::write(out.join(format!("{cross}.wasm")), wasm.bytes()).expect("Wasm evidence");
        std::fs::write(out.join(format!("{cross}.zir")), wire).expect("wire evidence");
        std::fs::write(out.join("ownership-runtime-v1.h"), &abi.declarations().native_header)
            .expect("exact issuer header");
        println!(
            "same owned seal -> JS/core-Wasm/native; modules={}, functions={}, exports={}",
            files.len(),
            p.functions().len(),
            p.scalar_abi().exports().len()
        );
    }
}

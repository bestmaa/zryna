//! Real source-to-executable owned successor conformance against independent allocation oracles.
use std::process::Command;
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
const VALUES: &str = include_str!("../../../../tests/m7-generic-owned-fixtures/values.zry");
const MAIN: &str = include_str!("../../../../tests/m7-generic-owned-fixtures/main.zry");

#[test]
fn core_wasm_owned_transfers_active_payload_loans_and_cleanup_execute() {
    for cross in [false, true] {
        let joined = format!("{VALUES}\n{MAIN}");
        let imported = format!(
            "import {{identity,keep,make,discard,inspect,inspectMut}} from \"./values.zry\";\n{MAIN}"
        );
        let files = if cross {
            vec![("main.zry", imported.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", joined.as_str())]
        };
        let snapshot: &[u8] = if cross {
            include_bytes!("../../../../tests/m7-generic-owned-fixtures/2-reference.json")
        } else {
            include_bytes!("../../../../tests/m7-generic-owned-fixtures/1-reference.json")
        };
        let sources = SourceMap::build(
            files
                .iter()
                .map(|(path, text)| SourceFileInput { path: (*path).into(), text: (*text).into() })
                .collect(),
        )
        .expect("source");
        let syntax = verify_snapshot(decode_snapshot(snapshot).expect("DTO"), &sources)
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
        let wire = owned_v2::wire::encode(&claim).expect("wire");
        let p = owned_v2::verify(
            owned_v2::wire::decode(&wire).expect("decode"),
            &syntax,
            &sources,
            entry,
            &linear,
            &linux,
            &abi,
        )
        .expect("seal");
        let artifact = super::emit(&p).expect("emission");
        assert_eq!(artifact, super::emit(&p).expect("deterministic emission"));
        hostile_module(artifact.bytes(), &p);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let file =
            std::env::temp_dir().join(format!("zryna-416-owned-{}-{id}.wasm", std::process::id()));
        std::fs::write(&file, artifact.bytes()).expect("executable");
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/m7-generic-owned-fixtures/webassembly.mjs"
            ))
            .arg(&file)
            .output()
            .expect("Node");
        std::fs::remove_file(&file).expect("cleanup");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        println!("{}", String::from_utf8_lossy(&output.stdout));
        if let Some(dir) = std::env::var_os("ZRYNA_GENERIC_OWNED_EVIDENCE_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::write(dir.join(format!("{}-modules.wasm", files.len())), artifact.bytes())
                .expect("retain executable");
            std::fs::write(dir.join(format!("{}-modules.zir", files.len())), wire)
                .expect("retain exact wire");
        }
    }
}

fn hostile_module(bytes: &[u8], program: &owned_v2::VerifiedOwnedProgram<'_>) {
    let layout = super::layout::Layout::new(program).expect("exact authority");
    super::audit::seal(bytes.to_vec(), &layout).expect("pristine byte audit");
    let mut attacks = Vec::new();
    for needle in
        [super::runtime::NAMESPACE.as_bytes(), b"stringFromUtf8Copy", b"some", "α".as_bytes()]
    {
        let mut attack = bytes.to_vec();
        let at = attack
            .windows(needle.len())
            .position(|v| v == needle)
            .expect("exact emitted identity/literal");
        attack[at] = b'x';
        attacks.push(attack);
    }
    let mut custom = bytes.to_vec();
    custom.extend_from_slice(&[0, 1, 0]);
    attacks.push(custom);
    let mut version = bytes.to_vec();
    version[4] = 2;
    attacks.push(version);
    let mut truncated = bytes.to_vec();
    truncated.pop();
    attacks.push(truncated);
    for attack in attacks {
        assert!(
            super::audit::seal(attack, &layout).is_err(),
            "no forged owned executable authority"
        );
    }
    super::audit::seal(bytes.to_vec(), &layout).expect("pristine replay after hostile modules");
}

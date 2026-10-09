use super::super::{
    authority::{InstanceAuthority, VerifiedLanguage},
    model::{Instance, Language, Selection},
};
use super::*;
use crate::native::native_c_v0::source_selection::EMITTER_ENTRIES;
use zryna_ir::{Expr, ExprId, ExprKind, Function, Program, Type};
use zryna_semantics::native_c_v0::LibraryMaterial;
use zryna_source::SourceFileInput;

mod h1_boundary;
mod replay;
mod witnesses;

const DECLARATIONS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/native-c-abi-v0/declarations.ffi.json"
));
const HEADER: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/native-c-abi-v0/candidate.h"));
const POLICY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/native-c-auth-v0/library-policy.json"
));

fn native_sources() -> SourceMap {
    SourceMap::build(
        [
            (
                "buffer",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/native-c-abi-v0/source-buffer.zry"
                )),
            ),
            (
                "handle",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/native-c-abi-v0/source-handle.zry"
                )),
            ),
            (
                "scalar",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/native-c-abi-v0/source-scalar.zry"
                )),
            ),
        ]
        .into_iter()
        .map(|(name, text)| SourceFileInput {
            path: format!("tests/native-c-abi-v0/source-{name}.zry"),
            text: text.into(),
        })
        .collect(),
    )
    .expect("original C source map")
}

fn native_authority() -> NativeAuthority {
    let sources = native_sources();
    let syntax =
        zryna_syntax::native_c_source_v0::authenticate_sources(&sources).expect("C syntax issuer");
    let declarations = zryna_semantics::native_c_v0::verify_report(
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
    .expect("genuine declaration/source seal");
    NativeAuthority { sources, declarations }
}

fn pure_authority(id: &str) -> InstanceAuthority {
    let text = "export function value(): i32 { return 1; }";
    let sources =
        SourceMap::build(vec![SourceFileInput { path: format!("{id}.zry"), text: text.into() }])
            .expect("pure source issuer");
    let file = sources.verify_file_id(0).expect("file");
    let start = u32::try_from(text.find('1').expect("literal")).expect("small source");
    let span = sources.span(file, start, start + 1).expect("actual literal span");
    let program = zryna_ir::verify(
        Program {
            functions: vec![Function {
                name: "value".into(),
                parameters: Vec::new(),
                return_type: Type::I32,
                expressions: vec![Expr { ty: Type::I32, span, kind: ExprKind::I32Literal(1) }],
                body: ExprId(0),
            }],
        },
        &sources,
    )
    .expect("genuine pure I32 issuer");
    InstanceAuthority { programs: vec![VerifiedLanguage::I32V1 { program, sources }] }
}

fn selection(row: Row) -> Selection {
    Selection {
        row,
        policy_version: super::super::model::POLICY.into(),
        world: None,
        approved: BTreeSet::new(),
        ceilings: [0; 10],
    }
}

fn fixture() -> (Input, Authorities, NativeAuthorities) {
    let input = Input {
        version: super::super::model::VERSION.into(),
        root: "A".into(),
        language: Language::I32V1,
        selections: vec![selection(Row::NativeHost)],
        instances: ["A", "B", "C"]
            .into_iter()
            .map(|id| Instance {
                id: id.into(),
                rows: BTreeSet::from([Row::NativeHost]),
                requirements: BTreeSet::new(),
                restrictions: BTreeSet::new(),
                reservation: Reservation::default(),
            })
            .collect(),
        edges: vec![("A".into(), "B".into()), ("B".into(), "C".into())],
    };
    let pure = Authorities {
        instances: ["A", "B"].into_iter().map(|id| (id.into(), pure_authority(id))).collect(),
        wit: None,
    };
    (input, pure, BTreeMap::from([("C".into(), native_authority())]))
}

fn rejects_emit(
    seal: &ValidatedNativeComposition,
    input: &Input,
    pure: &Authorities,
    native: &NativeAuthorities,
    code: &str,
) {
    EMITTER_ENTRIES.with(|entries| entries.set(0));
    let errors =
        seal.emit_entry(input, pure, native, "C", "imported").expect_err("reject before emission");
    assert_eq!(errors[0].code(), code, "{errors:?}");
    EMITTER_ENTRIES.with(|entries| assert_eq!(entries.get(), 0));
}

#[test]
fn mixed_and_non_native_selections_reject_with_derived_transitive_witness() {
    let (input, pure, native) = fixture();
    let seal = verify(&input, &pure, &native).expect("original native graph");
    for rows in [
        vec![Row::NativeHost, Row::JavaScriptNode],
        vec![Row::JavaScriptNode, Row::NativeHost],
        vec![Row::UniversalJavaScript],
        vec![Row::UniversalWebAssembly],
        vec![Row::UniversalNative],
        vec![Row::WitCommand],
        vec![Row::NativeHost, Row::UniversalJavaScript, Row::UniversalWebAssembly],
    ] {
        let mut changed = input.clone();
        changed.selections = rows.into_iter().map(selection).collect();
        let errors = verify(&changed, &pure, &native).expect_err("unsupported selection");
        assert_eq!(errors[0].code(), "ZRYNA-C4103");
        assert!(format!("{errors:?}").contains("A -> B -> C"));
        rejects_emit(&seal, &changed, &pure, &native, "ZRYNA-C4103");
        eprintln!(
            "417-transitive-evidence {}",
            serde_json::json!({"selections":changed.selections,
            "witness":["A","B","C"],"diagnostic":"ZRYNA-C4103","pre_emitter_entries":0})
        );
    }
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn native_only_graph_emits_real_object_through_retained_c_seal() {
    use object::{Object, ObjectSymbol};
    let (input, pure, native) = fixture();
    let seal = verify(&input, &pure, &native).expect("separate genuine pure and native issuers");
    assert_eq!(pure.instances.len(), 2);
    assert!(!pure.instances.contains_key("C"));
    assert_eq!(seal.witnesses["C"], ["A", "B", "C"]);
    assert!(seal.closures.values().all(|required| required == &BTreeSet::from(["C".into()])));
    EMITTER_ENTRIES.with(|entries| entries.set(0));
    let output =
        seal.emit_entry(&input, &pure, &native, "C", "imported").expect("same real emitter");
    EMITTER_ENTRIES.with(|entries| assert_eq!(entries.get(), 1));
    assert!(output.program().source().belongs_to(&native["C"].sources));
    let declarations =
        output.program().source().private_authority().body_authority().declaration_authority();
    assert_eq!(declarations.declaration_sha256(), native["C"].declarations.declaration_sha256());
    assert_eq!(declarations.header_bytes("fixture-c-v0@0"), Some(HEADER));
    assert_eq!(declarations.policy_bytes("fixture-c-v0@0"), Some(POLICY));
    let elf = object::File::parse(output.bytes()).expect("audited ELF");
    assert_eq!(elf.format(), object::BinaryFormat::Elf);
    assert_eq!(elf.architecture(), object::Architecture::X86_64);
    assert!(elf.symbols().any(|symbol| symbol.is_undefined() && symbol.name() == Ok("add")));
    eprintln!(
        "417-transitive-evidence {}",
        serde_json::json!({"selections":["NativeHost"],
        "witness":seal.witnesses["C"],"pre_emitter_entries":1,"original_c_issuer":true,
        "pure_issuers":["A","B"],"format":"ELF","import":"add","linked":false})
    );
}

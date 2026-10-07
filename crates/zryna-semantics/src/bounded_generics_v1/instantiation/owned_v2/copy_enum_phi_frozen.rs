//! Exact frozen source/DTO authority and independent source-bound attacks.
use crate::bounded_generics_v1::{
    SemanticInput, body_types::check_body_types, instantiation::layouts::verify_layouts,
    resolve_declarations,
};
use zryna_ir::generic_v1::{Failure, owned_v2, raw};
use zryna_layout::StorageTarget;
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5::{decode_snapshot, verify_snapshot};

#[path = "copy_enum_phi_attacks.rs"]
mod attacks;
#[path = "copy_enum_phi_boundaries.rs"]
mod boundaries;

const VALUES: &str =
    include_str!("../../../../../../tests/m7-generic-owned-copy-enum-phi/values.zry");
const MAIN: &str = include_str!("../../../../../../tests/m7-generic-owned-copy-enum-phi/main.zry");
const ONE: &[u8] =
    include_bytes!("../../../../../../tests/m7-generic-owned-copy-enum-phi/1-reference.json");
const TWO: &[u8] =
    include_bytes!("../../../../../../tests/m7-generic-owned-copy-enum-phi/2-reference.json");
const IMPORT: &str = concat!(
    "import {truthCode,optionSingleSelect,optionVariantsSelect,resultSingleSelect,",
    "resultVariantsSelect,optionEarlySelect,optionNestedSelect,resultRepeatedSelect,",
    "parallelSelect,bothReturnSelect} from \"./values.zry\";\n"
);

fn forms(mut test: impl FnMut(&[(&str, &str)], &[u8])) {
    for imported in [false, true] {
        let main = if imported { format!("{IMPORT}{MAIN}") } else { format!("{VALUES}\n{MAIN}") };
        let files = if imported {
            vec![("main.zry", main.as_str()), ("values.zry", VALUES)]
        } else {
            vec![("main.zry", main.as_str())]
        };
        test(&files, if imported { TWO } else { ONE });
    }
}

fn source_inputs(files: &[(&str, &str)]) -> Vec<SourceFileInput> {
    files
        .iter()
        .map(|(path, text)| SourceFileInput { path: (*path).into(), text: (*text).into() })
        .collect()
}

fn with_frozen_claim(
    files: &[(&str, &str)],
    bytes: &[u8],
    test: impl FnOnce(
        owned_v2::raw::Program,
        &zryna_syntax::v5::VerifiedProjectSyntaxV5,
        &SourceMap,
        &zryna_layout::generic_v1::VerifiedLayouts,
        &zryna_layout::generic_v1::VerifiedLayouts,
    ),
) {
    let sources = SourceMap::build(source_inputs(files)).expect("exact frozen sources");
    let syntax = verify_snapshot(decode_snapshot(bytes).expect("frozen v5 DTO"), &sources)
        .expect("complete source/arena authority, including comments");
    let entry = sources.verify_file_id(0).expect("entry");
    let declarations =
        resolve_declarations(SemanticInput::try_new(&syntax, &sources, entry).expect("input"))
            .expect("exact declarations");
    let bodies = check_body_types(&declarations).expect("original opaque bodies");
    let instances = super::discover(&bodies).expect("ownership and closed demand");
    let linear =
        verify_layouts(instances.instances(), StorageTarget::Linear32V1).expect("linear layouts");
    let linux =
        verify_layouts(instances.instances(), StorageTarget::LinuxX8664V1).expect("native layouts");
    let claim = super::produce_claim(&instances, &linear, &linux).expect("untrusted claim");
    test(claim, &syntax, &sources, &linear, &linux);
}

fn parallel_join(claim: &owned_v2::raw::Program) -> (usize, usize) {
    claim
        .graph
        .functions
        .iter()
        .enumerate()
        .find_map(|(fi, function)| {
            function.blocks.iter().enumerate().skip(1).find_map(|(bi, block)| {
                let parameters = &block.parameters;
                if parameters.len() != 4
                    || parameters[0].ty != parameters[1].ty
                    || parameters[2].ty != parameters[3].ty
                {
                    return None;
                }
                let key = |ty| match ty {
                    raw::Type::Stored(id) => &claim.graph.type_keys[id as usize][..],
                    _ => &[][..],
                };
                // Independently fixed complete canonical keys, not aggregate width equality.
                let option = [0x14, 1, 0, 0, 0, 1, 0, 0, 0, 1];
                let result = [0x15, 2, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0];
                (key(parameters[0].ty) == option.as_slice()
                    && key(parameters[2].ty) == result.as_slice())
                .then_some((fi, bi))
            })
        })
        .expect("four original concrete Option/Result join parameters")
}

fn function_named<'a>(
    claim: &'a owned_v2::raw::Program,
    files: &[(&str, &str)],
    head: &str,
) -> &'a raw::Function {
    claim
        .graph
        .functions
        .iter()
        .find(|function| {
            let span = function.span;
            files[span.file as usize].1[span.start as usize..span.end as usize].contains(head)
        })
        .expect("authenticated original template span")
}

#[test]
fn frozen_copy_enum_phi_seals_exact_single_and_imported_nine_exports() {
    forms(|files, bytes| {
        with_frozen_claim(files, bytes, |claim, syntax, sources, linear, linux| {
            let runtime = zryna_ownership_runtime_abi::generic_v1::verify_v1(
                zryna_ownership_runtime_abi::generic_v1::raw_v1(linear, linux)
                    .expect("runtime declarations"),
                linear,
                linux,
            )
            .expect("runtime authority");
            let check = |candidate: &owned_v2::raw::Program| {
                let bytes = owned_v2::wire::encode(candidate)?;
                let decoded = owned_v2::wire::decode(&bytes)?;
                owned_v2::verify(
                    decoded,
                    syntax,
                    sources,
                    sources.verify_file_id(0).expect("entry"),
                    linear,
                    linux,
                    &runtime,
                )
                .map(|seal| seal.scalar_abi().exports().len())
            };
            assert_eq!(check(&claim).expect("source-bound owned seal"), 9);
            let mut exports: Vec<_> = claim
                .graph
                .functions
                .iter()
                .filter_map(|function| function.public_export.as_deref())
                .collect();
            exports.sort_unstable();
            assert_eq!(
                exports,
                [
                    "bothReturn",
                    "optionEarly",
                    "optionNested",
                    "optionSingle",
                    "optionVariants",
                    "parallel",
                    "resultRepeated",
                    "resultSingle",
                    "resultVariants"
                ]
            );
            let (fi, bi) = parallel_join(&claim);
            let target = claim.graph.functions[fi].blocks[bi].id;
            let edges: Vec<_> = claim.graph.functions[fi]
                .blocks
                .iter()
                .filter_map(|block| match &block.terminator {
                    raw::Terminator::Jump(edge) if edge.target == target => Some(edge),
                    _ => None,
                })
                .collect();
            assert_eq!(edges.len(), 2);
            assert!(edges.iter().all(|edge| edge.arguments.len() == 4));
            let terminal = function_named(&claim, files, "bothReturnSelect<T");
            assert!(
                terminal.blocks.iter().skip(1).all(|block| block.parameters.is_empty()),
                "two terminal arms contribute no unreachable join parameters"
            );
            let early = function_named(&claim, files, "optionEarlySelect<T");
            let join = early
                .blocks
                .iter()
                .skip(1)
                .find(|block| !block.parameters.is_empty())
                .expect("one continuing Option replacement");
            assert_eq!(join.parameters.len(), 1);
            assert_eq!(
                early
                    .blocks
                    .iter()
                    .filter(|block| matches!(&block.terminator,
                raw::Terminator::Jump(edge) if edge.target == join.id))
                    .count(),
                1
            );
            assert!(
                claim
                    .plans
                    .iter()
                    .flat_map(|plan| &plan.steps)
                    .any(|step| step.failure && step.cleanup.len() >= 2)
            );
            assert!(
                claim
                    .plans
                    .iter()
                    .flat_map(|plan| &plan.steps)
                    .all(|step| step.end_loans.is_empty())
            );
            attacks::source_bound_attacks(&claim, fi, bi, &check);
            assert_eq!(check(&claim).expect("final pristine recovery"), 9);
        });
    });
}

//! Synthetic physical-layout and local amplification bounds, not source admission claims.

use super::{MAX_FUNCTION_LOCALS, MAX_TYPE_LANES, add, width};
use zryna_ir::generic_v1::raw::Type;
use zryna_layout::{
    StorageTarget,
    generic_v1::{raw, verify},
    raw::TypeKind as Base,
};
use zryna_source::{SourceFileInput, SourceMap};

#[test]
fn scalar_lane_exact_maximum_first_extra_and_empty_arrays() {
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: String::new() }])
            .expect("synthetic source identity");
    for (length, expected) in [
        (0, Some(0)),
        (u64::from(MAX_TYPE_LANES), Some(MAX_TYPE_LANES)),
        (u64::from(MAX_TYPE_LANES) + 1, None),
    ] {
        let graph = raw::Graph {
            modules: vec![raw::Module {
                id: raw::ModuleId(0),
                source_file: sources.verify_file_id(0).expect("file"),
                data_declarations: 0,
            }],
            declarations: vec![],
            types: [
                Base::Bool,
                Base::I32,
                Base::String,
                Base::FixedArray { element: raw::NodeId(1), length },
            ]
            .into_iter()
            .enumerate()
            .map(|(id, kind)| raw::TypeNode {
                id: raw::NodeId(u32::try_from(id).expect("small graph")),
                span: None,
                kind: raw::TypeKind::Base(kind),
            })
            .collect(),
            program_roots: vec![raw::NodeId(3)],
        };
        let layouts = verify(&graph, &sources, StorageTarget::Linear32V1)
            .expect("independent production physical layout");
        let types = layouts.types().collect::<Vec<_>>();
        let array = types
            .iter()
            .position(|ty| ty.category() == zryna_layout::TypeCategory::FixedArray)
            .expect("canonical array id");
        let result = width(
            Type::Stored(u32::try_from(array).expect("index")),
            &types,
            &mut vec![None; types.len()],
            0,
        );
        if let Some(expected) = expected {
            assert_eq!(result.expect("exact bound"), expected);
        } else {
            assert_eq!(result.expect_err("first extra scalar lane").code, "ZRYNA-W4001");
        }
    }
}

#[test]
fn function_local_exact_maximum_and_first_extra_reject_before_allocation() {
    assert_eq!(add(MAX_FUNCTION_LOCALS - 1, 1).expect("exact bound"), MAX_FUNCTION_LOCALS);
    assert_eq!(add(MAX_FUNCTION_LOCALS, 1).expect_err("first extra local").code, "ZRYNA-W4001");
    assert_eq!(add(u32::MAX, 1).expect_err("overflow").code, "ZRYNA-W4001");
}

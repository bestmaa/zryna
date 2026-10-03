//! Synthetic physical-layout and local amplification bounds, not source admission claims.

use super::{MAX_TYPE_LANES, width};
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
        let layouts = verify(&graph, &sources, StorageTarget::LinuxX8664V1)
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
            assert_eq!(result.expect_err("first extra scalar lane").code, "ZRYNA-N7001");
        }
    }
}

#[test]
fn complete_native_parameter_and_value_shapes_have_exact_and_extra_bounds() {
    use zryna_ir::generic_v1::raw as ir;
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: String::new() }])
            .expect("synthetic source brand");
    let graph = raw::Graph {
        modules: vec![raw::Module {
            id: raw::ModuleId(0),
            source_file: sources.verify_file_id(0).expect("file"),
            data_declarations: 0,
        }],
        declarations: vec![],
        types: [Base::Bool, Base::I32, Base::String]
            .into_iter()
            .enumerate()
            .map(|(id, kind)| raw::TypeNode {
                id: raw::NodeId(u32::try_from(id).expect("small graph")),
                span: None,
                kind: raw::TypeKind::Base(kind),
            })
            .collect(),
        program_roots: vec![raw::NodeId(1)],
    };
    let layouts = verify(&graph, &sources, StorageTarget::LinuxX8664V1).expect("physical issuer");
    let types = layouts.types().collect::<Vec<_>>();
    let span = zryna_source::UntrustedSpan { file: 0, start: 0, end: 0 };
    let mut f = ir::Function {
        key: vec![],
        span,
        public_export: None,
        parameters: vec![Type::Stored(1); 255],
        result: Type::Stored(1),
        blocks: vec![ir::Block {
            id: 0,
            parameters: vec![],
            instructions: vec![],
            span,
            terminator: ir::Terminator::Return(0),
        }],
    };
    super::plan::function(0, &f, &types, &mut [None; 3], &mut 0)
        .expect("255 i32 carriers plus result pointer");
    f.parameters.push(Type::Stored(1));
    assert_eq!(
        super::plan::function(0, &f, &types, &mut [None; 3], &mut 0)
            .expect_err("first extra parameter")
            .code,
        "ZRYNA-N7001"
    );
    f.parameters.clear();
    f.blocks[0].parameters =
        (0..super::MAX_VALUE_LANES).map(|id| ir::Definition { id, ty: Type::Stored(1) }).collect();
    super::plan::function(0, &f, &types, &mut [None; 3], &mut 0)
        .expect("complete exact lane arena");
    f.blocks[0].parameters.push(ir::Definition { id: super::MAX_VALUE_LANES, ty: Type::Stored(1) });
    assert_eq!(
        super::plan::function(0, &f, &types, &mut [None; 3], &mut 0)
            .expect_err("first extra complete value lane")
            .code,
        "ZRYNA-N7001"
    );
}

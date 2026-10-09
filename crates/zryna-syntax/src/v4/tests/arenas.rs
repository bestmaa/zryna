use super::*;
use crate::v4::verify::{
    canonical_i32, canonical_u32, expression_edge, is_place, own_expression_root, own_type,
    type_edge,
};

#[test]
fn top_level_arrays_allow_source_interleaving_but_reject_duplicate_names() {
    const INTERLEAVED: &str = "interface A extends ZrynaStruct { x: i32; }\nfunction one(): i32 { return 1; }\ninterface B extends ZrynaStruct { y: i32; }";
    let mut snapshot = RawProjectSyntaxSnapshot {
        schema_version: 4,
        diagnostics: vec![],
        files: vec![RawSourceUnit {
            id: 0,
            path: "src/main.zry".into(),
            imports: vec![],
            type_syntax: vec![
                named_type(INTERLEAVED, 0),
                named_type(INTERLEAVED, 1),
                named_type(INTERLEAVED, 2),
            ],
            data_declarations: vec![
                struct_decl(INTERLEAVED, "A", "x", 0, 0),
                struct_decl(INTERLEAVED, "B", "y", 1, 2),
            ],
            functions: vec![function_for(INTERLEAVED, 1)],
        }],
    };
    let authority = sources(INTERLEAVED);
    assert!(verify_snapshot(snapshot.clone(), &authority).is_ok());
    if let RawDataDeclarationKind::Struct { name, .. } =
        &mut snapshot.files[0].data_declarations[1].kind
    {
        name.text = "A".into();
    }
    assert!(verify_snapshot(snapshot, &authority).is_err());
}

#[test]
fn type_arena_rejects_orphans_forward_edges_sharing_and_depth() {
    let authority = sources(SOURCE);
    let mut orphan = raw();
    orphan.files[0]
        .type_syntax
        .push(RawTypeSyntax { span: span_range(0, 0), kind: RawTypeSyntaxKind::Missing });
    assert!(verify_snapshot(orphan, &authority).is_err());
    let mut owners = vec![0; 2];
    let mut depths = vec![1; 2];
    let path = NormalizedSourcePath::new("src/main.zry").unwrap();
    let mut errors = Errors::default();
    type_edge(1, 0, &mut owners, &mut depths, &path, &mut errors);
    assert!(!errors.items.is_empty());
    let mut owner = [0];
    own_type(0, &mut owner, &path, &mut errors);
    own_type(0, &mut owner, &path, &mut errors);
    assert_eq!(owner[0], 2);
    let mut chain_depth = vec![1; 130];
    let mut chain_owners = vec![0; 130];
    for parent in 1..130 {
        type_edge(
            u32::try_from(parent - 1).unwrap(),
            parent,
            &mut chain_owners,
            &mut chain_depth,
            &path,
            &mut errors,
        );
    }
    assert!(chain_depth[129] > MAX_NESTING_DEPTH);
}

#[test]
fn graph_ownership_and_place_checks_are_non_recursive() {
    let reference = RawExpressionSyntax {
        span: span_range(0, 0),
        kind: RawExpressionKind::Reference {
            name: RawIdentifierSyntax { text: "x".into(), span: span_range(0, 0) },
        },
    };
    let literal = RawExpressionSyntax {
        span: span_range(0, 0),
        kind: RawExpressionKind::I32Literal { spelling: "1".into() },
    };
    assert!(is_place(std::slice::from_ref(&reference), 0));
    assert!(!is_place(std::slice::from_ref(&literal), 0));
    let mut owners = [0];
    let mut depths = [1];
    assert!(expression_edge(0, 0, &mut owners, &mut depths).is_err());
    let mut roots = [0];
    assert!(own_expression_root(0, 1, &mut roots));
    assert!(own_expression_root(0, 1, &mut roots));
    assert_eq!(roots[0], 2);
}

#[test]
fn fixed_array_length_has_exact_profile_boundary() {
    assert!(canonical_u32("0"));
    assert!(canonical_u32("1048576"));
    assert!(!canonical_u32("1048577"));
    assert!(!canonical_u32("01"));
    assert!(!canonical_u32("4294967295"));
}

#[test]
fn integer_spelling_is_syntax_only() {
    assert!(canonical_i32("2147483648"));
    assert!(canonical_i32("-2147483649"));
    assert!(!canonical_i32("01"));
    assert!(!canonical_i32("-0"));
}

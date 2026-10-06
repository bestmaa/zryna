//! Literal retained-root identity and reverse failure cleanup order.
use super::*;
/// Two retained roots make failure cleanup order observable independently of graph effects.
pub(super) fn two_root_fixture() -> (raw::Program, VerifiedLayouts, VerifiedLayouts, SourceMap) {
    let (linear, linux, sources) = authorities();
    let blocks = vec![
        block(
            0,
            vec![stored(0, 2), stored(1, 2), stored(2, 0)],
            vec![
                int(3, 13),
                construct(4, OPTION, 1, Some(3)),
                boolean(5, false),
                construct(6, RESULT, 1, Some(5)),
            ],
            graph::Terminator::Branch { condition: 2, yes: e(1, &[]), no: e(2, &[]) },
        ),
        block(
            1,
            vec![],
            vec![
                placeholder(7, graph::Type::Stored(2)),
                placeholder(8, graph::Type::Unit),
                construct(9, OPTION, 0, None),
                boolean(10, true),
                construct(11, RESULT, 1, Some(10)),
            ],
            jump(3, &[9, 11]),
        ),
        block(
            2,
            vec![],
            vec![placeholder(12, graph::Type::Stored(2)), placeholder(13, graph::Type::Unit)],
            jump(3, &[4, 6]),
        ),
        block(
            3,
            vec![stored(14, OPTION), stored(15, RESULT)],
            vec![
                placeholder(16, graph::Type::Unit),
                placeholder(17, graph::Type::Unit),
                int(18, 7),
            ],
            graph::Terminator::Return(18),
        ),
    ];
    let extensions = vec![
        raw::Extension { result: 7, operation: raw::Operation::StringLiteral("二".into()) },
        raw::Extension { result: 8, operation: raw::Operation::Drop(7) },
        raw::Extension { result: 12, operation: raw::Operation::StringLiteral(Vec::new()) },
        raw::Extension { result: 13, operation: raw::Operation::Drop(12) },
        raw::Extension { result: 16, operation: raw::Operation::Drop(1) },
        raw::Extension { result: 17, operation: raw::Operation::Drop(0) },
    ];
    let mut p = claim(blocks, extensions, &linear, &linux);
    p.graph.functions[0].parameters =
        vec![graph::Type::Stored(2), graph::Type::Stored(2), graph::Type::Stored(0)];
    (p, linear, linux, sources)
}

#[test]
fn independent_copy_enum_join_retains_exact_multiple_roots_and_reverse_failure_order() {
    let (mut p, linear, linux, sources) = two_root_fixture();
    typed(&p, &linear, &linux, &sources).expect("two retained roots; no owner phi");
    let expected = vec![raw::Plan {
        steps: vec![
            raw::Step {
                block: 1,
                position: 0,
                failure: true,
                end_loans: vec![],
                cleanup: vec![1, 0],
            },
            raw::Step {
                block: 2,
                position: 0,
                failure: true,
                end_loans: vec![],
                cleanup: vec![1, 0],
            },
            raw::Step { block: 3, position: 3, failure: false, end_loans: vec![], cleanup: vec![] },
        ],
    }];
    for layouts in [&linear, &linux] {
        assert_eq!(
            derive(&p.graph, &p.extensions, layouts).expect("literal reverse creation order"),
            expected
        );
    }
    p.plans = expected.clone();
    let mut hostile = p.clone();
    hostile.plans[0].steps[0].cleanup.swap(0, 1);
    let bytes =
        super::super::super::wire::encode(&hostile).expect("wrong cleanup order remains raw wire");
    let decoded =
        super::super::super::wire::decode(&bytes).expect("raw wire grants no ownership seal");
    assert_ne!(
        derive(&decoded.claims().graph, &decoded.claims().extensions, &linear)
            .expect("derive exact multiple root cleanup"),
        decoded.claims().plans
    );
    assert_eq!(
        derive(&p.graph, &p.extensions, &linear).expect("fresh pristine multi-root recovery"),
        expected
    );
}

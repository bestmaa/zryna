//! Independent two-enum join with exact active matches and literal owner plans.
use super::*;
/// Literal first graph: true=(None,err(true)) -> 101+41=142;
/// false unchanged=(Some(13),ok(29)) -> 13+29=42. One root is retained.
pub(super) fn fixture() -> (raw::Program, VerifiedLayouts, VerifiedLayouts, SourceMap) {
    let (linear, linux, sources) = authorities();
    let blocks = vec![
        block(
            0,
            vec![stored(0, 2), stored(1, 0)],
            vec![
                int(2, 13),
                construct(3, OPTION, 1, Some(2)),
                int(4, 29),
                construct(5, RESULT, 0, Some(4)),
            ],
            graph::Terminator::Branch { condition: 1, yes: e(1, &[]), no: e(2, &[]) },
        ),
        block(
            1,
            vec![],
            vec![
                placeholder(6, graph::Type::Stored(2)),
                construct(7, OPTION, 0, None),
                boolean(8, true),
                construct(9, RESULT, 1, Some(8)),
                placeholder(10, graph::Type::Unit),
            ],
            jump(3, &[7, 9]),
        ),
        block(
            2,
            vec![],
            vec![placeholder(11, graph::Type::Stored(2)), placeholder(12, graph::Type::Unit)],
            jump(3, &[3, 5]),
        ),
        block(
            3,
            vec![stored(13, OPTION), stored(14, RESULT)],
            vec![],
            selection(OPTION, 13, vec![arm(0, None, 4, &[]), arm(1, Some(stored(16, 1)), 5, &[])]),
        ),
        block(4, vec![], vec![int(15, 101)], jump(6, &[15])),
        block(5, vec![stored(16, 1)], vec![], jump(6, &[16])),
        block(
            6,
            vec![stored(17, 1)],
            vec![],
            selection(
                RESULT,
                14,
                vec![arm(0, Some(stored(18, 1)), 7, &[17]), arm(1, Some(stored(20, 0)), 8, &[17])],
            ),
        ),
        block(7, vec![stored(18, 1), stored(19, 1)], vec![], jump(11, &[19, 18])),
        block(
            8,
            vec![stored(20, 0), stored(21, 1)],
            vec![],
            graph::Terminator::Branch { condition: 20, yes: e(9, &[]), no: e(10, &[]) },
        ),
        block(9, vec![], vec![int(22, 41)], jump(11, &[21, 22])),
        block(10, vec![], vec![int(23, 43)], jump(11, &[21, 23])),
        block(
            11,
            vec![stored(24, 1), stored(25, 1)],
            vec![
                ins(26, graph::Type::Stored(1), graph::Operation::I32Add { left: 24, right: 25 }),
                placeholder(27, graph::Type::Unit),
            ],
            graph::Terminator::Return(26),
        ),
    ];
    let extensions = vec![
        raw::Extension { result: 6, operation: raw::Operation::StringLiteral("左".into()) },
        raw::Extension { result: 10, operation: raw::Operation::Drop(6) },
        raw::Extension { result: 11, operation: raw::Operation::StringLiteral(Vec::new()) },
        raw::Extension { result: 12, operation: raw::Operation::Drop(11) },
        raw::Extension { result: 27, operation: raw::Operation::Drop(0) },
    ];
    (claim(blocks, extensions, &linear, &linux), linear, linux, sources)
}

pub(super) fn expected_plan() -> Vec<raw::Plan> {
    vec![raw::Plan {
        steps: vec![
            raw::Step { block: 1, position: 0, failure: true, end_loans: vec![], cleanup: vec![0] },
            raw::Step { block: 2, position: 0, failure: true, end_loans: vec![], cleanup: vec![0] },
            raw::Step {
                block: 11,
                position: 2,
                failure: false,
                end_loans: vec![],
                cleanup: vec![],
            },
        ],
    }]
}

#[test]
fn independent_copy_enum_join_exact_types_active_payloads_and_literal_cleanup() {
    let (mut p, linear, linux, sources) = fixture();
    typed(&p, &linear, &linux, &sources).expect("independent dual typed authority");
    let expected = expected_plan();
    for layouts in [&linear, &linux] {
        assert_eq!(
            derive(&p.graph, &p.extensions, layouts)
                .expect("root retained; local cleanup before join"),
            expected
        );
    }
    assert_eq!(observe(&p, true), 142);
    assert_eq!(observe(&p, false), 42);
    p.plans = expected;
    let bytes = super::super::super::wire::encode(&p).expect("raw v2 encode grants no authority");
    let decoded = super::super::super::wire::decode(&bytes).expect("independent raw decode");
    assert_eq!(decoded.claims(), &p);
    typed(decoded.claims(), &linear, &linux, &sources).expect("typed raw roundtrip");
    assert_eq!(
        derive(&decoded.claims().graph, &decoded.claims().extensions, &linear)
            .expect("decoded plan derivation"),
        decoded.claims().plans
    );
}

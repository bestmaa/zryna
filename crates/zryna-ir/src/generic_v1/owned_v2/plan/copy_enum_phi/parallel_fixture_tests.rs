//! Independent four-place parallel transport and all listed pair permutations.
use super::*;
/// Literal full four-place parallel transport, including actual exhaustive observations.
pub(super) fn parallel_fixture() -> (raw::Program, VerifiedLayouts, VerifiedLayouts, SourceMap) {
    let (linear, linux, sources) = authorities();
    let mut blocks = vec![
        block(
            0,
            vec![stored(0, 2), stored(1, 0)],
            vec![
                int(2, 71),
                construct(3, OPTION, 1, Some(2)),
                int(4, 73),
                construct(5, OPTION, 1, Some(4)),
                int(6, 79),
                construct(7, RESULT, 0, Some(6)),
                boolean(8, false),
                construct(9, RESULT, 1, Some(8)),
            ],
            graph::Terminator::Branch { condition: 1, yes: e(1, &[]), no: e(2, &[]) },
        ),
        block(
            1,
            vec![],
            vec![placeholder(10, graph::Type::Stored(2)), placeholder(11, graph::Type::Unit)],
            jump(3, &[5, 3, 9, 7]),
        ),
        block(
            2,
            vec![],
            vec![
                placeholder(12, graph::Type::Stored(2)),
                placeholder(13, graph::Type::Unit),
                construct(14, OPTION, 0, None),
                boolean(15, true),
                construct(16, RESULT, 1, Some(15)),
            ],
            jump(3, &[14, 5, 7, 16]),
        ),
    ];
    blocks.extend(option_observation_blocks());
    blocks.extend(first_result_observation_blocks());
    blocks.extend(second_result_and_score_blocks());
    let extensions = vec![
        raw::Extension { result: 10, operation: raw::Operation::StringLiteral("並".into()) },
        raw::Extension { result: 11, operation: raw::Operation::Drop(10) },
        raw::Extension { result: 12, operation: raw::Operation::StringLiteral(Vec::new()) },
        raw::Extension { result: 13, operation: raw::Operation::Drop(12) },
        raw::Extension { result: 64, operation: raw::Operation::Drop(0) },
    ];
    (claim(blocks, extensions, &linear, &linux), linear, linux, sources)
}

#[test]
fn independent_four_place_parallel_copy_transport_has_position_sensitive_observations() {
    let (p, linear, linux, sources) = parallel_fixture();
    typed(&p, &linear, &linux, &sources).expect("four concrete enum parameters");
    let expected = vec![raw::Plan {
        steps: vec![
            raw::Step { block: 1, position: 0, failure: true, end_loans: vec![], cleanup: vec![0] },
            raw::Step { block: 2, position: 0, failure: true, end_loans: vec![], cleanup: vec![0] },
            raw::Step {
                block: 19,
                position: 10,
                failure: false,
                end_loans: vec![],
                cleanup: vec![],
            },
        ],
    }];
    for layouts in [&linear, &linux] {
        assert_eq!(
            derive(&p.graph, &p.extensions, layouts).expect("literal parallel plan"),
            expected
        );
    }
    // Complete 2!*2! same-type pair permutations, not arbitrary mutation injectivity.
    for (flag, scores, bi) in [(true, [660, 662, 624, 626], 1), (false, [648, 680, 686, 718], 2)] {
        for (permutation, score) in scores.into_iter().enumerate() {
            let mut q = p.clone();
            let graph::Terminator::Jump(edge) = &mut q.graph.functions[0].blocks[bi].terminator
            else {
                panic!("continuing edge")
            };
            if permutation & 1 != 0 {
                edge.arguments.swap(0, 1);
            }
            if permutation & 2 != 0 {
                edge.arguments.swap(2, 3);
            }
            typed(&q, &linear, &linux, &sources).expect("same-type permutations are type-valid");
            assert_eq!(
                derive(&q.graph, &q.extensions, &linear)
                    .expect("Copy permutation preserves root state"),
                expected
            );
            assert_eq!(observe(&q, flag), score);
        }
    }
}

fn option_observation_blocks() -> Vec<graph::Block> {
    vec![
        block(
            3,
            vec![stored(17, OPTION), stored(18, OPTION), stored(19, RESULT), stored(20, RESULT)],
            vec![],
            selection(OPTION, 17, vec![arm(0, None, 4, &[]), arm(1, Some(stored(22, 1)), 5, &[])]),
        ),
        block(4, vec![], vec![int(21, 101)], jump(6, &[21])),
        block(5, vec![stored(22, 1)], vec![], jump(6, &[22])),
        block(
            6,
            vec![stored(23, 1)],
            vec![],
            selection(
                OPTION,
                18,
                vec![arm(0, None, 7, &[23]), arm(1, Some(stored(26, 1)), 8, &[23])],
            ),
        ),
        block(7, vec![stored(24, 1)], vec![int(25, 103)], jump(9, &[24, 25])),
        block(8, vec![stored(26, 1), stored(27, 1)], vec![], jump(9, &[27, 26])),
    ]
}

fn first_result_observation_blocks() -> Vec<graph::Block> {
    vec![
        block(
            9,
            vec![stored(28, 1), stored(29, 1)],
            vec![],
            selection(
                RESULT,
                19,
                vec![
                    arm(0, Some(stored(30, 1)), 10, &[28, 29]),
                    arm(1, Some(stored(33, 0)), 11, &[28, 29]),
                ],
            ),
        ),
        block(
            10,
            vec![stored(30, 1), stored(31, 1), stored(32, 1)],
            vec![],
            jump(14, &[31, 32, 30]),
        ),
        block(
            11,
            vec![stored(33, 0), stored(34, 1), stored(35, 1)],
            vec![],
            graph::Terminator::Branch { condition: 33, yes: e(12, &[]), no: e(13, &[]) },
        ),
        block(12, vec![], vec![int(36, 41)], jump(14, &[34, 35, 36])),
        block(13, vec![], vec![int(37, 43)], jump(14, &[34, 35, 37])),
    ]
}

fn second_result_and_score_blocks() -> Vec<graph::Block> {
    vec![
        block(
            14,
            vec![stored(38, 1), stored(39, 1), stored(40, 1)],
            vec![],
            selection(
                RESULT,
                20,
                vec![
                    arm(0, Some(stored(41, 1)), 15, &[38, 39, 40]),
                    arm(1, Some(stored(45, 0)), 16, &[38, 39, 40]),
                ],
            ),
        ),
        block(
            15,
            vec![stored(41, 1), stored(42, 1), stored(43, 1), stored(44, 1)],
            vec![],
            jump(19, &[42, 43, 44, 41]),
        ),
        block(
            16,
            vec![stored(45, 0), stored(46, 1), stored(47, 1), stored(48, 1)],
            vec![],
            graph::Terminator::Branch { condition: 45, yes: e(17, &[]), no: e(18, &[]) },
        ),
        block(17, vec![], vec![int(49, 41)], jump(19, &[46, 47, 48, 49])),
        block(18, vec![], vec![int(50, 43)], jump(19, &[46, 47, 48, 50])),
        block(
            19,
            vec![stored(51, 1), stored(52, 1), stored(53, 1), stored(54, 1)],
            vec![
                ins(55, graph::Type::Stored(1), graph::Operation::I32Add { left: 51, right: 52 }),
                ins(56, graph::Type::Stored(1), graph::Operation::I32Add { left: 55, right: 52 }),
                ins(57, graph::Type::Stored(1), graph::Operation::I32Add { left: 56, right: 53 }),
                ins(58, graph::Type::Stored(1), graph::Operation::I32Add { left: 57, right: 53 }),
                ins(59, graph::Type::Stored(1), graph::Operation::I32Add { left: 58, right: 53 }),
                ins(60, graph::Type::Stored(1), graph::Operation::I32Add { left: 59, right: 54 }),
                ins(61, graph::Type::Stored(1), graph::Operation::I32Add { left: 60, right: 54 }),
                ins(62, graph::Type::Stored(1), graph::Operation::I32Add { left: 61, right: 54 }),
                ins(63, graph::Type::Stored(1), graph::Operation::I32Add { left: 62, right: 54 }),
                placeholder(64, graph::Type::Unit),
            ],
            graph::Terminator::Return(63),
        ),
    ]
}

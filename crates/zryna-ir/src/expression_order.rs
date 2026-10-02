use super::{ExprId, ExprKind, Function, VerificationErrors, push_expression_error};

pub(super) fn predecessor(id: ExprId, current: usize) -> Option<usize> {
    usize::try_from(id.0).ok().filter(|index| *index < current)
}

pub(super) fn verify_canonical_postorder(
    function_index: usize,
    function: &Function,
    body_index: usize,
    valid_spans: &[bool],
    errors: &mut VerificationErrors,
) {
    let mut emitted = vec![false; function.expressions.len()];
    let mut expected = 0_usize;
    let mut stack = vec![(body_index, false)];
    while let Some((index, exiting)) = stack.pop() {
        if exiting {
            if emitted[index] {
                continue;
            }
            if index != expected {
                push_expression_error(
                    errors,
                    valid_spans[index],
                    function.expressions[index].span,
                    "ZRYNA-I1008",
                    format!(
                        "function #{function_index} expression arena is not canonical postorder: expected #{expected}, found #{index}"
                    ),
                    "emit the one expression tree left-to-right in exact postorder",
                );
                return;
            }
            emitted[index] = true;
            expected = expected.saturating_add(1);
            continue;
        }
        if emitted[index] {
            continue;
        }
        stack.push((index, true));
        if let ExprKind::I32Add { lhs, rhs } = function.expressions[index].kind {
            let Some(left) = predecessor(lhs, index) else {
                return;
            };
            let Some(right) = predecessor(rhs, index) else {
                return;
            };
            stack.push((right, false));
            stack.push((left, false));
        }
    }
}

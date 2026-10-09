//! Exact instruction-step matching for clone call and failure observations.

use super::*;

pub(super) fn audit_helper_calls(steps: &[Step], shape: &Shape) -> Result<(), Diagnostic> {
    for step in steps {
        if let Step::Call(callee) = step
            && !(matches!(*callee, 0 | 1)
                || *callee >= 6 && *callee < 6 + shape.type_count * 2
                || *callee == shape.run + 1
                || *callee == shape.run + 2)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn step(operator: &Operator<'_>) -> Step {
    match operator {
        Operator::I32Const { value } => Step::Constant(*value),
        Operator::GlobalGet { global_index } => Step::Get(*global_index),
        Operator::GlobalSet { global_index } => Step::Set(*global_index),
        Operator::Call { function_index } => Step::Call(*function_index),
        Operator::LocalTee { local_index } => Step::Tee(*local_index),
        Operator::I32Eq => Step::Equal,
        Operator::If { blockty: BlockType::Empty } => Step::If,
        Operator::End => Step::End,
        Operator::Return => Step::Return,
        Operator::Br { relative_depth } => Step::Branch(*relative_depth),
        _ => Step::Other,
    }
}

pub(super) fn cover(covered: &mut [bool], start: usize, length: usize) -> Result<(), Diagnostic> {
    let range = covered.get_mut(start..start + length).ok_or_else(invalid)?;
    if range.iter().any(|covered| *covered) {
        return Err(invalid());
    }
    range.fill(true);
    Ok(())
}

pub(super) fn audit_calls(
    steps: &[Step],
    covered: &mut [bool],
    bindings: &[Callsite],
    count: u32,
) -> Result<(), Diagnostic> {
    let mut next = 0;
    for (at, step) in steps.iter().enumerate() {
        let Step::Call(helper) = step else {
            continue;
        };
        if !(*helper >= 6 && *helper < 6 + count) {
            continue;
        }
        let callsite = bindings.get(next).ok_or_else(invalid)?;
        if *helper != callsite.helper {
            return Err(invalid());
        }
        next += 1;
        let Some(binding) = callsite.binding else {
            continue;
        };
        let start = at.checked_sub(8).ok_or_else(invalid)?;
        let expected = [
            Step::Constant(binding.module),
            Step::Set(6),
            Step::Constant(binding.declaration),
            Step::Set(7),
            Step::Constant(binding.root),
            Step::Set(8),
            Step::Constant(1),
            Step::Set(9),
            Step::Call(binding.helper),
            Step::Constant(0),
            Step::Set(9),
            Step::Get(1),
            Step::If,
            Step::Branch(1),
            Step::End,
        ];
        if steps.get(start..start + expected.len()) != Some(expected.as_slice()) {
            return Err(invalid());
        }
        cover(covered, start, expected.len())?;
    }
    if next != bindings.len() {
        return Err(invalid());
    }
    Ok(())
}

fn consume(recorder: u32) -> [Step; 13] {
    [
        Step::Get(9),
        Step::Constant(2),
        Step::Equal,
        Step::If,
        Step::Get(6),
        Step::Call(recorder),
        Step::Get(7),
        Step::Call(recorder),
        Step::Get(8),
        Step::Call(recorder),
        Step::End,
        Step::Constant(0),
        Step::Set(9),
    ]
}

pub(super) fn audit_helper(
    steps: &[Step],
    covered: &mut [bool],
    checks: usize,
    acquired: bool,
    recorder: u32,
) -> Result<(), Diagnostic> {
    let failure = consume(recorder);
    let acquisition = [
        Step::Get(9),
        Step::Constant(1),
        Step::Equal,
        Step::If,
        Step::Constant(2),
        Step::Set(9),
        Step::End,
    ];
    let mut failures = 0;
    let mut acquisitions = 0;
    for (at, step) in steps.iter().enumerate() {
        if *step != Step::Get(9) {
            continue;
        }
        if steps.get(at..at + failure.len()) == Some(failure.as_slice()) {
            let branch = at >= 2 && steps[at - 2..at] == [Step::Get(1), Step::If];
            let overflow = at >= 4
                && steps[at - 4..at] == [Step::Tee(1), Step::Constant(-1), Step::Equal, Step::If]
                && steps.get(at + failure.len()..at + failure.len() + 5)
                    == Some(
                        [
                            Step::Constant(4),
                            Step::Set(1),
                            Step::Constant(0),
                            Step::Return,
                            Step::End,
                        ]
                        .as_slice(),
                    );
            if !branch && !overflow {
                return Err(invalid());
            }
            cover(covered, at, failure.len())?;
            failures += 1;
        } else if steps.get(at..at + acquisition.len()) == Some(acquisition.as_slice()) {
            // Allocation failure returns before acquisition. The 13-step failure label
            // block is inside its status branch, independently fixed above.
            let before = at.checked_sub(19).ok_or_else(invalid)?;
            if steps.get(before..before + 3)
                != Some([Step::Call(0), Step::Get(1), Step::If].as_slice())
                || steps.get(before + 3..before + 16) != Some(failure.as_slice())
                || steps.get(before + 16..at)
                    != Some([Step::Constant(0), Step::Return, Step::End].as_slice())
                || steps.get(at + acquisition.len()) != Some(&Step::Tee(2))
            {
                return Err(invalid());
            }
            cover(covered, at, acquisition.len())?;
            acquisitions += 1;
        } else {
            return Err(invalid());
        }
    }
    for (at, pair) in steps.windows(2).enumerate() {
        if pair == [Step::Get(1), Step::If]
            && steps.get(at + 2..at + 2 + failure.len()) != Some(failure.as_slice())
        {
            return Err(invalid());
        }
    }
    if failures != checks || acquisitions != usize::from(acquired) {
        return Err(invalid());
    }
    Ok(())
}

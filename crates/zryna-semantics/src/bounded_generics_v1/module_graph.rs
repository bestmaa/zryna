use super::SemanticInput;
use super::diagnostics::Errors;
use super::imports::ImportFacts;

pub(super) fn validate(
    input: SemanticInput<'_>,
    imports: &[Vec<ImportFacts<'_>>],
    errors: &mut Errors,
) {
    let entry = input.entry().index() as usize;
    let mut state = vec![0_u8; imports.len()];
    let mut reachable = vec![false; imports.len()];
    let mut stack = vec![(entry, 0_usize)];
    state[entry] = 1;
    reachable[entry] = true;
    while let Some((module, next)) = stack.last_mut() {
        if *next == imports[*module].len() {
            state[*module] = 2;
            stack.pop();
            continue;
        }
        let edge = &imports[*module][*next];
        *next += 1;
        let Some(target) = edge.target_module else { continue };
        reachable[target] = true;
        match state[target] {
            0 => {
                state[target] = 1;
                stack.push((target, 0));
            }
            1 => errors.at(
                "ZRYNA-M3016",
                edge.span,
                "the authenticated module import graph contains a cycle",
                "remove the closing relative import edge",
            ),
            _ => {}
        }
    }
    for (module, reached) in reachable.into_iter().enumerate() {
        if !reached {
            errors.global(
                "ZRYNA-M3016",
                format!(
                    "original source module '{}' is unreachable from the selected entry",
                    input.syntax().files()[module].path
                ),
                "pass exactly the complete selected-entry module closure",
                vec![module],
            );
        }
    }
}

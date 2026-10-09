//! Independent ordered environment bridge audit against whole verified command authority.

mod normalize;
mod reference;

#[cfg(test)]
mod tests;

use wasmparser::{FunctionBody, ValType};
use zryna_diagnostics::Diagnostic;
use zryna_ir::command_h1_v1::VerifiedProgram;

pub(super) fn audit(body: &FunctionBody<'_>, program: &VerifiedProgram) -> Result<(), Diagnostic> {
    let locals = body
        .get_locals_reader()
        .map_err(|_| invalid())?
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid())?;
    if locals != [(12, ValType::I32)] {
        return Err(invalid());
    }
    let expected = reference::derive(program)?;
    let mut expected = expected.iter();
    let mut actual = body.get_operators_reader().map_err(|_| invalid())?;
    while !actual.eof() {
        let operator = normalize::operator(&actual.read().map_err(|_| invalid())?)?;
        if expected.next().map(String::as_str) != Some(operator.as_str()) {
            return Err(invalid());
        }
    }
    if expected.next().is_some() {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4103",
        None,
        "Command environment bridge differs from its independently verified instruction contract.",
        "Audit the complete ordered bridge against the whole verified command and result layout.",
    )
}

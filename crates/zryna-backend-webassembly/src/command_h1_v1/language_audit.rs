//! Closed command-core capabilities bound to the whole verified language body.

use wasmparser::{Encoding, ExternalKind, Operator, Parser, Payload, Validator, WasmFeatures};
use zryna_diagnostics::Diagnostic;
use zryna_ir::command_h1_v1::VerifiedProgram;

use super::run_audit::{self, TrapSite};
mod clone_protocol;
mod declarations;

pub(super) fn audit(bytes: &[u8], program: &VerifiedProgram) -> Result<Vec<TrapSite>, Diagnostic> {
    if bytes.len() > 1_048_576 {
        return Err(invalid());
    }
    Validator::new_with_features(WasmFeatures::WASM1).validate_all(bytes).map_err(|_| invalid())?;
    let shape = declarations::Shape::derive(program)?;
    let protocol = clone_protocol::Protocol::derive(program, &shape)?;
    let mut seen = [false; 5];
    let mut bodies = 0_u32;
    let mut traps = None;
    let mut environment_calls = 0_usize;
    let mut private_globals = false;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|_| invalid())? {
            Payload::Version { encoding: Encoding::Module, .. } | Payload::End(_) => {}
            Payload::TypeSection(types) if !seen[0] => {
                seen[0] = true;
                shape.types(types)?;
            }
            Payload::ImportSection(imports) if !seen[1] => {
                seen[1] = true;
                shape.imports(imports)?;
            }
            Payload::FunctionSection(functions) if !seen[2] => {
                seen[2] = true;
                let declarations =
                    functions.into_iter().collect::<Result<Vec<_>, _>>().map_err(|_| invalid())?;
                if declarations != shape.declarations {
                    return Err(invalid());
                }
            }
            Payload::GlobalSection(globals) if protocol.enabled() && !private_globals => {
                private_globals = true;
                declarations::Shape::globals(globals)?;
            }
            Payload::ExportSection(exports) if !seen[3] && exports.count() == 2 => {
                seen[3] = true;
                for (actual, (name, index)) in exports
                    .into_iter()
                    .zip([("run", shape.run), ("$zryna$observation", shape.run + 1)])
                {
                    let actual = actual.map_err(|_| invalid())?;
                    if actual.kind != ExternalKind::Func
                        || actual.name != name
                        || actual.index != index
                    {
                        return Err(invalid());
                    }
                }
            }
            Payload::CodeSectionStart { count, .. }
                if !seen[4] && usize::try_from(count).ok() == Some(shape.declarations.len()) =>
            {
                seen[4] = true;
            }
            Payload::CodeSectionEntry(body) if seen[4] => {
                let index = 6_u32.checked_add(bodies).ok_or_else(invalid)?;
                protocol.audit(&body, index, &shape)?;
                if index == shape.run {
                    traps = Some(run_audit::audit(&body, index, shape.main, protocol.enabled())?);
                }
                if shape.environment == Some(index) {
                    super::environment_audit::audit(&body, program)?;
                }
                let mut operators = body.get_operators_reader().map_err(|_| invalid())?;
                while !operators.eof() {
                    let operator = operators.read().map_err(|_| invalid())?;
                    if matches!(operator, Operator::Call { function_index } if shape.environment == Some(function_index))
                    {
                        if shape.environment_caller != Some(index) {
                            return Err(invalid());
                        }
                        environment_calls += 1;
                    }
                    check_operator(&operator, index, &shape, protocol.enabled())?;
                }
                bodies = bodies.checked_add(1).ok_or_else(invalid)?;
            }
            _ => return Err(invalid()),
        }
    }
    if private_globals != protocol.enabled()
        || seen != [true; 5]
        || usize::try_from(bodies).ok() != Some(shape.declarations.len())
    {
        return Err(invalid());
    }
    if environment_calls != usize::from(shape.environment.is_some()) {
        return Err(invalid());
    }
    traps.ok_or_else(invalid)
}

fn check_operator(
    operator: &Operator<'_>,
    caller: u32,
    shape: &declarations::Shape,
    clone_frontier: bool,
) -> Result<(), Diagnostic> {
    match operator {
        Operator::Call { function_index } => {
            let allowed = match function_index {
                0 | 1 => true,
                2 | 3 | 5 => shape.environment == Some(caller),
                4 => shape.environment == Some(caller) || caller == shape.run,
                index => *index >= 6 && *index <= shape.run + 2,
            };
            if !allowed {
                return Err(invalid());
            }
        }
        Operator::GlobalGet { global_index } | Operator::GlobalSet { global_index } => {
            // The separate protocol audit binds every access to private globals 6..9.
            if *global_index > if clone_frontier { 9 } else { 5 } {
                return Err(invalid());
            }
        }
        Operator::I32Load { memarg } | Operator::I32Store { memarg } => {
            if memarg.memory != 0 || memarg.align > 2 || memarg.offset > 16_777_216 {
                return Err(invalid());
            }
        }
        Operator::I32Load8U { memarg } | Operator::I32Store8 { memarg } => {
            if memarg.memory != 0 || memarg.align != 0 || memarg.offset > 16_777_216 {
                return Err(invalid());
            }
        }
        _ if crate::data_ownership_v1::approved_operator(operator)
            || matches!(operator, Operator::I32LeU | Operator::I32Or) => {}
        _ => return Err(invalid()),
    }
    Ok(())
}

fn invalid() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4103",
        None,
        "Command core differs from its verified body or closed capability contract.",
        "Emit and independently audit the complete verified command core.",
    )
}

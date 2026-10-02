//! Trap coordinates are extracted only after the complete independent storage audit.

use wasmparser::{Operator, Parser, Payload};
use zryna_diagnostics::Diagnostic;

/// An audited canonical-transfer failure location, without runtime authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterfaceTrapSite {
    function: u32,
    offset: u64,
}

impl InterfaceTrapSite {
    /// Returns the storage core function index.
    #[must_use]
    pub const fn function_index(self) -> u32 {
        self.function
    }

    /// Returns its exact component-relative opcode offset.
    #[must_use]
    pub const fn module_offset(self) -> u64 {
        self.offset
    }

    /// Returns the fixed failure identity assigned by the independent storage audit.
    #[must_use]
    pub const fn identity(self) -> &'static str {
        "zryna.command.interface-violation.v1"
    }
}

pub(super) fn audit(
    storage: &[u8],
    component: &[u8],
) -> Result<Vec<InterfaceTrapSite>, Diagnostic> {
    super::storage::audit::audit(storage)?;
    // The caller has already authenticated the complete storage-first component graph.
    let base = Parser::new(0)
        .parse_all(component)
        .find_map(|payload| match payload {
            Ok(Payload::ModuleSection { unchecked_range, .. }) => Some(unchecked_range.start),
            _ => None,
        })
        .ok_or_else(invalid)?;
    let mut sites = Vec::new();
    let mut function = 0_u32;
    for payload in Parser::new(0).parse_all(storage) {
        if let Payload::CodeSectionEntry(body) = payload.map_err(|_| invalid())? {
            let mut operators = body.get_operators_reader().map_err(|_| invalid())?;
            while !operators.eof() {
                let (operator, offset) = operators.read_with_offset().map_err(|_| invalid())?;
                if matches!(operator, Operator::Unreachable) {
                    sites.push(InterfaceTrapSite {
                        function,
                        offset: base.checked_add(offset).ok_or_else(invalid)?,
                    });
                }
            }
            function = function.checked_add(1).ok_or_else(invalid)?;
        }
    }
    if function != 9 || sites.is_empty() {
        return Err(invalid());
    }
    Ok(sites)
}

fn invalid() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4104",
        None,
        "Command interface failure locations do not match the audited storage core.",
        "Retain the complete independently audited storage-first component graph.",
    )
}

//! Independent source requirement admission for the bounded H1 command.

use std::collections::BTreeSet;

use zryna_diagnostics::{Diagnostic, Severity};
use zryna_source::{SourceMap, Span};

use crate::v4::{ProjectSyntaxSnapshot, RawExpressionKind};

mod coverage;
mod identifiers;
mod outcome_types;

const INTRINSIC: &str = "environmentLookup";
const OUTCOME: &str = "EnvLookupV1";

/// One source-authenticated literal-key environment requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentRequirement {
    key: String,
    call: Span,
    function: usize,
    expression: usize,
}

impl EnvironmentRequirement {
    /// Returns the exact unescaped UTF-8 key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the call range in the issuing source map.
    #[must_use]
    pub const fn call_span(&self) -> Span {
        self.call
    }

    /// Returns the declaring function's canonical source index.
    #[must_use]
    pub const fn function_index(&self) -> usize {
        self.function
    }

    /// Returns the intrinsic expression's canonical syntax index.
    #[must_use]
    pub const fn expression_index(&self) -> usize {
        self.expression
    }
}

/// Retains authenticated v4 syntax and independently derived H1 requirements.
///
/// This is source admission only; it does not type-check, seal IR, approve a host grant,
/// or construct an executable command.
#[derive(Clone, Debug)]
pub struct CommandSyntax {
    syntax: ProjectSyntaxSnapshot,
    requirement: Option<EnvironmentRequirement>,
}

impl CommandSyntax {
    /// Whether this source uses the closed builtin outcome type or its producing operation.
    #[must_use]
    pub fn uses_outcome(&self) -> bool {
        self.requirement.is_some() || self.syntax.files().iter().any(|file| {
            file.type_syntax().iter().any(|ty| {
                matches!(&ty.kind, crate::v4::RawTypeSyntaxKind::Named { name } if name.text == OUTCOME)
            })
        })
    }

    /// Returns the retained authenticated v4 snapshot.
    #[must_use]
    pub const fn syntax(&self) -> &ProjectSyntaxSnapshot {
        &self.syntax
    }

    /// Returns the sole source requirement, or none for a pure command.
    #[must_use]
    pub const fn environment(&self) -> Option<&EnvironmentRequirement> {
        self.requirement.as_ref()
    }

    /// Rechecks the immutable source-map authority before downstream admission.
    #[must_use]
    pub fn is_bound_to(&self, sources: &SourceMap) -> bool {
        self.syntax.is_bound_to(sources)
    }
}

/// Derives the exact command requirement from authenticated syntax and source bytes.
///
/// An independent lexical inventory prevents a provider from omitting a reserved call
/// or laundering a reserved name through a local declaration. Pure commands remain valid.
///
/// # Errors
///
/// Rejects stale syntax, provider errors, dependencies, reserved-name misuse, nonliteral
/// or oversized keys, and the first additional intrinsic site. Semantic types, matching,
/// ownership, entry signature and mandatory IR verification remain later obligations.
pub fn admit(
    syntax: &ProjectSyntaxSnapshot,
    sources: &SourceMap,
) -> Result<CommandSyntax, Diagnostic> {
    if !syntax.is_bound_to(sources)
        || sources.len() != 1
        || syntax.files().len() != 1
        || syntax.diagnostics().iter().any(|value| value.severity() == Severity::Error)
    {
        return Err(error("command syntax requires one successful source-bound v4 file"));
    }
    let file = &syntax.files()[0];
    if !file.imports().is_empty() {
        return Err(error("command H1 does not admit dependencies"));
    }
    let source = sources.source(file.id()).ok_or_else(|| error("command source is missing"))?;
    let inventory = identifiers::inventory(source.text()).map_err(error)?;
    let mut intrinsic_names = BTreeSet::new();
    let outcome_names = outcome_types::positions(file, source.text()).map_err(error)?;
    let mut requirement = None;
    for (function_index, function) in file.functions().iter().enumerate() {
        for (expression_index, expression) in function.body.expressions.iter().enumerate() {
            match &expression.kind {
                RawExpressionKind::Call { callee, arguments, .. } if callee.text == INTRINSIC => {
                    if requirement.is_some() {
                        return Err(error("command H1 admits at most one environment call site"));
                    }
                    let [argument] = arguments.as_slice() else {
                        return Err(error("environment lookup requires one literal key"));
                    };
                    let literal = usize::try_from(*argument)
                        .ok()
                        .and_then(|index| function.body.expressions.get(index))
                        .ok_or_else(|| error("environment key syntax is missing"))?;
                    let RawExpressionKind::StringLiteral { spelling } = &literal.kind else {
                        return Err(error("environment key must be an unescaped source literal"));
                    };
                    let key = literal_key(spelling)?;
                    let (literal_start, literal_end, call_end) =
                        identifiers::source_call(source.text(), callee.span.start, callee.span.end)
                            .map_err(error)?;
                    if literal.span.start != literal_start
                        || literal.span.end != literal_end
                        || expression.span.start != callee.span.start
                        || expression.span.end != call_end
                    {
                        return Err(error(
                            "environment operation does not match the independent source call",
                        ));
                    }
                    intrinsic_names.insert((callee.span.start, callee.span.end));
                    requirement = Some(EnvironmentRequirement {
                        key: key.to_owned(),
                        call: sources.verify_span(expression.span).map_err(|_| {
                            error("environment call span lost its source authority")
                        })?,
                        function: function_index,
                        expression: expression_index,
                    });
                }
                RawExpressionKind::EnumConstruction { type_name, .. }
                    if type_name.text == OUTCOME =>
                {
                    return Err(error("the environment outcome can only be constructed by lookup"));
                }
                _ => {}
            }
        }
    }
    for &(name, start, end) in &inventory {
        let allowed = match name {
            INTRINSIC => intrinsic_names.contains(&(start, end)),
            OUTCOME => outcome_names.contains(&(start, end)),
            _ => true,
        };
        if !allowed {
            return Err(error(
                "command reserved name was shadowed, omitted or used outside its gate",
            ));
        }
    }
    // Every admitted callee must also exist in the independently scanned source inventory.
    // The v4 verifier already authenticates exact spelling and spans; this intersection
    // additionally ensures comments and strings cannot be relabeled as an effect.
    for (start, end) in intrinsic_names {
        if !inventory.contains(&(INTRINSIC, start, end)) {
            return Err(error("environment call is not a source identifier"));
        }
    }
    for (start, end) in outcome_names {
        if !inventory.contains(&(OUTCOME, start, end)) {
            return Err(error("environment outcome type is not a source identifier"));
        }
    }
    coverage::verify(file, source.text()).map_err(error)?;
    Ok(CommandSyntax { syntax: syntax.clone(), requirement })
}

fn literal_key(spelling: &str) -> Result<&str, Diagnostic> {
    let bytes = spelling.as_bytes();
    if bytes.len() < 3 || !matches!(bytes[0], b'\'' | b'"') || bytes.last() != Some(&bytes[0]) {
        return Err(error("environment key must be a nonempty quoted literal"));
    }
    let key = &spelling[1..spelling.len() - 1];
    if key.len() > 64 || key.contains(['\\', '\n', '\r', '\0']) {
        return Err(error("environment key must contain 1 to 64 unescaped UTF-8 bytes"));
    }
    Ok(key)
}

fn error(message: &str) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-Y4100",
        None,
        message,
        "use one source file and one unshadowed environment lookup with a literal key",
    )
}

#[cfg(test)]
mod tests;

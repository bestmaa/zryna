use zryna_syntax::v4::{RawFieldInitializer, RawFieldInitializerKind, RawIdentifierSyntax};
use zryna_syntax::v5::{RawDataDeclarationKind, RawTypeArgumentList};

use super::super::DeclarationIdentity;
use super::model::{Head, Kind, Ty};
use super::{
    BodyTypeFailure, Checker, arguments, constraints, resources, signatures, substitution,
    type_resolution,
};

fn head(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    name: &RawIdentifierSyntax,
    list: Option<&RawTypeArgumentList>,
    structure: bool,
) -> Result<Option<Head>, BodyTypeFailure> {
    if checker.context.type_parameter(owner, &name.text).is_some() {
        constraints::opaque(checker, name.span, "nominal construction");
        return Ok(None);
    }
    let Some((kind, count)) = type_resolution::family(checker.context, owner, &name.text) else {
        if list.is_none() {
            legacy_error(
                checker,
                name.span,
                if structure {
                    format!("'{}' is not a local aggregate type", name.text)
                } else {
                    format!("'{}' is not a module-local enum type", name.text)
                },
                if structure {
                    "construct an exact declared struct"
                } else {
                    "construct one exact declared enum variant"
                },
            );
            return Ok(None);
        }
        arguments::error(checker, name.span, "construction head is not an original nominal family");
        return Ok(None);
    };
    Ok(arguments::source_arguments(checker, owner, count, list, name.span)?
        .map(|children| Head { kind, children }))
}

pub(super) fn structure(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
    name: &RawIdentifierSyntax,
    list: Option<&RawTypeArgumentList>,
    initializers: &[RawFieldInitializer],
) -> Result<Option<Ty>, BodyTypeFailure> {
    let Some(head) = head(checker, owner, name, list, true)? else { return Ok(None) };
    let at =
        resources::raw_function(checker.context, owner).body.expressions[expression as usize].span;
    let generic = head.children[0].is_some();
    let Kind::Nominal(target) = head.kind else {
        constraints::mismatch(
            checker,
            name.span,
            "struct construction requires an original struct",
        );
        return Ok(None);
    };
    let RawDataDeclarationKind::Struct { fields, .. } = signatures::data(checker.context, target)
    else {
        if generic {
            constraints::mismatch(checker, name.span, "enum cannot use a struct field initializer");
        } else {
            legacy_error(
                checker,
                at,
                "struct construction names an enum",
                "use enum variant construction for an enum",
            );
        }
        return Ok(None);
    };
    let environment = signatures::nominal_environment(checker, owner, head)?;
    if !generic {
        if !legacy_structure(checker, owner, target, name, initializers, at)? {
            return Ok(None);
        }
        return Ok(Some(substitution::issue_expression(checker, owner, expression, head)?));
    }
    let mut seen = resources::repeated(fields.len(), false)?;
    for initializer in initializers {
        let (field_name, value) = match &initializer.kind {
            RawFieldInitializerKind::Shorthand { name, value }
            | RawFieldInitializerKind::Explicit { name, value, .. } => (name, *value),
        };
        let Some(index) = fields.iter().position(|field| field.name.text == field_name.text) else {
            constraints::mismatch(checker, field_name.span, "initializer names no original field");
            continue;
        };
        if seen[index] {
            constraints::mismatch(checker, field_name.span, "field is initialized more than once");
        }
        seen[index] = true;
        let expected = Some(substitution::source(target, fields[index].type_syntax, environment));
        let actual = checker.expression(owner, value);
        let at =
            resources::raw_function(checker.context, owner).body.expressions[value as usize].span;
        constraints::require_generic(checker, owner, expected, actual, at, "field initializer")?;
    }
    if seen.iter().any(|seen| !seen) {
        constraints::mismatch(checker, name.span, "construction omits an original field");
    }
    Ok(Some(substitution::issue_expression(checker, owner, expression, head)?))
}

fn field_value(initializer: &RawFieldInitializer) -> (&RawIdentifierSyntax, u32) {
    match &initializer.kind {
        RawFieldInitializerKind::Shorthand { name, value }
        | RawFieldInitializerKind::Explicit { name, value, .. } => (name, *value),
    }
}

fn legacy_structure(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    target: DeclarationIdentity,
    name: &RawIdentifierSyntax,
    initializers: &[RawFieldInitializer],
    at: zryna_source::UntrustedSpan,
) -> Result<bool, BodyTypeFailure> {
    let RawDataDeclarationKind::Struct { fields, .. } = signatures::data(checker.context, target)
    else {
        return Err(BodyTypeFailure::InternalFailure);
    };
    let mut seen = resources::repeated(fields.len(), false)?;
    // Copy planning validates the complete field set before any field type constraint.
    for initializer in initializers {
        let (field_name, _) = field_value(initializer);
        let Some(index) = fields.iter().position(|field| field.name.text == field_name.text) else {
            legacy_error(
                checker,
                initializer.span,
                format!("struct '{}' has no field '{}'", name.text, field_name.text),
                "initialize exactly the declared field set",
            );
            return Ok(false);
        };
        if seen[index] {
            legacy_error(
                checker,
                initializer.span,
                format!("field '{}' is initialized more than once", field_name.text),
                "initialize every declared field exactly once",
            );
            return Ok(false);
        }
        seen[index] = true;
    }
    // Presence and value checks then follow the original declaration order.
    for field in fields {
        let Some(initializer) = initializers
            .iter()
            .find(|initializer| field_value(initializer).0.text == field.name.text)
        else {
            legacy_error(
                checker,
                at,
                format!("field '{}' is not initialized", field.name.text),
                "initialize every declared field exactly once",
            );
            return Ok(false);
        };
        let actual = checker.expression(owner, field_value(initializer).1);
        if !constraints::require(
            checker,
            owner,
            Some(substitution::source(target, field.type_syntax, 0)),
            actual,
            initializer.span,
            "struct field",
        )? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn enumeration(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    expression: u32,
    name: &RawIdentifierSyntax,
    variant: &RawIdentifierSyntax,
    list: Option<&RawTypeArgumentList>,
    payload: Option<u32>,
) -> Result<Option<Ty>, BodyTypeFailure> {
    let Some(head) = head(checker, owner, name, list, false)? else { return Ok(None) };
    let generic = head.children[0].is_some();
    let at =
        resources::raw_function(checker.context, owner).body.expressions[expression as usize].span;
    if let Kind::Nominal(target) = head.kind
        && !matches!(signatures::data(checker.context, target), RawDataDeclarationKind::Enum { .. })
    {
        if generic {
            constraints::mismatch(checker, at, "enum construction names a struct");
        } else {
            legacy_error(
                checker,
                at,
                "enum construction names a struct",
                "construct a declared enum variant",
            );
        }
        return Ok(None);
    }
    let environment = signatures::nominal_environment(checker, owner, head)?;
    let Some((_, expected)) =
        signatures::variant(checker.context, head, &variant.text, environment)
    else {
        if !generic {
            legacy_error(
                checker,
                variant.span,
                format!("enum '{}' has no variant '{}'", name.text, variant.text),
                "use one exact declared variant",
            );
            return Ok(None);
        }
        variant_error(checker, variant.span, "variant is not in the original enum family");
        return Ok(None);
    };
    match (expected, payload) {
        (None, None) => {}
        (Some(expected), Some(payload)) => {
            let actual = checker.expression(owner, payload);
            let at = resources::raw_function(checker.context, owner).body.expressions
                [payload as usize]
                .span;
            if generic {
                constraints::require_generic(
                    checker,
                    owner,
                    Some(expected),
                    actual,
                    at,
                    "variant payload",
                )?;
            } else if !constraints::require(
                checker,
                owner,
                Some(expected),
                actual,
                at,
                "enum payload",
            )? {
                return Ok(None);
            }
        }
        _ => {
            if generic {
                constraints::mismatch(
                    checker,
                    variant.span,
                    "variant payload arity differs from its original signature",
                );
            } else {
                legacy_error(
                    checker,
                    at,
                    "enum payload presence does not match the declared variant",
                    "supply exactly one payload only for a payload variant",
                );
            }
            return Ok(None);
        }
    }
    Ok(Some(substitution::issue_expression(checker, owner, expression, head)?))
}

/// Preserve the Copy constructor templates from `copy_lowering/expressions/{constructors,planning`}.
pub(super) fn legacy_error(
    checker: &mut Checker<'_, '_>,
    at: zryna_source::UntrustedSpan,
    message: impl Into<String>,
    guidance: &str,
) {
    let span = checker.span(at);
    checker.constraints.at("ZRYNA-M3005", span, message, guidance);
}

pub(super) fn variant_error(
    checker: &mut Checker<'_, '_>,
    at: zryna_source::UntrustedSpan,
    message: &str,
) {
    let span = checker.span(at);
    checker.constraints.at(
        "ZRYNA-M7004",
        span,
        message,
        "use each exact original family variant once and cover every possible variant",
    );
}

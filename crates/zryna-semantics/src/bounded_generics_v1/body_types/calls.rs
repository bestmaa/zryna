use zryna_syntax::v4::RawIdentifierSyntax;
use zryna_syntax::v5::RawTypeArgumentList;

use super::super::{DeclarationIdentity, DeclarationKind};
use super::model::{Kind, Scalar, Ty};
use super::value_names::Scope;
use super::{
    BodyTypeFailure, Checker, arguments, constraints, resources, substitution, type_resolution,
    value_names,
};

pub(super) fn check(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    callee: &RawIdentifierSyntax,
    type_arguments: Option<&RawTypeArgumentList>,
    values: &[u32],
    scope: &Scope<'_>,
) -> Result<Option<Ty>, BodyTypeFailure> {
    if let Some(binding) = scope.get(&callee.text) {
        if let Some(ty) = binding.ty
            && let Some(head) = substitution::head(&checker.tables, owner, ty)?
        {
            if matches!(head.kind, Kind::Parameter(_)) {
                constraints::opaque(checker, callee.span, "callable capability");
            } else {
                constraints::mismatch(
                    checker,
                    callee.span,
                    "callee is a value rather than an original function",
                );
            }
        }
        return Ok(None);
    }
    if callee.text == "concat" {
        if type_arguments.is_some() {
            arguments::error(checker, callee.span, "concat has no type parameters");
        }
        if values.len() != 2 {
            constraints::mismatch(checker, callee.span, "concat requires two String values");
        }
        for value in values {
            let actual = checker.expression(owner, *value);
            let at = resources::raw_function(checker.context, owner).body.expressions
                [*value as usize]
                .span;
            constraints::require(
                checker,
                owner,
                Some(Ty::scalar(Scalar::String)),
                actual,
                at,
                "concat argument",
            )?;
        }
        return Ok(Some(Ty::scalar(Scalar::String)));
    }
    let Some(target) = type_resolution::declaration(checker.context, owner.module(), &callee.text)
    else {
        value_names::missing(checker, callee);
        return Ok(None);
    };
    if target.kind() != DeclarationKind::Function {
        constraints::mismatch(checker, callee.span, "callee is not an original function");
        return Ok(None);
    }
    let count =
        checker.context.declaration(target).expect("original callee").type_parameters().count();
    let Some(arguments) =
        arguments::source_arguments(checker, owner, count, type_arguments, callee.span)?
    else {
        return Ok(None);
    };
    let environment = substitution::environment(checker, owner, target, arguments)?;
    let function = resources::raw_function(checker.context, target);
    if function.parameters.len() != values.len() {
        if count > 0 {
            constraints::mismatch(
                checker,
                callee.span,
                "generic call value arity differs from its original signature",
            );
        } else {
            let span = checker.span(callee.span);
            checker.constraints.at(
                "ZRYNA-M3007",
                span,
                "call has a different exact aggregate type",
                "use a value with the exact declared type",
            );
        }
    }
    for (parameter, value) in function.parameters.iter().zip(values) {
        let expected =
            type_resolution::source(checker, target.module().index(), parameter.type_syntax)
                .map(|_| substitution::source(target, parameter.type_syntax, environment));
        let actual = checker.expression(owner, *value);
        let at =
            resources::raw_function(checker.context, owner).body.expressions[*value as usize].span;
        if count > 0 {
            constraints::require_generic(checker, owner, expected, actual, at, "call argument")?;
        } else {
            constraints::require(checker, owner, expected, actual, at, "call argument")?;
        }
    }
    Ok(type_resolution::source(checker, target.module().index(), function.result_type)
        .map(|_| substitution::source(target, function.result_type, environment)))
}

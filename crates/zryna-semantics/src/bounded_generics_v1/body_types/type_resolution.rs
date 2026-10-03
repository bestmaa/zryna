use zryna_syntax::v5::RawTypeSyntaxKind;

use super::super::{DeclarationContext, DeclarationIdentity, DeclarationKind, ModuleIdentity};
use super::model::{Head, Kind, Scalar, Ty};
use super::{BodyTypeFailure, Checker, arguments, constraints};

pub(super) fn declaration(
    context: &DeclarationContext<'_>,
    module: ModuleIdentity,
    name: &str,
) -> Option<DeclarationIdentity> {
    let module = context.modules().nth(module.index() as usize)?;
    module
        .data_declarations()
        .chain(module.functions())
        .find(|declaration| declaration.name() == name)
        .map(super::super::DeclarationView::identity)
        .or_else(|| {
            module
                .imports()
                .find(|import| import.local_name() == name)
                .map(|import| import.target().identity())
        })
}

pub(super) fn family(
    context: &DeclarationContext<'_>,
    owner: DeclarationIdentity,
    name: &str,
) -> Option<(Kind, usize)> {
    match name {
        "Option" => Some((Kind::Option, 1)),
        "Result" => Some((Kind::Result, 2)),
        _ => {
            let target = declaration(context, owner.module(), name)?;
            if target.kind() == DeclarationKind::Function {
                return None;
            }
            Some((Kind::Nominal(target), context.declaration(target)?.type_parameters().count()))
        }
    }
}

pub(super) fn source(checker: &Checker<'_, '_>, module: u32, occurrence: u32) -> Option<Ty> {
    checker.tables.sources[module as usize][occurrence as usize]
        .head
        .map(|_| Ty::source(module, occurrence))
}

pub(super) fn resolve(checker: &mut Checker<'_, '_>) -> Result<(), BodyTypeFailure> {
    argument_occurrences(checker)?;
    for unit in checker.context.syntax().files() {
        for (index, node) in unit.type_syntax.iter().enumerate() {
            let owner = checker.tables.sources[unit.id as usize][index].owner;
            let child = |id| Ty::source(unit.id, id);
            let mut invalid = false;
            let head = match &node.kind {
                RawTypeSyntaxKind::Missing => {
                    let local = owner.kind() == DeclarationKind::Function
                        && unit.functions[owner.source_index() as usize].body.statements.iter().any(|statement| {
                            matches!(statement.kind, zryna_syntax::v4::RawStatementKind::LocalDeclaration { type_syntax, .. } if type_syntax as usize == index)
                        });
                    if !local {
                        let span = checker.span(node.span);
                        checker.constraints.at(
                            "ZRYNA-M3007",
                            span,
                            "signature requires an exact declared type",
                            "write the original parameter or result type",
                        );
                    }
                    None
                }
                RawTypeSyntaxKind::Named { name } => named(
                    checker,
                    owner,
                    name,
                    checker.tables.sources[unit.id as usize][index].argument_occurrence,
                    &mut invalid,
                ),
                RawTypeSyntaxKind::String { .. } => Some(Head::leaf(Kind::Scalar(Scalar::String))),
                RawTypeSyntaxKind::Vec { argument, .. } => {
                    Some(Head::unary(Kind::Vec, child(*argument)))
                }
                RawTypeSyntaxKind::Shared { argument, .. } => {
                    Some(Head::unary(Kind::Shared, child(*argument)))
                }
                RawTypeSyntaxKind::Weak { argument, .. } => {
                    Some(Head::unary(Kind::Weak, child(*argument)))
                }
                RawTypeSyntaxKind::Borrow { argument, .. } => {
                    Some(Head::unary(Kind::Borrow, child(*argument)))
                }
                RawTypeSyntaxKind::BorrowMut { argument, .. } => {
                    Some(Head::unary(Kind::BorrowMut, child(*argument)))
                }
                RawTypeSyntaxKind::FixedArray { element, length, .. } => {
                    Some(Head::unary(Kind::FixedArray(*length), child(*element)))
                }
                RawTypeSyntaxKind::Application { name, type_arguments } => {
                    if checker.context.type_parameter(owner, &name.text).is_some() {
                        constraints::opaque(checker, node.span, "higher-kinded application");
                        invalid = true;
                        None
                    } else if let Some((kind, count)) = family(checker.context, owner, &name.text) {
                        let children = arguments::source_arguments(
                            checker,
                            owner,
                            count,
                            Some(type_arguments),
                            name.span,
                        )?;
                        if let Some(children) = children {
                            Some(Head { kind, children })
                        } else {
                            invalid = true;
                            None
                        }
                    } else {
                        arguments::error(
                            checker,
                            name.span,
                            "application head is not an original nominal type",
                        );
                        invalid = true;
                        None
                    }
                }
            };
            // Children precede their parents in the authenticated occurrence forest.
            let head = head.filter(|head| {
                head.children.iter().flatten().all(|ty| {
                    let super::model::Origin::Source { occurrence, .. } = ty.origin else {
                        return false;
                    };
                    checker.tables.sources[unit.id as usize][occurrence as usize].head.is_some()
                })
            });
            checker.tables.sources[unit.id as usize][index].head = head;
            checker.tables.sources[unit.id as usize][index].invalid_arguments = invalid;
        }
    }
    Ok(())
}

fn argument_occurrences(checker: &mut Checker<'_, '_>) -> Result<(), BodyTypeFailure> {
    use zryna_syntax::v5::RawExpressionKind;
    let mut pending = super::resources::reserve(129)?;
    for unit in checker.context.syntax().files() {
        let mut mark = |root: u32| {
            pending.push(root);
            while let Some(index) = pending.pop() {
                let record = &mut checker.tables.sources[unit.id as usize][index as usize];
                if record.argument_occurrence {
                    continue;
                }
                record.argument_occurrence = true;
                match &unit.type_syntax[index as usize].kind {
                    RawTypeSyntaxKind::Application { type_arguments, .. } => {
                        pending.extend(type_arguments.arguments.iter().rev().copied());
                    }
                    RawTypeSyntaxKind::Vec { argument, .. }
                    | RawTypeSyntaxKind::Shared { argument, .. }
                    | RawTypeSyntaxKind::Weak { argument, .. }
                    | RawTypeSyntaxKind::Borrow { argument, .. }
                    | RawTypeSyntaxKind::BorrowMut { argument, .. } => pending.push(*argument),
                    RawTypeSyntaxKind::FixedArray { element, .. } => pending.push(*element),
                    RawTypeSyntaxKind::Missing
                    | RawTypeSyntaxKind::Named { .. }
                    | RawTypeSyntaxKind::String { .. } => {}
                }
            }
        };
        for node in &unit.type_syntax {
            if let RawTypeSyntaxKind::Application { type_arguments, .. } = &node.kind {
                for root in &type_arguments.arguments {
                    mark(*root);
                }
            }
        }
        for function in &unit.functions {
            for expression in &function.body.expressions {
                match &expression.kind {
                    RawExpressionKind::Call { type_arguments, .. }
                    | RawExpressionKind::StructConstruction { type_arguments, .. }
                    | RawExpressionKind::EnumConstruction { type_arguments, .. } => {
                        if let Some(list) = type_arguments {
                            for root in &list.arguments {
                                mark(*root);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn named(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    name: &zryna_syntax::v4::RawIdentifierSyntax,
    argument_occurrence: bool,
    invalid: &mut bool,
) -> Option<Head> {
    if let Some(parameter) = checker.context.type_parameter(owner, &name.text) {
        Some(Head::leaf(Kind::Parameter(parameter.identity())))
    } else if let Some(scalar) = match name.text.as_str() {
        "bool" => Some(Scalar::Bool),
        "i32" => Some(Scalar::I32),
        "unit" => Some(Scalar::Unit),
        _ => None,
    } {
        Some(Head::leaf(Kind::Scalar(scalar)))
    } else if let Some((kind, count)) = family(checker.context, owner, &name.text) {
        if count == 0 {
            Some(Head::leaf(kind))
        } else {
            arguments::error(checker, name.span, "generic type requires explicit arguments");
            *invalid = true;
            None
        }
    } else {
        if argument_occurrence {
            arguments::error(checker, name.span, "type name does not resolve to a value type");
        } else {
            let span = checker.span(name.span);
            checker.names.at(
                "ZRYNA-M3002",
                span,
                format!("type '{}' does not name a module-local aggregate", name.text),
                "use bool, i32, or an exact aggregate declaration name",
            );
        }
        *invalid = true;
        None
    }
}

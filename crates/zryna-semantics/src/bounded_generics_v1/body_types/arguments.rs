use zryna_source::UntrustedSpan;
use zryna_syntax::v5::RawTypeArgumentList;

use super::super::DeclarationIdentity;
use super::model::{Kind, Scalar, Ty};
use super::{BodyTypeFailure, Checker, type_resolution};

pub(super) fn error(checker: &mut Checker<'_, '_>, at: UntrustedSpan, message: &str) {
    let span = checker.span(at);
    checker.arguments.at(
        "ZRYNA-M7001",
        span,
        message,
        "supply the declaration's exact explicit ZrynaValue type arguments",
    );
}

pub(super) fn source_arguments(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    count: usize,
    list: Option<&RawTypeArgumentList>,
    omitted: UntrustedSpan,
) -> Result<Option<[Option<Ty>; 2]>, BodyTypeFailure> {
    if list.map_or(0, |list| list.arguments.len()) != count {
        error(
            checker,
            list.map_or(omitted, |list| list.span),
            "explicit type argument count differs from the declaration",
        );
        return Ok(None);
    }
    let mut arguments = [None, None];
    let mut valid = true;
    if let Some(list) = list {
        for (index, occurrence) in list.arguments.iter().enumerate() {
            let record =
                checker.tables.sources[owner.module().index() as usize][*occurrence as usize];
            let node = &checker.context.syntax().files()[owner.module().index() as usize]
                .type_syntax[*occurrence as usize];
            match record.head {
                Some(_) if storable(checker, owner.module().index(), *occurrence)? => {
                    arguments[index] =
                        type_resolution::source(checker, owner.module().index(), *occurrence);
                }
                _ => {
                    error(
                        checker,
                        node.span,
                        "type argument is not a known storable ZrynaValue type",
                    );
                    valid = false;
                }
            }
        }
    }
    Ok(valid.then_some(arguments))
}

fn storable(
    checker: &Checker<'_, '_>,
    module: u32,
    occurrence: u32,
) -> Result<bool, BodyTypeFailure> {
    let mut pending = super::resources::reserve(129)?;
    pending.push(occurrence);
    while let Some(index) = pending.pop() {
        let Some(head) = checker.tables.sources[module as usize][index as usize].head else {
            return Ok(false);
        };
        if matches!(
            head.kind,
            Kind::Scalar(Scalar::Unit) | Kind::Borrow | Kind::BorrowMut | Kind::Function(_)
        ) {
            return Ok(false);
        }
        for child in head.children.into_iter().flatten() {
            let super::model::Origin::Source { module: original, occurrence } = child.origin else {
                return Err(BodyTypeFailure::InternalFailure);
            };
            if original != module {
                return Err(BodyTypeFailure::InternalFailure);
            }
            pending.push(occurrence);
        }
    }
    Ok(true)
}

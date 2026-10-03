use std::collections::BTreeMap;

use zryna_source::Span;
use zryna_syntax::v4::RawIdentifierSyntax;
use zryna_syntax::v5::{RawDataDeclarationKind, RawTypeParameterList};

use super::diagnostics::Errors;
use super::{DeclarationIdentity, DeclarationKind, ModuleIdentity, SemanticInput};

#[derive(Debug)]
pub(super) struct DeclarationRecord {
    pub(super) identity: DeclarationIdentity,
    pub(super) name: Span,
    pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct Inventory<'a> {
    pub(super) data: Vec<DeclarationRecord>,
    pub(super) functions: Vec<DeclarationRecord>,
    pub(super) names: BTreeMap<&'a str, DeclarationIdentity>,
}

pub(super) fn inventories(input: SemanticInput<'_>) -> Vec<Inventory<'_>> {
    input
        .syntax()
        .files()
        .iter()
        .map(|unit| {
            let module = ModuleIdentity::new(
                input.sources().verify_file_id(unit.id).expect("authenticated v5 module"),
            );
            let mut names = BTreeMap::new();
            let data = unit
                .data_declarations
                .iter()
                .enumerate()
                .map(|(index, declaration)| {
                    let (name, kind) = match &declaration.kind {
                        RawDataDeclarationKind::Struct { name, .. } => {
                            (name, DeclarationKind::Struct)
                        }
                        RawDataDeclarationKind::Enum { name, .. } => (name, DeclarationKind::Enum),
                    };
                    let identity = DeclarationIdentity::new(module, kind, index);
                    names.insert(name.text.as_str(), identity);
                    DeclarationRecord {
                        identity,
                        name: input.sources().verify_span(name.span).expect("authenticated name"),
                        span: input
                            .sources()
                            .verify_span(declaration.span)
                            .expect("authenticated declaration"),
                    }
                })
                .collect();
            let functions = unit
                .functions
                .iter()
                .enumerate()
                .map(|(index, function)| {
                    let identity =
                        DeclarationIdentity::new(module, DeclarationKind::Function, index);
                    names.insert(function.name.text.as_str(), identity);
                    DeclarationRecord {
                        identity,
                        name: input
                            .sources()
                            .verify_span(function.name.span)
                            .expect("authenticated name"),
                        span: input
                            .sources()
                            .verify_span(function.span)
                            .expect("authenticated function"),
                    }
                })
                .collect();
            Inventory { data, functions, names }
        })
        .collect()
}

pub(super) fn name(
    input: SemanticInput<'_>,
    identity: DeclarationIdentity,
) -> &RawIdentifierSyntax {
    let unit = &input.syntax().files()[identity.module().index() as usize];
    let index = identity.source_index() as usize;
    match identity.kind() {
        DeclarationKind::Function => &unit.functions[index].name,
        DeclarationKind::Struct | DeclarationKind::Enum => {
            match &unit.data_declarations[index].kind {
                RawDataDeclarationKind::Struct { name, .. }
                | RawDataDeclarationKind::Enum { name, .. } => name,
            }
        }
    }
}

pub(super) fn exported(input: SemanticInput<'_>, identity: DeclarationIdentity) -> bool {
    let unit = &input.syntax().files()[identity.module().index() as usize];
    let index = identity.source_index() as usize;
    match identity.kind() {
        DeclarationKind::Function => unit.functions[index].export_span.is_some(),
        DeclarationKind::Struct | DeclarationKind::Enum => {
            unit.data_declarations[index].export_span.is_some()
        }
    }
}

pub(super) fn parameters(
    input: SemanticInput<'_>,
    identity: DeclarationIdentity,
) -> Option<&RawTypeParameterList> {
    let unit = &input.syntax().files()[identity.module().index() as usize];
    let index = identity.source_index() as usize;
    match identity.kind() {
        DeclarationKind::Function => unit.functions[index].type_parameters.as_ref(),
        DeclarationKind::Struct | DeclarationKind::Enum => {
            unit.data_declarations[index].type_parameters.as_ref()
        }
    }
}

pub(super) fn collisions(input: SemanticInput<'_>, inventory: &Inventory<'_>, errors: &mut Errors) {
    let mut ordered = inventory.data.iter().chain(&inventory.functions).collect::<Vec<_>>();
    ordered.sort_by_key(|record| record.name.start());
    let mut names = BTreeMap::new();
    for record in ordered {
        let original = name(input, record.identity);
        if names.insert(original.text.to_ascii_lowercase(), record.identity).is_some()
            || (record.identity.kind() == DeclarationKind::Function
                && original.text.eq_ignore_ascii_case("concat"))
        {
            errors.at(
                "ZRYNA-M3002",
                record.name,
                format!("declaration '{}' collides under portable naming rules", original.text),
                "give declarations exact unique portable names",
            );
        }
    }
}

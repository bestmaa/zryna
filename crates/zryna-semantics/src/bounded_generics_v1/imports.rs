use std::collections::BTreeMap;

use zryna_source::{Span, resolve_explicit_zry_import};
use zryna_syntax::v4::RawImportBindingSyntax;

use super::declarations::{self, Inventory};
use super::diagnostics::Errors;
use super::{DeclarationIdentity, SemanticInput};

#[derive(Debug)]
pub(super) struct Binding<'a> {
    pub(super) syntax: &'a RawImportBindingSyntax,
    pub(super) target: Option<DeclarationIdentity>,
    pub(super) span: Span,
}

#[derive(Debug)]
pub(super) struct ImportFacts<'a> {
    pub(super) target_module: Option<usize>,
    pub(super) span: Span,
    pub(super) bindings: Vec<Binding<'a>>,
}

/// Original target facts deliberately ignore graph validity, body/signature support and collisions.
pub(super) fn provisional<'a>(
    input: SemanticInput<'a>,
    inventories: &[Inventory<'a>],
) -> Vec<Vec<ImportFacts<'a>>> {
    let paths = input
        .syntax()
        .files()
        .iter()
        .enumerate()
        .map(|(index, unit)| (unit.path.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    input
        .syntax()
        .files()
        .iter()
        .map(|unit| {
            let source = input
                .sources()
                .source(input.sources().verify_file_id(unit.id).expect("authenticated module"))
                .expect("original source");
            unit.imports
                .iter()
                .map(|import| {
                    let target_module =
                        resolve_explicit_zry_import(source.path(), &import.specifier.text)
                            .ok()
                            .and_then(|path| paths.get(path.as_str()).copied());
                    let bindings = import
                        .bindings
                        .iter()
                        .map(|binding| {
                            let target = target_module.and_then(|module| {
                                inventories[module]
                                    .names
                                    .get(binding.imported.text.as_str())
                                    .copied()
                                    .filter(|identity| declarations::exported(input, *identity))
                            });
                            Binding {
                                syntax: binding,
                                target,
                                span: input
                                    .sources()
                                    .verify_span(binding.local.span)
                                    .expect("authenticated import alias"),
                            }
                        })
                        .collect();
                    ImportFacts {
                        target_module,
                        bindings,
                        span: input
                            .sources()
                            .verify_span(import.specifier.token_span)
                            .expect("authenticated import specifier"),
                    }
                })
                .collect()
        })
        .collect()
}

pub(super) fn validate(
    input: SemanticInput<'_>,
    inventory: &Inventory<'_>,
    imports: &[ImportFacts<'_>],
    errors: &mut Errors,
) {
    let mut names = inventory
        .names
        .keys()
        .map(|name| name.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    for import in imports {
        if import.target_module.is_none() {
            errors.at(
                "ZRYNA-M3016",
                import.span,
                "module import is absent from the authenticated exact .zry closure",
                "use one exact portable relative path in the complete source graph",
            );
            continue;
        }
        for binding in &import.bindings {
            if binding.target.is_none() {
                errors.at(
                    "ZRYNA-M3016",
                    input
                        .sources()
                        .verify_span(binding.syntax.imported.span)
                        .expect("authenticated imported identifier"),
                    format!(
                        "'{}' does not name an exact original exported declaration",
                        binding.syntax.imported.text
                    ),
                    "import an original declaration using its exact exported spelling",
                );
            } else if !names.insert(binding.syntax.local.text.to_ascii_lowercase()) {
                errors.at(
                    "ZRYNA-M3002",
                    binding.span,
                    format!(
                        "import alias '{}' collides under portable ASCII case folding",
                        binding.syntax.local.text
                    ),
                    "use one unique portable alias per original declaration binding",
                );
            }
        }
    }
}

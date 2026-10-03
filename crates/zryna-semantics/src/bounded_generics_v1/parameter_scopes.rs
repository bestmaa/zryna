use std::collections::BTreeSet;

use zryna_source::Span;
use zryna_syntax::v5::RawTypeParameter;

use super::declarations::{self, Inventory};
use super::diagnostics::Errors;
use super::imports::ImportFacts;
use super::{DeclarationKind, SemanticInput, TypeParameterIdentity};

pub(super) fn imported_shadows(
    input: SemanticInput<'_>,
    inventories: &[Inventory<'_>],
    imports: &[Vec<ImportFacts<'_>>],
    errors: &mut Errors,
) {
    for (module, inventory) in inventories.iter().enumerate() {
        let visible = imports[module]
            .iter()
            .flat_map(|import| &import.bindings)
            .filter(|binding| {
                binding.target.is_some_and(|target| target.kind() != DeclarationKind::Function)
            })
            .map(|binding| binding.syntax.local.text.as_str())
            .collect::<BTreeSet<_>>();
        for declaration in inventory.data.iter().chain(&inventory.functions) {
            let Some(parameters) = declarations::parameters(input, declaration.identity) else {
                continue;
            };
            for parameter in &parameters.parameters {
                if visible.contains(parameter.name.text.as_str()) {
                    errors.at(
                        "ZRYNA-D7001",
                        input
                            .sources()
                            .verify_span(parameter.name.span)
                            .expect("authenticated owning parameter"),
                        format!(
                            "type parameter '{}' shadows an original imported visible type",
                            parameter.name.text
                        ),
                        "choose a parameter name distinct from visible data declaration aliases",
                    );
                }
            }
        }
    }
}

/// Read-only source parameter view; this does not prove opaque body validity.
#[derive(Clone, Copy, Debug)]
pub struct TypeParameterView<'a> {
    pub(super) identity: TypeParameterIdentity,
    pub(super) syntax: &'a RawTypeParameter,
    pub(super) name: Span,
    pub(super) bound: Span,
}

impl<'a> TypeParameterView<'a> {
    /// Returns the issuing original declaration and parameter index.
    #[must_use]
    pub const fn identity(self) -> TypeParameterIdentity {
        self.identity
    }

    /// Returns the source-spelled parameter name.
    #[must_use]
    pub fn name(self) -> &'a str {
        &self.syntax.name.text
    }

    /// Returns the original authenticated parameter identifier span.
    #[must_use]
    pub const fn name_span(self) -> Span {
        self.name
    }

    /// Returns the original authenticated `ZrynaValue` marker span.
    #[must_use]
    pub const fn bound_span(self) -> Span {
        self.bound
    }
}

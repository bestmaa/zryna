//! Source-derived authorities for the internal bounded-generics contract.
//!
//! Declaration resolution, symbolic body checking and closed semantic discovery retain the
//! original v5/source/module authority. Layouts, ownership, executable IR and profile selection
//! require separate successor authorities.

use zryna_diagnostics::Diagnostic;
use zryna_source::{FileId, SourceMap, Span};
use zryna_syntax::v5::VerifiedProjectSyntaxV5;

mod declarations;
mod diagnostics;
mod identity;
mod imports;
mod input;
mod module_graph;
mod parameter_scopes;
mod resources;

pub mod body_types;
pub mod instantiation;

pub use identity::{DeclarationIdentity, DeclarationKind, ModuleIdentity, TypeParameterIdentity};
pub use input::SemanticInput;
pub use parameter_scopes::TypeParameterView;

use declarations::{DeclarationRecord, Inventory};
use imports::ImportFacts;

#[derive(Debug)]
struct Module<'a> {
    inventory: Inventory<'a>,
    imports: Vec<ImportFacts<'a>>,
}

/// Immutable original declaration bindings for this exact source map and selected entry.
///
/// This context cannot enter a backend or substitute for verified executable IR.
/// ```compile_fail
/// fn emit(context: &zryna_semantics::bounded_generics_v1::DeclarationContext<'_>) {
///     let _: &zryna_ir::data_ownership_v1::VerifiedProgram = context;
/// }
/// ```
#[derive(Debug)]
pub struct DeclarationContext<'a> {
    input: SemanticInput<'a>,
    modules: Vec<Module<'a>>,
}

impl DeclarationContext<'_> {
    /// Returns the retained original complete syntax without semantic reinterpretation.
    #[must_use]
    pub fn syntax(&self) -> &VerifiedProjectSyntaxV5 {
        self.input.syntax()
    }

    /// Returns the retained original immutable source map.
    #[must_use]
    pub fn sources(&self) -> &SourceMap {
        self.input.sources()
    }

    /// Returns the independently selected source entry.
    #[must_use]
    pub fn entry(&self) -> FileId {
        self.input.entry()
    }

    /// Iterates modules in exact source-map path order.
    #[must_use]
    pub fn modules(&self) -> impl ExactSizeIterator<Item = ModuleView<'_>> {
        (0..self.modules.len()).map(|index| ModuleView { context: self, index })
    }

    /// Looks up a declaration only in its issuing original source authority.
    #[must_use]
    pub fn declaration(&self, identity: DeclarationIdentity) -> Option<DeclarationView<'_>> {
        self.sources().source(identity.module().source_file())?;
        let inventory = &self.modules.get(identity.module().index() as usize)?.inventory;
        let records = if identity.kind() == DeclarationKind::Function {
            &inventory.functions
        } else {
            &inventory.data
        };
        let record = records.get(identity.source_index() as usize)?;
        (record.identity == identity).then_some(DeclarationView { context: self, record })
    }

    /// Resolves an original parameter only in its owning declaration's scope.
    #[must_use]
    pub fn type_parameter(
        &self,
        owner: DeclarationIdentity,
        name: &str,
    ) -> Option<TypeParameterView<'_>> {
        self.declaration(owner)?.type_parameters().find(|parameter| parameter.name() == name)
    }
}

/// Resolves exact original module, import, declaration and type-parameter bindings.
///
/// # Errors
/// Returns bounded source-bound declaration or inherited module/name diagnostics without a
/// partial context. All project-wide imported-type shadow errors precede every module error.
pub fn resolve_declarations(
    input: SemanticInput<'_>,
) -> Result<DeclarationContext<'_>, Vec<Diagnostic>> {
    resources::preflight(input).map_err(|failure| vec![failure.diagnostic()])?;
    let inventories = declarations::inventories(input);
    let imports = imports::provisional(input, &inventories);
    let mut declaration_errors = diagnostics::Errors::default();
    parameter_scopes::imported_shadows(input, &inventories, &imports, &mut declaration_errors);
    if !declaration_errors.is_empty() {
        return Err(declaration_errors.finish());
    }
    // Only after the complete D barrier may M candidates consume diagnostic slots.
    let mut errors = diagnostics::Errors::default();
    for (inventory, module_imports) in inventories.iter().zip(&imports) {
        declarations::collisions(input, inventory, &mut errors);
        imports::validate(input, inventory, module_imports, &mut errors);
    }
    module_graph::validate(input, &imports, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let modules = inventories
        .into_iter()
        .zip(imports)
        .map(|(inventory, imports)| Module { inventory, imports })
        .collect();
    Ok(DeclarationContext { input, modules })
}

/// Immutable original module view.
#[derive(Clone, Copy, Debug)]
pub struct ModuleView<'a> {
    context: &'a DeclarationContext<'a>,
    index: usize,
}

impl<'a> ModuleView<'a> {
    /// Returns the original source-map-branded module identity.
    ///
    /// # Panics
    /// Panics only if the retained authenticated module inventory is internally inconsistent.
    #[must_use]
    pub fn identity(self) -> ModuleIdentity {
        ModuleIdentity::new(
            self.context
                .sources()
                .verify_file_id(u32::try_from(self.index).expect("bounded module index"))
                .expect("original module"),
        )
    }

    /// Returns the original normalized workspace-relative source path.
    #[must_use]
    pub fn path(self) -> &'a str {
        &self.context.syntax().files()[self.index].path
    }

    /// Iterates original data declarations in their source order.
    #[must_use]
    pub fn data_declarations(self) -> impl ExactSizeIterator<Item = DeclarationView<'a>> {
        self.context.modules[self.index]
            .inventory
            .data
            .iter()
            .map(move |record| DeclarationView { context: self.context, record })
    }

    /// Iterates original functions in their separate source order.
    #[must_use]
    pub fn functions(self) -> impl ExactSizeIterator<Item = DeclarationView<'a>> {
        self.context.modules[self.index]
            .inventory
            .functions
            .iter()
            .map(move |record| DeclarationView { context: self.context, record })
    }

    /// Iterates exact successful original import bindings in source order.
    pub fn imports(self) -> impl Iterator<Item = ImportView<'a>> {
        self.context.modules[self.index]
            .imports
            .iter()
            .flat_map(|import| &import.bindings)
            .map(move |binding| ImportView { context: self.context, binding })
    }
}

/// Immutable original declaration view, including unused templates.
#[derive(Clone, Copy, Debug)]
pub struct DeclarationView<'a> {
    context: &'a DeclarationContext<'a>,
    record: &'a DeclarationRecord,
}

impl<'a> DeclarationView<'a> {
    /// Returns the issuing original declaration identity.
    #[must_use]
    pub const fn identity(self) -> DeclarationIdentity {
        self.record.identity
    }

    /// Returns the exact original source spelling.
    #[must_use]
    pub fn name(self) -> &'a str {
        &declarations::name(self.context.input, self.identity()).text
    }

    /// Returns the original authenticated declaration range.
    #[must_use]
    pub const fn span(self) -> Span {
        self.record.span
    }

    /// Returns the original authenticated declaration identifier range.
    #[must_use]
    pub const fn name_span(self) -> Span {
        self.record.name
    }

    /// Returns source-module declaration visibility, without executable ABI admission.
    #[must_use]
    pub fn is_exported(self) -> bool {
        declarations::exported(self.context.input, self.identity())
    }

    /// Iterates source-authenticated parameters in their owning declaration order.
    ///
    /// # Panics
    /// Panics only if retained authenticated parameter spans are internally inconsistent.
    pub fn type_parameters(self) -> impl Iterator<Item = TypeParameterView<'a>> {
        declarations::parameters(self.context.input, self.identity())
            .into_iter()
            .flat_map(|list| &list.parameters)
            .enumerate()
            .map(move |(index, syntax)| TypeParameterView {
                identity: TypeParameterIdentity::new(self.identity(), index),
                syntax,
                name: self
                    .context
                    .sources()
                    .verify_span(syntax.name.span)
                    .expect("original parameter"),
                bound: self
                    .context
                    .sources()
                    .verify_span(syntax.bound.span)
                    .expect("original parameter bound"),
            })
    }
}

/// Immutable import alias retaining its original target declaration identity.
#[derive(Clone, Copy, Debug)]
pub struct ImportView<'a> {
    context: &'a DeclarationContext<'a>,
    binding: &'a imports::Binding<'a>,
}

impl<'a> ImportView<'a> {
    /// Returns the local source-spelled binding name.
    #[must_use]
    pub fn local_name(self) -> &'a str {
        &self.binding.syntax.local.text
    }

    /// Returns the original exported name written at this import site.
    #[must_use]
    pub fn imported_name(self) -> &'a str {
        &self.binding.syntax.imported.text
    }

    /// Returns the exact original target, with no alias-generated declaration.
    ///
    /// # Panics
    /// Panics only if the retained resolved import inventory is internally inconsistent.
    #[must_use]
    pub fn target(self) -> DeclarationView<'a> {
        self.context
            .declaration(self.binding.target.expect("resolved original import"))
            .expect("original imported declaration retained")
    }

    /// Returns the authenticated local alias token range.
    #[must_use]
    pub const fn local_span(self) -> Span {
        self.binding.span
    }
}

#[cfg(test)]
mod tests;

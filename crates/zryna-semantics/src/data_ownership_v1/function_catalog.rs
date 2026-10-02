use std::collections::BTreeMap;

use zryna_ir::data_ownership_v1::raw;
use zryna_layout::raw as raw_layout;
use zryna_source::Span;
use zryna_syntax::v4::RawTypeSyntaxKind;

use super::SemanticInput;
use super::diagnostics::Errors;
use super::layout_graph::{Decl, semantic_type};
use super::type_model::Ty;
use crate::data_ownership_v1::diagnostics::span;

#[derive(Clone)]
pub(super) struct FunctionSignature {
    pub(super) id: raw::FunctionId,
    pub(super) name: String,
    pub(super) parameters: Vec<Ty>,
    pub(super) borrow_parameters: Vec<FunctionBorrowParameter>,
    pub(super) parameter_order: Vec<FunctionParameterOrder>,
    pub(super) result: Ty,
    pub(super) private: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FunctionBorrowParameter {
    pub(super) referent: Ty,
    pub(super) access: raw::BorrowAccess,
    pub(super) span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FunctionParameterOrder {
    Value(u32),
    Borrow(u32),
}

impl FunctionSignature {
    pub(super) fn has_borrow_parameters(&self) -> bool {
        debug_assert_eq!(
            self.parameter_order.len(),
            self.parameters.len() + self.borrow_parameters.len()
        );
        debug_assert!(self.parameter_order.iter().all(|parameter| match parameter {
            FunctionParameterOrder::Value(index) =>
                usize::try_from(*index).is_ok_and(|index| index < self.parameters.len()),
            FunctionParameterOrder::Borrow(index) =>
                usize::try_from(*index).is_ok_and(|index| index < self.borrow_parameters.len()),
        }));
        let has_borrow = self
            .parameter_order
            .iter()
            .any(|parameter| matches!(parameter, FunctionParameterOrder::Borrow(_)));
        debug_assert_eq!(has_borrow, !self.borrow_parameters.is_empty());
        has_borrow
    }
}

pub(super) struct FunctionCatalog {
    pub(super) modules: Vec<Vec<Option<FunctionSignature>>>,
}

pub(super) enum FunctionResolution<'a> {
    Exact(&'a FunctionSignature),
    WrongCase,
    Missing,
}

impl FunctionCatalog {
    pub(super) fn resolve(&self, module: usize, name: &str) -> FunctionResolution<'_> {
        let Some(signatures) = self.modules.get(module) else {
            return FunctionResolution::Missing;
        };
        if let Some(signature) =
            signatures.iter().flatten().find(|signature| signature.name == name)
        {
            return FunctionResolution::Exact(signature);
        }
        if signatures.iter().flatten().any(|signature| signature.name.eq_ignore_ascii_case(name)) {
            FunctionResolution::WrongCase
        } else {
            FunctionResolution::Missing
        }
    }

    pub(super) fn admits(&self, module: usize, id: raw::FunctionId) -> bool {
        self.modules.get(module).is_some_and(|signatures| {
            signatures.iter().flatten().any(|candidate| candidate.id == id)
        })
    }

    pub(super) fn bind_import(&mut self, module: usize, name: String, target: &FunctionSignature) {
        let mut alias = target.clone();
        alias.name = name;
        self.modules[module].push(Some(alias));
    }

    pub(super) fn callable_names(&self, module: usize) -> impl Iterator<Item = &str> {
        self.modules.get(module).into_iter().flat_map(|signatures| {
            signatures.iter().flatten().map(|signature| signature.name.as_str())
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn function_parameters(
    input: SemanticInput<'_>,
    file: &zryna_syntax::v4::SourceUnit,
    function: &zryna_syntax::v4::RawFunctionSyntax,
    module: usize,
    declarations: &[Decl],
    graph: &raw_layout::Graph,
    node_types: &[Option<Ty>],
    errors: &mut Errors<'_>,
) -> Option<(Vec<Ty>, Vec<FunctionBorrowParameter>, Vec<FunctionParameterOrder>)> {
    let mut parameters = Vec::with_capacity(function.parameters.len());
    let mut borrow_parameters = Vec::with_capacity(function.parameters.len());
    let mut parameter_order = Vec::with_capacity(function.parameters.len());
    let mut valid = true;
    let mut has_resolved_borrow = false;
    for parameter in &function.parameters {
        let Some(syntax) = usize::try_from(parameter.type_syntax)
            .ok()
            .and_then(|index| file.type_syntax().get(index))
        else {
            valid = false;
            continue;
        };
        let (type_syntax, access) = match syntax.kind {
            RawTypeSyntaxKind::Borrow { argument, .. } => {
                (argument, Some(raw::BorrowAccess::Shared))
            }
            RawTypeSyntaxKind::BorrowMut { argument, .. } => {
                (argument, Some(raw::BorrowAccess::Exclusive))
            }
            _ => (parameter.type_syntax, None),
        };
        let nested_borrow = access.is_some()
            && usize::try_from(type_syntax)
                .ok()
                .and_then(|index| file.type_syntax().get(index))
                .is_some_and(|referent| {
                    matches!(
                        referent.kind,
                        RawTypeSyntaxKind::Borrow { .. } | RawTypeSyntaxKind::BorrowMut { .. }
                    )
                });
        if nested_borrow {
            has_resolved_borrow = true;
            errors.at(
                "ZRYNA-M3016",
                span(input.sources(), syntax.span),
                "borrow parameters require one direct Copy referent",
                "borrow bool, i32, or a recursively Copy aggregate type",
            );
            valid = false;
            continue;
        }
        let Some(ty) =
            semantic_type(file, type_syntax, module, declarations, graph, node_types, errors)
        else {
            valid = false;
            continue;
        };
        if let Some(access) = access {
            has_resolved_borrow = true;
            let at = span(input.sources(), syntax.span);
            let index = u32::try_from(borrow_parameters.len()).unwrap_or(u32::MAX);
            borrow_parameters.push(FunctionBorrowParameter { referent: ty, access, span: at });
            parameter_order.push(FunctionParameterOrder::Borrow(index));
        } else {
            let index = u32::try_from(parameters.len()).unwrap_or(u32::MAX);
            parameters.push(ty);
            parameter_order.push(FunctionParameterOrder::Value(index));
        }
    }
    if has_resolved_borrow && let Some(export_span) = function.export_span {
        errors.at(
            "ZRYNA-M3016",
            span(input.sources(), export_span),
            "borrow-parameter functions must remain private",
            "remove export because borrow authority cannot cross the public ABI",
        );
        valid = false;
    }
    valid.then_some((parameters, borrow_parameters, parameter_order))
}

#[allow(clippy::too_many_arguments)]
fn function_result(
    input: SemanticInput<'_>,
    file: &zryna_syntax::v4::SourceUnit,
    function: &zryna_syntax::v4::RawFunctionSyntax,
    module: usize,
    declarations: &[Decl],
    graph: &raw_layout::Graph,
    node_types: &[Option<Ty>],
    errors: &mut Errors<'_>,
) -> Option<Ty> {
    let syntax =
        usize::try_from(function.result_type).ok().and_then(|index| file.type_syntax().get(index));
    if let Some(syntax) = syntax
        && matches!(
            syntax.kind,
            RawTypeSyntaxKind::Borrow { .. } | RawTypeSyntaxKind::BorrowMut { .. }
        )
    {
        errors.at(
            "ZRYNA-M3016",
            span(input.sources(), syntax.span),
            "borrow results are outside the nonescaping ownership profile",
            "return an exact Copy value read through the borrow instead",
        );
        return None;
    }
    semantic_type(file, function.result_type, module, declarations, graph, node_types, errors)
}

pub(super) fn build_function_catalog(
    input: SemanticInput<'_>,
    declarations: &[Decl],
    graph: &raw_layout::Graph,
    node_types: &[Option<Ty>],
    errors: &mut Errors<'_>,
) -> FunctionCatalog {
    let mut modules = Vec::with_capacity(input.syntax().files().len());
    for (module, file) in input.syntax().files().iter().enumerate() {
        let mut names = BTreeMap::<String, Span>::new();
        let mut signatures = Vec::with_capacity(file.functions().len());
        for (declaration, function) in file.functions().iter().enumerate() {
            let name_span = span(input.sources(), function.name.span);
            let folded = function.name.text.to_ascii_lowercase();
            if function.name.text.eq_ignore_ascii_case("concat") {
                errors.at(
                    "ZRYNA-M3002",
                    name_span,
                    "function name 'concat' collides with the sealed String builtin",
                    "rename the function so ordinary calls remain unambiguous",
                );
            }
            if names.insert(folded, name_span).is_some() {
                errors.at(
                    "ZRYNA-M3002",
                    name_span,
                    format!(
                        "function '{}' collides under portable ASCII case folding",
                        function.name.text
                    ),
                    "give every module-local function one portable case-insensitive unique name",
                );
            }
            let parameters = function_parameters(
                input,
                file,
                function,
                module,
                declarations,
                graph,
                node_types,
                errors,
            );
            let result = function_result(
                input,
                file,
                function,
                module,
                declarations,
                graph,
                node_types,
                errors,
            );
            let id = raw::FunctionId {
                module: raw::ModuleId(u32::try_from(module).unwrap_or(u32::MAX)),
                declaration: u32::try_from(declaration).unwrap_or(u32::MAX),
            };
            signatures.push(parameters.zip(result).map(
                |((parameters, borrow_parameters, parameter_order), result)| FunctionSignature {
                    id,
                    name: function.name.text.clone(),
                    parameters,
                    borrow_parameters,
                    parameter_order,
                    result,
                    private: input.command.is_some()
                        || function.export_span.is_none()
                        || file.id() != input.entry(),
                },
            ));
        }
        modules.push(signatures);
    }
    FunctionCatalog { modules }
}

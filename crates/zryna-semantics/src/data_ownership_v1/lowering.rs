use super::{
    Diagnostic, Errors, RuntimeContractIdentity, SemanticInput, StorageTarget,
    accumulate_generated_cfg_function, accumulate_generated_value_function, build_function_catalog,
    build_graph, import_resolution, layout, lower_function, map_node_types, ownership_runtime_abi,
    preflight_program_borrow_calls, raw, semantic_preflight, span,
};

pub(super) struct Candidate {
    pub(super) program: raw::Program,
    pub(super) linear: layout::VerifiedLayouts,
    pub(super) linux: layout::VerifiedLayouts,
    pub(super) runtime_abi: ownership_runtime_abi::VerifiedOwnershipRuntimeAbi,
}
#[allow(clippy::too_many_lines)]
pub(super) fn candidate(input: SemanticInput<'_>) -> Result<Candidate, Vec<Diagnostic>> {
    let mut errors = Errors::new(input.sources());
    semantic_preflight(input, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let (graph, declarations) = build_graph(input, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let linear = layout::verify(&graph, input.sources(), StorageTarget::Linear32V1)?;
    let linux = layout::verify(&graph, input.sources(), StorageTarget::LinuxX8664V1)?;
    if linear.universe_identity() != linux.universe_identity() {
        errors.global(
            "ZRYNA-M3004",
            "the independently derived layout universes disagree",
            "reduce the aggregate type graph and report this deterministic compiler failure",
        );
        return Err(errors.finish());
    }
    let runtime_abi = match ownership_runtime_abi::verify_v1(
        ownership_runtime_abi::raw_v1(&linear, &linux),
        &linear,
        &linux,
    ) {
        Ok(authority) => authority,
        Err(violations) => {
            for violation in violations {
                errors.global(
                    violation.code(),
                    violation.message(),
                    "reduce the program and report this deterministic runtime ABI authority failure",
                );
            }
            return Err(errors.finish());
        }
    };
    let node_types = map_node_types(&graph, &linear, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }

    let mut catalog =
        build_function_catalog(input, &declarations, &graph, &node_types, &mut errors);
    import_resolution::resolve_imports(input, &linear, &mut catalog, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let mut modules = Vec::with_capacity(input.syntax().files().len());
    let mut generated_values = 0_usize;
    let mut generated_blocks = 0_usize;
    let mut generated_edges = 0_usize;
    'modules: for (module_index, file) in input.syntax().files().iter().enumerate() {
        let mut functions = Vec::with_capacity(file.functions().len());
        for (function_index, function) in file.functions().iter().enumerate() {
            let diagnostics_before = errors.len();
            if let Some(mut lowered) = lower_function(
                input,
                module_index,
                function_index,
                function,
                &declarations,
                &graph,
                &node_types,
                &linear,
                &catalog,
                &mut errors,
            ) {
                if input.command.is_some() && function.export_span.is_some() {
                    lowered.entry_export = Some(function.name.text.clone());
                }
                let Some(values) =
                    accumulate_generated_value_function(generated_values, &lowered, &mut errors)
                else {
                    break 'modules;
                };
                let Some((blocks, edges)) = accumulate_generated_cfg_function(
                    generated_blocks,
                    generated_edges,
                    &lowered,
                    &mut errors,
                ) else {
                    break 'modules;
                };
                generated_values = values;
                generated_blocks = blocks;
                generated_edges = edges;
                functions.push(lowered);
            } else if errors.len() == diagnostics_before {
                errors.at(
                    "ZRYNA-M3008",
                    span(input.sources(), function.span),
                    format!("function '{}' could not be lowered", function.name.text),
                    "reduce the function to one exact supported semantic form",
                );
            }
        }
        modules.push(raw::Module {
            id: raw::ModuleId(u32::try_from(module_index).unwrap_or(u32::MAX)),
            source_file: file.id(),
            data_declarations: graph.modules[module_index].data_declarations,
            functions,
        });
    }
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    preflight_program_borrow_calls(input, &catalog, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let claims = raw::AuthorityClaims {
        runtime: input.command.map_or(RuntimeContractIdentity::OwnershipRuntimeV1, |_| {
            RuntimeContractIdentity::CommandH1V1
        }),
        type_universe: linear.universe_identity().as_bytes(),
        linear32_fingerprint: *linear.fingerprint(),
        linux_x86_64_fingerprint: *linux.fingerprint(),
    };
    Ok(Candidate {
        program: raw::Program {
            authorities: claims,
            entry_module: raw::ModuleId(input.entry().index()),
            modules,
        },
        linear,
        linux,
        runtime_abi,
    })
}

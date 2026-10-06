//! Getter observations complement the complete revision-bound sealed Debug observation.
use serde_json::{Value, json};
use std::fmt::Debug;
use zryna_source::Span;

fn debug(value: impl Debug) -> String {
    format!("{value:?}")
}
fn span(value: Span) -> Value {
    json!([value.file().index(), value.start(), value.end()])
}

pub fn m1(program: &zryna_ir::VerifiedProgram) -> Value {
    json!({"profile":debug(program.profile()),"abi":debug(program.scalar_abi()),
        "functions":program.functions().map(|function| json!({
            "export":debug(function.export_name()),"abi_export":debug(function.abi_export()),
            "parameters":debug(function.parameters()),"result":debug(function.return_type()),
            "expressions":debug(function.expressions()),"body":debug(function.body())
        })).collect::<Vec<_>>()})
}

pub fn m2(program: &zryna_ir::control_flow_v1::VerifiedProgram) -> Value {
    json!({"entry_module":debug(program.entry_module()),"abi":debug(program.scalar_abi()),
        "modules":program.modules().map(|module| json!({
            "id":debug(module.id()),"source_file":module.source_file().index(),
            "functions":module.functions().map(|function| json!({
                "id":debug(function.id()),"public_export":debug(function.public_export()),
                "parameters":function.parameters().map(|(id,ty,location)|json!({"id":debug(id),"type":debug(ty),"span":span(location)})).collect::<Vec<_>>(),
                "result":debug(function.result()),"span":span(function.span()),
                "blocks":function.blocks().map(|block| json!({
                    "id":debug(block.id()),
                    "parameters":block.parameters().map(|(id,ty,location)|json!({"id":debug(id),"type":debug(ty),"span":span(location)})).collect::<Vec<_>>(),
                    "instructions":block.instructions().map(|instruction|json!({
                        "result":debug(instruction.result()),"type":debug(instruction.ty()),
                        "span":span(instruction.span()),"operation":debug(instruction.kind())
                    })).collect::<Vec<_>>(),
                    "terminator":{"span":span(block.terminator().span()),"operation":debug(block.terminator().kind())}
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        })).collect::<Vec<_>>()})
}

pub fn m3(program: &zryna_ir::data_ownership_v1::VerifiedProgram) -> Value {
    json!({"identity":debug(program.identity()),"source_map_identity":debug(program.source_map_identity()),
        "type_universe_identity":debug(program.type_universe_identity()),
        "runtime_contract":debug(program.runtime_contract()),"abi":debug(program.scalar_abi()),
        "linear32_layouts":debug(program.linear32_layouts()),
        "linux_x86_64_layouts":debug(program.linux_x86_64_layouts()),
        "modules":program.modules().map(|module|json!({
            "id":debug(module.id()),"source_file":module.source_file().index(),
            "data_declarations":module.data_declarations(),
            "functions":module.functions().map(|function|json!({
                "id":debug(function.id()),"public_export":debug(function.public_export()),
                "result_type":debug(function.result_type()),
                "parameters":function.parameters().map(|value|json!({"id":debug(value.id()),"type":debug(value.ty()),"span":span(value.span())})).collect::<Vec<_>>(),
                "borrow_parameters":function.borrow_parameters().map(|value|json!({"id":debug(value.id()),"referent":debug(value.referent()),"access":debug(value.access()),"span":span(value.span())})).collect::<Vec<_>>(),
                "places":function.places().map(|place|json!({"id":debug(place.id()),"type":debug(place.ty()),"is_copy":place.is_copy(),"span":span(place.span()),"kind":debug(place.kind())})).collect::<Vec<_>>(),
                "cleanup_plans":function.cleanup_plans().map(|plan|json!({"id":debug(plan.id()),"actions":debug(plan.actions().collect::<Vec<_>>()),"span":span(plan.span()),"site":debug(plan.site())})).collect::<Vec<_>>(),
                "blocks":function.blocks().map(|block|json!({
                    "id":debug(block.id()),
                    "parameters":block.parameters().map(|value|json!({"id":debug(value.id()),"type":debug(value.ty()),"span":span(value.span())})).collect::<Vec<_>>(),
                    "instructions":block.instructions().map(|instruction|json!({
                        "operation":debug(instruction.kind()),"result":debug(instruction.result()),
                        "result_type":debug(instruction.result_type()),"span":span(instruction.span()),
                        "value_operands":debug(instruction.value_operands().collect::<Vec<_>>()),
                        "place_operands":debug(instruction.place_operands().collect::<Vec<_>>()),
                        "cleanup":debug(instruction.cleanup()),
                        "backend_view":debug(instruction.backend_instruction()),
                        "allocation_failure_actions":debug(instruction.allocation_failure_drop_actions().collect::<Vec<_>>()),
                        "drop_actions":debug(instruction.derived_drop_actions().collect::<Vec<_>>()),
                        "bool_literal":instruction.bool_literal(),"i32_literal":instruction.i32_literal(),
                        "string_utf8_bytes":instruction.string_utf8_bytes(),"callee":debug(instruction.callee()),
                        "call_arguments":debug(instruction.call_arguments().collect::<Vec<_>>()),
                        "variant":instruction.variant(),"borrow":debug(instruction.borrow()),
                        "borrow_access":debug(instruction.borrow_access()),
                        "vec_clone_element_cleanup":debug(instruction.vec_clone_element_cleanup()),
                        "aggregate_clone_element_cleanup":debug(instruction.aggregate_clone_element_cleanup()),
                        "aggregate_clone_fallible_leaf_count":instruction.aggregate_clone_fallible_leaf_count(),
                        "vec_clone_failure_actions":debug(instruction.vec_clone_element_failure_drop_actions().collect::<Vec<_>>()),
                        "aggregate_clone_failure_actions":debug(instruction.aggregate_clone_element_failure_drop_actions().collect::<Vec<_>>())
                    })).collect::<Vec<_>>(),
                    "terminator":{"operation":debug(block.terminator().kind()),"span":span(block.terminator().span()),
                        "value_operands":debug(block.terminator().value_operands().collect::<Vec<_>>()),
                        "place_operands":debug(block.terminator().place_operands().collect::<Vec<_>>()),
                        "cleanup":debug(block.terminator().cleanup()),"trap":debug(block.terminator().trap_identity()),
                        "backend_view":debug(block.terminator().backend_terminator()),
                        "edges":debug(block.terminator().edges().collect::<Vec<_>>()),
                        "enum_arms":debug(block.terminator().enum_arms().collect::<Vec<_>>()),
                        "drop_actions":debug(block.terminator().derived_drop_actions().collect::<Vec<_>>())}
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        })).collect::<Vec<_>>()})
}

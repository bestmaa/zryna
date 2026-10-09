/// Maximum retained semantic diagnostics, including the terminal budget diagnostic.
pub const MAX_SEMANTIC_DIAGNOSTICS: usize = 256;

const _: () = {
    assert!(zryna_syntax::v2::MAX_FUNCTIONS_PER_PROJECT <= zryna_ir::MAX_IR_FUNCTIONS);
    assert!(
        zryna_syntax::v2::MAX_PARAMETERS_PER_FUNCTION <= zryna_ir::MAX_IR_PARAMETERS_PER_FUNCTION
    );
    assert!(
        zryna_syntax::v2::MAX_PARAMETERS_PER_PROJECT <= zryna_ir::MAX_IR_PARAMETERS_PER_PROGRAM
    );
    assert!(
        zryna_syntax::v2::MAX_EXPRESSIONS_PER_FUNCTION <= zryna_ir::MAX_IR_EXPRESSIONS_PER_FUNCTION
    );
    assert!(
        zryna_syntax::v2::MAX_EXPRESSIONS_PER_PROJECT <= zryna_ir::MAX_IR_EXPRESSIONS_PER_PROGRAM
    );
    assert!(zryna_syntax::v2::MAX_EXPRESSION_DEPTH <= zryna_ir::MAX_IR_EXPRESSION_DEPTH);
};

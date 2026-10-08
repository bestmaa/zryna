use super::{
    MAX_AGGREGATE_OPERANDS_PER_PROJECT, MAX_AGGREGATE_SOURCE_BYTES, MAX_BLOCKS_PER_FUNCTION,
    MAX_BLOCKS_PER_PROJECT, MAX_DATA_DECLARATIONS_PER_MODULE, MAX_DATA_DECLARATIONS_PER_PROJECT,
    MAX_ELEMENTS_PER_CONSTRUCTION, MAX_EXPRESSIONS_PER_FUNCTION, MAX_EXPRESSIONS_PER_PROJECT,
    MAX_FUNCTIONS_PER_MODULE, MAX_FUNCTIONS_PER_PROJECT, MAX_IMPORTED_NAMES_PER_DECLARATION,
    MAX_IMPORTED_NAMES_PER_PROJECT, MAX_IMPORTS_PER_MODULE, MAX_IMPORTS_PER_PROJECT,
    MAX_INITIALIZERS_PER_CONSTRUCTION, MAX_MATCH_ARMS_PER_EXPRESSION, MAX_MATCH_ARMS_PER_PROJECT,
    MAX_MEMBERS_PER_DECLARATION, MAX_MEMBERS_PER_PROJECT, MAX_PARAMETERS_PER_FUNCTION,
    MAX_PARAMETERS_PER_PROJECT, MAX_PROVIDER_DIAGNOSTICS, MAX_SOURCE_FILES,
    MAX_STATEMENTS_PER_FUNCTION, MAX_STATEMENTS_PER_PROJECT, MAX_TYPE_NODES_PER_MODULE,
    MAX_TYPE_NODES_PER_PROJECT, RawDataDeclarationKind, RawExpressionKind,
    RawProjectSyntaxSnapshot, SourceMap,
};

#[derive(Default)]
struct Counts {
    imports: usize,
    bindings: usize,
    declarations: usize,
    members: usize,
    types: usize,
    functions: usize,
    parameters: usize,
    blocks: usize,
    statements: usize,
    expressions: usize,
    aggregate_operands: usize,
    match_arms: usize,
}
impl Counts {
    fn add(slot: &mut usize, value: usize) -> bool {
        if let Some(next) = slot.checked_add(value) {
            *slot = next;
            true
        } else {
            false
        }
    }
    fn exceeded(&self) -> bool {
        self.imports > MAX_IMPORTS_PER_PROJECT
            || self.bindings > MAX_IMPORTED_NAMES_PER_PROJECT
            || self.declarations > MAX_DATA_DECLARATIONS_PER_PROJECT
            || self.members > MAX_MEMBERS_PER_PROJECT
            || self.types > MAX_TYPE_NODES_PER_PROJECT
            || self.functions > MAX_FUNCTIONS_PER_PROJECT
            || self.parameters > MAX_PARAMETERS_PER_PROJECT
            || self.blocks > MAX_BLOCKS_PER_PROJECT
            || self.statements > MAX_STATEMENTS_PER_PROJECT
            || self.expressions > MAX_EXPRESSIONS_PER_PROJECT
            || self.aggregate_operands > MAX_AGGREGATE_OPERANDS_PER_PROJECT
            || self.match_arms > MAX_MATCH_ARMS_PER_PROJECT
    }
}

pub(in crate::v4) fn check_budgets(
    raw: &RawProjectSyntaxSnapshot,
    sources: &SourceMap,
) -> Result<(), &'static str> {
    if raw.files.len() > MAX_SOURCE_FILES {
        return Err("syntax snapshot exceeds the source-file limit");
    }
    if raw.diagnostics.len() > MAX_PROVIDER_DIAGNOSTICS {
        return Err("provider diagnostics exceed the protocol-v4 limit");
    }
    let mut source_bytes = 0usize;
    for index in 0..sources.len() {
        let id = sources
            .verify_file_id(u32::try_from(index).map_err(|_| "source id overflow")?)
            .map_err(|_| "source id is not canonical")?;
        let source = sources.source(id).ok_or("source file is unavailable")?;
        source_bytes =
            source_bytes.checked_add(source.text().len()).ok_or("source byte count overflow")?;
    }
    if source_bytes > MAX_AGGREGATE_SOURCE_BYTES {
        return Err("source map exceeds the protocol-v4 aggregate byte limit");
    }
    let mut c = Counts::default();
    for file in &raw.files {
        if file.imports.len() > MAX_IMPORTS_PER_MODULE
            || file.type_syntax.len() > MAX_TYPE_NODES_PER_MODULE
            || file.data_declarations.len() > MAX_DATA_DECLARATIONS_PER_MODULE
            || file.functions.len() > MAX_FUNCTIONS_PER_MODULE
        {
            return Err("one module exceeds a protocol-v4 collection limit");
        }
        if !Counts::add(&mut c.imports, file.imports.len())
            || !Counts::add(&mut c.declarations, file.data_declarations.len())
            || !Counts::add(&mut c.types, file.type_syntax.len())
            || !Counts::add(&mut c.functions, file.functions.len())
        {
            return Err("project count overflow");
        }
        for import in &file.imports {
            if import.bindings.is_empty()
                || import.bindings.len() > MAX_IMPORTED_NAMES_PER_DECLARATION
                || !Counts::add(&mut c.bindings, import.bindings.len())
            {
                return Err("import binding count overflow");
            }
        }
        for declaration in &file.data_declarations {
            let n = match &declaration.kind {
                RawDataDeclarationKind::Struct { fields, .. } => fields.len(),
                RawDataDeclarationKind::Enum { variants, .. } => variants.len(),
            };
            if n == 0 || n > MAX_MEMBERS_PER_DECLARATION || !Counts::add(&mut c.members, n) {
                return Err("data member count overflow");
            }
        }
        for function in &file.functions {
            if function.parameters.len() > MAX_PARAMETERS_PER_FUNCTION
                || function.body.blocks.is_empty()
                || function.body.blocks.len() > MAX_BLOCKS_PER_FUNCTION
                || function.body.statements.len() > MAX_STATEMENTS_PER_FUNCTION
                || function.body.expressions.len() > MAX_EXPRESSIONS_PER_FUNCTION
                || function
                    .body
                    .blocks
                    .iter()
                    .any(|block| block.statements.len() > MAX_STATEMENTS_PER_FUNCTION)
            {
                return Err("one function exceeds a protocol-v4 collection limit");
            }
            if !Counts::add(&mut c.parameters, function.parameters.len())
                || !Counts::add(&mut c.blocks, function.body.blocks.len())
                || !Counts::add(&mut c.statements, function.body.statements.len())
                || !Counts::add(&mut c.expressions, function.body.expressions.len())
            {
                return Err("function arena count overflow");
            }
            for expression in &function.body.expressions {
                match &expression.kind {
                    RawExpressionKind::Call { arguments, .. }
                        if arguments.len() > MAX_PARAMETERS_PER_FUNCTION =>
                    {
                        return Err("call argument count exceeds its protocol-v4 limit");
                    }
                    RawExpressionKind::StructConstruction { fields, .. } => {
                        if fields.len() > MAX_INITIALIZERS_PER_CONSTRUCTION {
                            return Err("construction field count exceeds its protocol-v4 limit");
                        }
                        if !Counts::add(&mut c.aggregate_operands, fields.len()) {
                            return Err("initializer count overflow");
                        }
                    }
                    RawExpressionKind::FixedArrayConstruction { elements, .. }
                    | RawExpressionKind::VecConstruction { elements, .. } => {
                        if elements.len() > MAX_ELEMENTS_PER_CONSTRUCTION {
                            return Err("construction element count exceeds its protocol-v4 limit");
                        }
                        if !Counts::add(&mut c.aggregate_operands, elements.len()) {
                            return Err("element count overflow");
                        }
                    }
                    RawExpressionKind::Match { arms, .. } => {
                        if arms.len() > MAX_MATCH_ARMS_PER_EXPRESSION {
                            return Err("match arm count exceeds its protocol-v4 limit");
                        }
                        if !Counts::add(&mut c.match_arms, arms.len()) {
                            return Err("match arm count overflow");
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if c.exceeded() {
        Err("syntax snapshot exceeds an aggregate protocol-v4 limit")
    } else {
        Ok(())
    }
}

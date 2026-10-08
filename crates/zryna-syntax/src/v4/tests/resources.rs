use super::*;

#[test]
fn public_raw_vectors_enforce_every_exact_collection_limit() {
    let authority = sources(SOURCE);
    let accepted = |value: &RawProjectSyntaxSnapshot| check_budgets(value, &authority).is_ok();
    let zero = span_range(0, 0);
    let ident = RawIdentifierSyntax { text: "x".into(), span: zero };
    let binding = RawImportBindingSyntax {
        span: zero,
        imported: ident.clone(),
        local: ident.clone(),
        as_span: None,
    };
    let import = RawImportSyntax {
        span: zero,
        import_span: zero,
        bindings: vec![binding.clone()],
        from_span: zero,
        specifier: RawModuleSpecifierSyntax {
            text: "./x.zry".into(),
            token_span: zero,
            value_span: zero,
        },
        semicolon_span: zero,
    };
    let field = RawDataField {
        span: zero,
        name: ident.clone(),
        colon_span: zero,
        type_syntax: 0,
        semicolon_span: zero,
    };
    let variant = RawEnumVariant {
        span: zero,
        name: ident.clone(),
        colon_span: zero,
        payload_type: None,
        none_span: Some(zero),
        semicolon_span: zero,
    };
    let struct_declaration = RawDataDeclaration {
        span: zero,
        export_span: None,
        kind: RawDataDeclarationKind::Struct {
            interface_span: zero,
            name: ident.clone(),
            extends_span: zero,
            marker_span: zero,
            open_brace_span: zero,
            fields: vec![field.clone()],
            close_brace_span: zero,
        },
    };
    let enum_declaration = RawDataDeclaration {
        span: zero,
        export_span: None,
        kind: RawDataDeclarationKind::Enum {
            interface_span: zero,
            name: ident.clone(),
            extends_span: zero,
            marker_span: zero,
            open_brace_span: zero,
            variants: vec![variant.clone()],
            close_brace_span: zero,
        },
    };
    let diagnostic = RawProviderDiagnostic {
        code: "P1".into(),
        severity: Severity::Warning,
        location: RawDiagnosticLocation::Global,
        message: String::new(),
        guidance: String::new(),
    };

    let mut value = raw();
    value.files = vec![value.files[0].clone(); MAX_SOURCE_FILES];
    assert!(accepted(&value));
    value.files.push(value.files[0].clone());
    assert!(!accepted(&value));

    let mut value = raw();
    value.diagnostics = vec![diagnostic; MAX_PROVIDER_DIAGNOSTICS];
    assert!(accepted(&value));
    value.diagnostics.push(value.diagnostics[0].clone());
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].imports = vec![import.clone(); MAX_IMPORTS_PER_MODULE];
    assert!(accepted(&value));
    value.files[0].imports.push(import.clone());
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].imports = vec![import.clone()];
    value.files[0].imports[0].bindings = vec![binding.clone(); MAX_IMPORTED_NAMES_PER_DECLARATION];
    assert!(accepted(&value));
    value.files[0].imports[0].bindings.push(binding);
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].type_syntax =
        vec![value.files[0].type_syntax[0].clone(); MAX_TYPE_NODES_PER_MODULE];
    assert!(accepted(&value));
    let extra = value.files[0].type_syntax[0].clone();
    value.files[0].type_syntax.push(extra);
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].data_declarations =
        vec![struct_declaration.clone(); MAX_DATA_DECLARATIONS_PER_MODULE];
    assert!(accepted(&value));
    value.files[0].data_declarations.push(struct_declaration.clone());
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].data_declarations = vec![struct_declaration.clone()];
    let RawDataDeclarationKind::Struct { fields, .. } =
        &mut value.files[0].data_declarations[0].kind
    else {
        unreachable!();
    };
    *fields = vec![field; MAX_MEMBERS_PER_DECLARATION];
    assert!(accepted(&value));
    let RawDataDeclarationKind::Struct { fields, .. } =
        &mut value.files[0].data_declarations[0].kind
    else {
        unreachable!();
    };
    fields.push(fields[0].clone());
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].data_declarations = vec![enum_declaration];
    let RawDataDeclarationKind::Enum { variants, .. } =
        &mut value.files[0].data_declarations[0].kind
    else {
        unreachable!();
    };
    *variants = vec![variant; MAX_MEMBERS_PER_DECLARATION];
    assert!(accepted(&value));
    let RawDataDeclarationKind::Enum { variants, .. } =
        &mut value.files[0].data_declarations[0].kind
    else {
        unreachable!();
    };
    variants.push(variants[0].clone());
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].functions = vec![value.files[0].functions[0].clone(); MAX_FUNCTIONS_PER_MODULE];
    assert!(accepted(&value));
    let extra = value.files[0].functions[0].clone();
    value.files[0].functions.push(extra);
    assert!(!accepted(&value));

    let parameter = RawParameterSyntax { span: zero, name: ident.clone(), type_syntax: 0 };
    let mut value = raw();
    value.files[0].functions[0].parameters = vec![parameter.clone(); MAX_PARAMETERS_PER_FUNCTION];
    assert!(accepted(&value));
    value.files[0].functions[0].parameters.push(parameter);
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].functions[0].body.blocks =
        vec![value.files[0].functions[0].body.blocks[0].clone(); MAX_BLOCKS_PER_FUNCTION];
    assert!(accepted(&value));
    let extra = value.files[0].functions[0].body.blocks[0].clone();
    value.files[0].functions[0].body.blocks.push(extra);
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].functions[0].body.statements =
        vec![value.files[0].functions[0].body.statements[0].clone(); MAX_STATEMENTS_PER_FUNCTION];
    assert!(accepted(&value));
    let extra = value.files[0].functions[0].body.statements[0].clone();
    value.files[0].functions[0].body.statements.push(extra);
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].functions[0].body.blocks[0].statements = vec![0; MAX_STATEMENTS_PER_FUNCTION];
    assert!(accepted(&value));
    value.files[0].functions[0].body.blocks[0].statements.push(0);
    assert!(!accepted(&value));

    let mut value = raw();
    value.files[0].functions[0].body.expressions =
        vec![value.files[0].functions[0].body.expressions[0].clone(); MAX_EXPRESSIONS_PER_FUNCTION];
    assert!(accepted(&value));
    let extra = value.files[0].functions[0].body.expressions[0].clone();
    value.files[0].functions[0].body.expressions.push(extra);
    assert!(!accepted(&value));

    let mut call = raw();
    call.files[0].functions[0].body.expressions[0].kind = RawExpressionKind::Call {
        callee: ident.clone(),
        open_paren_span: zero,
        arguments: vec![0; MAX_PARAMETERS_PER_FUNCTION],
        close_paren_span: zero,
    };
    assert!(accepted(&call));
    let RawExpressionKind::Call { arguments, .. } =
        &mut call.files[0].functions[0].body.expressions[0].kind
    else {
        unreachable!();
    };
    arguments.push(0);
    assert!(!accepted(&call));

    let initializer = RawFieldInitializer {
        span: zero,
        kind: RawFieldInitializerKind::Shorthand { name: ident.clone(), value: 0 },
    };
    let mut construction = raw();
    construction.files[0].functions[0].body.expressions[0].kind =
        RawExpressionKind::StructConstruction {
            type_name: ident.clone(),
            open_paren_span: zero,
            open_brace_span: zero,
            fields: vec![initializer.clone(); MAX_INITIALIZERS_PER_CONSTRUCTION],
            close_brace_span: zero,
            close_paren_span: zero,
        };
    assert!(accepted(&construction));
    let RawExpressionKind::StructConstruction { fields, .. } =
        &mut construction.files[0].functions[0].body.expressions[0].kind
    else {
        unreachable!();
    };
    fields.push(initializer);
    assert!(!accepted(&construction));

    for fixed in [false, true] {
        let mut construction = raw();
        let elements = vec![0; MAX_ELEMENTS_PER_CONSTRUCTION];
        construction.files[0].functions[0].body.expressions[0].kind = if fixed {
            RawExpressionKind::FixedArrayConstruction {
                type_syntax: 0,
                open_paren_span: zero,
                open_bracket_span: zero,
                elements,
                close_bracket_span: zero,
                close_paren_span: zero,
            }
        } else {
            RawExpressionKind::VecConstruction {
                type_syntax: 0,
                open_paren_span: zero,
                open_bracket_span: zero,
                elements,
                close_bracket_span: zero,
                close_paren_span: zero,
            }
        };
        assert!(accepted(&construction));
        match &mut construction.files[0].functions[0].body.expressions[0].kind {
            RawExpressionKind::FixedArrayConstruction { elements, .. }
            | RawExpressionKind::VecConstruction { elements, .. } => elements.push(0),
            _ => unreachable!(),
        }
        assert!(!accepted(&construction));
    }

    let arm = RawMatchArm {
        span: zero,
        type_name: ident.clone(),
        dot_span: zero,
        variant: ident,
        binding: None,
        arrow_span: zero,
        value: 0,
    };
    let mut matching = raw();
    matching.files[0].functions[0].body.expressions[0].kind = RawExpressionKind::Match {
        keyword_span: zero,
        open_paren_span: zero,
        scrutinee: 0,
        close_paren_span: zero,
        open_brace_span: zero,
        arms: vec![arm.clone(); MAX_MATCH_ARMS_PER_EXPRESSION],
        close_brace_span: zero,
    };
    assert!(accepted(&matching));
    let RawExpressionKind::Match { arms, .. } =
        &mut matching.files[0].functions[0].body.expressions[0].kind
    else {
        unreachable!();
    };
    arms.push(arm);
    assert!(!accepted(&matching));
}

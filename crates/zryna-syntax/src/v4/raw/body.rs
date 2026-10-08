use super::{
    Deserialize, RawExpressionSyntax, RawIdentifierSyntax, Serialize, UntrustedSpan, blocks,
    expressions, parameters, statement_ids, statements,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawFunctionSyntax {
    pub span: UntrustedSpan,
    pub export_span: Option<UntrustedSpan>,
    pub function_span: UntrustedSpan,
    pub name: RawIdentifierSyntax,
    #[serde(deserialize_with = "parameters")]
    pub parameters: Vec<RawParameterSyntax>,
    pub result_type: u32,
    pub body: RawFunctionBodySyntax,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawParameterSyntax {
    pub span: UntrustedSpan,
    pub name: RawIdentifierSyntax,
    pub type_syntax: u32,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawFunctionBodySyntax {
    pub span: UntrustedSpan,
    pub root_block: u32,
    #[serde(deserialize_with = "blocks")]
    pub blocks: Vec<RawBlockSyntax>,
    #[serde(deserialize_with = "statements")]
    pub statements: Vec<RawStatementSyntax>,
    #[serde(deserialize_with = "expressions")]
    pub expressions: Vec<RawExpressionSyntax>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawBlockSyntax {
    pub span: UntrustedSpan,
    pub open_brace_span: UntrustedSpan,
    #[serde(deserialize_with = "statement_ids")]
    pub statements: Vec<u32>,
    pub close_brace_span: UntrustedSpan,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawStatementSyntax {
    pub span: UntrustedSpan,
    pub kind: RawStatementKind,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RawStatementKind {
    LocalDeclaration {
        keyword_span: UntrustedSpan,
        mutable: bool,
        name: RawIdentifierSyntax,
        type_syntax: u32,
        equals_span: UntrustedSpan,
        initializer: u32,
        semicolon_span: UntrustedSpan,
    },
    Assignment {
        target: u32,
        equals_span: UntrustedSpan,
        value: u32,
        semicolon_span: UntrustedSpan,
    },
    Return {
        keyword_span: UntrustedSpan,
        value: u32,
        semicolon_span: UntrustedSpan,
    },
    Block {
        block: u32,
    },
    If {
        keyword_span: UntrustedSpan,
        open_paren_span: UntrustedSpan,
        condition: u32,
        close_paren_span: UntrustedSpan,
        then_block: u32,
        else_clause: Option<RawElseSyntax>,
    },
    While {
        keyword_span: UntrustedSpan,
        open_paren_span: UntrustedSpan,
        condition: u32,
        close_paren_span: UntrustedSpan,
        body_block: u32,
    },
    ExpressionStatement {
        expression: u32,
        semicolon_span: UntrustedSpan,
    },
    WeakUpgrade {
        keyword_span: UntrustedSpan,
        weak: u32,
        as_span: UntrustedSpan,
        binding: RawIdentifierSyntax,
        success_block: u32,
        else_span: UntrustedSpan,
        failure_block: u32,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawElseSyntax {
    pub keyword_span: UntrustedSpan,
    pub block: u32,
}

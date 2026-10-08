use super::{
    Deserialize, RawIdentifierSyntax, Serialize, UntrustedSpan, arguments, arms, elements,
    initializers,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawExpressionSyntax {
    pub span: UntrustedSpan,
    pub kind: RawExpressionKind,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RawExpressionKind {
    Reference {
        name: RawIdentifierSyntax,
    },
    BoolLiteral {
        value: bool,
    },
    I32Literal {
        spelling: String,
    },
    StringLiteral {
        spelling: String,
    },
    Negation {
        operator_span: UntrustedSpan,
        operand: u32,
    },
    Addition {
        operator_span: UntrustedSpan,
        lhs: u32,
        rhs: u32,
    },
    Subtraction {
        operator_span: UntrustedSpan,
        lhs: u32,
        rhs: u32,
    },
    Multiplication {
        operator_span: UntrustedSpan,
        lhs: u32,
        rhs: u32,
    },
    Equal {
        operator_span: UntrustedSpan,
        lhs: u32,
        rhs: u32,
    },
    NotEqual {
        operator_span: UntrustedSpan,
        lhs: u32,
        rhs: u32,
    },
    LessThan {
        operator_span: UntrustedSpan,
        lhs: u32,
        rhs: u32,
    },
    LessEqual {
        operator_span: UntrustedSpan,
        lhs: u32,
        rhs: u32,
    },
    GreaterThan {
        operator_span: UntrustedSpan,
        lhs: u32,
        rhs: u32,
    },
    GreaterEqual {
        operator_span: UntrustedSpan,
        lhs: u32,
        rhs: u32,
    },
    Call {
        callee: RawIdentifierSyntax,
        open_paren_span: UntrustedSpan,
        #[serde(deserialize_with = "arguments")]
        arguments: Vec<u32>,
        close_paren_span: UntrustedSpan,
    },
    StructConstruction {
        type_name: RawIdentifierSyntax,
        open_paren_span: UntrustedSpan,
        open_brace_span: UntrustedSpan,
        #[serde(deserialize_with = "initializers")]
        fields: Vec<RawFieldInitializer>,
        close_brace_span: UntrustedSpan,
        close_paren_span: UntrustedSpan,
    },
    EnumConstruction {
        type_name: RawIdentifierSyntax,
        dot_span: UntrustedSpan,
        variant: RawIdentifierSyntax,
        open_paren_span: UntrustedSpan,
        payload: Option<u32>,
        close_paren_span: UntrustedSpan,
    },
    FixedArrayConstruction {
        type_syntax: u32,
        open_paren_span: UntrustedSpan,
        open_bracket_span: UntrustedSpan,
        #[serde(deserialize_with = "elements")]
        elements: Vec<u32>,
        close_bracket_span: UntrustedSpan,
        close_paren_span: UntrustedSpan,
    },
    VecConstruction {
        type_syntax: u32,
        open_paren_span: UntrustedSpan,
        open_bracket_span: UntrustedSpan,
        #[serde(deserialize_with = "elements")]
        elements: Vec<u32>,
        close_bracket_span: UntrustedSpan,
        close_paren_span: UntrustedSpan,
    },
    FieldAccess {
        base: u32,
        dot_span: UntrustedSpan,
        field: RawIdentifierSyntax,
    },
    Index {
        base: u32,
        open_bracket_span: UntrustedSpan,
        index: u32,
        close_bracket_span: UntrustedSpan,
    },
    Clone {
        keyword_span: UntrustedSpan,
        open_paren_span: UntrustedSpan,
        value: u32,
        close_paren_span: UntrustedSpan,
    },
    Shared {
        keyword_span: UntrustedSpan,
        open_paren_span: UntrustedSpan,
        value: u32,
        close_paren_span: UntrustedSpan,
    },
    Downgrade {
        keyword_span: UntrustedSpan,
        open_paren_span: UntrustedSpan,
        value: u32,
        close_paren_span: UntrustedSpan,
    },
    Borrow {
        keyword_span: UntrustedSpan,
        open_paren_span: UntrustedSpan,
        value: u32,
        close_paren_span: UntrustedSpan,
    },
    BorrowMut {
        keyword_span: UntrustedSpan,
        open_paren_span: UntrustedSpan,
        value: u32,
        close_paren_span: UntrustedSpan,
    },
    VecPush {
        keyword_span: UntrustedSpan,
        open_paren_span: UntrustedSpan,
        vector: u32,
        comma_span: UntrustedSpan,
        value: u32,
        close_paren_span: UntrustedSpan,
    },
    Match {
        keyword_span: UntrustedSpan,
        open_paren_span: UntrustedSpan,
        scrutinee: u32,
        close_paren_span: UntrustedSpan,
        open_brace_span: UntrustedSpan,
        #[serde(deserialize_with = "arms")]
        arms: Vec<RawMatchArm>,
        close_brace_span: UntrustedSpan,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawFieldInitializer {
    pub span: UntrustedSpan,
    pub kind: RawFieldInitializerKind,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RawFieldInitializerKind {
    Shorthand { name: RawIdentifierSyntax, value: u32 },
    Explicit { name: RawIdentifierSyntax, colon_span: UntrustedSpan, value: u32 },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawMatchArm {
    pub span: UntrustedSpan,
    pub type_name: RawIdentifierSyntax,
    pub dot_span: UntrustedSpan,
    pub variant: RawIdentifierSyntax,
    pub binding: Option<RawIdentifierSyntax>,
    pub arrow_span: UntrustedSpan,
    pub value: u32,
}

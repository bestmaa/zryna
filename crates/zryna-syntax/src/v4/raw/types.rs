use super::{Deserialize, RawIdentifierSyntax, Serialize, UntrustedSpan, fields, variants};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawTypeSyntax {
    pub span: UntrustedSpan,
    pub kind: RawTypeSyntaxKind,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RawTypeSyntaxKind {
    Missing,
    Named {
        name: RawIdentifierSyntax,
    },
    String {
        keyword_span: UntrustedSpan,
    },
    Vec {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    Shared {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    Weak {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    Borrow {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    BorrowMut {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        argument: u32,
        greater_than_span: UntrustedSpan,
    },
    FixedArray {
        keyword_span: UntrustedSpan,
        less_than_span: UntrustedSpan,
        element: u32,
        comma_span: UntrustedSpan,
        length_span: UntrustedSpan,
        length_spelling: String,
        length: u32,
        greater_than_span: UntrustedSpan,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawDataDeclaration {
    pub span: UntrustedSpan,
    pub export_span: Option<UntrustedSpan>,
    pub kind: RawDataDeclarationKind,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RawDataDeclarationKind {
    Struct {
        interface_span: UntrustedSpan,
        name: RawIdentifierSyntax,
        extends_span: UntrustedSpan,
        marker_span: UntrustedSpan,
        open_brace_span: UntrustedSpan,
        #[serde(deserialize_with = "fields")]
        fields: Vec<RawDataField>,
        close_brace_span: UntrustedSpan,
    },
    Enum {
        interface_span: UntrustedSpan,
        name: RawIdentifierSyntax,
        extends_span: UntrustedSpan,
        marker_span: UntrustedSpan,
        open_brace_span: UntrustedSpan,
        #[serde(deserialize_with = "variants")]
        variants: Vec<RawEnumVariant>,
        close_brace_span: UntrustedSpan,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawDataField {
    pub span: UntrustedSpan,
    pub name: RawIdentifierSyntax,
    pub colon_span: UntrustedSpan,
    pub type_syntax: u32,
    pub semicolon_span: UntrustedSpan,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawEnumVariant {
    pub span: UntrustedSpan,
    pub name: RawIdentifierSyntax,
    pub colon_span: UntrustedSpan,
    pub payload_type: Option<u32>,
    pub none_span: Option<UntrustedSpan>,
    pub semicolon_span: UntrustedSpan,
}

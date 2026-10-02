use serde::{Deserialize, Serialize};
use zryna_source::UntrustedSpan;

use super::decode::bounded;
use super::expressions::RawExpressionSyntax;
use super::{RawTypeParameterList, RawTypeSyntax};
use crate::v4::{
    RawBlockSyntax, RawDataField, RawEnumVariant, RawIdentifierSyntax, RawImportSyntax,
    RawProviderDiagnostic, RawStatementSyntax,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawProjectSyntaxSnapshot {
    pub schema_version: u32,
    #[serde(deserialize_with = "bounded::<_, _, 4096>")]
    pub files: Vec<RawSourceUnit>,
    #[serde(deserialize_with = "bounded::<_, _, 256>")]
    pub diagnostics: Vec<RawProviderDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawSourceUnit {
    pub id: u32,
    pub path: String,
    #[serde(deserialize_with = "bounded::<_, _, 4096>")]
    pub imports: Vec<RawImportSyntax>,
    #[serde(deserialize_with = "bounded::<_, _, 65536>")]
    pub type_syntax: Vec<RawTypeSyntax>,
    #[serde(deserialize_with = "bounded::<_, _, 4096>")]
    pub data_declarations: Vec<RawDataDeclaration>,
    #[serde(deserialize_with = "bounded::<_, _, 4096>")]
    pub functions: Vec<RawFunctionSyntax>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawDataDeclaration {
    pub span: UntrustedSpan,
    pub export_span: Option<UntrustedSpan>,
    pub type_parameters: Option<RawTypeParameterList>,
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
        #[serde(deserialize_with = "bounded::<_, _, 1024>")]
        fields: Vec<RawDataField>,
        close_brace_span: UntrustedSpan,
    },
    Enum {
        interface_span: UntrustedSpan,
        name: RawIdentifierSyntax,
        extends_span: UntrustedSpan,
        marker_span: UntrustedSpan,
        open_brace_span: UntrustedSpan,
        #[serde(deserialize_with = "bounded::<_, _, 1024>")]
        variants: Vec<RawEnumVariant>,
        close_brace_span: UntrustedSpan,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawFunctionSyntax {
    pub span: UntrustedSpan,
    pub export_span: Option<UntrustedSpan>,
    pub function_span: UntrustedSpan,
    pub name: RawIdentifierSyntax,
    pub type_parameters: Option<RawTypeParameterList>,
    #[serde(deserialize_with = "bounded::<_, _, 256>")]
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
    #[serde(deserialize_with = "bounded::<_, _, 4096>")]
    pub blocks: Vec<RawBlockSyntax>,
    #[serde(deserialize_with = "bounded::<_, _, 4096>")]
    pub statements: Vec<RawStatementSyntax>,
    #[serde(deserialize_with = "bounded::<_, _, 16384>")]
    pub expressions: Vec<RawExpressionSyntax>,
}

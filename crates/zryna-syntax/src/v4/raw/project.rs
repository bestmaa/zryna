use super::{
    Deserialize, RawDataDeclaration, RawFunctionSyntax, RawProviderDiagnostic, RawTypeSyntax,
    Serialize, UntrustedSpan, bindings, declarations, diagnostics, files, functions, imports,
    types,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawProjectSyntaxSnapshot {
    pub schema_version: u32,
    #[serde(deserialize_with = "files")]
    pub files: Vec<RawSourceUnit>,
    #[serde(deserialize_with = "diagnostics")]
    pub diagnostics: Vec<RawProviderDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawSourceUnit {
    pub id: u32,
    pub path: String,
    #[serde(deserialize_with = "imports")]
    pub imports: Vec<RawImportSyntax>,
    #[serde(deserialize_with = "types")]
    pub type_syntax: Vec<RawTypeSyntax>,
    #[serde(deserialize_with = "declarations")]
    pub data_declarations: Vec<RawDataDeclaration>,
    #[serde(deserialize_with = "functions")]
    pub functions: Vec<RawFunctionSyntax>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawIdentifierSyntax {
    pub text: String,
    pub span: UntrustedSpan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawImportSyntax {
    pub span: UntrustedSpan,
    pub import_span: UntrustedSpan,
    #[serde(deserialize_with = "bindings")]
    pub bindings: Vec<RawImportBindingSyntax>,
    pub from_span: UntrustedSpan,
    pub specifier: RawModuleSpecifierSyntax,
    pub semicolon_span: UntrustedSpan,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawImportBindingSyntax {
    pub span: UntrustedSpan,
    pub imported: RawIdentifierSyntax,
    pub local: RawIdentifierSyntax,
    pub as_span: Option<UntrustedSpan>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawModuleSpecifierSyntax {
    pub text: String,
    pub token_span: UntrustedSpan,
    pub value_span: UntrustedSpan,
}

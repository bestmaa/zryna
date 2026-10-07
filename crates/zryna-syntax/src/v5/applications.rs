use serde::{Deserialize, Serialize};
use zryna_source::UntrustedSpan;

use super::decode::bounded;
use crate::v4::RawIdentifierSyntax;

/// A source-spelled parameter, never a resolved type or runtime trait.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawTypeParameter {
    pub span: UntrustedSpan,
    pub name: RawIdentifierSyntax,
    pub extends_span: UntrustedSpan,
    pub bound: RawIdentifierSyntax,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawTypeParameterList {
    pub span: UntrustedSpan,
    pub less_than_span: UntrustedSpan,
    #[serde(deserialize_with = "bounded::<_, _, 2>")]
    pub parameters: Vec<RawTypeParameter>,
    #[serde(deserialize_with = "bounded::<_, _, 2>")]
    pub comma_spans: Vec<UntrustedSpan>,
    pub greater_than_span: UntrustedSpan,
}

/// Ordered references to distinct type occurrences in the module type arena.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawTypeArgumentList {
    pub span: UntrustedSpan,
    pub less_than_span: UntrustedSpan,
    #[serde(deserialize_with = "bounded::<_, _, 2>")]
    pub arguments: Vec<u32>,
    #[serde(deserialize_with = "bounded::<_, _, 2>")]
    pub comma_spans: Vec<UntrustedSpan>,
    pub greater_than_span: UntrustedSpan,
}

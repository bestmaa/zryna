//! Untrusted bounded-generics syntax protocol version 5.
//!
//! Decoding and declaration admission do not construct executable syntax authority.
//! Complete source/arena verification constructs a separate source-bound syntax seal.
//! Provider conformance, semantic admission and target execution remain separate gates.

#![allow(missing_docs)]

mod applications;
mod arena;
mod ast;
mod body;
mod constructions;
mod coverage;
mod declarations;
mod decode;
mod diagnostic_order;
mod diagnostics;
mod expression_source;
mod expressions;
mod matches;
mod modules;
mod rejections;
mod resources;
mod seal;
mod source;
mod statements;
mod type_ownership;
mod types;
mod verification;
mod wire;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod verification_tests;

#[cfg(test)]
mod keyword_tests;

pub use applications::{RawTypeArgumentList, RawTypeParameter, RawTypeParameterList};
pub use ast::*;
pub use decode::decode_snapshot;
pub use diagnostics::{DeclarationError, SyntaxDecodeError};
pub use expressions::{RawExpressionKind, RawExpressionSyntax};
pub use modules::validate_declarations;
pub use seal::VerifiedProjectSyntaxV5;
pub use types::{RawTypeSyntax, RawTypeSyntaxKind};
pub use verification::verify_snapshot;

pub const PROTOCOL_VERSION: u32 = 5;
pub const MAX_TYPE_PARAMETERS: usize = 2;
pub const MAX_TYPE_ARGUMENTS: usize = 2;

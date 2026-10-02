//! Versioned provider-neutral Zryna syntax contracts.

#![forbid(unsafe_code)]

/// Isolated untrusted native C v0 declaration wire; it grants no executable syntax authority.
pub mod native_c_v0;

/// Independently authenticated restricted foreign source syntax, without executable body proof.
pub mod native_c_source_v0;

/// Executable provider-neutral syntax protocol version 2.
pub mod v2;
pub mod v3;
/// Provider-neutral data and ownership syntax protocol version 4.
pub mod v4;

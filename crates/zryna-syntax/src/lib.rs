//! Versioned provider-neutral Zryna syntax contracts.

#![forbid(unsafe_code)]

/// Executable provider-neutral syntax protocol version 2.
pub mod v2;
pub mod v3;
/// Provider-neutral data and ownership syntax protocol version 4.
pub mod v4;
/// Untrusted bounded-generics syntax protocol version 5; no executable authority.
pub mod v5;

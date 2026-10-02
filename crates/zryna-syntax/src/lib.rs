//! Versioned provider-neutral Zryna syntax contracts.

#![forbid(unsafe_code)]

/// Source-bound requirements for the separate bounded command gate.
pub mod command_h1_v1;
/// Executable provider-neutral syntax protocol version 2.
pub mod v2;
pub mod v3;
/// Provider-neutral data and ownership syntax protocol version 4.
pub mod v4;

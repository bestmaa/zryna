//! Private admission of one immutable command input against its verified source key.

mod capture;
mod wire;

pub(crate) use capture::CapturedRequest;

#[cfg(test)]
mod tests;

use zryna_diagnostics::Diagnostic;

pub(crate) const MAX_REQUEST_BYTES: usize = 4096;
const MAX_KEY_BYTES: usize = 64;
const MAX_VALUE_BYTES: usize = 1024;
const SCHEMA: &str = "zryna.wasi-command-request.v1";
const WORLD: &str = "zryna:capability-profiles/command@0.1.0";

/// Owned input admitted against the complete verified command requirement.
///
/// Parsing does not approve a root request or confer host capability authority.
/// Secret bytes have no debug or serialization view.
#[derive(Eq, PartialEq)]
pub(crate) struct CommandRequest {
    key: String,
    input: Input,
}

#[derive(Eq, PartialEq)]
enum Input {
    Missing,
    Present(String),
}

impl CommandRequest {
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn value(&self) -> Option<&str> {
        self.input.value()
    }

    pub(crate) fn value_byte_count(&self) -> usize {
        self.value().map_or(0, str::len)
    }
}

/// Validates only the captured bytes and exact source-derived requirement.
///
/// Omission is handled by the caller. A supplied request cannot match a pure
/// command, and the caller must derive `required_key` from verified source.
pub(crate) fn admit(
    bytes: &[u8],
    required_key: Option<&str>,
) -> Result<CommandRequest, Diagnostic> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(rejection());
    }
    let required_key = required_key.ok_or_else(rejection)?;
    let text = std::str::from_utf8(bytes).map_err(|_| rejection())?;
    let request = wire::decode(text).map_err(|_| rejection())?;
    if request.schema != SCHEMA
        || request.world != WORLD
        || request.grant.capability != "environment"
        || request.grant.key.is_empty()
        || request.grant.key.len() > MAX_KEY_BYTES
        || request.grant.key != required_key
        || request.input.value().is_some_and(|value| value.len() > MAX_VALUE_BYTES)
    {
        return Err(rejection());
    }
    Ok(CommandRequest { key: request.grant.key, input: request.input })
}

impl Input {
    fn value(&self) -> Option<&str> {
        match self {
            Self::Missing => None,
            Self::Present(value) => Some(value),
        }
    }
}

fn rejection() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-D4101",
        None,
        "Command request does not match the required bounded input contract.",
        "Supply one valid private command request matching the verified source requirement.",
    )
}

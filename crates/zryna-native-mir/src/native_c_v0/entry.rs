//! Fixed compiler-private channels. These records alone grant no executable or source authority.

use super::abi::{Location, Register};

/// Version of the checked private context/input/outcome channel protocol.
pub const PRIVATE_ENTRY_CONTRACT: &str = "zryna-native-c-private-entry-v0";
/// Hidden driver dispatcher, separate from source-declared C exports and runtime helpers.
pub const DISPATCH_SYMBOL: &str = "zryna_c_v0_i_dispatch";

/// Role of one compiler-private physical argument, never a foreign source parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelRole {
    /// Driver-created checked execution instance with a shared foreign obligation ledger.
    Context,
    /// Typed private/scalar inputs bound to the original function's genuine layouts.
    Inputs,
    /// Typed result or terminal failure, exposed only after required cleanup.
    Outcome,
    /// Complete-program function ordinal selected by a checked driver invocation.
    FunctionOrdinal,
}

/// Fixed INTEGER lane for a compiler-private channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelLane {
    /// Context/input/outcome pointer or dispatcher ordinal.
    pub role: ChannelRole,
    /// Sixty-four pointer bits, or the dispatcher's low thirty-two ordinal bits.
    pub bits: u8,
    /// Exact System V INTEGER argument location.
    pub location: Location,
}

/// Closed in-process tagged result classes; there is no scalar encoding of process failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutcomeTag {
    /// Typed scalar/private-owned result after all required releases confirm.
    Returned,
    /// Exact declared operation/status, without a language scalar result.
    ForeignError,
    /// Exact foreign or genuinely issued private language trap identity.
    ControlledTrap,
    /// Host/ABI category plus unresolved obligations, overriding a pending result on release fault.
    HostAbiFailure,
}
impl OutcomeTag {
    /// Exact low-width tag, mirrored by the typed outcome channel and private entry status.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Returned => 0,
            Self::ForeignError => 1,
            Self::ControlledTrap => 2,
            Self::HostAbiFailure => 3,
        }
    }
}

/// Untrusted physical entry claims; original input layouts and result remain in the function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateEntry {
    /// Exact compiler-local symbol derived from original file/function identity.
    pub symbol: String,
    /// Fixed checked-channel contract, never inferred from C header types.
    pub contract: String,
    /// Checked context, typed input and typed outcome pointers, in that order.
    pub parameters: [ChannelLane; 3],
    /// Only the low thirty-two RAX bits carry the in-process outcome tag.
    pub result_bits: u8,
    /// Fixed sixteen-byte System V stack alignment.
    pub stack_alignment: u32,
    /// Process observations belong to the bounded driver, without promised in-process cleanup.
    pub process_failures_out_of_band: bool,
}

/// Complete-program hidden dispatch entry; its ordinal is not a source-level function call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatcherEntry {
    /// Fixed hidden symbol, independently checked against all declared/generated symbols.
    pub symbol: String,
    /// Exact compiler-private channel protocol.
    pub contract: String,
    /// Same three channels followed by one low-width function ordinal.
    pub parameters: [ChannelLane; 4],
    /// Low thirty-two RAX bits mirror the closed in-process outcome tag.
    pub result_bits: u8,
    /// Required System V stack alignment.
    pub stack_alignment: u32,
    /// Process failure cannot be returned through either tag or result channel.
    pub process_failures_out_of_band: bool,
}

pub(crate) fn private(file: u32, ordinal: usize) -> PrivateEntry {
    PrivateEntry {
        symbol: format!("zryna_c_v0_i_{file}_{ordinal}"),
        contract: PRIVATE_ENTRY_CONTRACT.into(),
        parameters: channels(),
        result_bits: 32,
        stack_alignment: 16,
        process_failures_out_of_band: true,
    }
}
pub(crate) fn dispatcher() -> DispatcherEntry {
    let [context, inputs, outcome] = channels();
    DispatcherEntry {
        symbol: DISPATCH_SYMBOL.into(),
        contract: PRIVATE_ENTRY_CONTRACT.into(),
        parameters: [
            context,
            inputs,
            outcome,
            ChannelLane {
                role: ChannelRole::FunctionOrdinal,
                bits: 32,
                location: Location::Register(Register::Rcx),
            },
        ],
        result_bits: 32,
        stack_alignment: 16,
        process_failures_out_of_band: true,
    }
}
fn channels() -> [ChannelLane; 3] {
    let roles = [ChannelRole::Context, ChannelRole::Inputs, ChannelRole::Outcome];
    let registers = [Register::Rdi, Register::Rsi, Register::Rdx];
    std::array::from_fn(|index| ChannelLane {
        role: roles[index],
        bits: 64,
        location: Location::Register(registers[index]),
    })
}

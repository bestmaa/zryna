//! Untrusted native C declaration records; decoding does not verify their policies.

use serde::{Deserialize, Deserializer, Serialize};

macro_rules! wire_enum {
    ($name:ident, $description:literal, {$($variant:ident => $wire:literal),+ $(,)?}) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
        pub enum $name {
            $(#[doc = concat!("Untrusted `", $wire, "` tag.")]
            #[serde(rename = $wire)]
            $variant),+
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let tag = String::deserialize(deserializer)?;
                match tag.as_str() {
                    $($wire => Ok(Self::$variant),)+
                    _ => Err(serde::de::Error::unknown_variant(&tag, &[$($wire),+])),
                }
            }
        }
    };
}

wire_enum!(AbiType, "Exact declared C carrier spelling, without inferred compatibility.", {
    CI32 => "c-i32", CInt => "c-int", Bool32 => "bool32", Count => "count",
    BytesIn => "bytes-in", BytesOwnedOut => "bytes-owned-out", CountOut => "count-out",
    I32Out => "i32-out", HandleIn => "handle-in", HandleOut => "handle-out",
    BytesRelease => "bytes-release", Unit => "unit",
});
wire_enum!(Direction, "Claimed operation direction.", { Import => "import", Export => "export" });
wire_enum!(Mode, "Claimed result channel.", { Direct => "direct", Status => "status", Void => "void" });
wire_enum!(Effects, "Claimed operation effects; not a proof of body totality.", { Total => "total", Foreign => "foreign" });
wire_enum!(Category, "Claimed foreign allocator category.", { Handle => "handle", Bytes => "bytes" });
wire_enum!(Access, "Claimed resource transition.", { Read => "read", Create => "create", Consume => "consume" });
wire_enum!(Owner, "Claimed resource owner.", { None => "none", Caller => "caller", Consumed => "consumed" });
wire_enum!(BorrowEnd, "Claimed borrow duration.", { Return => "return", None => "none" });
wire_enum!(NullRule, "Claimed pointer nullability.", { NullZero => "null-zero", Nonnull => "nonnull" });
wire_enum!(Encoding, "Claimed byte interpretation.", { Bytes => "bytes", Utf8 => "utf8", None => "none" });
wire_enum!(StatusKind, "Claimed status disposition.", { Success => "success", Recoverable => "recoverable" });
wire_enum!(Condition, "Claimed status condition.", {
    Success => "success", NegativeFirstI32 => "negative-first-i32", AllocationFailure => "allocation-failure",
    RawOverLimit => "raw-over-limit", RawOverLimitOrAllocation => "raw-over-limit-or-allocation",
});
wire_enum!(Primitive, "Reserved foreign primitive name; it supplies no source-node authority.", {
    RawCall => "rawCall", BorrowBytes => "borrowBytes", BorrowUtf8 => "borrowUtf8", ByteLength => "byteLength",
    OutI32 => "outI32", OutHandle => "outHandle", OutBytes => "outBytes", OutCount => "outCount",
    ReadI32 => "readI32", TakeHandle => "takeHandle", TakeBytes => "takeBytes", CopyBytes => "copyBytes",
    Release => "release", ForeignError => "foreignError",
});
wire_enum!(Safety, "Claimed primitive safety marker.", { UnsafeRaw => "unsafe-raw", Safe => "safe" });
wire_enum!(Execution, "Closed invoking-thread C execution promise; arbitrary C is not proved safe.", {
    Synchronous => "sync-invoking-thread-no-retention-callback-reentry-concurrency-unwind",
});

fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// Untrusted source-bound native C v0 declaration set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeclarationSet {
    /// Exact wire format claim.
    pub format: String,
    /// Wire version claim.
    pub version: u8,
    /// Declared target; never inferred from the execution host.
    pub target: String,
    /// ABI identity claim.
    pub abi: String,
    /// Calling convention claim.
    pub convention: String,
    /// Carrier model claim.
    pub carriers: String,
    /// Foreign ownership model claim.
    pub ownership: String,
    /// Language runtime compatibility claim; not a foreign allocator identity.
    pub runtime_contract: String,
    /// Complete claimed source inventory.
    pub sources: Vec<Source>,
    /// Complete claimed foreign library inventory.
    pub libraries: Vec<Library>,
    /// Imports and total scalar exports, including unused declarations.
    pub operations: Vec<Operation>,
    /// Claimed intrinsic source sites.
    pub sites: Vec<Site>,
}

/// Untrusted exact source-byte identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Claimed portable source path.
    pub path: String,
    /// Claimed SHA-256 of the exact source bytes.
    pub sha256: String,
}

/// Untrusted library policy and header identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Library {
    /// Exact logical name and version claim.
    pub id: String,
    /// Separate exact version claim.
    pub version: String,
    /// Claimed reviewed header byte digest.
    pub header_sha256: String,
    /// Claimed canonical library policy digest.
    pub policy_sha256: String,
    /// Nominal foreign kind claims.
    pub kinds: Vec<String>,
    /// Claimed allocator and release pairs.
    pub allocators: Vec<Allocator>,
}

/// Untrusted same-library allocation and release pair.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Allocator {
    /// Exact qualified allocator claim.
    pub id: String,
    /// Exact qualified nominal kind claim.
    pub kind: String,
    /// Claimed allocation category.
    pub category: Category,
    /// Claimed creation operation key.
    pub create: String,
    /// Claimed release operation key.
    pub release: String,
}

/// Untrusted operation signature, source binding and resource policies.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    /// Canonical operation key claim.
    pub key: String,
    /// Import or export direction claim.
    pub direction: Direction,
    /// Exact library or output-module identity claim.
    pub library: String,
    /// Logical function name claim.
    pub logical_name: String,
    /// Exact C symbol claim.
    pub symbol: String,
    /// Claimed parameters in ABI order.
    pub parameters: Vec<Parameter>,
    /// Claimed result carrier.
    pub result: AbiType,
    /// Claimed indexed resource groups.
    pub resources: Vec<Resource>,
    /// Claimed result channel.
    pub mode: Mode,
    /// Claimed successful and recoverable statuses.
    pub statuses: Vec<Status>,
    /// Claimed body effects.
    pub effects: Effects,
    /// Exact execution promise claim.
    pub execution: Execution,
    /// Claimed complete declaration or intrinsic call binding.
    pub source_binding: Binding,
}

/// Untrusted ordered ABI parameter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    /// Parameter name claim.
    pub name: String,
    /// Exact ABI spelling claim.
    pub abi: AbiType,
    /// Required nullable resource group claim; omission must not become null.
    #[serde(deserialize_with = "required_nullable")]
    pub resource: Option<u8>,
}

/// Untrusted foreign resource policy; no claim creates an owner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resource {
    /// Nominal kind claim.
    pub kind: String,
    /// Ordered parameter slot claims.
    pub slots: Vec<u8>,
    /// Read, creation or consumption claim.
    pub access: Access,
    /// Owner before entry claim.
    pub owner_before: Owner,
    /// Owner after successful return claim.
    pub owner_after: Owner,
    /// Borrow end claim.
    pub borrow_end: BorrowEnd,
    /// Pointer nullability claim.
    pub null_rule: NullRule,
    /// Required nullable byte bound claim.
    #[serde(deserialize_with = "required_nullable")]
    pub max_bytes: Option<u32>,
    /// Byte interpretation claim.
    pub encoding: Encoding,
    /// Allocator identity claim, including explicit `none`.
    pub allocator: String,
    /// Exact release identity claim, including explicit `none`.
    pub release: String,
    /// Claimed library pointer-validity guarantee.
    pub valid_pointer_guarantee: bool,
    /// Claimed releasability despite malformed metadata.
    pub releasable_on_malformed: bool,
    /// Claimed fresh acquisition.
    pub fresh: bool,
    /// Required nullable exact input-count slot claim.
    #[serde(deserialize_with = "required_nullable")]
    pub expected_length_slot: Option<u8>,
}

/// Untrusted status and failure-atomicity claims.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Status {
    /// Claimed nonnegative signed-i32 status.
    pub code: u32,
    /// Claimed status disposition.
    pub kind: StatusKind,
    /// Claimed initialized output slots.
    pub initialized: Vec<u8>,
    /// Claimed created resource groups.
    pub new_owners: Vec<u8>,
    /// Claimed status condition.
    pub condition: Condition,
    /// Claimed unchanged inputs; decoding does not establish it.
    pub preserves_inputs: bool,
}

/// Untrusted source-bound operation location.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// Claimed source path.
    pub path: String,
    /// Claimed source-byte digest.
    pub sha256: String,
    /// Claimed half-open UTF-8 start offset.
    pub start: u64,
    /// Claimed half-open UTF-8 end offset.
    pub end: u64,
    /// Claimed canonical operation-record ordinal.
    pub ordinal: u64,
}

/// Untrusted exact reserved primitive site.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Site {
    /// Claimed primitive name.
    pub primitive: Primitive,
    /// Claimed explicit safety marker.
    pub safety: Safety,
    /// Required nullable import operation key claim.
    #[serde(deserialize_with = "required_nullable")]
    pub operation: Option<String>,
    /// Claimed source path.
    pub path: String,
    /// Claimed source-byte digest.
    pub source_sha256: String,
    /// Claimed half-open UTF-8 start offset.
    pub start: u64,
    /// Claimed half-open UTF-8 end offset.
    pub end: u64,
    /// Claimed exact primitive call bytes.
    pub spelling: String,
}

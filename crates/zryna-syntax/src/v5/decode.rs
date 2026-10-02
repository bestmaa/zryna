use std::{collections::BTreeSet, fmt, marker::PhantomData};

use serde::{
    Deserialize, Deserializer,
    de::{Error as _, IgnoredAny, MapAccess, SeqAccess, Visitor},
};

use super::{PROTOCOL_VERSION, RawProjectSyntaxSnapshot, SyntaxDecodeError};
use crate::v4::MAX_RESPONSE_BYTES;

/// Decodes bounded, closed DTOs. The result has no source or executable authority.
///
/// # Errors
/// Rejects duplicate keys, missing/unknown fields, trailing input, wrong version and byte excess.
pub fn decode_snapshot(bytes: &[u8]) -> Result<RawProjectSyntaxSnapshot, SyntaxDecodeError> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(SyntaxDecodeError::ResponseTooLarge {
            actual: bytes.len(),
            limit: MAX_RESPONSE_BYTES,
        });
    }
    reject_duplicate_json_keys(bytes)?;
    let raw: RawProjectSyntaxSnapshot =
        serde_json::from_slice(bytes).map_err(|_| SyntaxDecodeError::InvalidSnapshot)?;
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| SyntaxDecodeError::InvalidSnapshot)?;
    // Serde's nullable fields otherwise also admit omission. Wire records require every field.
    let exact = serde_json::to_value(&raw).map_err(|_| SyntaxDecodeError::InvalidSnapshot)?;
    if raw.schema_version != PROTOCOL_VERSION || exact != value || !super::wire::valid(&value) {
        return Err(SyntaxDecodeError::InvalidSnapshot);
    }
    Ok(raw)
}

pub(super) fn bounded<'de, D, T, const MAX: usize>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct B<T, const MAX: usize>(&'static str, PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const MAX: usize> Visitor<'de> for B<T, MAX> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "at most {MAX} {}", self.0)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX));
            while out.len() < MAX {
                match seq.next_element()? {
                    Some(v) => out.push(v),
                    None => return Ok(out),
                }
            }
            if seq.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom(format_args!("{} exceeds limit {MAX}", self.0)));
            }
            Ok(out)
        }
    }
    d.deserialize_seq(B::<T, MAX>("items", PhantomData))
}
struct DuplicateCheckedValue;
impl<'de> Deserialize<'de> for DuplicateCheckedValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DuplicateCheckedVisitor;
        impl<'de> Visitor<'de> for DuplicateCheckedVisitor {
            type Value = DuplicateCheckedValue;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON value without duplicate object keys")
            }
            fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
                Ok(DuplicateCheckedValue)
            }
            fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
                Ok(DuplicateCheckedValue)
            }
            fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
                Ok(DuplicateCheckedValue)
            }
            fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
                Ok(DuplicateCheckedValue)
            }
            fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
                Ok(DuplicateCheckedValue)
            }
            fn visit_string<E>(self, _: String) -> Result<Self::Value, E> {
                Ok(DuplicateCheckedValue)
            }
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(DuplicateCheckedValue)
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(DuplicateCheckedValue)
            }
            fn visit_some<D: Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                DuplicateCheckedValue::deserialize(deserializer)
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                while sequence.next_element::<DuplicateCheckedValue>()?.is_some() {}
                Ok(DuplicateCheckedValue)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut keys = BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !keys.insert(key) {
                        return Err(A::Error::custom("duplicate JSON object key"));
                    }
                    map.next_value::<DuplicateCheckedValue>()?;
                }
                Ok(DuplicateCheckedValue)
            }
        }
        deserializer.deserialize_any(DuplicateCheckedVisitor)
    }
}
fn reject_duplicate_json_keys(bytes: &[u8]) -> Result<(), SyntaxDecodeError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    DuplicateCheckedValue::deserialize(&mut deserializer)
        .map_err(|_| SyntaxDecodeError::InvalidSnapshot)?;
    deserializer.end().map_err(|_| SyntaxDecodeError::InvalidSnapshot)
}

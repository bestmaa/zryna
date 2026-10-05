//! Preserve wire fields before v5's closed decoder; never canonicalize missing nullable claims.
use serde::{
    Deserialize, Deserializer,
    de::{Error as _, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use std::fmt;

pub(super) struct ClosedValue(pub(super) Value);

impl<'de> Deserialize<'de> for ClosedValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ClosedVisitor;
        impl<'de> Visitor<'de> for ClosedVisitor {
            type Value = ClosedValue;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("JSON without duplicate fields")
            }
            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(ClosedValue(Value::Bool(value)))
            }
            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(ClosedValue(Value::Number(value.into())))
            }
            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(ClosedValue(Value::Number(value.into())))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Number::from_f64(value)
                    .map(|number| ClosedValue(Value::Number(number)))
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(ClosedValue(Value::String(value.into())))
            }
            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(ClosedValue(Value::String(value)))
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(ClosedValue(Value::Null))
            }
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(ClosedValue(Value::Null))
            }
            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
                ClosedValue::deserialize(d)
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(ClosedValue(value)) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(ClosedValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(A::Error::custom("duplicate JSON field"));
                    }
                    let ClosedValue(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(ClosedValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(ClosedVisitor)
    }
}

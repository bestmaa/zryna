//! Closed object-only JSON decoding before semantic admission.

use std::{fmt, marker::PhantomData};

use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, Visitor},
};

use super::Input;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub(super) schema: String,
    pub(super) world: String,
    #[serde(deserialize_with = "object")]
    pub(super) grant: Grant,
    pub(super) input: Input,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Grant {
    pub(super) capability: String,
    pub(super) key: String,
}

pub(super) fn decode(text: &str) -> Result<Request, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let request = object(&mut deserializer)?;
    deserializer.end()?;
    Ok(request)
}

// Derived struct visitors preserve unknown/duplicate-field rejection. Requiring
// a map here also excludes their positional-sequence representation.
fn object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON object")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}

impl<'de> Deserialize<'de> for Input {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(InputVisitor)
    }
}

struct InputVisitor;

impl<'de> Visitor<'de> for InputVisitor {
    type Value = Input;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded command input object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Input, A::Error> {
        let mut present = None;
        let mut value = None;
        while let Some(field) = map.next_key::<String>()? {
            match field.as_str() {
                "present" => {
                    if present.is_some() {
                        return Err(de::Error::duplicate_field("present"));
                    }
                    present = Some(map.next_value::<bool>()?);
                }
                "value" => {
                    if value.is_some() {
                        return Err(de::Error::duplicate_field("value"));
                    }
                    // A present field must contain a string. Null cannot be
                    // confused with a missing field through Option decoding.
                    value = Some(map.next_value::<String>()?);
                }
                _ => return Err(de::Error::unknown_field(&field, &["present", "value"])),
            }
        }
        match (present, value) {
            (Some(false), None) => Ok(Input::Missing),
            (Some(true), Some(value)) => Ok(Input::Present(value)),
            _ => Err(de::Error::custom("invalid command input presence")),
        }
    }
}

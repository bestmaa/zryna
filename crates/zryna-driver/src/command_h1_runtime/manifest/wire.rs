//! Object-only records and non-null optional fields close Serde's alternate representations.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{fmt, marker::PhantomData};

pub(super) fn object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct Object<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> de::Visitor<'de> for Object<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("one closed JSON object")
        }
        fn visit_map<A: de::MapAccess<'de>>(self, fields: A) -> Result<T, A::Error> {
            T::deserialize(de::value::MapAccessDeserializer::new(fields))
        }
    }
    deserializer.deserialize_map(Object(PhantomData))
}

pub(super) fn objects<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    struct Object<T>(T);
    impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            object(deserializer).map(Self)
        }
    }
    Vec::<Object<T>>::deserialize(deserializer)
        .map(|items| items.into_iter().map(|item| item.0).collect())
}

pub(super) fn string_enum<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    let value = String::deserialize(deserializer)?;
    T::deserialize(de::value::StringDeserializer::<D::Error>::new(value))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) enum Optional<T> {
    #[default]
    Absent,
    Present(T),
}

impl<T> Optional<T> {
    pub(super) const fn absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
}

impl<T: Serialize> Serialize for Optional<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Absent => serializer.serialize_none(),
            Self::Present(value) => value.serialize(serializer),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Optional<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Present)
    }
}

pub(super) fn optional_object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Optional<T>, D::Error> {
    object(deserializer).map(Optional::Present)
}

pub(super) fn optional_string_enum<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Optional<T>, D::Error> {
    string_enum(deserializer).map(Optional::Present)
}

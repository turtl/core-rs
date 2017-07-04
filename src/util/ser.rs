pub mod int_converter {
    use ::error::{TResult, TError};
    use ::serde::ser::Serializer;
    use ::serde::de::{self, Deserializer, Visitor};
    use ::jedi::Value;

    pub fn serialize<S>(val: &String, ser: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        ser.serialize_i64(val.parse().unwrap())
    }

    pub fn deserialize<'de, D>(des: D) -> Result<String, D::Error>
        where D: Deserializer<'de>
    {
        struct StringOrI64 {};

        impl<'de> Visitor<'de> for StringOrI64 {
            type Value = String;

            fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                formatter.write_str("string or i64")
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(value)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(value.to_string())
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(value.to_string())
            }

            fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(value.to_string())
            }

            fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(value.to_string())
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(value.to_string())
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(value.to_string())
            }
        }

        des.deserialize_any(StringOrI64 {})
    }

    pub fn from_value(val: Value) -> TResult<Option<String>> {
        match val {
            Value::Number(num) => {
                match num.as_i64() {
                    Some(x) => Ok(Some(x.to_string())),
                    None => {
                        match num.as_u64() {
                            Some(x) => Ok(Some(x.to_string())),
                            None => Ok(None),
                        }
                    }
                }
            },
            Value::String(s) => {
                Ok(Some(s))
            },
            _ => Err(TError::BadValue(String::from("int_converter::from_value() -- expecting int or string. got another type."))),
        }
    }
}

/*
pub mod false_as_none {
    use ::serde::de::{self, Deserializer, Deserialize, Visitor, MapAccess};
    use std::marker::PhantomData;

    pub fn deserialize<'de, T, D>(des: D) -> Result<Option<T>, D::Error>
        where D: Deserializer<'de>,
              T: Deserialize<'de>
    {
        struct FalseAsNone<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for FalseAsNone<T>
            where T: Deserialize<'de>
        {
            type Value = Option<T>;

            fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                formatter.write_str("null, bool, struct")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(None)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(None)
            }

            fn visit_bool<E>(self, _val: bool) -> Result<Self::Value, E>
                where E: de::Error
            {
                Ok(None)
            }

            fn visit_map<M>(self, visitor: M) -> Result<Self::Value, M::Error>
                where M: MapAccess<'de>
            {
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))
                    .map(|x| Some(x))
            }
        }

        des.deserialize_any(FalseAsNone(PhantomData))
    }
}
*/


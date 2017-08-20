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
        struct StringOrInt;
        impl<'de> Visitor<'de> for StringOrInt {
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

        des.deserialize_any(StringOrInt {})
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
            _ => TErr!(TError::BadValue(String::from("expecting int or string (got another type)"))),
        }
    }
}

pub mod int_opt_converter {
    use ::serde::ser::Serializer;
    use ::serde::de::{Deserialize, Deserializer};
    use ::jedi::Value;

    #[allow(dead_code)]
    pub fn serialize<S>(val: &Option<String>, ser: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        match val {
            &Some(ref x) => ser.serialize_i64(x.parse().unwrap()),
            &None => ser.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(des: D) -> Result<Option<String>, D::Error>
        where D: Deserializer<'de>
    {
        // fuck it, deserialize to intermediate Value. sick of trying to figure
        // out how serde deals with Option...
        let val: Value = Deserialize::deserialize(des)?;
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
            }
            Value::String(s) => {
                Ok(Some(s))
            }
            _ => {
                Ok(None)
            }
        }
    }
}

pub mod base64_converter {
    use ::error::TResult;
    use ::serde::ser::{self, Serializer};
    use ::serde::de::{self, Deserializer, Deserialize};
    use ::jedi::Value;
    use ::crypto;

    #[allow(dead_code)]
    pub fn serialize<S>(val: &Option<Vec<u8>>, ser: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        match val {
            &Some(ref x) => {
                let base64: String = crypto::to_base64(x)
                    .map_err(|_| ser::Error::custom("could not base64 encode the given value"))?;
                ser.serialize_str(&base64[..])
            },
            &None => ser.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(des: D) -> Result<Option<Vec<u8>>, D::Error>
        where D: Deserializer<'de>
    {
        let val: Option<String> = Deserialize::deserialize(des)?;
        match val {
            Some(x) => {
                let parsed = crypto::from_base64(&x)
                    .map_err(|_| de::Error::custom("invalid base64 given"))?;
                Ok(Some(parsed))
            }
            None => { Ok(None) }
        }
    }

    pub fn from_value(_val: Value) -> TResult<Option<Option<Vec<u8>>>> {
        Ok(None)
    }
}


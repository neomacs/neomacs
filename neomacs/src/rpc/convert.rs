use std::{collections::HashMap, hash::Hash};

use crate::error::{NeomacsError, Result};
use rmpv::Value;

pub trait DecodeValue: Sized {
    fn decode_value(value: Value) -> Result<Self>;
}

impl<T: DecodeValue> DecodeValue for Vec<T> {
    fn decode_value(value: Value) -> Result<Self> {
        if !value.is_array() {
            return Err(NeomacsError::MessagePackParse(format!(
                "Unable to decode Vec from {}",
                value
            )));
        }
        let mut parsed = Vec::new();
        for val in value.as_array().unwrap() {
            parsed.push(DecodeValue::decode_value(val.clone())?);
        }
        Ok(parsed)
    }
}

impl<K: DecodeValue + Hash + Eq, V: DecodeValue> DecodeValue for HashMap<K, V> {
    fn decode_value(value: Value) -> Result<Self> {
        if !value.is_map() {
            return Err(NeomacsError::MessagePackParse(format!(
                "Unable to decode HashMap from {}",
                value
            )));
        }
        let mut parsed = HashMap::new();
        for (k, v) in value.as_map().unwrap() {
            parsed.insert(
                DecodeValue::decode_value(k.clone())?,
                DecodeValue::decode_value(v.clone())?,
            );
        }
        Ok(parsed)
    }
}

impl DecodeValue for u64 {
    fn decode_value(value: Value) -> Result<Self> {
        value.clone().try_into().map_err(|_| {
            NeomacsError::MessagePackParse(format!("Unable to decode u64 from {}", value))
        })
    }
}

impl DecodeValue for i64 {
    fn decode_value(value: Value) -> Result<Self> {
        value.clone().try_into().map_err(|_| {
            NeomacsError::MessagePackParse(format!("Unable to decode i64 from {}", value))
        })
    }
}

impl DecodeValue for f64 {
    fn decode_value(value: Value) -> Result<Self> {
        value.clone().try_into().map_err(|_| {
            NeomacsError::MessagePackParse(format!("Unable to decode f64 from {}", value))
        })
    }
}

impl DecodeValue for String {
    fn decode_value(value: Value) -> Result<Self> {
        value.clone().try_into().map_err(|_| {
            NeomacsError::MessagePackParse(format!("Unable to decode String from {}", value))
        })
    }
}

impl DecodeValue for bool {
    fn decode_value(value: Value) -> Result<Self> {
        value.clone().try_into().map_err(|_| {
            NeomacsError::MessagePackParse(format!("Unable to decode bool from {}", value))
        })
    }
}

impl DecodeValue for f32 {
    fn decode_value(value: Value) -> Result<Self> {
        value.clone().try_into().map_err(|_| {
            NeomacsError::MessagePackParse(format!("Unable to decode f32 from {}", value))
        })
    }
}

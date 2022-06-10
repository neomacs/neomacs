use crate::error::{NeomacsError, Result};
use std::io::Cursor;

use rmpv::{decode::read_value, encode::write_value, Value};

use super::convert::{DecodeValue, EncodeValue};

pub const EXTENSION_TYPE_OPTIONAL: i8 = 0;

pub fn encode_optional<V: EncodeValue>(optional: &Option<V>) -> Value {
    let mut val_bytes = Cursor::new(Vec::new());
    match optional {
        None => write_value(&mut val_bytes, &Value::Nil),
        Some(v) => write_value(&mut val_bytes, &v.encode_value()),
    }
    .expect("Failed to encode Optional");
    Value::Ext(EXTENSION_TYPE_OPTIONAL, val_bytes.into_inner())
}

pub fn decode_optional<V: DecodeValue>(optional: &Value) -> Result<Option<V>> {
    let opt = optional.as_ext().ok_or_else(|| {
        NeomacsError::MessagePackParse(format!("Invalid optional value: {}", optional))
    })?;
    match opt {
        (EXTENSION_TYPE_OPTIONAL, mut data) => {
            let val = read_value(&mut data)?;
            match val {
                Value::Nil => Ok(None),
                _ => Ok(Some(V::decode_value(&val)?)),
            }
        }
        _ => Err(NeomacsError::MessagePackParse(format!(
            "Invalid optional value: {}",
            optional
        ))),
    }
}

pub fn is_optional(val: &Value) -> bool {
    matches!(val, Value::Ext(EXTENSION_TYPE_OPTIONAL, _))
}

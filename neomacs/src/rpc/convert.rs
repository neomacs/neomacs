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

pub trait EncodeValue {
    fn encode_value(&self) -> Value;
}

impl<T: EncodeValue> EncodeValue for Vec<T> {
    fn encode_value(&self) -> Value {
        Value::Array(self.iter().map(|v| v.encode_value()).collect())
    }
}

impl<K: EncodeValue + Hash + Eq, V: EncodeValue> EncodeValue for HashMap<K, V> {
    fn encode_value(&self) -> Value {
        Value::Map(
            self.iter()
                .map(|(k, v)| (k.encode_value(), v.encode_value()))
                .collect(),
        )
    }
}

impl EncodeValue for u64 {
    fn encode_value(&self) -> Value {
        Value::from(*self)
    }
}

impl EncodeValue for i64 {
    fn encode_value(&self) -> Value {
        Value::from(*self)
    }
}

impl EncodeValue for f64 {
    fn encode_value(&self) -> Value {
        Value::from(*self)
    }
}

impl EncodeValue for String {
    fn encode_value(&self) -> Value {
        Value::from(self.clone())
    }
}

impl EncodeValue for bool {
    fn encode_value(&self) -> Value {
        Value::from(*self)
    }
}

impl EncodeValue for f32 {
    fn encode_value(&self) -> Value {
        Value::from(*self)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use maplit::hashmap;
    use neomacs_proc_macros::{DecodeValue, EncodeValue};
    use rmpv::Value;

    use crate::{
        error::Result,
        rpc::convert::{DecodeValue, EncodeValue},
    };

    #[test]
    fn test_derive_convert_value() -> Result<()> {
        #[derive(EncodeValue, DecodeValue, Debug, PartialEq, Clone)]
        struct Widget {
            color: String,
            num_gears: u64,
        }

        #[derive(EncodeValue, DecodeValue, Debug, PartialEq, Clone)]
        struct Machine {
            efficiency: f32,
            widgets: Vec<Widget>,
            components: HashMap<String, Widget>,
        }

        let val = Value::Map(vec![
            (Value::String("efficiency".into()), Value::F32(14.5)),
            (
                Value::String("widgets".into()),
                Value::Array(vec![
                    Value::Map(vec![
                        (Value::String("color".into()), Value::String("red".into())),
                        (
                            Value::String("num_gears".into()),
                            Value::Integer(8u32.into()),
                        ),
                    ]),
                    Value::Map(vec![
                        (Value::String("color".into()), Value::String("green".into())),
                        (
                            Value::String("num_gears".into()),
                            Value::Integer(12u32.into()),
                        ),
                    ]),
                ]),
            ),
            (
                Value::String("components".into()),
                Value::Map(vec![
                    (
                        Value::String("main_widget".into()),
                        Value::Map(vec![
                            (
                                Value::String("color".into()),
                                Value::String("yellow".into()),
                            ),
                            (
                                Value::String("num_gears".into()),
                                Value::Integer(24u32.into()),
                            ),
                        ]),
                    ),
                    (
                        Value::String("secondary_widget".into()),
                        Value::Map(vec![
                            (Value::String("color".into()), Value::String("blue".into())),
                            (
                                Value::String("num_gears".into()),
                                Value::Integer(3u32.into()),
                            ),
                        ]),
                    ),
                ]),
            ),
        ]);

        let parsed: Machine = DecodeValue::decode_value(val)?;
        assert_eq!(
            parsed,
            Machine {
                efficiency: 14.5,
                widgets: vec![
                    Widget {
                        color: "red".to_string(),
                        num_gears: 8
                    },
                    Widget {
                        color: "green".to_string(),
                        num_gears: 12
                    }
                ],
                components: hashmap! {
                    "main_widget".to_string() => Widget {
                        color: "yellow".to_string(),
                        num_gears: 24
                    },
                    "secondary_widget".to_string() => Widget {
                        color: "blue".to_string(),
                        num_gears: 3
                    }
                }
            }
        );
        let encoded: Value = parsed.encode_value();
        assert!(encoded.is_map());
        assert_eq!(get_from_map(&encoded, "efficiency"), Some(Value::F32(14.5)));
        assert!(get_from_map(&encoded, "widgets").unwrap().is_array());
        assert_eq!(
            get_from_map(
                &get_from_map(&encoded, "widgets")
                    .unwrap()
                    .as_array()
                    .unwrap()[0],
                "color"
            ),
            Some(Value::String("red".into()))
        );
        assert!(get_from_map(&encoded, "components").unwrap().is_map());
        assert!(get_from_map(
            &get_from_map(&encoded, "components").unwrap(),
            "main_widget"
        )
        .unwrap()
        .is_map());
        assert_eq!(
            get_from_map(
                &get_from_map(
                    &get_from_map(&encoded, "components").unwrap(),
                    "main_widget"
                )
                .unwrap(),
                "color"
            ),
            Some(Value::String("yellow".into()))
        );

        #[derive(DecodeValue, EncodeValue, Debug, PartialEq)]
        struct TwoMachines(Machine, Machine);
        let mach1 = Machine {
            efficiency: 14.5,
            widgets: vec![
                Widget {
                    color: "red".to_string(),
                    num_gears: 8,
                },
                Widget {
                    color: "green".to_string(),
                    num_gears: 12,
                },
            ],
            components: hashmap! {
                "main_widget".to_string() => Widget {
                    color: "yellow".to_string(),
                    num_gears: 24
                },
                "secondary_widget".to_string() => Widget {
                    color: "blue".to_string(),
                    num_gears: 3
                }
            },
        };
        let mach2 = Machine {
            efficiency: 2.2,
            ..mach1.clone()
        };
        let both = TwoMachines(mach1, mach2);
        let encoded = both.encode_value();
        assert!(encoded.is_array());
        assert_eq!(encoded.as_array().unwrap().len(), 2);
        assert!(encoded.as_array().unwrap()[0].is_map());
        assert_eq!(
            get_from_map(&encoded.as_array().unwrap()[0], "efficiency"),
            Some(Value::F32(14.5))
        );
        assert_eq!(
            get_from_map(&encoded.as_array().unwrap()[1], "efficiency"),
            Some(Value::F32(2.2))
        );
        let decoded: TwoMachines = DecodeValue::decode_value(encoded).unwrap();
        assert_eq!(decoded, both);
        Ok(())
    }

    fn get_from_map(map: &Value, key: &str) -> Option<Value> {
        map.as_map().and_then(|m| {
            m.iter().find_map(|(k, v)| {
                if k.clone() == Value::String(key.into()) {
                    Some(v.clone())
                } else {
                    None
                }
            })
        })
    }
}

pub mod app;
pub mod buffer;
pub mod error;
pub mod events;
pub mod ping;
pub mod rpc;

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

        let parsed: Machine = DecodeValue::decode_value(val.clone())?;
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

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
    use neomacs_convert::DecodeValue;
    use rmpv::Value;

    use crate::{error::Result, rpc::convert::DecodeValue};

    #[test]
    fn test_derive_try_into_value() -> Result<()> {
        #[derive(DecodeValue, Debug, PartialEq)]
        struct Widget {
            color: String,
            num_gears: u64,
        }

        #[derive(DecodeValue, Debug, PartialEq)]
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
        Ok(())
    }
}

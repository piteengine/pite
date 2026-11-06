// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::props::PropValue;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DefaultPropVal {
    Num(f64),
    Int(i64),
    Bool(bool),
    Vec2(f64, f64),
    Str(&'static str),
    Color([u8; 4]),
}

impl DefaultPropVal {
    pub fn to_prop_value(&self) -> PropValue {
        match self {
            Self::Num(f) => PropValue::Num(*f),
            Self::Int(i) => PropValue::Int(*i),
            Self::Bool(b) => PropValue::Bool(*b),
            Self::Vec2(x, y) => PropValue::Vec2(*x, *y),
            Self::Str(s) => PropValue::Str((*s).to_string()),
            Self::Color([r, g, b, a]) => PropValue::Color(*r, *g, *b, *a),
        }
    }

    pub fn matches(&self, val: &PropValue) -> bool {
        match (self, val) {
            (Self::Num(a), PropValue::Num(b)) => (a - b).abs() < f64::EPSILON,
            (Self::Int(a), PropValue::Int(b)) => a == b,
            (Self::Bool(a), PropValue::Bool(b)) => a == b,
            (Self::Vec2(x1, y1), PropValue::Vec2(x2, y2)) => {
                (x1 - x2).abs() < f64::EPSILON && (y1 - y2).abs() < f64::EPSILON
            }
            (Self::Str(a), PropValue::Str(b)) => *a == b.as_str(),
            (Self::Color([r1, g1, b1, a1]), PropValue::Color(r2, g2, b2, a2)) => {
                r1 == r2 && g1 == g2 && b1 == b2 && a1 == a2
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PropHint {
    Vec2 { speed: f64 },
    AngleDeg,
    Num { speed: f64, min: f64, max: f64 },
    Int { speed: f64, min: i64, max: i64 },
    Bool,
    Text,
    Enum(&'static [&'static str]),
    Color,
    Asset { filter: &'static [&'static str] },
}

#[derive(Clone, Copy, Debug)]
pub struct PropSchema {
    pub key: &'static str,
    pub label: &'static str,
    pub default_value: DefaultPropVal,
    pub hint: PropHint,
    pub read_only: bool, // To Python
}

#[derive(Clone, Copy, Debug)]
pub struct NodeTypeSchema {
    pub type_name: &'static str,
    pub inherits: Option<&'static str>,
    pub props: &'static [PropSchema],
}

pub static NODE_SCHEMAS: &[NodeTypeSchema] = &[
    NodeTypeSchema {
        type_name: "Node",
        inherits: None,
        props: &[],
    },
    NodeTypeSchema {
        type_name: "Node2D",
        inherits: Some("Node"),
        props: &[
            PropSchema {
                key: "position",
                label: "Position",
                default_value: DefaultPropVal::Vec2(0.0, 0.0),
                hint: PropHint::Vec2 { speed: 1.0 },
                read_only: false,
            },
            PropSchema {
                key: "rotation",
                label: "Rotation",
                default_value: DefaultPropVal::Num(0.0),
                hint: PropHint::AngleDeg,
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "Sprite2D",
        inherits: Some("Node2D"),
        props: &[
            PropSchema {
                key: "texture",
                label: "Texture",
                default_value: DefaultPropVal::Str(""),
                hint: PropHint::Asset {
                    filter: &["png", "jpg", "jpeg", "webp"],
                },
                read_only: false,
            },
            PropSchema {
                key: "atlas",
                label: "Atlas",
                default_value: DefaultPropVal::Str(""),
                hint: PropHint::Asset {
                    filter: &["atlas.json", "json"],
                },
                read_only: false,
            },
            PropSchema {
                key: "frame",
                label: "Frame",
                default_value: DefaultPropVal::Str(""),
                hint: PropHint::Text,
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "Camera2D",
        inherits: Some("Node2D"),
        props: &[PropSchema {
            key: "zoom",
            label: "Zoom",
            default_value: DefaultPropVal::Vec2(1.0, 1.0),
            hint: PropHint::Vec2 { speed: 0.05 },
            read_only: false,
        }],
    },
    NodeTypeSchema {
        type_name: "CollisionShape2D",
        inherits: Some("Node2D"),
        props: &[
            PropSchema {
                key: "shape",
                label: "Shape",
                default_value: DefaultPropVal::Str("box"),
                hint: PropHint::Enum(&["box", "circle", "capsule"]),
                read_only: false,
            },
            PropSchema {
                key: "size",
                label: "Size",
                default_value: DefaultPropVal::Vec2(32.0, 32.0),
                hint: PropHint::Vec2 { speed: 1.0 },
                read_only: false,
            },
            PropSchema {
                key: "radius",
                label: "Radius",
                default_value: DefaultPropVal::Num(16.0),
                hint: PropHint::Num {
                    speed: 1.0,
                    min: 0.1,
                    max: 10000.0,
                },
                read_only: false,
            },
            PropSchema {
                key: "height",
                label: "Height",
                default_value: DefaultPropVal::Num(32.0),
                hint: PropHint::Num {
                    speed: 1.0,
                    min: 0.1,
                    max: 10000.0,
                },
                read_only: false,
            },
            PropSchema {
                key: "disabled",
                label: "Disabled",
                default_value: DefaultPropVal::Bool(false),
                hint: PropHint::Bool,
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "Area2D",
        inherits: Some("Node2D"),
        props: &[
            PropSchema {
                key: "collision_layer",
                label: "Layer",
                default_value: DefaultPropVal::Int(1),
                hint: PropHint::Int {
                    speed: 1.0,
                    min: 1,
                    max: 256,
                },
                read_only: false,
            },
            PropSchema {
                key: "collision_mask",
                label: "Mask",
                default_value: DefaultPropVal::Int(1),
                hint: PropHint::Int {
                    speed: 1.0,
                    min: 1,
                    max: 256,
                },
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "StaticBody2D",
        inherits: Some("Node2D"),
        props: &[
            PropSchema {
                key: "collision_layer",
                label: "Layer",
                default_value: DefaultPropVal::Int(1),
                hint: PropHint::Int {
                    speed: 1.0,
                    min: 1,
                    max: 256,
                },
                read_only: false,
            },
            PropSchema {
                key: "collision_mask",
                label: "Mask",
                default_value: DefaultPropVal::Int(1),
                hint: PropHint::Int {
                    speed: 1.0,
                    min: 1,
                    max: 256,
                },
                read_only: false,
            },
            PropSchema {
                key: "friction",
                label: "Friction",
                default_value: DefaultPropVal::Num(0.5),
                hint: PropHint::Num {
                    speed: 0.05,
                    min: 0.0,
                    max: 2.0,
                },
                read_only: false,
            },
            PropSchema {
                key: "restitution",
                label: "Restitution",
                default_value: DefaultPropVal::Num(0.0),
                hint: PropHint::Num {
                    speed: 0.05,
                    min: 0.0,
                    max: 1.0,
                },
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "CharacterBody2D",
        inherits: Some("Node2D"),
        props: &[
            PropSchema {
                key: "velocity",
                label: "Velocity",
                default_value: DefaultPropVal::Vec2(0.0, 0.0),
                hint: PropHint::Vec2 { speed: 1.0 },
                read_only: false,
            },
            PropSchema {
                key: "collision_layer",
                label: "Layer",
                default_value: DefaultPropVal::Int(1),
                hint: PropHint::Int {
                    speed: 1.0,
                    min: 1,
                    max: 256,
                },
                read_only: false,
            },
            PropSchema {
                key: "collision_mask",
                label: "Mask",
                default_value: DefaultPropVal::Int(1),
                hint: PropHint::Int {
                    speed: 1.0,
                    min: 1,
                    max: 256,
                },
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "RigidBody2D",
        inherits: Some("Node2D"),
        props: &[
            PropSchema {
                key: "mass",
                label: "Mass",
                default_value: DefaultPropVal::Num(1.0),
                hint: PropHint::Num {
                    speed: 0.1,
                    min: 0.01,
                    max: 1000.0,
                },
                read_only: false,
            },
            PropSchema {
                key: "gravity_scale",
                label: "Gravity Scale",
                default_value: DefaultPropVal::Num(1.0),
                hint: PropHint::Num {
                    speed: 0.1,
                    min: -10.0,
                    max: 10.0,
                },
                read_only: false,
            },
            PropSchema {
                key: "lock_rotation",
                label: "Lock Rotation",
                default_value: DefaultPropVal::Bool(false),
                hint: PropHint::Bool,
                read_only: false,
            },
            PropSchema {
                key: "collision_layer",
                label: "Layer",
                default_value: DefaultPropVal::Int(1),
                hint: PropHint::Int {
                    speed: 1.0,
                    min: 1,
                    max: 256,
                },
                read_only: false,
            },
            PropSchema {
                key: "collision_mask",
                label: "Mask",
                default_value: DefaultPropVal::Int(1),
                hint: PropHint::Int {
                    speed: 1.0,
                    min: 1,
                    max: 256,
                },
                read_only: false,
            },
            PropSchema {
                key: "friction",
                label: "Friction",
                default_value: DefaultPropVal::Num(0.5),
                hint: PropHint::Num {
                    speed: 0.05,
                    min: 0.0,
                    max: 2.0,
                },
                read_only: false,
            },
            PropSchema {
                key: "restitution",
                label: "Restitution",
                default_value: DefaultPropVal::Num(0.0),
                hint: PropHint::Num {
                    speed: 0.05,
                    min: 0.0,
                    max: 1.0,
                },
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "Control",
        inherits: Some("Node"),
        props: &[
            PropSchema {
                key: "position",
                label: "Position",
                default_value: DefaultPropVal::Vec2(0.0, 0.0),
                hint: PropHint::Vec2 { speed: 1.0 },
                read_only: false,
            },
            PropSchema {
                key: "size",
                label: "Size",
                default_value: DefaultPropVal::Vec2(100.0, 40.0),
                hint: PropHint::Vec2 { speed: 1.0 },
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "Label",
        inherits: Some("Control"),
        props: &[
            PropSchema {
                key: "text",
                label: "Text",
                default_value: DefaultPropVal::Str(""),
                hint: PropHint::Text,
                read_only: false,
            },
            PropSchema {
                key: "font_size",
                label: "Font Size",
                default_value: DefaultPropVal::Num(16.0),
                hint: PropHint::Num {
                    speed: 1.0,
                    min: 6.0,
                    max: 128.0,
                },
                read_only: false,
            },
            PropSchema {
                key: "color",
                label: "Color",
                default_value: DefaultPropVal::Color([255, 255, 255, 255]),
                hint: PropHint::Color,
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "Button",
        inherits: Some("Control"),
        props: &[
            PropSchema {
                key: "text",
                label: "Text",
                default_value: DefaultPropVal::Str(""),
                hint: PropHint::Text,
                read_only: false,
            },
            PropSchema {
                key: "font_size",
                label: "Font Size",
                default_value: DefaultPropVal::Num(16.0),
                hint: PropHint::Num {
                    speed: 1.0,
                    min: 6.0,
                    max: 128.0,
                },
                read_only: false,
            },
            PropSchema {
                key: "color",
                label: "Color",
                default_value: DefaultPropVal::Color([51, 65, 85, 255]),
                hint: PropHint::Color,
                read_only: false,
            },
            PropSchema {
                key: "size",
                label: "Size",
                default_value: DefaultPropVal::Vec2(120.0, 40.0),
                hint: PropHint::Vec2 { speed: 1.0 },
                read_only: false,
            },
            PropSchema {
                key: "disabled",
                label: "Disabled",
                default_value: DefaultPropVal::Bool(false),
                hint: PropHint::Bool,
                read_only: false,
            },
        ],
    },
    NodeTypeSchema {
        type_name: "Timer",
        inherits: Some("Node"),
        props: &[PropSchema {
            key: "wait_time",
            label: "Wait Time",
            default_value: DefaultPropVal::Num(1.0),
            hint: PropHint::Num {
                speed: 0.1,
                min: 0.01,
                max: 3600.0,
            },
            read_only: false,
        }],
    },
];

pub fn node_type_schema(type_name: &str) -> Option<&'static NodeTypeSchema> {
    NODE_SCHEMAS.iter().find(|s| s.type_name == type_name)
}

pub fn inheritance_chain<'a>(type_name: &'a str) -> Vec<&'a str> {
    let mut chain = Vec::new();
    let mut current = Some(type_name);
    while let Some(curr) = current {
        chain.push(curr);
        current = match node_type_schema(curr) {
            Some(schema) => schema.inherits,
            None => {
                if curr != "Node" {
                    Some("Node")
                } else {
                    None
                }
            }
        };
    }
    chain
}

pub fn type_props(type_name: &str) -> &'static [PropSchema] {
    node_type_schema(type_name).map(|s| s.props).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inheritance_chains() {
        assert_eq!(
            inheritance_chain("Sprite2D"),
            vec!["Sprite2D", "Node2D", "Node"]
        );
        assert_eq!(
            inheritance_chain("CharacterBody2D"),
            vec!["CharacterBody2D", "Node2D", "Node"]
        );
        assert_eq!(
            inheritance_chain("Button"),
            vec!["Button", "Control", "Node"]
        );
        assert_eq!(inheritance_chain("Node"), vec!["Node"]);
        assert_eq!(inheritance_chain("Custom"), vec!["Custom", "Node"]);
    }

    #[test]
    fn test_type_props() {
        let props = type_props("Sprite2D");
        let keys: Vec<&str> = props.iter().map(|p| p.key).collect();
        assert_eq!(keys, vec!["texture", "atlas", "frame"]);
    }
}

use std::collections::HashSet;

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
    /// One-line description surfaced as a hover tooltip in the inspector
    /// and as the Description column in the generated node reference
    /// (`docs/reference/nodes/*.md`). Required (no default) so a prop
    /// without docs fails to compile.
    pub doc: &'static str,
    pub default_value: DefaultPropVal,
    pub hint: PropHint,
    pub read_only: bool, // To Python
}

#[derive(Clone, Copy, Debug)]
pub struct NodeTypeSchema {
    pub type_name: &'static str,
    /// One-line description surfaced in the editor's add-node dialog,
    /// tree-row hover tooltip, and generated node reference. Required.
    pub description: &'static str,
    pub inherits: Option<&'static str>,
    pub props: &'static [PropSchema],
    /// Abstract types exist in the schema chain so subclasses inherit their
    /// props, but they are hidden from the editor's add-node menu and are
    /// not intended to be instantiated directly.
    pub abstract_class: bool,
}

/// All node schemas, keyed by name. Order is arbitrary; `find` matches by
/// `type_name`, so lookups are O(n) over 16 entries — fine.
pub use crate::nodes::ALL_NODES as NODE_SCHEMAS;

pub fn node_type_schema(type_name: &str) -> Option<&'static NodeTypeSchema> {
    NODE_SCHEMAS
        .iter()
        .copied()
        .find(|s| s.type_name == type_name)
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

/// All props for `type_name`, walking the inheritance chain from leaf to
/// root and deduplicating by key (leaf declarations shadow ancestors).
/// The leaf's version of an overridden prop (its default, hint, `read_only`)
/// wins, so `Button.color` sees `[51, 65, 85, 255]`, not `Control.color`'s
/// white. Unknown types fall through to `Node` (which declares no props).
pub fn all_props(type_name: &str) -> Vec<&'static PropSchema> {
    let mut seen: HashSet<&'static str> = HashSet::new();
    let mut out = Vec::new();
    let mut current: Option<String> = Some(type_name.to_string());
    while let Some(name) = current {
        if let Some(schema) = node_type_schema(&name) {
            for prop in schema.props {
                if seen.insert(prop.key) {
                    out.push(prop);
                }
            }
            current = schema.inherits.map(String::from);
        } else if name != "Node" {
            current = Some("Node".to_string());
        } else {
            current = None;
        }
    }
    out
}

/// True when `type_str` names a type a `[props]` entry may declare.
/// Accepts aliases so hand-written manifests stay forgiving.
pub fn is_valid_custom_prop_type(type_str: &str) -> bool {
    matches!(
        type_str,
        "string"
            | "str"
            | "int"
            | "integer"
            | "number"
            | "num"
            | "float"
            | "bool"
            | "boolean"
            | "vec2"
            | "color"
    )
}

/// Check a `PropValue` against a `[props]` declared type name. Errors name
/// the expected and actual types so `pite check` and Python `set_custom_property`
/// surface the same message.
pub fn validate_custom_prop_type(type_str: &str, value: &PropValue) -> Result<(), String> {
    let want = match type_str {
        "string" | "str" => matches!(value, PropValue::Str(_)),
        "int" | "integer" => matches!(value, PropValue::Int(_)),
        "number" | "num" | "float" => matches!(value, PropValue::Num(_)),
        "bool" | "boolean" => matches!(value, PropValue::Bool(_)),
        "vec2" => matches!(value, PropValue::Vec2(_, _)),
        "color" => matches!(value, PropValue::Color(_, _, _, _)),
        other => {
            return Err(format!(
                "unknown custom prop type {:?} (want one of: {})",
                other,
                crate::CUSTOM_PROP_TYPES.join(", ")
            ));
        }
    };
    if want {
        Ok(())
    } else {
        Err(format!(
            "expected {}, got {}",
            canonical_type_name(type_str),
            value.type_name()
        ))
    }
}

fn canonical_type_name(type_str: &str) -> &'static str {
    match type_str {
        "string" | "str" => "string",
        "int" | "integer" => "int",
        "number" | "num" | "float" => "number",
        "bool" | "boolean" => "bool",
        "vec2" => "vec2",
        "color" => "color",
        _ => "unknown",
    }
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
            vec![
                "CharacterBody2D",
                "PhysicsBody2D",
                "CollisionObject2D",
                "Node2D",
                "Node"
            ]
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

        // PhysicsBody2D declares friction/restitution; RigidBody2D declares only
        // its own new props (mass, gravity_scale, lock_rotation).
        let rigid = type_props("RigidBody2D");
        let keys: Vec<&str> = rigid.iter().map(|p| p.key).collect();
        assert_eq!(keys, vec!["mass", "gravity_scale", "lock_rotation"]);
    }

    #[test]
    fn test_all_props_walks_chain() {
        // Sprite2D → Node2D → Node
        let keys: Vec<&str> = all_props("Sprite2D").iter().map(|p| p.key).collect();
        assert_eq!(
            keys,
            vec!["texture", "atlas", "frame", "position", "rotation"]
        );

        // Button shadows Control's color and size; Control's color/size are
        // filtered out after Button's versions are seen first.
        let keys: Vec<&str> = all_props("Button").iter().map(|p| p.key).collect();
        assert_eq!(
            keys,
            vec!["text", "color", "size", "disabled", "position", "font_size"]
        );
        // Button's own color default wins over Control's white.
        let color = all_props("Button")
            .into_iter()
            .find(|p| p.key == "color")
            .unwrap();
        assert_eq!(
            color.default_value,
            super::DefaultPropVal::Color([51, 65, 85, 255])
        );

        // CharacterBody2D → PhysicsBody2D → CollisionObject2D → Node2D → Node
        let keys: Vec<&str> = all_props("CharacterBody2D").iter().map(|p| p.key).collect();
        assert_eq!(
            keys,
            vec![
                "velocity",
                "friction",
                "restitution",
                "collision_layer",
                "collision_mask",
                "position",
                "rotation"
            ]
        );

        // Unknown type falls through to Node (no props).
        assert!(all_props("Custom").is_empty());
    }

    #[test]
    fn custom_prop_types_validate() {
        use crate::PropValue;
        // Aliases are accepted.
        assert!(is_valid_custom_prop_type("string"));
        assert!(is_valid_custom_prop_type("str"));
        assert!(is_valid_custom_prop_type("number"));
        assert!(is_valid_custom_prop_type("float"));
        assert!(is_valid_custom_prop_type("bool"));
        assert!(is_valid_custom_prop_type("boolean"));
        assert!(!is_valid_custom_prop_type("bytes"));
        assert!(!is_valid_custom_prop_type(""));

        // Value checks.
        assert!(validate_custom_prop_type("string", &PropValue::Str("hi".into())).is_ok());
        assert!(validate_custom_prop_type("string", &PropValue::Int(1)).is_err());
        assert!(validate_custom_prop_type("int", &PropValue::Int(1)).is_ok());
        assert!(validate_custom_prop_type("int", &PropValue::Num(1.5)).is_err());
        assert!(validate_custom_prop_type("number", &PropValue::Num(1.5)).is_ok());
        assert!(validate_custom_prop_type("number", &PropValue::Int(1)).is_err());
        assert!(validate_custom_prop_type("bool", &PropValue::Bool(true)).is_ok());
        assert!(validate_custom_prop_type("vec2", &PropValue::Vec2(1.0, 2.0)).is_ok());
        assert!(validate_custom_prop_type("color", &PropValue::Color(1, 2, 3, 4)).is_ok());

        // Unknown type name errors with the canonical list.
        let err = validate_custom_prop_type("bytes", &PropValue::Str("".into())).unwrap_err();
        assert!(err.contains("unknown custom prop type"), "got {err:?}");
        assert!(
            err.contains("string, int, number, bool, vec2, color"),
            "got {err:?}"
        );
    }

    #[test]
    fn every_schema_has_description_and_every_prop_has_doc() {
        for schema in crate::NODE_SCHEMAS {
            assert!(
                !schema.description.is_empty(),
                "{} has no description",
                schema.type_name
            );
            for prop in schema.props {
                assert!(
                    !prop.doc.is_empty(),
                    "{}::{} has no doc",
                    schema.type_name,
                    prop.key
                );
                assert!(
                    !prop.label.is_empty(),
                    "{}::{} has no label",
                    schema.type_name,
                    prop.key
                );
            }
        }
    }
}

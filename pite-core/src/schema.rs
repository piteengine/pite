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
}

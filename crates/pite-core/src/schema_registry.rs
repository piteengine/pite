// SPDX-License-Identifier: MIT OR Apache-2.0
//! Runtime node schema registry supporting built-in engine nodes and plugin-defined types.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::node::Node;
use crate::props::PropValue;
use crate::registry::NodeTypeRegistry;
use crate::schema::{NodeTypeSchema, PropHint, PropSchema, SchemaError, NODE_SCHEMAS};

/// Runtime UI presentation and editing hint for inspector property widgets.
#[derive(Clone, PartialEq, Debug)]
pub enum PropHintSpec {
    Vec2 { speed: f64 },
    AngleDeg,
    Num { speed: f64, min: f64, max: f64 },
    Int { speed: f64, min: i64, max: i64 },
    Bool,
    Text,
    Enum(Vec<Cow<'static, str>>),
    Color,
    Asset { filter: Vec<Cow<'static, str>> },
}

impl From<PropHint> for PropHintSpec {
    fn from(hint: PropHint) -> Self {
        match hint {
            PropHint::Vec2 { speed } => Self::Vec2 { speed },
            PropHint::AngleDeg => Self::AngleDeg,
            PropHint::Num { speed, min, max } => Self::Num { speed, min, max },
            PropHint::Int { speed, min, max } => Self::Int { speed, min, max },
            PropHint::Bool => Self::Bool,
            PropHint::Text => Self::Text,
            PropHint::Enum(options) => {
                Self::Enum(options.iter().copied().map(Cow::Borrowed).collect())
            }
            PropHint::Color => Self::Color,
            PropHint::Asset { filter } => Self::Asset {
                filter: filter.iter().copied().map(Cow::Borrowed).collect(),
            },
        }
    }
}

/// Runtime metadata describing a property on a node type.
#[derive(Clone, Debug)]
pub struct PropDef {
    pub key: Cow<'static, str>,
    pub label: Cow<'static, str>,
    pub doc: Cow<'static, str>,
    pub default_value: PropValue,
    pub hint: PropHintSpec,
    pub read_only: bool,
}

impl PropDef {
    /// Checks whether `val` matches this property's schema default.
    pub fn matches_default(&self, val: &PropValue) -> bool {
        match (&self.default_value, val) {
            (PropValue::Num(a), PropValue::Num(b)) => (a - b).abs() < f64::EPSILON,
            (PropValue::Int(a), PropValue::Int(b)) => a == b,
            (PropValue::Bool(a), PropValue::Bool(b)) => a == b,
            (PropValue::Vec2(x1, y1), PropValue::Vec2(x2, y2)) => {
                (x1 - x2).abs() < f64::EPSILON && (y1 - y2).abs() < f64::EPSILON
            }
            (PropValue::Str(a), PropValue::Str(b)) => a == b,
            (PropValue::Color(r1, g1, b1, a1), PropValue::Color(r2, g2, b2, a2)) => {
                r1 == r2 && g1 == g2 && b1 == b2 && a1 == a2
            }
            _ => false,
        }
    }
}

impl From<&'static PropSchema> for PropDef {
    fn from(prop: &'static PropSchema) -> Self {
        Self {
            key: Cow::Borrowed(prop.key),
            label: Cow::Borrowed(prop.label),
            doc: Cow::Borrowed(prop.doc),
            default_value: prop.default_value.to_prop_value(),
            hint: PropHintSpec::from(prop.hint),
            read_only: prop.read_only,
        }
    }
}

/// Schema metadata for an engine- or plugin-declared signal on a node type.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SignalSchema {
    pub name: Cow<'static, str>,
    pub payload: Vec<Cow<'static, str>>,
}

/// Runtime schema definition for a built-in or plugin-registered node type.
#[derive(Clone, Debug)]
pub struct NodeSchema {
    pub type_name: Cow<'static, str>,
    pub description: Cow<'static, str>,
    pub inherits: Option<Cow<'static, str>>,
    pub props: Vec<PropDef>,
    pub signals: Vec<SignalSchema>,
    pub abstract_class: bool,
    /// Owning plugin ID, or `None` for built-in engine node types.
    pub plugin_id: Option<String>,
}

impl From<&'static NodeTypeSchema> for NodeSchema {
    fn from(schema: &'static NodeTypeSchema) -> Self {
        Self {
            type_name: Cow::Borrowed(schema.type_name),
            description: Cow::Borrowed(schema.description),
            inherits: schema.inherits.map(Cow::Borrowed),
            props: schema.props.iter().map(PropDef::from).collect(),
            signals: builtin_signals_for(schema.type_name),
            abstract_class: schema.abstract_class,
            plugin_id: None,
        }
    }
}

fn builtin_signals_for(type_name: &str) -> Vec<SignalSchema> {
    match type_name {
        "Button" => vec![SignalSchema {
            name: Cow::Borrowed("pressed"),
            payload: Vec::new(),
        }],
        "Timer" => vec![SignalSchema {
            name: Cow::Borrowed("timeout"),
            payload: Vec::new(),
        }],
        "AnimatedSprite2D" => vec![
            SignalSchema {
                name: Cow::Borrowed("animation_finished"),
                payload: vec![Cow::Borrowed("str")],
            },
            SignalSchema {
                name: Cow::Borrowed("animation_looped"),
                payload: vec![Cow::Borrowed("str")],
            },
        ],
        "Area2D" => vec![
            SignalSchema {
                name: Cow::Borrowed("body_entered"),
                payload: vec![Cow::Borrowed("str")],
            },
            SignalSchema {
                name: Cow::Borrowed("body_exited"),
                payload: vec![Cow::Borrowed("str")],
            },
            SignalSchema {
                name: Cow::Borrowed("area_entered"),
                payload: vec![Cow::Borrowed("str")],
            },
            SignalSchema {
                name: Cow::Borrowed("area_exited"),
                payload: vec![Cow::Borrowed("str")],
            },
        ],
        _ => Vec::new(),
    }
}

/// Runtime registry of all known node schemas (built-in engine types plus plugin nodes).
#[derive(Clone, Debug)]
pub struct SchemaRegistry {
    schemas: Vec<NodeSchema>,
    by_name: HashMap<String, usize>,
}

impl SchemaRegistry {
    /// Initializes a registry pre-populated with all built-in engine node schemas.
    pub fn new() -> Self {
        let mut reg = Self {
            schemas: Vec::with_capacity(NODE_SCHEMAS.len()),
            by_name: HashMap::with_capacity(NODE_SCHEMAS.len()),
        };
        for &schema in NODE_SCHEMAS {
            let idx = reg.schemas.len();
            reg.by_name.insert(schema.type_name.to_string(), idx);
            reg.schemas.push(NodeSchema::from(schema));
        }
        reg
    }

    /// Registers a new node schema (for example from a plugin manifest).
    ///
    /// # Errors
    /// Returns [`SchemaError`] if `type_name` is empty or already registered,
    /// `inherits` is missing/unknown/cyclic, or duplicate property/signal names exist.
    pub fn register(&mut self, schema: NodeSchema) -> std::result::Result<(), SchemaError> {
        let type_name = schema.type_name.as_ref();
        if type_name.is_empty() {
            return Err(SchemaError::EmptyNodeType);
        }
        if let Some(&existing_idx) = self.by_name.get(type_name) {
            let existing = &self.schemas[existing_idx];
            let existing_owner = existing
                .plugin_id
                .as_deref()
                .map(|id| format!("plugin `{id}`"))
                .unwrap_or_else(|| "built-in engine".to_string());
            let new_owner = schema
                .plugin_id
                .as_deref()
                .map(|id| format!("plugin `{id}`"))
                .unwrap_or_else(|| "built-in engine".to_string());
            return Err(SchemaError::DuplicateNodeType {
                type_name: type_name.to_string(),
                existing_owner,
                new_owner,
            });
        }
        let Some(parent) = schema.inherits.as_deref() else {
            return Err(SchemaError::MissingBaseType {
                type_name: type_name.to_string(),
            });
        };
        if parent == type_name {
            return Err(SchemaError::InheritanceCycle {
                type_name: type_name.to_string(),
            });
        }
        if !self.contains(parent) {
            return Err(SchemaError::UnknownBaseType {
                type_name: type_name.to_string(),
                inherits: parent.to_string(),
            });
        }
        let mut seen_props = HashSet::new();
        for prop in &schema.props {
            if !seen_props.insert(prop.key.as_ref()) {
                return Err(SchemaError::DuplicateProp {
                    type_name: type_name.to_string(),
                    key: prop.key.to_string(),
                });
            }
        }
        let mut seen_signals = HashSet::new();
        for sig in &schema.signals {
            if !seen_signals.insert(sig.name.as_ref()) {
                return Err(SchemaError::DuplicateSignal {
                    type_name: type_name.to_string(),
                    name: sig.name.to_string(),
                });
            }
        }

        let idx = self.schemas.len();
        self.by_name.insert(type_name.to_string(), idx);
        self.schemas.push(schema);
        Ok(())
    }

    /// Looks up a node schema by `type_name`.
    pub fn get(&self, type_name: &str) -> Option<&NodeSchema> {
        self.by_name
            .get(type_name)
            .and_then(|&idx| self.schemas.get(idx))
    }

    /// Returns `true` if `type_name` is registered.
    pub fn contains(&self, type_name: &str) -> bool {
        self.by_name.contains_key(type_name)
    }

    /// Iterates over all registered node schemas in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &NodeSchema> {
        self.schemas.iter()
    }

    /// Computes the inheritance chain for `type_name` from leaf to root.
    ///
    /// Unknown types fall through to `Node` (matching [`crate::inheritance_chain`]).
    pub fn inheritance_chain<'a>(&'a self, type_name: &'a str) -> Vec<&'a str> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut current = Some(type_name);
        while let Some(curr) = current {
            if !seen.insert(curr) {
                break;
            }
            chain.push(curr);
            current = match self.get(curr) {
                Some(schema) => schema.inherits.as_deref(),
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

    /// Returns the properties declared directly on `type_name`.
    pub fn type_props(&self, type_name: &str) -> &[PropDef] {
        self.get(type_name)
            .map(|s| s.props.as_slice())
            .unwrap_or(&[])
    }

    /// Returns all properties for `type_name` from leaf to root, deduplicated by key.
    pub fn all_props(&self, type_name: &str) -> Vec<&PropDef> {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut out = Vec::new();
        for owner in self.inheritance_chain(type_name) {
            for prop in self.type_props(owner) {
                if seen.insert(prop.key.as_ref()) {
                    out.push(prop);
                }
            }
        }
        out
    }

    /// Returns all engine- or plugin-declared signals for `type_name`, walking leaf to root.
    pub fn signals_for(&self, type_name: &str) -> Vec<&SignalSchema> {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut out = Vec::new();
        for owner in self.inheritance_chain(type_name) {
            if let Some(schema) = self.get(owner) {
                for sig in &schema.signals {
                    if seen.insert(sig.name.as_ref()) {
                        out.push(sig);
                    }
                }
            }
        }
        out
    }

    /// Returns the set of all property keys declared on any registered node schema.
    pub fn all_prop_keys(&self) -> HashSet<&str> {
        self.schemas
            .iter()
            .flat_map(|s| s.props.iter().map(|p| p.key.as_ref()))
            .collect()
    }

    /// Builds a [`NodeTypeRegistry`] containing factories for every registered node type.
    pub fn to_node_type_registry(&self) -> NodeTypeRegistry {
        let mut reg = NodeTypeRegistry::empty();
        for schema in &self.schemas {
            let type_name = schema.type_name.to_string();
            let factory_type = type_name.clone();
            reg.register(
                type_name,
                Box::new(move |id| Node::new(id, factory_type.clone())),
            );
        }
        reg
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialogue_schema(type_name: &str, inherits: &str, plugin: &str) -> NodeSchema {
        NodeSchema {
            type_name: Cow::Owned(type_name.to_string()),
            description: Cow::Borrowed("Interactive dialogue box."),
            inherits: Some(Cow::Owned(inherits.to_string())),
            props: vec![
                PropDef {
                    key: Cow::Borrowed("typing_speed"),
                    label: Cow::Borrowed("Typing Speed"),
                    doc: Cow::Borrowed("Characters revealed per second."),
                    default_value: PropValue::Num(30.0),
                    hint: PropHintSpec::Num {
                        speed: 1.0,
                        min: 1.0,
                        max: 200.0,
                    },
                    read_only: false,
                },
                PropDef {
                    key: Cow::Borrowed("color"),
                    label: Cow::Borrowed("Color"),
                    doc: Cow::Borrowed("Dialogue text tint."),
                    default_value: PropValue::Color(200, 220, 255, 255),
                    hint: PropHintSpec::Color,
                    read_only: false,
                },
            ],
            signals: vec![SignalSchema {
                name: Cow::Borrowed("dialogue_finished"),
                payload: vec![Cow::Borrowed("str")],
            }],
            abstract_class: false,
            plugin_id: Some(plugin.to_string()),
        }
    }

    #[test]
    fn builtin_registry_matches_static_schemas() {
        let reg = SchemaRegistry::new();
        for &static_schema in NODE_SCHEMAS {
            let dyn_schema = reg
                .get(static_schema.type_name)
                .expect("built-in type must exist");
            assert_eq!(dyn_schema.abstract_class, static_schema.abstract_class);
            assert_eq!(dyn_schema.inherits.as_deref(), static_schema.inherits);
            let static_keys: Vec<&str> = crate::schema::all_props(static_schema.type_name)
                .into_iter()
                .map(|p| p.key)
                .collect();
            let dyn_keys: Vec<&str> = reg
                .all_props(static_schema.type_name)
                .into_iter()
                .map(|p| p.key.as_ref())
                .collect();
            assert_eq!(dyn_keys, static_keys);
        }
        assert_eq!(
            reg.signals_for("Button")
                .iter()
                .map(|s| s.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["pressed"]
        );
        assert_eq!(
            reg.signals_for("Area2D")
                .iter()
                .map(|s| s.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["body_entered", "body_exited", "area_entered", "area_exited"]
        );
    }

    #[test]
    fn plugin_node_inherits_props_signals_and_overrides_leaf_defaults() {
        let mut reg = SchemaRegistry::new();
        reg.register(dialogue_schema("DialogueBox", "Button", "dialogue"))
            .unwrap();

        assert_eq!(
            reg.inheritance_chain("DialogueBox"),
            vec!["DialogueBox", "Button", "Control", "Node2D", "Node"]
        );

        // Leaf `color` shadows Button's `color`, while inheriting Button/Control/Node2D props.
        let props = reg.all_props("DialogueBox");
        let color_count = props.iter().filter(|p| p.key == "color").count();
        assert_eq!(color_count, 1);
        let color = props.iter().find(|p| p.key == "color").unwrap();
        assert_eq!(color.default_value, PropValue::Color(200, 220, 255, 255));
        assert!(props.iter().any(|p| p.key == "typing_speed"));
        assert!(props.iter().any(|p| p.key == "position"));

        // Inherits Button's `pressed` signal alongside its own `dialogue_finished`.
        let signals: Vec<&str> = reg
            .signals_for("DialogueBox")
            .into_iter()
            .map(|s| s.name.as_ref())
            .collect();
        assert_eq!(signals, vec!["dialogue_finished", "pressed"]);

        let node_reg = reg.to_node_type_registry();
        assert!(node_reg.contains("DialogueBox"));
        let created = node_reg
            .create(crate::NodeId::from("dlg"), "DialogueBox")
            .unwrap();
        assert_eq!(created.type_name, "DialogueBox");
    }

    #[test]
    fn registry_rejects_duplicates_unknown_bases_and_cycles() {
        let mut reg = SchemaRegistry::new();
        reg.register(dialogue_schema("DialogueBox", "Control", "dialogue"))
            .unwrap();

        let dup = reg
            .register(dialogue_schema("DialogueBox", "Control", "other_plugin"))
            .unwrap_err();
        assert!(matches!(dup, SchemaError::DuplicateNodeType { .. }));
        assert!(dup.to_string().contains("plugin `dialogue`"));
        assert!(dup.to_string().contains("plugin `other_plugin`"));

        let builtin_dup = reg
            .register(dialogue_schema("Sprite2D", "Node2D", "other_plugin"))
            .unwrap_err();
        assert!(builtin_dup.to_string().contains("built-in engine"));

        let missing_base = reg
            .register(dialogue_schema("Orphan", "DoesNotExist", "dialogue"))
            .unwrap_err();
        assert!(matches!(missing_base, SchemaError::UnknownBaseType { .. }));

        let self_cycle = reg
            .register(dialogue_schema("LoopNode", "LoopNode", "dialogue"))
            .unwrap_err();
        assert!(matches!(self_cycle, SchemaError::InheritanceCycle { .. }));
    }
}

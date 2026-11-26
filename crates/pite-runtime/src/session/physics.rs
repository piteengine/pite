// SPDX-License-Identifier: MIT OR Apache-2.0

use pite_core::{global_positions, Node, PropValue, SchemaRegistry};
use pite_physics::{
    world::{ColliderProps, CollisionData},
    PhysicsWorld, ShapeConfig,
};

use super::GameSession;

/// Read `key` from `node.props`, falling back to the type's schema default in `schemas`
/// (and coercing integer literals to numbers).
pub(crate) fn prop_num_in(schemas: &SchemaRegistry, node: &Node, key: &str) -> f32 {
    if let Some(PropValue::Num(n)) = node.props.get(key) {
        return *n as f32;
    }
    if let Some(PropValue::Int(n)) = node.props.get(key) {
        return *n as f32;
    }
    schemas
        .all_props(&node.type_name)
        .into_iter()
        .find(|p| p.key == key)
        .and_then(|p| match p.default_value {
            PropValue::Num(n) => Some(n as f32),
            PropValue::Int(n) => Some(n as f32),
            _ => None,
        })
        .unwrap_or(0.0)
}

pub(crate) fn prop_int_in(schemas: &SchemaRegistry, node: &Node, key: &str) -> i64 {
    if let Some(PropValue::Int(n)) = node.props.get(key) {
        return *n;
    }
    if let Some(PropValue::Num(n)) = node.props.get(key) {
        return *n as i64;
    }
    schemas
        .all_props(&node.type_name)
        .into_iter()
        .find(|p| p.key == key)
        .and_then(|p| match p.default_value {
            PropValue::Int(n) => Some(n),
            _ => None,
        })
        .unwrap_or(0)
}

pub(crate) fn prop_bool_in(schemas: &SchemaRegistry, node: &Node, key: &str) -> bool {
    if let Some(PropValue::Bool(b)) = node.props.get(key) {
        return *b;
    }
    schemas
        .all_props(&node.type_name)
        .into_iter()
        .find(|p| p.key == key)
        .map(|p| matches!(p.default_value, PropValue::Bool(true)))
        .unwrap_or(false)
}

/// Which physics body a node type becomes. One place decides; both
/// `rebuild_physics` and `CollisionShape2D`'s parent-body walker ask this
/// instead of repeating the type match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhysicsRole {
    Static,
    Character,
    Rigid,
    Area,
}

pub(crate) fn physics_role_in(schemas: &SchemaRegistry, type_name: &str) -> Option<PhysicsRole> {
    for ancestor in schemas.inheritance_chain(type_name) {
        match ancestor {
            "StaticBody2D" => return Some(PhysicsRole::Static),
            "CharacterBody2D" => return Some(PhysicsRole::Character),
            "RigidBody2D" => return Some(PhysicsRole::Rigid),
            "Area2D" => return Some(PhysicsRole::Area),
            _ => {}
        }
    }
    None
}

impl GameSession {
    pub fn rebuild_physics(&mut self) {
        let schemas = self.schemas.clone();
        let mut world = PhysicsWorld::new();
        self.host.with_tree(|tree| {
            let globals = global_positions(tree);
            for node in tree.iter() {
                let pos = globals.get(&node.id).copied().unwrap_or((0.0, 0.0));
                let rot = prop_num_in(&schemas, node, "rotation");
                if let Some(role) = physics_role_in(&schemas, &node.type_name) {
                    match role {
                        PhysicsRole::Static => {
                            world.add_static_body(
                                node.id.clone(),
                                (pos.0 as f32, pos.1 as f32),
                                rot,
                            );
                        }
                        PhysicsRole::Character => {
                            world.add_character_body(
                                node.id.clone(),
                                (pos.0 as f32, pos.1 as f32),
                                rot,
                            );
                        }
                        PhysicsRole::Rigid => {
                            world.add_rigid_body(
                                node.id.clone(),
                                (pos.0 as f32, pos.1 as f32),
                                rot,
                                prop_num_in(&schemas, node, "mass"),
                                prop_num_in(&schemas, node, "gravity_scale"),
                                prop_bool_in(&schemas, node, "lock_rotation"),
                            );
                        }
                        PhysicsRole::Area => {
                            world.add_area(node.id.clone(), (pos.0 as f32, pos.1 as f32), rot);
                        }
                    }
                } else if node.type_name == "TileMap2D" {
                    if let Some(doc) = self.host.get_tilemap_doc(node.id.as_str()) {
                        let tile_size = (doc.tile_size[0] as f32, doc.tile_size[1] as f32);
                        for (idx, layer) in doc.layers.iter().enumerate() {
                            if layer.collision_enabled {
                                let solid_cells = doc.solid_cells(idx);
                                world.add_tilemap_colliders(
                                    &node.id,
                                    idx,
                                    (pos.0 as f32, pos.1 as f32),
                                    tile_size,
                                    &solid_cells,
                                    CollisionData {
                                        layer: layer.collision_layer,
                                        mask: layer.collision_mask,
                                    },
                                );
                            }
                        }
                    }
                }
            }

            for node in tree.iter().filter(|n| n.type_name == "CollisionShape2D") {
                let disabled = match node.props.get("disabled") {
                    Some(PropValue::Bool(b)) => *b,
                    _ => false,
                };
                if disabled {
                    continue;
                }

                let mut parent_id = node.parent.clone();
                let mut parent_body: Option<&pite_core::Node> = None;
                while let Some(pid) = parent_id {
                    if let Some(pnode) = tree.get(&pid) {
                        if physics_role_in(&schemas, &pnode.type_name).is_some() {
                            parent_body = Some(pnode);
                            break;
                        }
                        parent_id = pnode.parent.clone();
                    } else {
                        break;
                    }
                }

                let Some(body) = parent_body else {
                    continue;
                };

                let body_pos = globals.get(&body.id).copied().unwrap_or((0.0, 0.0));
                let shape_pos = globals.get(&node.id).copied().unwrap_or((0.0, 0.0));
                let offset = (
                    (shape_pos.0 - body_pos.0) as f32,
                    (shape_pos.1 - body_pos.1) as f32,
                );

                let shape_rot = match node.props.get("rotation") {
                    Some(PropValue::Num(r)) => *r as f32,
                    _ => 0.0,
                };

                let shape_type = match node.props.get("shape") {
                    Some(PropValue::Str(s)) => s.as_str(),
                    _ => "box",
                };

                let config = match shape_type {
                    "circle" => {
                        let radius = match node.props.get("radius") {
                            Some(PropValue::Num(r)) => *r as f32,
                            _ => 16.0,
                        };
                        ShapeConfig::Circle { radius }
                    }
                    "capsule" => {
                        let radius = match node.props.get("radius") {
                            Some(PropValue::Num(r)) => *r as f32,
                            _ => 16.0,
                        };
                        let height = match node.props.get("height") {
                            Some(PropValue::Num(h)) => *h as f32,
                            _ => 32.0,
                        };
                        ShapeConfig::Capsule { radius, height }
                    }
                    _ => {
                        let (w, h) = match node.props.get("size") {
                            Some(PropValue::Vec2(w, h)) => (*w as f32, *h as f32),
                            _ => (32.0, 32.0),
                        };
                        ShapeConfig::Box {
                            width: w,
                            height: h,
                        }
                    }
                };

                let layer = prop_int_in(&schemas, body, "collision_layer") as u32;
                let mask = prop_int_in(&schemas, body, "collision_mask") as u32;
                // `friction`/`restitution` live on `PhysicsBody2D`; Area2D
                // doesn't inherit them, so it falls through to 0.0 (harmless
                // — sensors don't apply either).
                let friction = prop_num_in(&schemas, body, "friction");
                let restitution = prop_num_in(&schemas, body, "restitution");

                let is_sensor =
                    physics_role_in(&schemas, &body.type_name) == Some(PhysicsRole::Area);

                world.attach_shape(
                    &body.id,
                    node.id.clone(),
                    config,
                    &CollisionData { layer, mask },
                    ColliderProps {
                        offset,
                        rotation: shape_rot,
                        is_sensor,
                        friction,
                        restitution,
                    },
                );
            }
        });

        *self.physics.lock().unwrap_or_else(|poisoned| {
            tracing::error!("physics mutex poisoned; recovering");
            poisoned.into_inner()
        }) = world;
        pite_script::set_current_physics(Some(self.physics.clone()));
    }
}

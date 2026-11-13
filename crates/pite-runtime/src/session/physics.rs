// SPDX-License-Identifier: MIT OR Apache-2.0

use pite_core::{global_position, Node, PropValue};
use pite_physics::{PhysicsWorld, ShapeConfig};

use super::GameSession;

/// Read `key` from `node.props`, falling back to the type's schema default
/// (and coercing integer literals to numbers). `rebuild_physics` uses these
/// instead of hardcoded fallbacks so a schema default change propagates
/// without touching the runtime.
pub(crate) fn prop_num(node: &Node, key: &str) -> f32 {
    if let Some(PropValue::Num(n)) = node.props.get(key) {
        return *n as f32;
    }
    if let Some(PropValue::Int(n)) = node.props.get(key) {
        return *n as f32;
    }
    pite_core::all_props(&node.type_name)
        .iter()
        .find(|p| p.key == key)
        .and_then(|p| match p.default_value {
            pite_core::DefaultPropVal::Num(n) => Some(n as f32),
            pite_core::DefaultPropVal::Int(n) => Some(n as f32),
            _ => None,
        })
        .unwrap_or(0.0)
}

pub(crate) fn prop_int(node: &Node, key: &str) -> i64 {
    if let Some(PropValue::Int(n)) = node.props.get(key) {
        return *n;
    }
    if let Some(PropValue::Num(n)) = node.props.get(key) {
        return *n as i64;
    }
    pite_core::all_props(&node.type_name)
        .iter()
        .find(|p| p.key == key)
        .and_then(|p| match p.default_value {
            pite_core::DefaultPropVal::Int(n) => Some(n),
            _ => None,
        })
        .unwrap_or(0)
}

pub(crate) fn prop_bool(node: &Node, key: &str) -> bool {
    if let Some(PropValue::Bool(b)) = node.props.get(key) {
        return *b;
    }
    pite_core::all_props(&node.type_name)
        .iter()
        .find(|p| p.key == key)
        .map(|p| matches!(p.default_value, pite_core::DefaultPropVal::Bool(true)))
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

pub(crate) fn physics_role(type_name: &str) -> Option<PhysicsRole> {
    match type_name {
        "StaticBody2D" => Some(PhysicsRole::Static),
        "CharacterBody2D" => Some(PhysicsRole::Character),
        "RigidBody2D" => Some(PhysicsRole::Rigid),
        "Area2D" => Some(PhysicsRole::Area),
        _ => None,
    }
}

impl GameSession {
    pub fn rebuild_physics(&mut self) {
        let mut world = PhysicsWorld::new();
        self.host.with_tree(|tree| {
            for node in tree.iter() {
                let pos = global_position(tree, &node.id);
                let rot = prop_num(node, "rotation");
                if let Some(role) = physics_role(&node.type_name) {
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
                                prop_num(node, "mass"),
                                prop_num(node, "gravity_scale"),
                                prop_bool(node, "lock_rotation"),
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
                                    layer.collision_layer,
                                    layer.collision_mask,
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
                        if physics_role(&pnode.type_name).is_some() {
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

                let body_pos = global_position(tree, &body.id);
                let shape_pos = global_position(tree, &node.id);
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

                let layer = prop_int(body, "collision_layer") as u32;
                let mask = prop_int(body, "collision_mask") as u32;
                // `friction`/`restitution` live on `PhysicsBody2D`; Area2D
                // doesn't inherit them, so it falls through to 0.0 (harmless
                // — sensors don't apply either).
                let friction = prop_num(body, "friction");
                let restitution = prop_num(body, "restitution");

                let is_sensor = body.type_name == "Area2D";

                world.attach_shape(
                    &body.id,
                    node.id.clone(),
                    config,
                    offset,
                    shape_rot,
                    is_sensor,
                    layer,
                    mask,
                    friction,
                    restitution,
                );
            }
        });

        *self.physics.lock().unwrap() = world;
        pite_script::set_current_physics(Some(self.physics.clone()));
    }
}

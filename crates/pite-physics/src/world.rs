// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use pite_core::NodeId;
use rapier2d::control::{CharacterLength, KinematicCharacterController};
use rapier2d::dynamics::{CCDSolver, SoftBodyTearEvent};
use rapier2d::prelude::*;

use crate::character::CharacterMoveResult;
use crate::query::RaycastHit;
use crate::shape::ShapeConfig;

/// Physics events dispatched when bodies enter or exit trigger areas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicsEvent {
    /// A physics body entered an Area2D trigger zone.
    BodyEntered { area_id: NodeId, body_id: NodeId },
    /// A physics body exited an Area2D trigger zone.
    BodyExited { area_id: NodeId, body_id: NodeId },
    /// An Area2D overlapped with another Area2D trigger zone.
    AreaEntered {
        area_id: NodeId,
        other_area_id: NodeId,
    },
    /// An Area2D stopped overlapping with another Area2D trigger zone.
    AreaExited {
        area_id: NodeId,
        other_area_id: NodeId,
    },
}

#[derive(Default)]
struct PhysicsEventCollector {
    events: Mutex<Vec<CollisionEvent>>,
}

impl EventHandler for PhysicsEventCollector {
    fn handle_collision_event(
        &self,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        event: CollisionEvent,
        _: Option<&ContactPair>,
    ) {
        if let Ok(mut lock) = self.events.lock() {
            lock.push(event);
        }
    }

    fn handle_contact_force_event(
        &self,
        _dt: Real,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        _contact_pair: &ContactPair,
        _total_force_magnitude: Real,
    ) {
    }

    fn handle_soft_body_tear_event(&self, _soft_bodies: &SoftBodySet, _event: &SoftBodyTearEvent) {}
}

/// 2D physics simulation world managing Rapier rigid bodies, colliders, and collision events.
pub struct PhysicsWorld {
    /// Global gravity vector applied to dynamic rigid bodies (pixels per second squared).
    pub gravity: Vector,
    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    rigid_body_set: RigidBodySet,
    collider_set: ColliderSet,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    soft_body_set: SoftBodySet,
    ccd_solver: CCDSolver,
    character_controller: KinematicCharacterController,
    event_collector: PhysicsEventCollector,

    // Node <-> Handle mappings
    node_to_body: HashMap<NodeId, RigidBodyHandle>,
    body_to_node: HashMap<RigidBodyHandle, NodeId>,
    node_to_collider: HashMap<NodeId, ColliderHandle>,
    collider_to_node: HashMap<ColliderHandle, NodeId>,
    collider_to_body: HashMap<ColliderHandle, NodeId>,

    // Shape configs and offsets
    shape_configs: HashMap<NodeId, (ShapeConfig, Vector)>,

    // Body kinds & tags
    area_nodes: HashSet<NodeId>,
    character_nodes: HashSet<NodeId>,
    rigid_nodes: HashSet<NodeId>,
    static_nodes: HashSet<NodeId>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsWorld {
    /// Creates a new 2D physics world initialized with 60 Hz fixed timestep and downward gravity.
    pub fn new() -> Self {
        let mut character_controller = KinematicCharacterController::default();
        character_controller.up = Vector::new(0.0, -1.0); // Standard 2D coordinate system: -Y is up
        character_controller.offset = CharacterLength::Absolute(0.01);
        character_controller.slide = true;

        let mut integration_parameters = IntegrationParameters::default();
        integration_parameters.dt = 1.0 / 60.0;

        Self {
            gravity: Vector::new(0.0, 980.0), // 980 pixels/sec^2
            integration_parameters,
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            soft_body_set: SoftBodySet::new(),
            ccd_solver: CCDSolver::new(),
            character_controller,
            event_collector: PhysicsEventCollector::default(),

            node_to_body: HashMap::new(),
            body_to_node: HashMap::new(),
            node_to_collider: HashMap::new(),
            collider_to_node: HashMap::new(),
            collider_to_body: HashMap::new(),

            shape_configs: HashMap::new(),

            area_nodes: HashSet::new(),
            character_nodes: HashSet::new(),
            rigid_nodes: HashSet::new(),
            static_nodes: HashSet::new(),
        }
    }

    /// Add a StaticBody2D to the physics world.
    pub fn add_static_body(&mut self, id: NodeId, position: (f32, f32), rotation: f32) {
        self.remove_node(&id);
        let rb = RigidBodyBuilder::fixed()
            .translation(Vector::new(position.0, position.1))
            .rotation(rotation)
            .build();
        let handle = self.rigid_body_set.insert(rb);
        self.node_to_body.insert(id.clone(), handle);
        self.body_to_node.insert(handle, id.clone());
        self.static_nodes.insert(id);
    }

    /// Add a CharacterBody2D (kinematic position-based) to the physics world.
    pub fn add_character_body(&mut self, id: NodeId, position: (f32, f32), rotation: f32) {
        self.remove_node(&id);
        let rb = RigidBodyBuilder::kinematic_position_based()
            .translation(Vector::new(position.0, position.1))
            .rotation(rotation)
            .build();
        let handle = self.rigid_body_set.insert(rb);
        self.node_to_body.insert(id.clone(), handle);
        self.body_to_node.insert(handle, id.clone());
        self.character_nodes.insert(id);
    }

    /// Add a RigidBody2D (dynamic) to the physics world.
    pub fn add_rigid_body(
        &mut self,
        id: NodeId,
        position: (f32, f32),
        rotation: f32,
        mass: f32,
        gravity_scale: f32,
        lock_rotation: bool,
    ) {
        self.remove_node(&id);
        let mut builder = RigidBodyBuilder::dynamic()
            .translation(Vector::new(position.0, position.1))
            .rotation(rotation)
            .gravity_scale(gravity_scale)
            .additional_mass(mass.max(0.001));
        if lock_rotation {
            builder = builder.lock_rotations();
        }
        let handle = self.rigid_body_set.insert(builder.build());
        self.node_to_body.insert(id.clone(), handle);
        self.body_to_node.insert(handle, id.clone());
        self.rigid_nodes.insert(id);
    }

    /// Add an Area2D to the physics world.
    pub fn add_area(&mut self, id: NodeId, position: (f32, f32), rotation: f32) {
        self.remove_node(&id);
        let rb = RigidBodyBuilder::fixed()
            .translation(Vector::new(position.0, position.1))
            .rotation(rotation)
            .build();
        let handle = self.rigid_body_set.insert(rb);
        self.node_to_body.insert(id.clone(), handle);
        self.body_to_node.insert(handle, id.clone());
        self.area_nodes.insert(id);
    }

    /// Attach a CollisionShape2D collider to a parent body or area.
    pub fn attach_shape(
        &mut self,
        body_id: &NodeId,
        shape_node_id: NodeId,
        config: ShapeConfig,
        offset: (f32, f32),
        rotation: f32,
        is_sensor: bool,
        collision_layer: u32,
        collision_mask: u32,
        friction: f32,
        restitution: f32,
    ) {
        if let Some(&body_handle) = self.node_to_body.get(body_id) {
            let shared_shape = config.to_shared_shape();
            let mut collider_builder = ColliderBuilder::new(shared_shape)
                .translation(Vector::new(offset.0, offset.1))
                .rotation(rotation)
                .friction(friction)
                .restitution(restitution)
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .active_collision_types(ActiveCollisionTypes::all());

            if is_sensor || self.area_nodes.contains(body_id) {
                collider_builder = collider_builder.sensor(true);
            }

            // Set collision filter groups
            let memberships = Group::from_bits_truncate(collision_layer);
            let filter = Group::from_bits_truncate(collision_mask);
            collider_builder = collider_builder.collision_groups(InteractionGroups::new(
                memberships,
                filter,
                InteractionTestMode::And,
            ));

            let collider = collider_builder.build();
            let collider_handle = self.collider_set.insert_with_parent(
                collider,
                body_handle,
                &mut self.rigid_body_set,
            );

            if let Some(c) = self.collider_set.get(collider_handle) {
                let aabb =
                    c.compute_broad_phase_aabb(&self.integration_parameters, &self.rigid_body_set);
                self.broad_phase
                    .set_aabb(&self.integration_parameters, collider_handle, aabb);
            }

            self.node_to_collider
                .insert(shape_node_id.clone(), collider_handle);
            self.collider_to_node
                .insert(collider_handle, shape_node_id.clone());
            self.collider_to_body
                .insert(collider_handle, body_id.clone());
            self.shape_configs
                .insert(shape_node_id, (config, Vector::new(offset.0, offset.1)));
        }
    }

    /// Add static merged colliders for a TileMap2D layer.
    pub fn add_tilemap_colliders(
        &mut self,
        tilemap_node_id: &NodeId,
        layer_idx: usize,
        origin: (f32, f32),
        tile_size: (f32, f32),
        solid_cells: &[(i32, i32)],
        collision_layer: u32,
        collision_mask: u32,
    ) {
        let merged = crate::tilemap::merge_tile_cells(solid_cells);
        if merged.is_empty() {
            return;
        }

        let layer_body_id = NodeId::from(format!("{tilemap_node_id}_layer_{layer_idx}"));
        self.add_static_body(layer_body_id.clone(), origin, 0.0);

        for (i, rect) in merged.into_iter().enumerate() {
            let shape_id = NodeId::from(format!("{layer_body_id}_rect_{i}"));
            let w_world = rect.width as f32 * tile_size.0;
            let h_world = rect.height as f32 * tile_size.1;
            let offset_x = (rect.col as f32 + rect.width as f32 * 0.5) * tile_size.0;
            let offset_y = (rect.row as f32 + rect.height as f32 * 0.5) * tile_size.1;

            self.attach_shape(
                &layer_body_id,
                shape_id,
                ShapeConfig::Box {
                    width: w_world,
                    height: h_world,
                },
                (offset_x, offset_y),
                0.0,
                false,
                collision_layer,
                collision_mask,
                0.0,
                0.0,
            );
        }
    }

    /// Remove a node (body or shape) from the physics world.
    pub fn remove_node(&mut self, id: &NodeId) {
        if let Some(body_handle) = self.node_to_body.remove(id) {
            self.body_to_node.remove(&body_handle);
            self.area_nodes.remove(id);
            self.character_nodes.remove(id);
            self.rigid_nodes.remove(id);
            self.static_nodes.remove(id);
            self.rigid_body_set.remove(
                body_handle,
                &mut self.island_manager,
                &mut self.collider_set,
                &mut self.impulse_joint_set,
                &mut self.multibody_joint_set,
                &mut self.soft_body_set,
                true,
            );
        }
        if let Some(collider_handle) = self.node_to_collider.remove(id) {
            self.collider_to_node.remove(&collider_handle);
            self.collider_to_body.remove(&collider_handle);
            self.shape_configs.remove(id);
            self.collider_set.remove(
                collider_handle,
                &mut self.island_manager,
                &mut self.rigid_body_set,
                &mut self.soft_body_set,
                true,
            );
        }
    }

    /// Step the simulation by the fixed timestep.
    pub fn step(&mut self) {
        let physics_hooks = ();
        self.physics_pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.soft_body_set,
            &mut self.ccd_solver,
            &physics_hooks,
            &self.event_collector,
        );
    }

    /// Execute `move_and_slide` for a `CharacterBody2D`.
    pub fn move_and_slide(
        &mut self,
        character_id: &NodeId,
        desired_velocity: (f32, f32),
        dt: f32,
        up_direction: (f32, f32),
    ) -> CharacterMoveResult {
        let Some(&body_handle) = self.node_to_body.get(character_id) else {
            return CharacterMoveResult::default();
        };

        let (rb_pos, first_collider) = match self.rigid_body_set.get(body_handle) {
            Some(b) => {
                let t = b.translation();
                let c = b.colliders().first().copied();
                (t, c)
            }
            None => return CharacterMoveResult::default(),
        };

        let Some(collider_handle) = first_collider else {
            // No colliders attached: simply move without collision
            let movement = (desired_velocity.0 * dt, desired_velocity.1 * dt);
            if let Some(b) = self.rigid_body_set.get_mut(body_handle) {
                b.set_translation(
                    Vector::new(rb_pos.x + movement.0, rb_pos.y + movement.1),
                    true,
                );
            }
            return CharacterMoveResult {
                movement,
                velocity: desired_velocity,
                is_on_floor: false,
                is_on_wall: false,
                is_on_ceiling: false,
            };
        };

        let collider = match self.collider_set.get(collider_handle) {
            Some(c) => c,
            None => return CharacterMoveResult::default(),
        };

        let shape = collider.shape();
        let current_pos = collider.position();
        let desired_translation = Vector::new(desired_velocity.0 * dt, desired_velocity.1 * dt);

        let up_vec = Vector::new(up_direction.0, up_direction.1);
        let up = if up_vec.length() > 0.0001 {
            up_vec.normalize()
        } else {
            Vector::new(0.0, -1.0)
        };
        self.character_controller.up = up;

        let filter = QueryFilter::default()
            .exclude_rigid_body(body_handle)
            .exclude_sensors();

        for (handle, collider) in self.collider_set.iter() {
            let aabb = collider
                .compute_broad_phase_aabb(&self.integration_parameters, &self.rigid_body_set);
            self.broad_phase
                .set_aabb(&self.integration_parameters, handle, aabb);
        }

        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.rigid_body_set,
            &self.collider_set,
            filter,
        );

        let mut is_on_floor = false;
        let mut is_on_wall = false;
        let mut is_on_ceiling = false;

        let movement = self.character_controller.move_shape(
            dt,
            &query_pipeline,
            shape,
            current_pos,
            desired_translation,
            |collision| {
                let normal = collision.hit.normal1;
                let dot = normal.dot(up);
                if dot > 0.7 {
                    is_on_floor = true;
                } else if dot < -0.7 {
                    is_on_ceiling = true;
                } else {
                    is_on_wall = true;
                }
            },
        );

        if movement.grounded {
            is_on_floor = true;
        }

        // Apply effective translation to character rigid body
        let actual_translation = movement.translation;
        let new_pos = Vector::new(
            rb_pos.x + actual_translation.x,
            rb_pos.y + actual_translation.y,
        );
        if let Some(b) = self.rigid_body_set.get_mut(body_handle) {
            b.set_translation(new_pos, true);
        }

        let effective_vel = if dt > 0.0 {
            (actual_translation.x / dt, actual_translation.y / dt)
        } else {
            desired_velocity
        };

        CharacterMoveResult {
            movement: (actual_translation.x, actual_translation.y),
            velocity: effective_vel,
            is_on_floor,
            is_on_wall,
            is_on_ceiling,
        }
    }

    /// Apply an impulse to a RigidBody2D.
    pub fn apply_impulse(&mut self, id: &NodeId, impulse: (f32, f32)) {
        if let Some(&body_handle) = self.node_to_body.get(id) {
            if let Some(body) = self.rigid_body_set.get_mut(body_handle) {
                body.apply_impulse(Vector::new(impulse.0, impulse.1), true);
            }
        }
    }

    /// Apply a continuous force to a RigidBody2D.
    pub fn apply_force(&mut self, id: &NodeId, force: (f32, f32)) {
        if let Some(&body_handle) = self.node_to_body.get(id) {
            if let Some(body) = self.rigid_body_set.get_mut(body_handle) {
                body.add_force(Vector::new(force.0, force.1), true);
            }
        }
    }

    pub fn is_rigid_body(&self, id: &NodeId) -> bool {
        self.rigid_nodes.contains(id)
    }

    /// Retrieve the current global position of a body.
    pub fn get_body_position(&self, id: &NodeId) -> Option<(f64, f64)> {
        let handle = self.node_to_body.get(id)?;
        let body = self.rigid_body_set.get(*handle)?;
        let t = body.translation();
        Some((t.x as f64, t.y as f64))
    }

    /// Set the position of a body.
    pub fn set_body_position(&mut self, id: &NodeId, pos: (f64, f64)) {
        if let Some(&handle) = self.node_to_body.get(id) {
            if let Some(body) = self.rigid_body_set.get_mut(handle) {
                body.set_translation(Vector::new(pos.0 as f32, pos.1 as f32), true);
            }
        }
    }

    /// Retrieve the current rotation of a body.
    pub fn get_body_rotation(&self, id: &NodeId) -> Option<f64> {
        let handle = self.node_to_body.get(id)?;
        let body = self.rigid_body_set.get(*handle)?;
        Some(body.rotation().angle() as f64)
    }

    /// Perform a 2D raycast query.
    pub fn raycast(&self, from: (f32, f32), to: (f32, f32), mask: u32) -> Option<RaycastHit> {
        let ray_origin = Vector::new(from.0, from.1);
        let dir = Vector::new(to.0 - from.0, to.1 - from.1);
        let max_toi = dir.length();
        if max_toi <= 0.0001 {
            return None;
        }
        let ray_dir = dir / max_toi;
        let ray = Ray::new(ray_origin, ray_dir);

        let groups = InteractionGroups::new(
            Group::all(),
            Group::from_bits_truncate(mask),
            InteractionTestMode::And,
        );
        let filter = QueryFilter::default().groups(groups);

        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.rigid_body_set,
            &self.collider_set,
            filter,
        );

        let (handle, intersection) = query_pipeline.cast_ray_and_get_normal(&ray, max_toi, true)?;

        let collider_node = self.collider_to_node.get(&handle)?.clone();
        let body_node = self.collider_to_body.get(&handle).cloned();
        let hit_point = ray.point_at(intersection.time_of_impact);
        let normal = intersection.normal;

        Some(RaycastHit {
            point: (hit_point.x, hit_point.y),
            normal: (normal.x, normal.y),
            distance: intersection.time_of_impact,
            collider_node,
            body_node,
        })
    }

    /// Drain queued collision events (body/area enter and exit).
    pub fn drain_events(&mut self) -> Vec<PhysicsEvent> {
        let mut raw_events = Vec::new();
        if let Ok(mut lock) = self.event_collector.events.lock() {
            raw_events.append(&mut *lock);
        }

        let mut events = Vec::new();
        for ev in raw_events {
            match ev {
                CollisionEvent::Started(h1, h2, _flags) => {
                    self.resolve_collision_pair(h1, h2, true, &mut events);
                }
                CollisionEvent::Stopped(h1, h2, _flags) => {
                    self.resolve_collision_pair(h1, h2, false, &mut events);
                }
            }
        }
        events
    }

    fn resolve_collision_pair(
        &self,
        h1: ColliderHandle,
        h2: ColliderHandle,
        started: bool,
        out: &mut Vec<PhysicsEvent>,
    ) {
        let b1 = self.collider_to_body.get(&h1);
        let b2 = self.collider_to_body.get(&h2);
        let Some(node1) = b1 else { return };
        let Some(node2) = b2 else { return };

        let is_area1 = self.area_nodes.contains(node1);
        let is_area2 = self.area_nodes.contains(node2);

        if is_area1 && is_area2 {
            if started {
                out.push(PhysicsEvent::AreaEntered {
                    area_id: node1.clone(),
                    other_area_id: node2.clone(),
                });
                out.push(PhysicsEvent::AreaEntered {
                    area_id: node2.clone(),
                    other_area_id: node1.clone(),
                });
            } else {
                out.push(PhysicsEvent::AreaExited {
                    area_id: node1.clone(),
                    other_area_id: node2.clone(),
                });
                out.push(PhysicsEvent::AreaExited {
                    area_id: node2.clone(),
                    other_area_id: node1.clone(),
                });
            }
        } else if is_area1 && !is_area2 {
            if started {
                out.push(PhysicsEvent::BodyEntered {
                    area_id: node1.clone(),
                    body_id: node2.clone(),
                });
            } else {
                out.push(PhysicsEvent::BodyExited {
                    area_id: node1.clone(),
                    body_id: node2.clone(),
                });
            }
        } else if !is_area1 && is_area2 {
            if started {
                out.push(PhysicsEvent::BodyEntered {
                    area_id: node2.clone(),
                    body_id: node1.clone(),
                });
            } else {
                out.push(PhysicsEvent::BodyExited {
                    area_id: node2.clone(),
                    body_id: node1.clone(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_character_moves_and_hits_floor() {
        let mut world = PhysicsWorld::new();

        // Floor at y = 100, box 200 x 20
        let floor_id = NodeId::from("Floor");
        world.add_static_body(floor_id.clone(), (0.0, 100.0), 0.0);
        world.attach_shape(
            &floor_id,
            NodeId::from("FloorShape"),
            ShapeConfig::Box {
                width: 200.0,
                height: 20.0,
            },
            (0.0, 0.0),
            0.0,
            false,
            1,
            1,
            0.5,
            0.0,
        );

        // Character starting at y = 70 with a box 20 x 20 (bottom at y = 80)
        let player_id = NodeId::from("Player");
        world.add_character_body(player_id.clone(), (0.0, 70.0), 0.0);
        world.attach_shape(
            &player_id,
            NodeId::from("PlayerShape"),
            ShapeConfig::Box {
                width: 20.0,
                height: 20.0,
            },
            (0.0, 0.0),
            0.0,
            false,
            1,
            1,
            0.0,
            0.0,
        );

        // Move downward by 50 units (trying to reach y = 120, but floor top is at 90)
        let res = world.move_and_slide(&player_id, (0.0, 50.0), 1.0, (0.0, -1.0));
        assert!(res.is_on_floor);
        let pos = world.get_body_position(&player_id).unwrap();
        // Should have stopped around y = 80 (since player half-height = 10, floor top = 90)
        assert!((pos.1 - 80.0).abs() < 1.0, "Expected y ~ 80, got {}", pos.1);
    }

    #[test]
    fn test_rigidbody_falls_under_gravity() {
        let mut world = PhysicsWorld::new();
        let crate_id = NodeId::from("Crate");
        world.add_rigid_body(crate_id.clone(), (0.0, 0.0), 0.0, 1.0, 1.0, false);
        world.attach_shape(
            &crate_id,
            NodeId::from("CrateShape"),
            ShapeConfig::Box {
                width: 10.0,
                height: 10.0,
            },
            (0.0, 0.0),
            0.0,
            false,
            1,
            1,
            0.5,
            0.0,
        );

        // Step for 10 frames (1/60s each)
        for _ in 0..10 {
            world.step();
        }

        let pos = world.get_body_position(&crate_id).unwrap();
        assert!(pos.1 > 0.0, "RigidBody should have fallen under gravity");
    }

    #[test]
    fn test_area2d_signals_on_overlap() {
        let mut world = PhysicsWorld::new();

        let area_id = NodeId::from("Coin");
        world.add_area(area_id.clone(), (0.0, 0.0), 0.0);
        world.attach_shape(
            &area_id,
            NodeId::from("CoinShape"),
            ShapeConfig::Circle { radius: 20.0 },
            (0.0, 0.0),
            0.0,
            true,
            1,
            1,
            0.0,
            0.0,
        );

        let body_id = NodeId::from("Ball");
        world.add_rigid_body(body_id.clone(), (0.0, 0.0), 0.0, 1.0, 0.0, false);
        world.attach_shape(
            &body_id,
            NodeId::from("BallShape"),
            ShapeConfig::Circle { radius: 10.0 },
            (0.0, 0.0),
            0.0,
            false,
            1,
            1,
            0.0,
            0.0,
        );

        world.step();
        let events = world.drain_events();
        assert!(
            events.contains(&PhysicsEvent::BodyEntered {
                area_id: area_id.clone(),
                body_id: body_id.clone()
            }),
            "Expected BodyEntered event for area and body"
        );
    }

    #[test]
    fn test_character_triggers_area() {
        let mut world = PhysicsWorld::new();
        let area_id = NodeId::from("Area");
        world.add_area(area_id.clone(), (0.0, 20.0), 0.0);
        world.attach_shape(
            &area_id,
            NodeId::from("AreaShape"),
            ShapeConfig::Box {
                width: 50.0,
                height: 50.0,
            },
            (0.0, 0.0),
            0.0,
            true,
            1,
            1,
            0.0,
            0.0,
        );

        let char_id = NodeId::from("Char");
        world.add_character_body(char_id.clone(), (0.0, 0.0), 0.0);
        world.attach_shape(
            &char_id,
            NodeId::from("CharShape"),
            ShapeConfig::Box {
                width: 20.0,
                height: 20.0,
            },
            (0.0, 0.0),
            0.0,
            false,
            1,
            1,
            0.0,
            0.0,
        );

        world.move_and_slide(&char_id, (0.0, 200.0), 1.0 / 60.0, (0.0, -1.0));
        world.step();
        let events = world.drain_events();
        println!("Events: {events:?}");
        assert!(!events.is_empty());
    }

    #[test]
    fn test_raycast_detects_solid() {
        let mut world = PhysicsWorld::new();

        let wall_id = NodeId::from("Wall");
        world.add_static_body(wall_id.clone(), (100.0, 0.0), 0.0);
        world.attach_shape(
            &wall_id,
            NodeId::from("WallShape"),
            ShapeConfig::Box {
                width: 20.0,
                height: 100.0,
            },
            (0.0, 0.0),
            0.0,
            false,
            1,
            1,
            0.0,
            0.0,
        );

        world.step(); // update query pipeline

        // Cast ray from (0, 0) to (200, 0)
        let hit = world
            .raycast((0.0, 0.0), (200.0, 0.0), 1)
            .expect("ray should hit wall");
        assert_eq!(hit.collider_node, NodeId::from("WallShape"));
        assert_eq!(hit.body_node, Some(wall_id));
        // Wall left face is at x = 90
        assert!((hit.point.0 - 90.0).abs() < 1.0);
        assert_eq!(hit.normal, (-1.0, 0.0));

        // Cast ray with non-matching mask
        let miss = world.raycast((0.0, 0.0), (200.0, 0.0), 2);
        assert!(miss.is_none());
    }
}

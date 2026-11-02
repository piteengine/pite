// SPDX-License-Identifier: MIT OR Apache-2.0
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyKind {
    Static,
    Character,
    Rigid,
    Area,
}

#[derive(Debug, Clone)]
pub struct RigidBodyConfig {
    pub mass: f32,
    pub gravity_scale: f32,
    pub restitution: f32,
    pub friction: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub lock_rotation: bool,
}

impl Default for RigidBodyConfig {
    fn default() -> Self {
        Self {
            mass: 1.0,
            gravity_scale: 1.0,
            restitution: 0.0,
            friction: 0.5,
            linear_damping: 0.0,
            angular_damping: 0.0,
            lock_rotation: false,
        }
    }
}

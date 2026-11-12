// SPDX-License-Identifier: MIT OR Apache-2.0
use rapier2d::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShapeConfig {
    Box { width: f32, height: f32 },
    Circle { radius: f32 },
    Capsule { radius: f32, height: f32 },
}

impl ShapeConfig {
    pub fn to_shared_shape(&self) -> SharedShape {
        match self {
            Self::Box { width, height } => {
                let hw = (width.abs() * 0.5).max(0.001);
                let hh = (height.abs() * 0.5).max(0.001);
                SharedShape::cuboid(hw, hh)
            }
            Self::Circle { radius } => {
                let r = radius.abs().max(0.001);
                SharedShape::ball(r)
            }
            Self::Capsule { radius, height } => {
                let r = radius.abs().max(0.001);
                let hh = (height.abs() * 0.5).max(0.001);
                SharedShape::capsule_y(hh, r)
            }
        }
    }
}

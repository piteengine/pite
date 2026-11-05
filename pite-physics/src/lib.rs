// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-physics`: 2D physics simulation, kinematic character controller,
//! collision shapes, and queries powered by pure-Rust `rapier2d`.

pub mod body;
pub mod character;
pub mod query;
pub mod shape;
pub mod tilemap;
pub mod world;

pub use body::BodyKind;
pub use character::CharacterMoveResult;
pub use query::RaycastHit;
pub use shape::ShapeConfig;
pub use tilemap::{merge_tile_cells, MergedTileRect};
pub use world::{PhysicsEvent, PhysicsWorld};

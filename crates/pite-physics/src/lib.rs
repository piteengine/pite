// SPDX-License-Identifier: MIT OR Apache-2.0
//! 2D physics simulation, kinematic character controller, and collision queries for Pite.
//!
//! Powered by pure-Rust [`rapier2d`], `pite-physics` provides:
//! - [`PhysicsWorld`]: The central physics simulation container managing bodies, colliders, and collision events.
//! - Kinematic character controller with slide mechanics, slope handling, and snap-to-ground behavior.
//! - Collision shapes ([`ShapeConfig`]: rectangles, circles, capsules, convex polygons, and tile collision grids).
//! - Raycasting and spatial queries ([`RaycastHit`]).
//! - Tilemap collider merging ([`merge_tile_cells`]) to produce minimal, non-snagging collision geometry.

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

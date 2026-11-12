// SPDX-License-Identifier: MIT OR Apache-2.0
//! Kinematic character controller motion outcome and collision flags.

use serde::{Deserialize, Serialize};

/// Outcome of a kinematic character controller motion step (`move_and_slide`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CharacterMoveResult {
    /// Actual displacement vector applied during the step.
    pub movement: (f32, f32),
    /// Resulting linear velocity vector after resolving sliding collisions.
    pub velocity: (f32, f32),
    /// True if the character body was in contact with a floor surface during the step.
    pub is_on_floor: bool,
    /// True if the character body was in contact with a wall surface during the step.
    pub is_on_wall: bool,
    /// True if the character body was in contact with a ceiling surface during the step.
    pub is_on_ceiling: bool,
}

impl Default for CharacterMoveResult {
    fn default() -> Self {
        Self {
            movement: (0.0, 0.0),
            velocity: (0.0, 0.0),
            is_on_floor: false,
            is_on_wall: false,
            is_on_ceiling: false,
        }
    }
}

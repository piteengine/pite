// SPDX-License-Identifier: MIT OR Apache-2.0
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CharacterMoveResult {
    pub movement: (f32, f32),
    pub velocity: (f32, f32),
    pub is_on_floor: bool,
    pub is_on_wall: bool,
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

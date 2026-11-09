// SPDX-License-Identifier: MIT OR Apache-2.0
//! Thread-safe global physics world accessor bridging scripting and simulation.

use pite_physics::PhysicsWorld;
use std::sync::{Arc, Mutex, RwLock};

static CURRENT_PHYSICS: RwLock<Option<Arc<Mutex<PhysicsWorld>>>> = RwLock::new(None);

/// Installs the active physics world handle for the current execution context.
pub fn set_current_physics(physics: Option<Arc<Mutex<PhysicsWorld>>>) {
    *CURRENT_PHYSICS.write().unwrap() = physics;
}

/// Retrieves a clone of the active physics world handle, if installed.
pub fn current_physics() -> Option<Arc<Mutex<PhysicsWorld>>> {
    CURRENT_PHYSICS.read().unwrap().clone()
}

/// Executes a closure with exclusive access to the current physics world.
pub fn with_physics<R>(f: impl FnOnce(&mut PhysicsWorld) -> R) -> Option<R> {
    let arc = current_physics()?;
    let mut lock = arc.lock().ok()?;
    Some(f(&mut lock))
}

// SPDX-License-Identifier: MIT OR Apache-2.0
use pite_physics::PhysicsWorld;
use std::sync::{Arc, Mutex, RwLock};

static CURRENT_PHYSICS: RwLock<Option<Arc<Mutex<PhysicsWorld>>>> = RwLock::new(None);

pub fn set_current_physics(physics: Option<Arc<Mutex<PhysicsWorld>>>) {
    *CURRENT_PHYSICS.write().unwrap() = physics;
}

pub fn current_physics() -> Option<Arc<Mutex<PhysicsWorld>>> {
    CURRENT_PHYSICS.read().unwrap().clone()
}

pub fn with_physics<R>(f: impl FnOnce(&mut PhysicsWorld) -> R) -> Option<R> {
    let arc = current_physics()?;
    let mut lock = arc.lock().ok()?;
    Some(f(&mut lock))
}

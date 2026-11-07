// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};

static INPUT: LazyLock<RwLock<InputState>> = LazyLock::new(|| RwLock::new(InputState::default()));

#[derive(Debug, Default)]
pub struct InputState {
    held: HashSet<String>,
    pressed: HashSet<String>,
    released: HashSet<String>,
    mouse: (f64, f64),
}

pub fn input_begin_frame() {
    let mut state = INPUT.write().unwrap();
    state.pressed.clear();
    state.released.clear();
}

/// Drop all key state (held + edges). The editor calls this on play/stop
/// transitions so a key held across the transition never sticks.
pub fn input_clear() {
    let mut state = INPUT.write().unwrap();
    state.held.clear();
    state.pressed.clear();
    state.released.clear();
}

pub fn input_set_key(name: &str, held: bool) {
    let mut state = INPUT.write().unwrap();
    if held && state.held.insert(name.to_string()) {
        state.pressed.insert(name.to_string());
    } else if !held && state.held.remove(name) {
        state.released.insert(name.to_string());
    }
}

pub fn input_set_mouse(x: f64, y: f64) {
    INPUT.write().unwrap().mouse = (x, y);
}

pub fn input_held(key: &str) -> bool {
    INPUT.read().unwrap().held.contains(key)
}

pub fn input_pressed(key: &str) -> bool {
    INPUT.read().unwrap().pressed.contains(key)
}

pub fn input_released(key: &str) -> bool {
    INPUT.read().unwrap().released.contains(key)
}

pub fn input_mouse() -> (f64, f64) {
    INPUT.read().unwrap().mouse
}

// ---------------------------------------------------------------------------
// Camera view for screen ↔ world conversion from inside pyclasses.
//
// `PyButton::_process` hit-tests clicks against the button's *world* position,
// so it needs the current camera and viewport. The runtime calls
// `set_camera_view` once per frame (from `GameSession::update`) before the
// `_process` loop, exactly parallel to how `input_set_mouse`/`input_mouse`
// work — one shared, per-frame state, no per-node plumbing.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct CameraState {
    cam: (f64, f64),
    zoom: f64,
    viewport: (u32, u32),
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            cam: (0.0, 0.0),
            zoom: 1.0,
            viewport: (800, 600),
        }
    }
}

static CAMERA: LazyLock<RwLock<CameraState>> =
    LazyLock::new(|| RwLock::new(CameraState::default()));

/// Runtime pushes the current camera pose and viewport before each frame's
/// `_process` loop. Both `screen_to_world` and the pyclasses read it back.
pub fn set_camera_view(cam: (f64, f64), zoom: f64, viewport: (u32, u32)) {
    let mut state = CAMERA.write().unwrap();
    state.cam = cam;
    state.zoom = if zoom > 0.0 { zoom } else { 1.0 };
    state.viewport = (viewport.0.max(1), viewport.1.max(1));
}

/// Convert a screen-space point (physical pixels, matching `input_mouse()`)
/// into world coordinates using the camera pose installed by `set_camera_view`.
pub fn screen_to_world(screen: (f64, f64)) -> (f64, f64) {
    let state = CAMERA.read().unwrap();
    let vw = state.viewport.0 as f64;
    let vh = state.viewport.1 as f64;
    (
        state.cam.0 + (screen.0 - vw / 2.0) / state.zoom,
        state.cam.1 + (screen.1 - vh / 2.0) / state.zoom,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_edges_last_one_frame() {
        let _guard = crate::host::HOST_SERIAL.lock().unwrap();
        input_begin_frame();
        input_set_key("T-KeyEdgesLastOneFrame", false);
        input_begin_frame();
        assert!(!input_held("T-KeyEdgesLastOneFrame"));
        input_set_key("T-KeyEdgesLastOneFrame", true);
        assert!(input_held("T-KeyEdgesLastOneFrame"));
        assert!(input_pressed("T-KeyEdgesLastOneFrame"));
        input_begin_frame();
        assert!(input_held("T-KeyEdgesLastOneFrame"));
        assert!(!input_pressed("T-KeyEdgesLastOneFrame"));
        input_set_key("T-KeyEdgesLastOneFrame", false);
        assert!(!input_held("T-KeyEdgesLastOneFrame"));
        assert!(input_released("T-KeyEdgesLastOneFrame"));
        input_begin_frame();
        assert!(!input_released("T-KeyEdgesLastOneFrame"));
    }

    #[test]
    fn clear_drops_held_and_edges() {
        let _guard = crate::host::HOST_SERIAL.lock().unwrap();
        input_begin_frame();
        input_set_key("T-ClearDropsState", true);
        assert!(input_held("T-ClearDropsState"));
        assert!(input_pressed("T-ClearDropsState"));
        input_clear();
        assert!(!input_held("T-ClearDropsState"));
        assert!(!input_pressed("T-ClearDropsState"));
        assert!(!input_released("T-ClearDropsState"));
        input_set_key("T-ClearDropsState", false);
        assert!(!input_released("T-ClearDropsState"));
        input_begin_frame();
    }

    #[test]
    fn screen_to_world_inverts_runtime_projection() {
        set_camera_view((100.0, 200.0), 2.0, (800, 600));
        // Screen center = world camera pos.
        let (wx, wy) = screen_to_world((400.0, 300.0));
        assert!((wx - 100.0).abs() < 1e-9 && (wy - 200.0).abs() < 1e-9);
        // Screen (400 + 2*zoom, 300) = world (100 + 2, 200).
        let (wx, wy) = screen_to_world((400.0 + 2.0 * 2.0, 300.0));
        assert!((wx - 102.0).abs() < 1e-9 && (wy - 200.0).abs() < 1e-9);
        // Reset for other tests.
        set_camera_view((0.0, 0.0), 1.0, (800, 600));
    }
}

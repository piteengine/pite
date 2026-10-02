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
        // Releasing after a clear must not resurrect a release edge.
        input_set_key("T-ClearDropsState", false);
        assert!(!input_released("T-ClearDropsState"));
        input_begin_frame();
    }
}

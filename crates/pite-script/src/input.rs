// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

static INPUT: LazyLock<RwLock<InputState>> = LazyLock::new(|| RwLock::new(InputState::default()));
static ACTION_MAP: LazyLock<RwLock<ActionMapState>> =
    LazyLock::new(|| RwLock::new(ActionMapState::default()));

#[derive(Debug, Default)]
struct ActionMapState {
    actions: HashMap<String, Vec<Vec<String>>>,
    warned_unknown: HashSet<String>,
}

fn read_or_recover<'a, T>(lock: &'a RwLock<T>) -> RwLockReadGuard<'a, T> {
    lock.read().unwrap_or_else(|poisoned| {
        tracing::error!("input/camera rwlock read poisoned; recovering");
        poisoned.into_inner()
    })
}

fn write_or_recover<'a, T>(lock: &'a RwLock<T>) -> RwLockWriteGuard<'a, T> {
    lock.write().unwrap_or_else(|poisoned| {
        tracing::error!("input/camera rwlock write poisoned; recovering");
        poisoned.into_inner()
    })
}

#[derive(Debug, Default)]
pub struct InputState {
    held: HashSet<String>,
    pressed: HashSet<String>,
    released: HashSet<String>,
    mouse: (f64, f64),
}

pub fn input_begin_frame() {
    let mut state = write_or_recover(&INPUT);
    state.pressed.clear();
    state.released.clear();
}

/// Drop all key state (held + edges). The editor calls this on play/stop
/// transitions so a key held across the transition never sticks.
pub fn input_clear() {
    let mut state = write_or_recover(&INPUT);
    state.held.clear();
    state.pressed.clear();
    state.released.clear();
}

pub fn input_set_key(name: &str, held: bool) {
    let mut state = write_or_recover(&INPUT);
    if held && state.held.insert(name.to_string()) {
        state.pressed.insert(name.to_string());
    } else if !held && state.held.remove(name) {
        state.released.insert(name.to_string());
    }
}

pub fn input_set_mouse(x: f64, y: f64) {
    write_or_recover(&INPUT).mouse = (x, y);
}

pub fn input_held(key: &str) -> bool {
    read_or_recover(&INPUT).held.contains(key)
}

pub fn input_pressed(key: &str) -> bool {
    read_or_recover(&INPUT).pressed.contains(key)
}

pub fn input_released(key: &str) -> bool {
    read_or_recover(&INPUT).released.contains(key)
}

pub fn input_mouse() -> (f64, f64) {
    read_or_recover(&INPUT).mouse
}

/// Install the active input action map (action name -> list of key chords).
pub fn input_set_actions(actions: HashMap<String, Vec<Vec<String>>>) {
    let mut state = write_or_recover(&ACTION_MAP);
    state.actions = actions;
    state.warned_unknown.clear();
}

fn warn_unknown_action(name: &str) {
    let mut map = write_or_recover(&ACTION_MAP);
    if !map.actions.contains_key(name) && map.warned_unknown.insert(name.to_string()) {
        tracing::warn!("unknown input action `{name}`");
    }
}

fn chords_held(chords: &[Vec<String>], input: &InputState) -> bool {
    chords
        .iter()
        .any(|chord| !chord.is_empty() && chord.iter().all(|k| input.held.contains(k)))
}

fn resolve_chords<F: FnOnce(&[Vec<String>], &InputState) -> bool>(name: &str, eval: F) -> bool {
    {
        let map = read_or_recover(&ACTION_MAP);
        if let Some(chords) = map.actions.get(name) {
            let input = read_or_recover(&INPUT);
            return eval(chords, &input);
        }
    }
    warn_unknown_action(name);
    false
}

/// Return `true` while any binding chord for `name` is fully held.
pub fn action_held(name: &str) -> bool {
    resolve_chords(name, chords_held)
}

/// Return `true` on the frame any binding chord for `name` becomes fully held.
pub fn action_pressed(name: &str) -> bool {
    resolve_chords(name, |chords, input| {
        chords.iter().any(|chord| {
            !chord.is_empty()
                && chord.iter().all(|k| input.held.contains(k))
                && chord.iter().any(|k| input.pressed.contains(k))
        })
    })
}

/// Return `true` on the frame a held binding chord for `name` releases any of its keys.
pub fn action_released(name: &str) -> bool {
    resolve_chords(name, |chords, input| {
        chords.iter().any(|chord| {
            !chord.is_empty()
                && chord.iter().any(|k| input.released.contains(k))
                && chord
                    .iter()
                    .all(|k| input.held.contains(k) || input.released.contains(k))
        })
    })
}

fn eval_held_locked(map: &ActionMapState, input: &InputState, name: &str) -> Option<bool> {
    map.actions.get(name).map(|c| chords_held(c, input))
}

/// Compute a 1D axis in `[-1.0, 1.0]` from `negative` and `positive` actions.
pub fn action_axis(negative: &str, positive: &str) -> f64 {
    let (pos, neg) = {
        let map = read_or_recover(&ACTION_MAP);
        let input = read_or_recover(&INPUT);
        (
            eval_held_locked(&map, &input, positive),
            eval_held_locked(&map, &input, negative),
        )
    };
    if pos.is_none() {
        warn_unknown_action(positive);
    }
    if neg.is_none() {
        warn_unknown_action(negative);
    }
    let p = if pos.unwrap_or(false) { 1.0 } else { 0.0 };
    let n = if neg.unwrap_or(false) { 1.0 } else { 0.0 };
    p - n
}

/// Compute a 2D movement vector from four directional actions, normalized if length > 1.
pub fn action_vector(left: &str, right: &str, up: &str, down: &str) -> (f64, f64) {
    let (l, r, u, d) = {
        let map = read_or_recover(&ACTION_MAP);
        let input = read_or_recover(&INPUT);
        (
            eval_held_locked(&map, &input, left),
            eval_held_locked(&map, &input, right),
            eval_held_locked(&map, &input, up),
            eval_held_locked(&map, &input, down),
        )
    };
    for (name, res) in [(left, l), (right, r), (up, u), (down, d)] {
        if res.is_none() {
            warn_unknown_action(name);
        }
    }
    let x: f64 =
        (if r.unwrap_or(false) { 1.0 } else { 0.0 }) - (if l.unwrap_or(false) { 1.0 } else { 0.0 });
    let y: f64 =
        (if d.unwrap_or(false) { 1.0 } else { 0.0 }) - (if u.unwrap_or(false) { 1.0 } else { 0.0 });
    let len_sq = x * x + y * y;
    if len_sq > 1.0 {
        let len = len_sq.sqrt();
        (x / len, y / len)
    } else {
        (x, y)
    }
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
    let mut state = write_or_recover(&CAMERA);
    state.cam = cam;
    state.zoom = if zoom > 0.0 { zoom } else { 1.0 };
    state.viewport = (viewport.0.max(1), viewport.1.max(1));
}

/// Convert a screen-space point (physical pixels, matching `input_mouse()`)
/// into world coordinates using the camera pose installed by `set_camera_view`.
pub fn screen_to_world(screen: (f64, f64)) -> (f64, f64) {
    let state = read_or_recover(&CAMERA);
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

    #[test]
    fn action_chords_edges_axis_and_vector_work() {
        let _guard = crate::host::HOST_SERIAL.lock().unwrap();
        input_clear();

        let mut map = HashMap::new();
        map.insert(
            "save".to_string(),
            vec![vec!["T-Control".to_string(), "T-S".to_string()]],
        );
        map.insert("left".to_string(), vec![vec!["T-A".to_string()]]);
        map.insert("right".to_string(), vec![vec!["T-D".to_string()]]);
        map.insert("up".to_string(), vec![vec!["T-W".to_string()]]);
        map.insert("down".to_string(), vec![vec!["T-Down".to_string()]]);
        input_set_actions(map);

        // Unknown action returns false/0.0 without panicking.
        assert!(!action_held("T-MissingAction"));
        assert!(!action_pressed("T-MissingAction"));
        assert!(!action_released("T-MissingAction"));

        // Partial chord is not active.
        input_begin_frame();
        input_set_key("T-Control", true);
        assert!(!action_held("save"));
        assert!(!action_pressed("save"));

        // Completing the chord triggers pressed & held on this frame.
        input_begin_frame();
        input_set_key("T-S", true);
        assert!(action_held("save"));
        assert!(action_pressed("save"));

        // Next frame: still held, no longer pressed.
        input_begin_frame();
        assert!(action_held("save"));
        assert!(!action_pressed("save"));

        // Releasing one key of the chord triggers action_released.
        input_begin_frame();
        input_set_key("T-S", false);
        assert!(!action_held("save"));
        assert!(action_released("save"));

        // Axis & normalized diagonal vector.
        input_begin_frame();
        input_set_key("T-D", true);
        input_set_key("T-W", true);
        assert_eq!(action_axis("left", "right"), 1.0);
        let (vx, vy) = action_vector("left", "right", "up", "down");
        let expected = 1.0 / 2.0_f64.sqrt();
        assert!((vx - expected).abs() < 1e-6);
        assert!((vy + expected).abs() < 1e-6);

        input_clear();
        input_set_actions(HashMap::new());
    }
}

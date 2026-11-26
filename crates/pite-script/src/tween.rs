// SPDX-License-Identifier: MIT OR Apache-2.0
//! Runtime-managed property & callback tweens exposed as `pite.tween()`.

use std::sync::{LazyLock, Mutex};

use pyo3::prelude::*;

use crate::host::ScriptHost;

/// Easing direction applied to a [`TransType`] curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EaseType {
    In,
    Out,
    #[default]
    InOut,
    OutIn,
}

impl EaseType {
    fn parse(name: &str) -> PyResult<Self> {
        match name.to_ascii_lowercase().as_str() {
            "in" => Ok(Self::In),
            "out" => Ok(Self::Out),
            "in_out" | "inout" => Ok(Self::InOut),
            "out_in" | "outin" => Ok(Self::OutIn),
            _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unknown ease {name:?} (expected 'in', 'out', 'in_out', or 'out_in')"
            ))),
        }
    }
}

/// Interpolation transition curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransType {
    #[default]
    Linear,
    Quad,
    Cubic,
    Sine,
    Back,
    Bounce,
    Elastic,
}

impl TransType {
    fn parse(name: &str) -> PyResult<Self> {
        match name.to_ascii_lowercase().as_str() {
            "linear" => Ok(Self::Linear),
            "quad" => Ok(Self::Quad),
            "cubic" => Ok(Self::Cubic),
            "sine" => Ok(Self::Sine),
            "back" => Ok(Self::Back),
            "bounce" => Ok(Self::Bounce),
            "elastic" => Ok(Self::Elastic),
            _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unknown trans {name:?} (expected 'linear', 'quad', 'cubic', 'sine', 'back', 'bounce', or 'elastic')"
            ))),
        }
    }
}

fn eval_trans_in(trans: TransType, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match trans {
        TransType::Linear => t,
        TransType::Quad => t * t,
        TransType::Cubic => t * t * t,
        TransType::Sine => 1.0 - (t * std::f64::consts::FRAC_PI_2).cos(),
        TransType::Back => {
            const C1: f64 = 1.70158;
            const C3: f64 = C1 + 1.0;
            C3 * t * t * t - C1 * t * t
        }
        TransType::Bounce => 1.0 - eval_bounce_out(1.0 - t),
        TransType::Elastic => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                const C4: f64 = (2.0 * std::f64::consts::PI) / 3.0;
                -(2.0_f64.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * C4).sin()
            }
        }
    }
}

fn eval_bounce_out(mut t: f64) -> f64 {
    const N1: f64 = 7.5625;
    const D1: f64 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        t -= 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        t -= 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        t -= 2.625 / D1;
        N1 * t * t + 0.984375
    }
}

/// Evaluate `trans` + `ease` at normalized progress `t` in `[0.0, 1.0]`.
pub fn eval_curve(trans: TransType, ease: EaseType, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if matches!(trans, TransType::Linear) {
        return t;
    }
    match ease {
        EaseType::In => eval_trans_in(trans, t),
        EaseType::Out => 1.0 - eval_trans_in(trans, 1.0 - t),
        EaseType::InOut => {
            if t < 0.5 {
                0.5 * eval_trans_in(trans, t * 2.0)
            } else {
                1.0 - 0.5 * eval_trans_in(trans, (1.0 - t) * 2.0)
            }
        }
        EaseType::OutIn => {
            if t < 0.5 {
                0.5 * (1.0 - eval_trans_in(trans, 1.0 - t * 2.0))
            } else {
                0.5 + 0.5 * eval_trans_in(trans, (t - 0.5) * 2.0)
            }
        }
    }
}

/// Interpolatable property value for a [`PropertyTweener`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TweenValue {
    Num(f64),
    Vec2(f64, f64),
    Color(u8, u8, u8, u8),
}

impl TweenValue {
    fn from_prop(v: &pite_core::PropValue) -> Option<Self> {
        match v {
            pite_core::PropValue::Num(n) => Some(Self::Num(*n)),
            pite_core::PropValue::Int(i) => Some(Self::Num(*i as f64)),
            pite_core::PropValue::Vec2(x, y) => Some(Self::Vec2(*x, *y)),
            pite_core::PropValue::Color(r, g, b, a) => Some(Self::Color(*r, *g, *b, *a)),
            _ => None,
        }
    }

    fn from_py(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(v) = obj.extract::<crate::vec2::Vec2>() {
            return Ok(Self::Vec2(v.x(), v.y()));
        }
        if let Ok((x, y)) = obj.extract::<(f64, f64)>() {
            return Ok(Self::Vec2(x, y));
        }
        if let Ok((r, g, b, a)) = obj.extract::<(u8, u8, u8, u8)>() {
            return Ok(Self::Color(r, g, b, a));
        }
        if let Ok(s) = obj.extract::<String>() {
            if let Some(c) = parse_hex_color(&s) {
                return Ok(Self::Color(c.0, c.1, c.2, c.3));
            }
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "invalid color string {s:?} for tween"
            )));
        }
        if let Ok(n) = obj.extract::<f64>() {
            return Ok(Self::Num(n));
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "tween value must be a number, Vec2/(x, y), or Color/(r, g, b, a)/#hex",
        ))
    }

    fn lerp(self, to: Self, t: f64) -> Self {
        match (self, to) {
            (Self::Num(a), Self::Num(b)) => Self::Num(a + (b - a) * t),
            (Self::Vec2(ax, ay), Self::Vec2(bx, by)) => {
                Self::Vec2(ax + (bx - ax) * t, ay + (by - ay) * t)
            }
            (Self::Color(ar, ag, ab, aa), Self::Color(br, bg, bb, ba)) => {
                let ch = |a: u8, b: u8| -> u8 {
                    (a as f64 + (b as f64 - a as f64) * t)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                Self::Color(ch(ar, br), ch(ag, bg), ch(ab, bb), ch(aa, ba))
            }
            (_, other) => other,
        }
    }

    fn into_prop(self) -> pite_core::PropValue {
        match self {
            Self::Num(n) => pite_core::PropValue::Num(n),
            Self::Vec2(x, y) => pite_core::PropValue::Vec2(x, y),
            Self::Color(r, g, b, a) => pite_core::PropValue::Color(r, g, b, a),
        }
    }
}

fn parse_hex_color(s: &str) -> Option<(u8, u8, u8, u8)> {
    let hex = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        6 => Some((byte(0)?, byte(2)?, byte(4)?, 255)),
        8 => Some((byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

enum TweenStep {
    Property {
        node_id: String,
        prop: String,
        from: Option<TweenValue>,
        resolved_from: Option<TweenValue>,
        to: TweenValue,
        duration: f64,
        ease: EaseType,
        trans: TransType,
        delay: f64,
        elapsed: f64,
        done: bool,
    },
    Callback {
        func: Py<PyAny>,
        delay: f64,
        elapsed: f64,
        done: bool,
    },
    Interval {
        duration: f64,
        elapsed: f64,
        done: bool,
    },
}

impl TweenStep {
    fn reset(&mut self) {
        match self {
            Self::Property {
                resolved_from,
                elapsed,
                done,
                ..
            } => {
                *resolved_from = None;
                *elapsed = 0.0;
                *done = false;
            }
            Self::Callback { elapsed, done, .. } | Self::Interval { elapsed, done, .. } => {
                *elapsed = 0.0;
                *done = false;
            }
        }
    }

    fn is_done(&self) -> bool {
        match self {
            Self::Property { done, .. }
            | Self::Callback { done, .. }
            | Self::Interval { done, .. } => *done,
        }
    }
}

struct TweenEntry {
    id: u64,
    groups: Vec<Vec<TweenStep>>,
    current_group: usize,
    next_parallel: bool,
    /// `None` = infinite loops; `Some(n)` = remaining runs including the current one.
    loops_remaining: Option<u32>,
    paused: bool,
    killed: bool,
    on_finished: Option<Py<PyAny>>,
}

impl TweenEntry {
    fn new(id: u64) -> Self {
        Self {
            id,
            groups: Vec::new(),
            current_group: 0,
            next_parallel: false,
            loops_remaining: Some(1),
            paused: false,
            killed: false,
            on_finished: None,
        }
    }

    fn push_step(&mut self, step: TweenStep) -> (usize, usize) {
        if self.next_parallel && !self.groups.is_empty() {
            self.next_parallel = false;
            let g_idx = self.groups.len() - 1;
            let s_idx = self.groups[g_idx].len();
            self.groups[g_idx].push(step);
            (g_idx, s_idx)
        } else {
            self.next_parallel = false;
            let g_idx = self.groups.len();
            self.groups.push(vec![step]);
            (g_idx, 0)
        }
    }
}

#[derive(Default)]
struct TweenRegistry {
    next_id: u64,
    tweens: Vec<TweenEntry>,
}

static TWEENS: LazyLock<Mutex<TweenRegistry>> =
    LazyLock::new(|| Mutex::new(TweenRegistry::default()));

fn lock_tweens() -> std::sync::MutexGuard<'static, TweenRegistry> {
    TWEENS.lock().unwrap_or_else(|p| p.into_inner())
}

/// Remove all active tweens (called on scene reload / stop).
pub fn tween_clear() {
    let mut reg = lock_tweens();
    reg.tweens.clear();
}

/// Advance all active tweens by `delta` seconds, updating `host`'s `NodeTree` in one write lock
/// and firing any completed callbacks under a single GIL acquisition. Returns error messages.
pub fn tween_tick(host: &ScriptHost, delta: f64) -> Vec<String> {
    let mut callbacks_to_run: Vec<Py<PyAny>> = Vec::new();
    let mut reg = lock_tweens();

    if reg.tweens.is_empty() {
        return Vec::new();
    }

    host.with_tree_mut(|tree| {
        for tween in &mut reg.tweens {
            if tween.paused || tween.killed {
                continue;
            }
            if tween.groups.is_empty() {
                tween.killed = true;
                continue;
            }
            while tween.current_group < tween.groups.len() {
                let group = &mut tween.groups[tween.current_group];
                for step in group.iter_mut() {
                    if step.is_done() {
                        continue;
                    }
                    match step {
                        TweenStep::Property {
                            node_id,
                            prop,
                            from,
                            resolved_from,
                            to,
                            duration,
                            ease,
                            trans,
                            delay,
                            elapsed,
                            done,
                        } => {
                            *elapsed += delta;
                            if *elapsed < *delay {
                                continue;
                            }
                            let active_t = *elapsed - *delay;
                            let nid = pite_core::NodeId::from(node_id.clone());
                            let Some(node) = tree.get_mut(&nid) else {
                                *done = true;
                                continue;
                            };
                            if resolved_from.is_none() {
                                let start = from.or_else(|| {
                                    node.props.get(prop).and_then(TweenValue::from_prop)
                                });
                                *resolved_from = Some(start.unwrap_or(*to));
                            }
                            let start = resolved_from.unwrap_or(*to);
                            let progress = if *duration <= 0.0 {
                                1.0
                            } else {
                                (active_t / *duration).clamp(0.0, 1.0)
                            };
                            let curved = eval_curve(*trans, *ease, progress);
                            let val = start.lerp(*to, curved);
                            node.props.insert(prop.clone(), val.into_prop());
                            if progress >= 1.0 {
                                *done = true;
                            }
                        }
                        TweenStep::Callback {
                            func,
                            delay,
                            elapsed,
                            done,
                        } => {
                            *elapsed += delta;
                            if *elapsed >= *delay {
                                *done = true;
                                Python::attach(|py| {
                                    callbacks_to_run.push(func.clone_ref(py));
                                });
                            }
                        }
                        TweenStep::Interval {
                            duration,
                            elapsed,
                            done,
                        } => {
                            *elapsed += delta;
                            if *elapsed >= *duration {
                                *done = true;
                            }
                        }
                    }
                }
                if group.iter().all(TweenStep::is_done) {
                    tween.current_group += 1;
                } else {
                    break;
                }
            }

            if tween.current_group >= tween.groups.len() {
                let repeat = match tween.loops_remaining {
                    None => true,
                    Some(n) if n > 1 => {
                        tween.loops_remaining = Some(n - 1);
                        true
                    }
                    Some(_) => false,
                };
                if repeat {
                    tween.current_group = 0;
                    for g in &mut tween.groups {
                        for s in g {
                            s.reset();
                        }
                    }
                } else {
                    tween.killed = true;
                    if let Some(cb) = tween.on_finished.take() {
                        callbacks_to_run.push(cb);
                    }
                }
            }
        }
    });

    reg.tweens.retain(|t| !t.killed);
    drop(reg);

    let mut errors = Vec::new();
    if !callbacks_to_run.is_empty() {
        Python::attach(|py| {
            for cb in callbacks_to_run {
                if let Err(err) = cb.bind(py).call0() {
                    errors.push(err.to_string());
                }
            }
        });
    }
    errors
}

fn extract_node_id(target: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(s) = target.extract::<String>() {
        return Ok(s);
    }
    if let Ok(path) = target
        .getattr("node_path")
        .and_then(|v| v.extract::<String>())
    {
        return Ok(path);
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "tween target must be a Node, NodeProxy, or node id string",
    ))
}

/// Fluent runtime tween handle returned by `pite.tween()`.
#[pyclass(name = "Tween")]
#[derive(Clone)]
pub struct PyTween {
    id: u64,
}

pub(crate) fn create_tween() -> PyTween {
    let mut reg = lock_tweens();
    reg.next_id += 1;
    let id = reg.next_id;
    reg.tweens.push(TweenEntry::new(id));
    PyTween { id }
}

#[pymethods]
impl PyTween {
    /// Animate `prop` on `target` to `final_val` over `duration` seconds.
    pub fn property(
        &self,
        target: &Bound<'_, PyAny>,
        prop: &str,
        final_val: &Bound<'_, PyAny>,
        duration: f64,
    ) -> PyResult<PyPropertyTweener> {
        let node_id = extract_node_id(target)?;
        let to = TweenValue::from_py(final_val)?;
        let mut reg = lock_tweens();
        let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) else {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "tween is no longer active",
            ));
        };
        let (group_idx, step_idx) = entry.push_step(TweenStep::Property {
            node_id,
            prop: prop.to_string(),
            from: None,
            resolved_from: None,
            to,
            duration: duration.max(0.0),
            ease: EaseType::default(),
            trans: TransType::default(),
            delay: 0.0,
            elapsed: 0.0,
            done: false,
        });
        Ok(PyPropertyTweener {
            tween_id: self.id,
            group_idx,
            step_idx,
        })
    }

    /// Invoke `func()` when this step is reached (after optional `delay` seconds).
    #[pyo3(signature = (func, delay = 0.0))]
    pub fn callback(&self, func: Py<PyAny>, delay: f64) -> PyResult<Self> {
        let mut reg = lock_tweens();
        let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) else {
            return Ok(self.clone());
        };
        entry.push_step(TweenStep::Callback {
            func,
            delay: delay.max(0.0),
            elapsed: 0.0,
            done: false,
        });
        Ok(self.clone())
    }

    /// Wait `duration` seconds before advancing to the next step.
    pub fn interval(&self, duration: f64) -> Self {
        let mut reg = lock_tweens();
        if let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) {
            entry.push_step(TweenStep::Interval {
                duration: duration.max(0.0),
                elapsed: 0.0,
                done: false,
            });
        }
        self.clone()
    }

    /// Run the next appended step in parallel with the previous step.
    pub fn parallel(&self) -> Self {
        let mut reg = lock_tweens();
        if let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) {
            entry.next_parallel = true;
        }
        self.clone()
    }

    /// Set total loop count (`0` means infinite).
    pub fn loops(&self, count: u32) -> Self {
        let mut reg = lock_tweens();
        if let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) {
            entry.loops_remaining = if count == 0 { None } else { Some(count) };
        }
        self.clone()
    }

    /// Register a callback invoked when all loops finish.
    pub fn on_finished(&self, func: Py<PyAny>) -> Self {
        let mut reg = lock_tweens();
        if let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) {
            entry.on_finished = Some(func);
        }
        self.clone()
    }

    pub fn pause(&self) -> Self {
        let mut reg = lock_tweens();
        if let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) {
            entry.paused = true;
        }
        self.clone()
    }

    pub fn play(&self) -> Self {
        let mut reg = lock_tweens();
        if let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) {
            entry.paused = false;
        }
        self.clone()
    }

    pub fn stop(&self) -> Self {
        let mut reg = lock_tweens();
        if let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) {
            entry.paused = true;
            entry.current_group = 0;
            for g in &mut entry.groups {
                for s in g {
                    s.reset();
                }
            }
        }
        self.clone()
    }

    pub fn kill(&self) {
        let mut reg = lock_tweens();
        if let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.id) {
            entry.killed = true;
        }
    }

    pub fn is_running(&self) -> bool {
        let reg = lock_tweens();
        reg.tweens
            .iter()
            .any(|t| t.id == self.id && !t.paused && !t.killed)
    }
}

/// Configuration handle for a single property tween step.
#[pyclass(name = "PropertyTweener")]
#[derive(Clone)]
pub struct PyPropertyTweener {
    tween_id: u64,
    group_idx: usize,
    step_idx: usize,
}

impl PyPropertyTweener {
    fn with_step(&self, f: impl FnOnce(&mut TweenStep) -> PyResult<()>) -> PyResult<Self> {
        let mut reg = lock_tweens();
        if let Some(entry) = reg.tweens.iter_mut().find(|t| t.id == self.tween_id) {
            if let Some(step) = entry
                .groups
                .get_mut(self.group_idx)
                .and_then(|g| g.get_mut(self.step_idx))
            {
                f(step)?;
            }
        }
        Ok(self.clone())
    }
}

#[pymethods]
impl PyPropertyTweener {
    pub fn ease(&self, name: &str) -> PyResult<Self> {
        let parsed = EaseType::parse(name)?;
        self.with_step(|s| {
            if let TweenStep::Property { ease, .. } = s {
                *ease = parsed;
            }
            Ok(())
        })
    }

    pub fn trans(&self, name: &str) -> PyResult<Self> {
        let parsed = TransType::parse(name)?;
        self.with_step(|s| {
            if let TweenStep::Property { trans, .. } = s {
                *trans = parsed;
            }
            Ok(())
        })
    }

    pub fn from_(&self, val: &Bound<'_, PyAny>) -> PyResult<Self> {
        let parsed = TweenValue::from_py(val)?;
        self.with_step(|s| {
            if let TweenStep::Property { from, .. } = s {
                *from = Some(parsed);
            }
            Ok(())
        })
    }

    pub fn delay(&self, seconds: f64) -> PyResult<Self> {
        self.with_step(|s| {
            if let TweenStep::Property { delay, .. } = s {
                *delay = seconds.max(0.0);
            }
            Ok(())
        })
    }

    /// Chain another property tween sequentially (or in parallel after `.parallel()`).
    pub fn property(
        &self,
        target: &Bound<'_, PyAny>,
        prop: &str,
        final_val: &Bound<'_, PyAny>,
        duration: f64,
    ) -> PyResult<Self> {
        PyTween { id: self.tween_id }.property(target, prop, final_val, duration)
    }

    #[pyo3(signature = (func, delay = 0.0))]
    pub fn callback(&self, func: Py<PyAny>, delay: f64) -> PyResult<PyTween> {
        PyTween { id: self.tween_id }.callback(func, delay)
    }

    pub fn interval(&self, duration: f64) -> PyTween {
        PyTween { id: self.tween_id }.interval(duration)
    }

    pub fn parallel(&self) -> PyTween {
        PyTween { id: self.tween_id }.parallel()
    }

    pub fn loops(&self, count: u32) -> PyTween {
        PyTween { id: self.tween_id }.loops(count)
    }

    pub fn on_finished(&self, func: Py<PyAny>) -> PyTween {
        PyTween { id: self.tween_id }.on_finished(func)
    }

    pub fn kill(&self) {
        PyTween { id: self.tween_id }.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_endpoints_and_easings_are_bounded() {
        for trans in [
            TransType::Linear,
            TransType::Quad,
            TransType::Cubic,
            TransType::Sine,
            TransType::Back,
            TransType::Bounce,
            TransType::Elastic,
        ] {
            for ease in [
                EaseType::In,
                EaseType::Out,
                EaseType::InOut,
                EaseType::OutIn,
            ] {
                assert!((eval_curve(trans, ease, 0.0) - 0.0).abs() < 1e-6);
                assert!((eval_curve(trans, ease, 1.0) - 1.0).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn tween_interpolates_vec2_color_and_cleans_up() {
        let _guard = crate::host::HOST_SERIAL.lock().unwrap();
        crate::ensure_python_init();
        tween_clear();

        let mut tree = pite_core::NodeTree::new();
        let mut props = pite_core::Props::default();
        props.insert("position", pite_core::PropValue::Vec2(0.0, 0.0));
        props.insert("modulate", pite_core::PropValue::Color(0, 0, 0, 255));
        tree.insert(pite_core::NodeDesc {
            id: pite_core::NodeId::from("box"),
            name: "box".into(),
            type_name: "Sprite2D".into(),
            parent: None,
            props,
            script: None,
        })
        .unwrap();
        let host = std::sync::Arc::new(ScriptHost::new(tree));

        let tw = create_tween();
        {
            let mut reg = lock_tweens();
            let entry = reg.tweens.iter_mut().find(|t| t.id == tw.id).unwrap();
            entry.push_step(TweenStep::Property {
                node_id: "box".into(),
                prop: "position".into(),
                from: None,
                resolved_from: None,
                to: TweenValue::Vec2(100.0, 200.0),
                duration: 1.0,
                ease: EaseType::In,
                trans: TransType::Linear,
                delay: 0.0,
                elapsed: 0.0,
                done: false,
            });
            entry.next_parallel = true;
            entry.push_step(TweenStep::Property {
                node_id: "box".into(),
                prop: "modulate".into(),
                from: None,
                resolved_from: None,
                to: TweenValue::Color(200, 100, 50, 255),
                duration: 1.0,
                ease: EaseType::In,
                trans: TransType::Linear,
                delay: 0.0,
                elapsed: 0.0,
                done: false,
            });
        }

        assert!(tw.is_running());
        assert!(tween_tick(&host, 0.5).is_empty());
        assert_eq!(host.get_vec2("box", "position").x, 50.0);
        assert_eq!(host.get_vec2("box", "position").y, 100.0);

        assert!(tween_tick(&host, 0.5).is_empty());
        assert_eq!(host.get_vec2("box", "position").x, 100.0);
        assert!(!tw.is_running());
        tween_clear();
    }
}

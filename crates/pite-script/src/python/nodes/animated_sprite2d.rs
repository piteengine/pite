// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "AnimatedSprite2D")]
pub(crate) struct PyAnimatedSprite2D;

#[pymethods]
impl PyAnimatedSprite2D {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyAnimatedSprite2D))
    }

    /// Play an animation. If `animation` is specified and non-empty, sets the animation
    /// and resets the frame index to 0. If `loop_override` is specified, overrides the
    /// default looping behavior for this animation.
    #[pyo3(signature = (animation=None, loop_override=None))]
    fn play(
        slf: &Bound<'_, Self>,
        animation: Option<&str>,
        loop_override: Option<bool>,
    ) -> PyResult<()> {
        let (host, node) = crate::python::host_and_path(slf.as_any())?;
        if let Some(anim) = animation {
            if !anim.is_empty() {
                host.set_prop(
                    &node,
                    "animation",
                    pite_core::PropValue::Str(anim.to_string()),
                );
                host.set_prop(&node, "frame", pite_core::PropValue::Int(0));
            }
        }
        if let Some(l) = loop_override {
            host.set_prop(&node, "_loop_override", pite_core::PropValue::Bool(l));
        }
        host.set_prop(&node, "playing", pite_core::PropValue::Bool(true));
        Ok(())
    }

    /// Pause the currently playing animation without resetting the frame index.
    fn pause(slf: &Bound<'_, Self>) -> PyResult<()> {
        let (host, node) = crate::python::host_and_path(slf.as_any())?;
        host.set_prop(&node, "playing", pite_core::PropValue::Bool(false));
        Ok(())
    }

    /// Stop playback and reset the frame index to 0.
    fn stop(slf: &Bound<'_, Self>) -> PyResult<()> {
        let (host, node) = crate::python::host_and_path(slf.as_any())?;
        host.set_prop(&node, "playing", pite_core::PropValue::Bool(false));
        host.set_prop(&node, "frame", pite_core::PropValue::Int(0));
        Ok(())
    }

    /// Return true if the animation is currently playing.
    fn is_playing(slf: &Bound<'_, Self>) -> PyResult<bool> {
        let (host, node) = crate::python::host_and_path(slf.as_any())?;
        Ok(host.get_bool(&node, "playing"))
    }

    #[getter]
    fn animation_finished(slf: &Bound<'_, Self>) -> PyResult<crate::host::BoundSignal> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let host = crate::host::current_host().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("signal access has no script host")
        })?;
        Ok(crate::host::BoundSignal::with_host(
            host,
            node_path,
            "animation_finished".to_string(),
        ))
    }

    #[getter]
    fn animation_looped(slf: &Bound<'_, Self>) -> PyResult<crate::host::BoundSignal> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let host = crate::host::current_host().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("signal access has no script host")
        })?;
        Ok(crate::host::BoundSignal::with_host(
            host,
            node_path,
            "animation_looped".to_string(),
        ))
    }
}

crate::pite_props! {
    PyAnimatedSprite2D {
        atlas: Str,
        animation: Str,
        frame: Int,
        speed_scale: Num,
        playing: Bool,
        autoplay: Str,
    }
}

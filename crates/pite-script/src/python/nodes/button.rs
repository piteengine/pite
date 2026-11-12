// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "Button")]
pub(crate) struct PyButton {
    pub(crate) size: Py<crate::vec2::Vec2>,
    /// Press-through state: true between a press inside the button and the
    /// matching release. Reset by reload (new `PyButton` instance), which is
    /// what we want — a click that started before a reload must not fire.
    armed: bool,
}

#[pymethods]
impl PyButton {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        let size = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyButton { size, armed: false }))
    }

    /// Per-frame click polling. Press-inside + release-inside fires
    /// `pressed` once; a press-then-release-outside cancels. Inherited by
    /// `MyButton(pite.Button)` — a subclass that overrides `_process` must
    /// call `super()._process(delta)` or clicks stop working (documented
    /// footgun, CHANGELOG entry).
    fn _process(slf: &Bound<'_, Self>, _delta: f64) -> PyResult<()> {
        let py = slf.py();
        let (host, node) = crate::python::host_and_path(slf.as_any())?;

        // Size + world-space center for hit-testing.
        let (w, h) = {
            let v = host.get_vec2(&node, "size");
            (v.x, v.y)
        };
        let (px, pyw) = host.with_tree(|tree| {
            let id = pite_core::NodeId::from(node.clone());
            tree.get(&id)
                .map(|n| pite_core::global_position(tree, &n.id))
                .unwrap_or((0.0, 0.0))
        });

        // Mouse is in screen pixels; the button is in world space. Convert
        // through the camera pose the runtime installed this frame.
        let (mx, my) = crate::input_mouse();
        let (wx, wy) = crate::screen_to_world((mx, my));
        let inside = (wx - px).abs() <= w / 2.0 && (wy - pyw).abs() <= h / 2.0;

        let mut armed = slf.borrow().armed;
        if crate::input_pressed("MouseLeft") && inside {
            armed = true;
        }
        if crate::input_released("MouseLeft") {
            if armed && inside {
                let empty = pyo3::types::PyTuple::empty(py);
                let _ = host.emit(py, &node, "pressed", &empty);
            }
            armed = false;
        }
        slf.borrow_mut().armed = armed;
        Ok(())
    }

    #[getter]
    fn pressed(slf: &Bound<'_, Self>) -> PyResult<crate::host::BoundSignal> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let host = crate::host::current_host().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("signal access has no script host")
        })?;
        Ok(crate::host::BoundSignal::with_host(
            host,
            node_path,
            "pressed".to_string(),
        ))
    }
}

crate::pite_props! {
    PyButton {
        text: Str,
        font_size: Num,
        color: Color,
        size: Vec2,
        disabled: Bool,
    }
}

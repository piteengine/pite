// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "RigidBody2D")]
pub(crate) struct PyRigidBody2D {
    #[pyo3(get, set)]
    mass: f64,
    #[pyo3(get, set)]
    gravity_scale: f64,
    #[pyo3(get, set)]
    restitution: f64,
    #[pyo3(get, set)]
    friction: f64,
    #[pyo3(get, set)]
    lock_rotation: bool,
    #[pyo3(get, set)]
    collision_layer: u32,
    #[pyo3(get, set)]
    collision_mask: u32,
}

#[pymethods]
impl PyRigidBody2D {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyRigidBody2D {
                mass: 1.0,
                gravity_scale: 1.0,
                restitution: 0.0,
                friction: 0.5,
                lock_rotation: false,
                collision_layer: 1,
                collision_mask: 1,
            }))
    }

    fn apply_impulse(slf: &Bound<'_, Self>, impulse: (f64, f64)) -> PyResult<()> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let node_id = pite_core::NodeId::from(node_path);
        crate::with_physics(|w| w.apply_impulse(&node_id, (impulse.0 as f32, impulse.1 as f32)));
        Ok(())
    }

    fn apply_force(slf: &Bound<'_, Self>, force: (f64, f64)) -> PyResult<()> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let node_id = pite_core::NodeId::from(node_path);
        crate::with_physics(|w| w.apply_force(&node_id, (force.0 as f32, force.1 as f32)));
        Ok(())
    }
}

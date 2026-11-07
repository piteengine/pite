// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "RigidBody2D")]
pub(crate) struct PyRigidBody2D;

#[pymethods]
impl PyRigidBody2D {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyRigidBody2D))
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

crate::pite_props! {
    PyRigidBody2D {
        mass: Num,
        gravity_scale: Num,
        restitution: Num,
        friction: Num,
        lock_rotation: Bool,
        collision_layer: Int,
        collision_mask: Int,
    }
}

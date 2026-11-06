// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "CharacterBody2D")]
pub(crate) struct PyCharacterBody2D {
    velocity: Py<crate::vec2::Vec2>,
    up_direction: Py<crate::vec2::Vec2>,
    #[pyo3(get, set)]
    collision_layer: u32,
    #[pyo3(get, set)]
    collision_mask: u32,
    #[pyo3(get, set)]
    _is_on_floor: bool,
    #[pyo3(get, set)]
    _is_on_wall: bool,
    #[pyo3(get, set)]
    _is_on_ceiling: bool,
}

#[pymethods]
impl PyCharacterBody2D {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        let vel = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        let up = Py::new(py, crate::vec2::Vec2::new(0.0, -1.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyCharacterBody2D {
                velocity: vel,
                up_direction: up,
                collision_layer: 1,
                collision_mask: 1,
                _is_on_floor: false,
                _is_on_wall: false,
                _is_on_ceiling: false,
            }))
    }

    #[getter]
    fn velocity(&self, py: Python<'_>) -> Py<crate::vec2::Vec2> {
        self.velocity.clone_ref(py)
    }

    #[setter]
    fn set_velocity(&self, py: Python<'_>, val: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut b = self.velocity.borrow_mut(py);
        if let Ok(v) = val.extract::<crate::vec2::Vec2>() {
            b.set_x(v.x());
            b.set_y(v.y());
        } else if let Ok((vx, vy)) = val.extract::<(f64, f64)>() {
            b.set_x(vx);
            b.set_y(vy);
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "velocity must be Vec2 or (vx, vy) tuple",
            ));
        }
        Ok(())
    }

    #[getter]
    fn up_direction(&self, py: Python<'_>) -> Py<crate::vec2::Vec2> {
        self.up_direction.clone_ref(py)
    }

    #[setter]
    fn set_up_direction(&self, py: Python<'_>, val: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut b = self.up_direction.borrow_mut(py);
        if let Ok(v) = val.extract::<crate::vec2::Vec2>() {
            b.set_x(v.x());
            b.set_y(v.y());
        } else if let Ok((ux, uy)) = val.extract::<(f64, f64)>() {
            b.set_x(ux);
            b.set_y(uy);
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "up_direction must be Vec2 or (ux, uy) tuple",
            ));
        }
        Ok(())
    }

    fn is_on_floor(&self) -> bool {
        self._is_on_floor
    }

    fn is_on_wall(&self) -> bool {
        self._is_on_wall
    }

    fn is_on_ceiling(&self) -> bool {
        self._is_on_ceiling
    }

    fn move_and_slide(slf: &Bound<'_, Self>) -> PyResult<bool> {
        let py = slf.py();
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let node_id = pite_core::NodeId::from(node_path);
        let (vel, up) = {
            let b = slf.borrow();
            let v = b.velocity.borrow(py);
            let u = b.up_direction.borrow(py);
            ((v.x(), v.y()), (u.x(), u.y()))
        };

        let dt = 1.0 / 60.0;
        let res = crate::with_physics(|world| {
            world.move_and_slide(
                &node_id,
                (vel.0 as f32, vel.1 as f32),
                dt as f32,
                (up.0 as f32, up.1 as f32),
            )
        });

        if let Some(res) = res {
            {
                let b = slf.borrow();
                let mut v = b.velocity.borrow_mut(py);
                v.set_x(res.velocity.0 as f64);
                v.set_y(res.velocity.1 as f64);
            }
            {
                let mut b = slf.borrow_mut();
                b._is_on_floor = res.is_on_floor;
                b._is_on_wall = res.is_on_wall;
                b._is_on_ceiling = res.is_on_ceiling;
            }
            if let Some(Some(new_pos)) = crate::with_physics(|w| w.get_body_position(&node_id)) {
                slf.setattr("position", new_pos)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

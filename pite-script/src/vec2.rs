// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use std::sync::Arc;

#[pyclass(name = "Vec2", subclass)]
#[derive(Clone)]
pub struct Vec2 {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) target: Option<(Arc<crate::ScriptHost>, String, String)>,
}

impl std::fmt::Debug for Vec2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vec2")
            .field("x", &self.x())
            .field("y", &self.y())
            .finish()
    }
}

impl Vec2 {
    pub fn x(&self) -> f64 {
        if let Some((host, node, key)) = &self.target {
            host.get_vec2(node, key).x
        } else {
            self.x
        }
    }

    pub fn y(&self) -> f64 {
        if let Some((host, node, key)) = &self.target {
            host.get_vec2(node, key).y
        } else {
            self.y
        }
    }

    pub fn set_target(&mut self, host: Arc<crate::ScriptHost>, node: String, key: String) {
        let vec = host.get_vec2(&node, &key);
        self.x = vec.x;
        self.y = vec.y;
        self.target = Some((host, node, key));
    }
}

impl<'py> pyo3::FromPyObject<'_, 'py> for Vec2 {
    type Error = PyErr;
    fn extract(ob: pyo3::Borrowed<'_, 'py, pyo3::PyAny>) -> Result<Self, Self::Error> {
        let b: pyo3::PyRef<'py, Vec2> = ob.extract()?;
        Ok(b.clone())
    }
}

#[pymethods]
impl Vec2 {
    #[new]
    #[pyo3(signature = (x = 0.0, y = 0.0))]
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y, target: None }
    }

    #[getter]
    pub fn get_x(&self) -> f64 {
        self.x()
    }

    #[setter]
    pub fn set_x(&mut self, val: f64) {
        self.x = val;
        if let Some((host, node, key)) = &self.target {
            let cur_y = self.y();
            self.y = cur_y;
            host.set_prop(node, key, pite_core::PropValue::Vec2(val, cur_y));
        }
    }

    #[getter]
    pub fn get_y(&self) -> f64 {
        self.y()
    }

    #[setter]
    pub fn set_y(&mut self, val: f64) {
        self.y = val;
        if let Some((host, node, key)) = &self.target {
            let cur_x = self.x();
            self.x = cur_x;
            host.set_prop(node, key, pite_core::PropValue::Vec2(cur_x, val));
        }
    }

    pub fn set_xy(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
        if let Some((host, node, key)) = &self.target {
            host.set_prop(node, key, pite_core::PropValue::Vec2(x, y));
        }
    }

    fn __repr__(&self) -> String {
        format!("Vec2({}, {})", self.x(), self.y())
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        let (ox, oy) = if let Ok(other_vec) = other.extract::<Vec2>() {
            (other_vec.x(), other_vec.y())
        } else if let Ok((x, y)) = other.extract::<(f64, f64)>() {
            (x, y)
        } else {
            return false;
        };
        (self.x() - ox).abs() < 1e-6 && (self.y() - oy).abs() < 1e-6
    }

    fn __add__(&self, other: &Bound<'_, PyAny>) -> PyResult<Vec2> {
        let (ox, oy) = if let Ok(other_vec) = other.extract::<Vec2>() {
            (other_vec.x(), other_vec.y())
        } else if let Ok(t) = other.extract::<(f64, f64)>() {
            t
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "unsupported operand type for +: 'Vec2' and non-vector",
            ));
        };
        Ok(Vec2 {
            x: self.x() + ox,
            y: self.y() + oy,
            target: None,
        })
    }

    fn __sub__(&self, other: &Bound<'_, PyAny>) -> PyResult<Vec2> {
        let (ox, oy) = if let Ok(other_vec) = other.extract::<Vec2>() {
            (other_vec.x(), other_vec.y())
        } else if let Ok(t) = other.extract::<(f64, f64)>() {
            t
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "unsupported operand type for -: 'Vec2' and non-vector",
            ));
        };
        Ok(Vec2 {
            x: self.x() - ox,
            y: self.y() - oy,
            target: None,
        })
    }

    fn __mul__(&self, scalar: f64) -> Vec2 {
        Vec2 {
            x: self.x() * scalar,
            y: self.y() * scalar,
            target: None,
        }
    }

    fn __rmul__(&self, scalar: f64) -> Vec2 {
        Vec2 {
            x: self.x() * scalar,
            y: self.y() * scalar,
            target: None,
        }
    }

    fn __truediv__(&self, scalar: f64) -> PyResult<Vec2> {
        if scalar == 0.0 {
            return Err(pyo3::exceptions::PyZeroDivisionError::new_err(
                "division by zero",
            ));
        }
        Ok(Vec2 {
            x: self.x() / scalar,
            y: self.y() / scalar,
            target: None,
        })
    }

    fn __iadd__(&mut self, other: &Bound<'_, PyAny>) -> PyResult<()> {
        let (ox, oy) = if let Ok(other_vec) = other.extract::<Vec2>() {
            (other_vec.x(), other_vec.y())
        } else if let Ok(t) = other.extract::<(f64, f64)>() {
            t
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "unsupported operand type for +=: 'Vec2' and non-vector",
            ));
        };
        let cur_x = self.x();
        let cur_y = self.y();
        self.set_x(cur_x + ox);
        self.set_y(cur_y + oy);
        Ok(())
    }

    fn __isub__(&mut self, other: &Bound<'_, PyAny>) -> PyResult<()> {
        let (ox, oy) = if let Ok(other_vec) = other.extract::<Vec2>() {
            (other_vec.x(), other_vec.y())
        } else if let Ok(t) = other.extract::<(f64, f64)>() {
            t
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "unsupported operand type for -=: 'Vec2' and non-vector",
            ));
        };
        let cur_x = self.x();
        let cur_y = self.y();
        self.set_x(cur_x - ox);
        self.set_y(cur_y - oy);
        Ok(())
    }

    fn __imul__(&mut self, scalar: f64) {
        let cur_x = self.x();
        let cur_y = self.y();
        self.set_x(cur_x * scalar);
        self.set_y(cur_y * scalar);
    }

    fn __itruediv__(&mut self, scalar: f64) -> PyResult<()> {
        if scalar == 0.0 {
            return Err(pyo3::exceptions::PyZeroDivisionError::new_err(
                "division by zero",
            ));
        }
        let cur_x = self.x();
        let cur_y = self.y();
        self.set_x(cur_x / scalar);
        self.set_y(cur_y / scalar);
        Ok(())
    }

    fn __len__(&self) -> usize {
        2
    }

    fn __getitem__(&self, idx: isize) -> PyResult<f64> {
        match idx {
            0 | -2 => Ok(self.x()),
            1 | -1 => Ok(self.y()),
            _ => Err(pyo3::exceptions::PyIndexError::new_err(
                "Vec2 index out of range",
            )),
        }
    }

    fn __setitem__(&mut self, idx: isize, val: f64) -> PyResult<()> {
        match idx {
            0 | -2 => {
                self.set_x(val);
                Ok(())
            }
            1 | -1 => {
                self.set_y(val);
                Ok(())
            }
            _ => Err(pyo3::exceptions::PyIndexError::new_err(
                "Vec2 index out of range",
            )),
        }
    }

    fn __iter__(slf: Bound<'_, Self>) -> PyResult<Bound<'_, pyo3::types::PyIterator>> {
        let x = slf.borrow().x();
        let y = slf.borrow().y();
        let tuple = pyo3::types::PyTuple::new(slf.py(), [x, y])?;
        tuple.try_iter()
    }

    fn length(&self) -> f64 {
        let x = self.x();
        let y = self.y();
        (x * x + y * y).sqrt()
    }

    fn normalized(&self) -> Vec2 {
        let len = self.length();
        if len > 1e-6 {
            let x = self.x();
            let y = self.y();
            Vec2 {
                x: x / len,
                y: y / len,
                target: None,
            }
        } else {
            Vec2 {
                x: 0.0,
                y: 0.0,
                target: None,
            }
        }
    }

    fn to_tuple(&self) -> (f64, f64) {
        (self.x(), self.y())
    }
}

impl From<(f64, f64)> for Vec2 {
    fn from(t: (f64, f64)) -> Self {
        Self {
            x: t.0,
            y: t.1,
            target: None,
        }
    }
}

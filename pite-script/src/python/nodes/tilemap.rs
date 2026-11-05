// SPDX-License-Identifier: MIT OR Apache-2.0
use pyo3::prelude::*;
use pyo3::PyClassInitializer;

use super::node::PyNode;
use super::node2d::PyNode2D;

#[pyclass(extends = PyNode2D, subclass, name = "TileMap2D")]
pub(crate) struct PyTileMap2D;

#[pymethods]
impl PyTileMap2D {
    #[new]
    pub(crate) fn new(py: Python<'_>) -> PyResult<PyClassInitializer<Self>> {
        let pos = Py::new(py, crate::vec2::Vec2::new(0.0, 0.0))?;
        Ok(PyClassInitializer::from(PyNode::new())
            .add_subclass(PyNode2D { position: pos })
            .add_subclass(PyTileMap2D))
    }

    #[pyo3(signature = (col, row, layer = 0))]
    fn get_cell(
        slf: &Bound<'_, Self>,
        col: i32,
        row: i32,
        layer: usize,
    ) -> PyResult<Option<String>> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        if let Some(host) = crate::host::current_host() {
            Ok(host.get_tilemap_cell(&node_path, layer, col, row))
        } else {
            Ok(None)
        }
    }

    #[pyo3(signature = (col, row, frame, layer = 0))]
    fn set_cell(
        slf: &Bound<'_, Self>,
        col: i32,
        row: i32,
        frame: &str,
        layer: usize,
    ) -> PyResult<()> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        if let Some(host) = crate::host::current_host() {
            host.set_tilemap_cell(&node_path, layer, col, row, frame);
        }
        Ok(())
    }

    #[pyo3(signature = (col, row, layer = 0))]
    fn erase_cell(slf: &Bound<'_, Self>, col: i32, row: i32, layer: usize) -> PyResult<bool> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        if let Some(host) = crate::host::current_host() {
            Ok(host.erase_tilemap_cell(&node_path, layer, col, row))
        } else {
            Ok(false)
        }
    }

    #[pyo3(signature = (layer = None))]
    fn clear(slf: &Bound<'_, Self>, layer: Option<usize>) -> PyResult<()> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        if let Some(host) = crate::host::current_host() {
            host.clear_tilemap(&node_path, layer);
        }
        Ok(())
    }

    #[pyo3(signature = (layer = 0))]
    fn get_used_cells(slf: &Bound<'_, Self>, layer: usize) -> PyResult<Vec<(i32, i32)>> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        if let Some(host) = crate::host::current_host() {
            Ok(host.get_tilemap_used_cells(&node_path, layer))
        } else {
            Ok(Vec::new())
        }
    }

    #[getter]
    fn tile_size(slf: &Bound<'_, Self>) -> PyResult<(u32, u32)> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        if let Some(host) = crate::host::current_host() {
            let (w, h) = host.get_tilemap_tile_size(&node_path);
            Ok((w as u32, h as u32))
        } else {
            Ok((16, 16))
        }
    }

    #[getter]
    fn layers(slf: &Bound<'_, Self>) -> PyResult<Vec<String>> {
        let node_path: String = slf.getattr("node_path")?.extract()?;
        if let Some(host) = crate::host::current_host() {
            Ok(host.get_tilemap_layers(&node_path))
        } else {
            Ok(Vec::new())
        }
    }

    #[pyo3(signature = (*args))]
    fn world_to_map(
        slf: &Bound<'_, Self>,
        args: &Bound<'_, pyo3::types::PyTuple>,
    ) -> PyResult<(i32, i32)> {
        let (wx, wy) = if args.len() == 2 {
            let x: f64 = args.get_item(0)?.extract()?;
            let y: f64 = args.get_item(1)?.extract()?;
            (x, y)
        } else if args.len() == 1 {
            let item = args.get_item(0)?;
            if let Ok(v) = item.extract::<crate::vec2::Vec2>() {
                (v.x(), v.y())
            } else if let Ok(t) = item.extract::<(f64, f64)>() {
                t
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "pos must be Vec2 or (x, y) tuple",
                ));
            }
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "world_to_map expects 1 or 2 arguments (pos: Vec2 or x, y)",
            ));
        };
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let origin = if let Some(host) = crate::host::current_host() {
            host.get_node_position(&node_path).unwrap_or((0.0, 0.0))
        } else {
            let pos_attr = slf.getattr("position")?;
            let v = pos_attr.extract::<crate::vec2::Vec2>()?;
            (v.x(), v.y())
        };
        let tile_size = if let Some(host) = crate::host::current_host() {
            host.get_tilemap_tile_size(&node_path)
        } else {
            (16.0, 16.0)
        };
        Ok(pite_scene::world_to_map((wx, wy), origin, tile_size))
    }

    fn map_to_world(slf: &Bound<'_, Self>, col: i32, row: i32) -> PyResult<Py<crate::vec2::Vec2>> {
        let py = slf.py();
        let node_path: String = slf.getattr("node_path")?.extract()?;
        let origin = if let Some(host) = crate::host::current_host() {
            host.get_node_position(&node_path).unwrap_or((0.0, 0.0))
        } else {
            let pos_attr = slf.getattr("position")?;
            let v = pos_attr.extract::<crate::vec2::Vec2>()?;
            (v.x(), v.y())
        };
        let tile_size = if let Some(host) = crate::host::current_host() {
            host.get_tilemap_tile_size(&node_path)
        } else {
            (16.0, 16.0)
        };
        let (wx, wy) = pite_scene::map_to_world((col, row), origin, tile_size);
        Py::new(py, crate::vec2::Vec2::new(wx, wy))
    }
}

crate::pite_props! {
    PyTileMap2D {
        tilemap: Str,
    }
}

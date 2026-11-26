// SPDX-License-Identifier: MIT OR Apache-2.0
//! Scoped 2D immediate-mode drawing canvas passed to per-node `_draw(self, canvas)` hooks.

use pyo3::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

/// A single 2D draw primitive emitted by a Python `_draw(self, canvas)` method.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect {
        pos: (f64, f64),
        size: (f64, f64),
        color: [u8; 4],
        z_index: i32,
    },
    Line {
        from: (f64, f64),
        to: (f64, f64),
        color: [u8; 4],
        thickness: f64,
        z_index: i32,
    },
    Text {
        text: String,
        pos: (f64, f64),
        size: f32,
        color: [u8; 4],
        z_index: i32,
    },
    Sprite {
        texture: String,
        pos: (f64, f64),
        rotation: f64,
        z_index: i32,
    },
}

/// Scoped 2D drawing surface passed to `Node._draw(self, canvas)` during frame rendering.
#[pyclass(name = "Canvas2D")]
pub struct PyCanvas2D {
    origin: (f64, f64),
    default_z: i32,
    active: Arc<AtomicBool>,
    commands: Arc<Mutex<Vec<DrawCommand>>>,
}

impl PyCanvas2D {
    /// Creates a new active `Canvas2D` whose coordinates are offset by `origin`.
    pub fn new(
        origin: (f64, f64),
        default_z: i32,
    ) -> (Self, Arc<AtomicBool>, Arc<Mutex<Vec<DrawCommand>>>) {
        let active = Arc::new(AtomicBool::new(true));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let canvas = Self {
            origin,
            default_z,
            active: Arc::clone(&active),
            commands: Arc::clone(&commands),
        };
        (canvas, active, commands)
    }

    fn ensure_active(&self) -> PyResult<()> {
        if !self.active.load(Ordering::Relaxed) {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Canvas2D is only valid during _draw(self, canvas)",
            ));
        }
        Ok(())
    }

    fn push(&self, cmd: DrawCommand) -> PyResult<()> {
        self.ensure_active()?;
        let mut guard = self.commands.lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("Canvas2D command buffer lock poisoned")
        })?;
        guard.push(cmd);
        Ok(())
    }
}

fn extract_vec2(val: &Bound<'_, PyAny>) -> PyResult<(f64, f64)> {
    if let Ok(v) = val.extract::<crate::vec2::Vec2>() {
        Ok((v.x(), v.y()))
    } else if let Ok(t) = val.extract::<(f64, f64)>() {
        Ok(t)
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "expected Vec2 or (x, y)",
        ))
    }
}

fn extract_color(val: Option<&Bound<'_, PyAny>>) -> PyResult<[u8; 4]> {
    let Some(val) = val else {
        return Ok([255, 255, 255, 255]);
    };
    if let Ok(s) = val.extract::<String>() {
        let c = pite_scene::parse_hex_color(&s).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid hex color: {s}"))
        })?;
        Ok([c.r, c.g, c.b, c.a])
    } else if let Ok((r, g, b, a)) = val.extract::<(u8, u8, u8, u8)>() {
        Ok([r, g, b, a])
    } else if let Ok((r, g, b)) = val.extract::<(u8, u8, u8)>() {
        Ok([r, g, b, 255])
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "color must be hex string, (r, g, b), or (r, g, b, a)",
        ))
    }
}

#[pymethods]
impl PyCanvas2D {
    #[pyo3(signature = (pos, size, color = None, z_index = None))]
    fn draw_rect(
        &self,
        pos: &Bound<'_, PyAny>,
        size: &Bound<'_, PyAny>,
        color: Option<&Bound<'_, PyAny>>,
        z_index: Option<i32>,
    ) -> PyResult<()> {
        let p = extract_vec2(pos)?;
        let s = extract_vec2(size)?;
        let c = extract_color(color)?;
        self.push(DrawCommand::Rect {
            pos: (self.origin.0 + p.0, self.origin.1 + p.1),
            size: s,
            color: c,
            z_index: z_index.unwrap_or(self.default_z),
        })
    }

    #[pyo3(signature = (start, end, color = None, width = 1.0, z_index = None))]
    fn draw_line(
        &self,
        start: &Bound<'_, PyAny>,
        end: &Bound<'_, PyAny>,
        color: Option<&Bound<'_, PyAny>>,
        width: f64,
        z_index: Option<i32>,
    ) -> PyResult<()> {
        let a = extract_vec2(start)?;
        let b = extract_vec2(end)?;
        let c = extract_color(color)?;
        self.push(DrawCommand::Line {
            from: (self.origin.0 + a.0, self.origin.1 + a.1),
            to: (self.origin.0 + b.0, self.origin.1 + b.1),
            color: c,
            thickness: width,
            z_index: z_index.unwrap_or(self.default_z),
        })
    }

    #[pyo3(signature = (text, pos, size = 16.0, color = None, z_index = None))]
    fn draw_text(
        &self,
        text: String,
        pos: &Bound<'_, PyAny>,
        size: f32,
        color: Option<&Bound<'_, PyAny>>,
        z_index: Option<i32>,
    ) -> PyResult<()> {
        let p = extract_vec2(pos)?;
        let c = extract_color(color)?;
        self.push(DrawCommand::Text {
            text,
            pos: (self.origin.0 + p.0, self.origin.1 + p.1),
            size,
            color: c,
            z_index: z_index.unwrap_or(self.default_z),
        })
    }

    #[pyo3(signature = (texture, pos, rotation = 0.0, z_index = None))]
    fn draw_sprite(
        &self,
        texture: String,
        pos: &Bound<'_, PyAny>,
        rotation: f64,
        z_index: Option<i32>,
    ) -> PyResult<()> {
        let p = extract_vec2(pos)?;
        self.push(DrawCommand::Sprite {
            texture,
            pos: (self.origin.0 + p.0, self.origin.1 + p.1),
            rotation,
            z_index: z_index.unwrap_or(self.default_z),
        })
    }
}

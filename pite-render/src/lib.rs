//! `pite-render`: `Renderer2D` trait. wgpu is the only impl (M1);
//! the viewport talks to the trait so batching/3D later doesn't touch
//! tree or editor code.

use anyhow::Result;

pub trait Renderer2D {
    fn begin_frame(&mut self) -> Result<()>;
    fn draw_sprite(&mut self, texture: &str, x: f64, y: f64) -> Result<()>;
    fn end_frame(&mut self) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct NoopRenderer;

impl Renderer2D for NoopRenderer {
    fn begin_frame(&mut self) -> Result<()> {
        Ok(())
    }

    fn draw_sprite(&mut self, _texture: &str, _x: f64, _y: f64) -> Result<()> {
        Ok(())
    }

    fn end_frame(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct WgpuRenderer {
    inner: NoopRenderer,
}

impl WgpuRenderer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: NoopRenderer,
        })
    }
}

impl Default for WgpuRenderer {
    fn default() -> Self {
        Self {
            inner: NoopRenderer,
        }
    }
}

impl Renderer2D for WgpuRenderer {
    fn begin_frame(&mut self) -> Result<()> {
        self.inner.begin_frame()
    }

    fn draw_sprite(&mut self, texture: &str, x: f64, y: f64) -> Result<()> {
        self.inner.draw_sprite(texture, x, y)
    }

    fn end_frame(&mut self) -> Result<()> {
        self.inner.end_frame()
    }
}

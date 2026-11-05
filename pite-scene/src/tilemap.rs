// SPDX-License-Identifier: MIT OR Apache-2.0
//! `tilemap`: TileMap document schema, TOML serialization, coordinate math,
//! and layer management.
//!
//! Stores sparse tile layers referencing atlas frames. TOML is canonical
//! and human-readable, consistent with `.ptscn`.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_one() -> u32 {
    1
}

/// One tile cell in a sparse layer: `[col, row, "frame_name"]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileCell(pub i32, pub i32, pub String);

impl TileCell {
    pub fn new(col: i32, row: i32, frame: impl Into<String>) -> Self {
        Self(col, row, frame.into())
    }

    pub fn col(&self) -> i32 {
        self.0
    }

    pub fn row(&self) -> i32 {
        self.1
    }

    pub fn frame(&self) -> &str {
        &self.2
    }
}

/// One layer in a tilemap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileMapLayer {
    pub name: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub collision_enabled: bool,
    #[serde(default = "default_one")]
    pub collision_layer: u32,
    #[serde(default = "default_one")]
    pub collision_mask: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solid_frames: Option<Vec<String>>,
    #[serde(default)]
    pub tiles: Vec<TileCell>,
}

impl TileMapLayer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            visible: true,
            collision_enabled: false,
            collision_layer: 1,
            collision_mask: 1,
            solid_frames: None,
            tiles: Vec::new(),
        }
    }
}

/// Canonical `.ptmap` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileMapDoc {
    pub atlas: String,
    pub tile_size: [u32; 2],
    #[serde(default, rename = "layer")]
    pub layers: Vec<TileMapLayer>,
}

impl TileMapDoc {
    pub fn new(atlas: impl Into<String>, tile_w: u32, tile_h: u32) -> Self {
        Self {
            atlas: atlas.into(),
            tile_size: [tile_w, tile_h],
            layers: Vec::new(),
        }
    }

    pub fn layer(&self, idx: usize) -> Option<&TileMapLayer> {
        self.layers.get(idx)
    }

    pub fn layer_mut(&mut self, idx: usize) -> Option<&mut TileMapLayer> {
        self.layers.get_mut(idx)
    }

    pub fn layer_names(&self) -> Vec<&str> {
        self.layers.iter().map(|l| l.name.as_str()).collect()
    }

    pub fn get_cell(&self, layer_idx: usize, col: i32, row: i32) -> Option<&str> {
        let layer = self.layers.get(layer_idx)?;
        layer
            .tiles
            .iter()
            .find(|t| t.0 == col && t.1 == row)
            .map(|t| t.2.as_str())
    }

    pub fn set_cell(&mut self, layer_idx: usize, col: i32, row: i32, frame: impl Into<String>) {
        if let Some(layer) = self.layers.get_mut(layer_idx) {
            let frame = frame.into();
            if let Some(cell) = layer.tiles.iter_mut().find(|t| t.0 == col && t.1 == row) {
                cell.2 = frame;
            } else {
                layer.tiles.push(TileCell(col, row, frame));
            }
        }
    }

    pub fn erase_cell(&mut self, layer_idx: usize, col: i32, row: i32) -> bool {
        if let Some(layer) = self.layers.get_mut(layer_idx) {
            if let Some(pos) = layer.tiles.iter().position(|t| t.0 == col && t.1 == row) {
                layer.tiles.remove(pos);
                return true;
            }
        }
        false
    }

    pub fn clear_layer(&mut self, layer_idx: usize) {
        if let Some(layer) = self.layers.get_mut(layer_idx) {
            layer.tiles.clear();
        }
    }

    pub fn used_cells(&self, layer_idx: usize) -> Vec<(i32, i32)> {
        let Some(layer) = self.layers.get(layer_idx) else {
            return Vec::new();
        };
        layer.tiles.iter().map(|t| (t.0, t.1)).collect()
    }

    /// Solid cells for a layer taking `collision_enabled` and `solid_frames` into account.
    pub fn solid_cells(&self, layer_idx: usize) -> Vec<(i32, i32)> {
        let Some(layer) = self.layers.get(layer_idx) else {
            return Vec::new();
        };
        if !layer.collision_enabled {
            return Vec::new();
        }
        match &layer.solid_frames {
            Some(solids) => {
                let solid_set: HashSet<&str> = solids.iter().map(String::as_str).collect();
                layer
                    .tiles
                    .iter()
                    .filter(|t| solid_set.contains(t.2.as_str()))
                    .map(|t| (t.0, t.1))
                    .collect()
            }
            None => layer.tiles.iter().map(|t| (t.0, t.1)).collect(),
        }
    }
}

/// Convert world position to tilemap grid coordinate `(col, row)`.
pub fn world_to_map(world: (f64, f64), origin: (f64, f64), tile_size: (f64, f64)) -> (i32, i32) {
    let w = if tile_size.0 <= 0.0 {
        16.0
    } else {
        tile_size.0
    };
    let h = if tile_size.1 <= 0.0 {
        16.0
    } else {
        tile_size.1
    };
    let col = ((world.0 - origin.0) / w).floor() as i32;
    let row = ((world.1 - origin.1) / h).floor() as i32;
    (col, row)
}

/// Convert grid coordinate `(col, row)` to world center position `(x, y)`.
pub fn map_to_world(cell: (i32, i32), origin: (f64, f64), tile_size: (f64, f64)) -> (f64, f64) {
    let w = if tile_size.0 <= 0.0 {
        16.0
    } else {
        tile_size.0
    };
    let h = if tile_size.1 <= 0.0 {
        16.0
    } else {
        tile_size.1
    };
    (
        origin.0 + (cell.0 as f64 + 0.5) * w,
        origin.1 + (cell.1 as f64 + 0.5) * h,
    )
}

pub fn parse_tilemap_str(text: &str) -> Result<TileMapDoc> {
    toml::from_str(text).context("invalid tilemap TOML")
}

pub fn load_tilemap(path: &Path) -> Result<TileMapDoc> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read tilemap {}", path.display()))?;
    parse_tilemap_str(&text).with_context(|| format!("cannot parse tilemap {}", path.display()))
}

pub fn save_tilemap(path: &Path, doc: &TileMapDoc) -> Result<()> {
    let text = toml::to_string(doc).context("cannot serialize tilemap")?;
    std::fs::write(path, text).with_context(|| format!("cannot write tilemap {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_tilemap_toml() {
        let toml_src = r#"atlas = "res://assets/tileset.atlas.json"
tile_size = [16, 16]

[[layer]]
name = "ground"
visible = true
collision_enabled = false
collision_layer = 1
collision_mask = 1
tiles = [
  [0, 0, "grass"],
  [1, 0, "grass"],
  [2, 0, "path"],
]

[[layer]]
name = "walls"
visible = true
collision_enabled = true
collision_layer = 2
collision_mask = 1
solid_frames = ["wall_brick"]
tiles = [
  [0, 1, "wall_brick"],
  [1, 1, "wall_window"],
]
"#;
        let doc = parse_tilemap_str(toml_src).unwrap();
        assert_eq!(doc.atlas, "res://assets/tileset.atlas.json");
        assert_eq!(doc.tile_size, [16, 16]);
        assert_eq!(doc.layers.len(), 2);
        assert_eq!(doc.get_cell(0, 0, 0), Some("grass"));
        assert_eq!(doc.get_cell(0, 2, 0), Some("path"));
        assert_eq!(doc.get_cell(0, 3, 0), None);

        // Check collision query: only wall_brick is solid on layer 1
        let solids = doc.solid_cells(1);
        assert_eq!(solids, vec![(0, 1)]);

        // Reserialize
        let serialized = toml::to_string(&doc).unwrap();
        let doc2 = parse_tilemap_str(&serialized).unwrap();
        assert_eq!(doc, doc2);
    }

    #[test]
    fn coordinate_conversion() {
        let origin = (100.0, 200.0);
        let size = (16.0, 16.0);

        let cell = (2, 3);
        let world = map_to_world(cell, origin, size);
        assert_eq!(world, (100.0 + 2.5 * 16.0, 200.0 + 3.5 * 16.0));

        let back_cell = world_to_map(world, origin, size);
        assert_eq!(back_cell, cell);

        // Negative coordinates
        let neg_cell = (-3, -5);
        let neg_world = map_to_world(neg_cell, origin, size);
        let back_neg = world_to_map(neg_world, origin, size);
        assert_eq!(back_neg, neg_cell);
    }

    #[test]
    fn mutations() {
        let mut doc = TileMapDoc::new("res://tiles.atlas.json", 16, 16);
        doc.layers.push(TileMapLayer::new("main"));
        assert_eq!(doc.get_cell(0, 0, 0), None);

        doc.set_cell(0, 0, 0, "stone");
        assert_eq!(doc.get_cell(0, 0, 0), Some("stone"));

        // Overwrite
        doc.set_cell(0, 0, 0, "wood");
        assert_eq!(doc.get_cell(0, 0, 0), Some("wood"));

        // Erase
        assert!(doc.erase_cell(0, 0, 0));
        assert_eq!(doc.get_cell(0, 0, 0), None);
        assert!(!doc.erase_cell(0, 0, 0));
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Spatial chunk baking and caching for TileMap2D nodes.

use std::collections::BTreeMap;

use pite_scene::TileMapDoc;

/// Chunk dimension along both axes (16x16 tiles per chunk).
pub const CHUNK_SIZE: i32 = 16;

#[derive(Clone, Debug, PartialEq)]
pub struct BakedTile {
    pub col: i32,
    pub row: i32,
    pub local_x: f64,
    pub local_y: f64,
    pub frame: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakedChunk {
    pub tiles: Vec<BakedTile>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BakedLayer {
    pub name: String,
    pub visible: bool,
    /// (chunk_col, chunk_row) -> BakedChunk
    pub chunks: BTreeMap<(i32, i32), BakedChunk>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BakedTileMap {
    pub atlas_path: String,
    pub sheet_str: String,
    pub tile_size: [u32; 2],
    pub layers: Vec<BakedLayer>,
    pub version: u64,
}

impl BakedTileMap {
    pub fn bake(doc: &TileMapDoc, sheet_str: String, version: u64) -> Self {
        let tile_w = doc.tile_size[0] as f64;
        let tile_h = doc.tile_size[1] as f64;
        let mut layers = Vec::with_capacity(doc.layers.len());
        for layer in &doc.layers {
            let mut chunks: BTreeMap<(i32, i32), BakedChunk> = BTreeMap::new();
            for tile in &layer.tiles {
                let chunk_col = tile.0.div_euclid(CHUNK_SIZE);
                let chunk_row = tile.1.div_euclid(CHUNK_SIZE);
                let local_x = (tile.0 as f64 + 0.5) * tile_w;
                let local_y = (tile.1 as f64 + 0.5) * tile_h;
                let chunk = chunks.entry((chunk_col, chunk_row)).or_default();
                chunk.tiles.push(BakedTile {
                    col: tile.0,
                    row: tile.1,
                    local_x,
                    local_y,
                    frame: tile.2.clone(),
                });
            }
            layers.push(BakedLayer {
                name: layer.name.clone(),
                visible: layer.visible,
                chunks,
            });
        }
        Self {
            atlas_path: doc.atlas.clone(),
            sheet_str,
            tile_size: doc.tile_size,
            layers,
            version,
        }
    }

    /// Re-bakes a single chunk coordinate `(chunk_col, chunk_row)` in `layer_idx`.
    pub fn rebake_chunk(
        &mut self,
        doc: &TileMapDoc,
        layer_idx: usize,
        chunk_col: i32,
        chunk_row: i32,
    ) {
        if layer_idx >= self.layers.len() {
            *self = Self::bake(doc, self.sheet_str.clone(), self.version);
            return;
        }
        let layer = &mut self.layers[layer_idx];
        let Some(doc_layer) = doc.layers.get(layer_idx) else {
            return;
        };
        layer.visible = doc_layer.visible;
        let min_col = chunk_col * CHUNK_SIZE;
        let max_col = min_col + CHUNK_SIZE;
        let min_row = chunk_row * CHUNK_SIZE;
        let max_row = min_row + CHUNK_SIZE;

        let tile_w = doc.tile_size[0] as f64;
        let tile_h = doc.tile_size[1] as f64;
        let mut new_tiles = Vec::new();
        for tile in &doc_layer.tiles {
            if tile.0 >= min_col && tile.0 < max_col && tile.1 >= min_row && tile.1 < max_row {
                let local_x = (tile.0 as f64 + 0.5) * tile_w;
                let local_y = (tile.1 as f64 + 0.5) * tile_h;
                new_tiles.push(BakedTile {
                    col: tile.0,
                    row: tile.1,
                    local_x,
                    local_y,
                    frame: tile.2.clone(),
                });
            }
        }
        if new_tiles.is_empty() {
            layer.chunks.remove(&(chunk_col, chunk_row));
        } else {
            layer
                .chunks
                .insert((chunk_col, chunk_row), BakedChunk { tiles: new_tiles });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pite_scene::TileCell;

    #[test]
    fn chunk_baking_partitions_tiles_into_16x16_grids() {
        let mut doc = TileMapDoc::new("res://assets/sheet.atlas.json", 16, 16);
        let mut layer = pite_scene::TileMapLayer::new("Main");
        // Chunk (0, 0): (0, 0) and (15, 15)
        layer.tiles.push(TileCell(0, 0, "grass".to_string()));
        layer.tiles.push(TileCell(15, 15, "grass".to_string()));
        // Chunk (1, 0): (16, 0)
        layer.tiles.push(TileCell(16, 0, "dirt".to_string()));
        // Chunk (-1, -1): (-1, -1)
        layer.tiles.push(TileCell(-1, -1, "stone".to_string()));
        doc.layers.push(layer);

        let baked = BakedTileMap::bake(&doc, "sheet.png".to_string(), 1);
        assert_eq!(baked.layers.len(), 1);
        let layer = &baked.layers[0];
        assert_eq!(layer.chunks.len(), 3);
        let chunk_0_0 = layer.chunks.get(&(0, 0)).unwrap();
        assert_eq!(chunk_0_0.tiles.len(), 2);
        assert_eq!(chunk_0_0.tiles[0].local_x, 8.0);
        assert_eq!(chunk_0_0.tiles[0].local_y, 8.0);
        assert_eq!(layer.chunks.get(&(1, 0)).unwrap().tiles.len(), 1);
        assert_eq!(layer.chunks.get(&(-1, -1)).unwrap().tiles.len(), 1);
    }

    #[test]
    fn rebake_chunk_updates_only_target_chunk() {
        let mut doc = TileMapDoc::new("res://assets/sheet.atlas.json", 16, 16);
        let mut layer = pite_scene::TileMapLayer::new("Main");
        layer.tiles.push(TileCell(0, 0, "grass".to_string()));
        layer.tiles.push(TileCell(20, 20, "stone".to_string()));
        doc.layers.push(layer);

        let mut baked = BakedTileMap::bake(&doc, "sheet.png".to_string(), 1);
        assert_eq!(baked.layers[0].chunks.get(&(0, 0)).unwrap().tiles.len(), 1);

        // Edit cell in chunk (0, 0)
        doc.set_cell(0, 0, 1, "water");
        baked.rebake_chunk(&doc, 0, 0, 0);

        let chunk_0_0 = baked.layers[0].chunks.get(&(0, 0)).unwrap();
        assert_eq!(chunk_0_0.tiles.len(), 2);
        // Chunk (1, 1) remains unaffected
        let chunk_1_1 = baked.layers[0].chunks.get(&(1, 1)).unwrap();
        assert_eq!(chunk_1_1.tiles.len(), 1);
    }
}

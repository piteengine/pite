// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tilemap collision meshing: greedy 2D rectangle merger for Rapier2D static colliders.
//!
//! Coalesces adjacent solid grid cells into maximum rectangular bounding boxes,
//! eliminating internal edge catching for character controllers (`move_and_slide()`)
//! and reducing collider counts by an order of magnitude.

use std::collections::HashSet;

/// A merged 2D rectangle of cells: `(col, row, width, height)` in cell units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergedTileRect {
    pub col: i32,
    pub row: i32,
    pub width: u32,
    pub height: u32,
}

/// Greedily merges contiguous 2D grid cells into a minimal set of non-overlapping
/// rectangles, preventing internal collision edge catching in Rapier2D.
pub fn merge_tile_cells(cells: &[(i32, i32)]) -> Vec<MergedTileRect> {
    if cells.is_empty() {
        return Vec::new();
    }

    let mut unvisited: HashSet<(i32, i32)> = cells.iter().copied().collect();
    let mut sorted_cells: Vec<(i32, i32)> = cells.to_vec();
    // Sort primarily by row (Y), secondarily by col (X)
    sorted_cells.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    sorted_cells.dedup();

    let mut rects = Vec::new();

    for &(x, y) in &sorted_cells {
        if !unvisited.contains(&(x, y)) {
            continue;
        }

        // 1. Expand width to the right as far as possible
        let mut w = 1;
        while unvisited.contains(&(x + w, y)) {
            w += 1;
        }

        // 2. Expand height downwards as far as possible
        let mut h = 1;
        'outer: loop {
            let next_y = y + h;
            for dx in 0..w {
                if !unvisited.contains(&(x + dx, next_y)) {
                    break 'outer;
                }
            }
            h += 1;
        }

        // 3. Mark all cells in this rectangle as visited
        for dy in 0..h {
            for dx in 0..w {
                unvisited.remove(&(x + dx, y + dy));
            }
        }

        rects.push(MergedTileRect {
            col: x,
            row: y,
            width: w as u32,
            height: h as u32,
        });
    }

    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_and_single() {
        assert_eq!(merge_tile_cells(&[]), vec![]);

        let single = merge_tile_cells(&[(5, 10)]);
        assert_eq!(
            single,
            vec![MergedTileRect {
                col: 5,
                row: 10,
                width: 1,
                height: 1
            }]
        );
    }

    #[test]
    fn test_contiguous_rectangle() {
        // 3x2 rectangle
        let cells = vec![(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)];
        let merged = merge_tile_cells(&cells);
        assert_eq!(
            merged,
            vec![MergedTileRect {
                col: 0,
                row: 0,
                width: 3,
                height: 2
            }]
        );
    }

    #[test]
    fn test_l_shape() {
        // L-shape:
        // [X, X, X]
        // [X, ., .]
        let cells = vec![(0, 0), (1, 0), (2, 0), (0, 1)];
        let merged = merge_tile_cells(&cells);
        assert_eq!(
            merged,
            vec![
                MergedTileRect {
                    col: 0,
                    row: 0,
                    width: 3,
                    height: 1
                },
                MergedTileRect {
                    col: 0,
                    row: 1,
                    width: 1,
                    height: 1
                },
            ]
        );
    }

    #[test]
    fn test_negative_coordinates() {
        let cells = vec![(-2, -3), (-1, -3), (-2, -2), (-1, -2)];
        let merged = merge_tile_cells(&cells);
        assert_eq!(
            merged,
            vec![MergedTileRect {
                col: -2,
                row: -3,
                width: 2,
                height: 2
            }]
        );
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Coordinate-space conversions between world space, screen pixels, and NDC.

/// Converts a world-space point into screen-pixel coordinates for a camera
/// centered at `cam` with zoom factor `zoom` on a `size` viewport.
pub fn world_to_screen(
    world: (f64, f64),
    cam: (f64, f64),
    zoom: f64,
    size: (u32, u32),
) -> (f32, f32) {
    let sx = (size.0 as f64 / 2.0 + (world.0 - cam.0) * zoom) as f32;
    let sy = (size.1 as f64 / 2.0 + (world.1 - cam.1) * zoom) as f32;
    (sx, sy)
}

/// Converts screen-pixel coordinates back into world space.
pub fn screen_to_world(
    screen: (f32, f32),
    cam: (f64, f64),
    zoom: f64,
    size: (u32, u32),
) -> (f64, f64) {
    let zoom = if zoom == 0.0 { 1.0 } else { zoom };
    (
        cam.0 + (screen.0 as f64 - size.0 as f64 / 2.0) / zoom,
        cam.1 + (screen.1 as f64 - size.1 as f64 / 2.0) / zoom,
    )
}

/// Uniform "contain" scale that fits `design` inside `actual` keeping aspect:
/// the game renders the design resolution through `zoom * fit_scale` so
/// content grows and shrinks with the window instead of showing more world.
/// Zero sizes fall back to 1.0 (a minimized window must not divide).
pub fn fit_scale(design: (u32, u32), actual: (u32, u32)) -> f64 {
    if design.0 == 0 || design.1 == 0 || actual.0 == 0 || actual.1 == 0 {
        return 1.0;
    }
    (actual.0 as f64 / design.0 as f64).min(actual.1 as f64 / design.1 as f64)
}

/// Converts screen-pixel coordinates (y-down) into normalized device coordinates (y-up).
pub fn screen_to_ndc(screen: (f32, f32), size: (u32, u32)) -> (f32, f32) {
    (
        screen.0 / size.0 as f32 * 2.0 - 1.0,
        1.0 - screen.1 / size.1 as f32 * 2.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_frames_world_into_ndc() {
        let size = (800, 600);
        let (sx, sy) = world_to_screen((100.0, 200.0), (100.0, 200.0), 1.0, size);
        assert_eq!((sx, sy), (400.0, 300.0));
        let (nx, ny) = screen_to_ndc((sx, sy), size);
        assert!((nx.abs() < 1e-6) && (ny.abs() < 1e-6));
        let (sx2, _) = world_to_screen((110.0, 200.0), (100.0, 200.0), 2.0, size);
        assert!((sx2 - 420.0).abs() < 1e-6);
    }

    #[test]
    fn screen_to_world_inverts_projection() {
        let size = (800, 600);
        let world = (150.0, -40.0);
        let screen = world_to_screen(world, (100.0, 200.0), 2.0, size);
        let back = screen_to_world(screen, (100.0, 200.0), 2.0, size);
        assert!((back.0 - world.0).abs() < 1e-3 && (back.1 - world.1).abs() < 1e-3);
        let center = screen_to_world((400.0, 300.0), (100.0, 200.0), 1.0, size);
        assert!((center.0 - 100.0).abs() < 1e-9 && (center.1 - 200.0).abs() < 1e-9);
    }

    #[test]
    fn fit_scale_contains_and_guards_zeros() {
        assert!((fit_scale((1280, 720), (1920, 1080)) - 1.5).abs() < 1e-9);
        assert!((fit_scale((1280, 720), (1920, 800)) - (800.0 / 720.0)).abs() < 1e-9);
        assert!((fit_scale((1280, 720), (640, 360)) - 0.5).abs() < 1e-9);
        assert_eq!(fit_scale((0, 720), (1920, 1080)), 1.0);
        assert_eq!(fit_scale((1280, 720), (0, 0)), 1.0);
    }
}

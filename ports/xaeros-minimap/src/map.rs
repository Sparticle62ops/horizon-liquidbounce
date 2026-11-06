//! Minimap frame and rendering math: the square and circle shapes, zoom,
//! rotation lock with the direction arrow, and the enlarge keybind behaviors.

use crate::config::{map_size_pixels, Config, Shape};

/// A point on the map canvas, in world-block coordinates relative to the map center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapPoint {
    pub x: f64,
    pub z: f64,
}

/// Which world-block a screen pixel inside the map widget samples, given zoom.
/// `zoom_level` follows the mod's multiple zoom options: each step doubles the
/// world blocks the widget covers.
pub fn sample_block(cfg: &Config, widget_size: u32, px: i32, pz: i32, screen_px: i32, screen_pz: i32) -> Option<MapPoint> {
    let size = widget_size as i32;
    let half = size as f64 / 2.0;
    if cfg.shape == Shape::Circle {
        let dx = (screen_px - size / 2) as f64;
        let dz = (screen_pz - size / 2) as f64;
        if dx * dx + dz * dz > half * half {
            return None;
        }
    } else if screen_px < 0 || screen_pz < 0 || screen_px >= size || screen_pz >= size {
        return None;
    }
    let blocks_per_pixel = (1usize << cfg.zoom) as f64;
    let rel_x = (screen_px - size / 2) as f64 * blocks_per_pixel;
    let rel_z = (screen_pz - size / 2) as f64 * blocks_per_pixel;
    Some(MapPoint { x: px as f64 + rel_x, z: pz as f64 + rel_z })
}

/// The player marker's angle on the map, in radians clockwise from map-up.
/// With lock_north the map never rotates and the facing shows as an arrow, so
/// the marker angle is the player's yaw; without it the map rotates under the
/// player, so the marker always points up.
pub fn player_marker_angle(cfg: &Config, yaw_radians: f64) -> f64 {
    if cfg.lock_north {
        yaw_radians
    } else {
        0.0
    }
}

/// The on-screen position of the widget's top-left corner, from the config
/// offsets (any pixel shift, including 1-pixel nudges, is honored).
pub fn widget_origin(cfg: &Config, screen_w: u32, screen_h: u32, widget_size: u32) -> (i32, i32) {
    let _ = (screen_w, screen_h);
    (cfg.offset_x, cfg.offset_y)
}

/// The enlarge keybind's (Z by default) temporary zoom and position: centered
/// on screen when the centered setting is on, and fully zoomed out when that
/// setting is on. Returns the effective widget size and zoom while enlarged.
pub fn enlarged_state(cfg: &Config, screen_w: u32, screen_h: u32, normal_size: u32) -> (u32, u8) {
    let mut size = normal_size.max(map_minimum_for(screen_w, screen_h));
    if cfg.enlarge_centered {
        size = map_minimum_for(screen_w, screen_h);
    }
    let zoom = if cfg.enlarge_full_zoom_out { u8::MAX } else { cfg.zoom };
    (size, zoom)
}

fn map_minimum_for(screen_w: u32, screen_h: u32) -> u32 {
    screen_w.min(screen_h) * 4 / 5
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MapSize;

    fn cfg(shape: Shape) -> Config {
        Config { shape, ..Config::default() }
    }

    #[test]
    fn zoom_zero_maps_one_block_per_pixel() {
        let c = cfg(Shape::Square);
        let p = sample_block(&c, 128, 0, 0, 64, 64).unwrap();
        assert_eq!((p.x, p.z), (0.0, 0.0));
    }

    #[test]
    fn zoom_two_covers_four_blocks_per_pixel() {
        let mut c = cfg(Shape::Square);
        c.zoom = 2;
        let p = sample_block(&c, 128, 0, 0, 64 + 4, 64).unwrap();
        assert_eq!(p.x, 16.0);
    }

    #[test]
    fn square_rejects_outside_pixels_circle_rejects_corners() {
        let c = cfg(Shape::Square);
        assert!(sample_block(&c, 128, 0, 0, 200, 64).is_none());
        let circle = cfg(Shape::Circle);
        assert!(sample_block(&circle, 128, 0, 0, 0, 0).is_none(), "corner of a circle map is outside");
        assert!(sample_block(&circle, 128, 0, 0, 64, 64).is_some());
    }

    #[test]
    fn lock_north_shows_facing_yaw_otherwise_marker_points_up() {
        assert_eq!(player_marker_angle(&Config { lock_north: true, ..Config::default() }, 1.25), 1.25);
        assert_eq!(player_marker_angle(&Config { lock_north: false, ..Config::default() }, 1.25), 0.0);
    }

    #[test]
    fn offsets_shift_by_single_pixels() {
        let mut c = cfg(Shape::Square);
        c.offset_x = 3;
        c.offset_y = 7;
        assert_eq!(widget_origin(&c, 1920, 1080, 192), (3, 7));
    }

    #[test]
    fn automatic_size_scales_with_screen() {
        let c = Config { size: MapSize::Automatic, ..Config::default() };
        assert_eq!(map_size_pixels(&c, 1920, 1080), 432);
    }

    #[test]
    fn enlarge_full_zoom_out_pushes_zoom_to_max() {
        let c = Config { enlarge_full_zoom_out: true, ..Config::default() };
        let (_, zoom) = enlarged_state(&c, 1920, 1080, 192);
        assert_eq!(zoom, u8::MAX);
    }

    #[test]
    fn enlarge_centered_grows_widget() {
        let c = Config { enlarge_centered: true, ..Config::default() };
        let (size, zoom) = enlarged_state(&c, 1920, 1080, 192);
        assert!(size > 192);
        assert_eq!(zoom, c.zoom);
    }
}

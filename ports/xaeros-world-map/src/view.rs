//! The map screen: open with M, pan with the mouse, zoom with the wheel,
//! Google-Maps-style. Holds the view state and the world-to-screen transform.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    /// Center of the viewport, in world blocks.
    pub center_x: f64,
    pub center_z: f64,
    /// Zoom level: 0 = 1 block per pixel, each step halves the scale.
    pub zoom: u8,
    /// Viewport size in pixels.
    pub width: u32,
    pub height: u32,
}

impl ViewState {
    pub fn new(center_x: f64, center_z: f64, width: u32, height: u32) -> Self {
        Self { center_x, center_z, zoom: 0, width, height }
    }

    /// Screen position of a world block, top-left origin.
    pub fn to_screen(&self, wx: f64, wz: f64) -> (f64, f64) {
        let scale = self.blocks_per_pixel();
        (self.width as f64 / 2.0 + (wx - self.center_x) / scale, self.height as f64 / 2.0 + (wz - self.center_z) / scale)
    }

    /// World block under a screen pixel.
    pub fn to_world(&self, sx: f64, sz: f64) -> (f64, f64) {
        let scale = self.blocks_per_pixel();
        (self.center_x + (sx - self.width as f64 / 2.0) * scale, self.center_z + (sz - self.height as f64 / 2.0) * scale)
    }

    fn blocks_per_pixel(&self) -> f64 {
        (1usize << self.zoom) as f64
    }

    /// Mouse-drag panning: the map moves with the cursor.
    pub fn pan(&mut self, dx_pixels: f64, dz_pixels: f64) {
        let scale = self.blocks_per_pixel();
        self.center_x -= dx_pixels * scale;
        self.center_z -= dz_pixels * scale;
    }

    /// Mouse-wheel zoom, clamped to the mod's zoom range.
    pub fn zoom_by(&mut self, steps: i8) {
        let z = self.zoom as i16 - steps as i16;
        self.zoom = z.clamp(0, 12) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_pixel_is_center_world() {
        let v = ViewState::new(100.0, -50.0, 640, 480);
        assert_eq!(v.to_world(320.0, 240.0), (100.0, -50.0));
    }

    #[test]
    fn zoom_one_two_blocks_per_pixel() {
        let mut v = ViewState::new(0.0, 0.0, 640, 480);
        v.zoom = 1;
        assert_eq!(v.to_screen(64.0, 0.0), (352.0, 240.0));
        assert_eq!(v.to_world(352.0, 240.0), (64.0, 0.0));
    }

    #[test]
    fn pan_moves_center_opposite_the_drag() {
        let mut v = ViewState::new(0.0, 0.0, 640, 480);
        v.pan(32.0, 0.0);
        assert_eq!(v.center_x, -32.0);
    }

    #[test]
    fn zoom_wheel_steps_and_clamps() {
        let mut v = ViewState::new(0.0, 0.0, 640, 480);
        v.zoom_by(-1);
        assert_eq!(v.zoom, 1);
        for _ in 0..20 { v.zoom_by(-1); }
        assert_eq!(v.zoom, 12);
        for _ in 0..20 { v.zoom_by(1); }
        assert_eq!(v.zoom, 0);
    }
}

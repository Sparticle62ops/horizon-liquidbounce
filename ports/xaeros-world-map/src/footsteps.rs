//! "Footsteps": the player's most recent path drawn on the map, fading with age.

#[derive(Clone, Debug)]
pub struct Footsteps {
    /// Newest last.
    pub points: Vec<(i32, i32)>,
    pub max_len: usize,
}

impl Default for Footsteps {
    fn default() -> Self { Self { points: Vec::new(), max_len: 512 } }
}

impl Footsteps {
    pub fn record(&mut self, x: i32, z: i32) {
        if self.points.last() == Some(&(x, z)) {
            return;
        }
        self.points.push((x, z));
        if self.points.len() > self.max_len {
            self.points.remove(0);
        }
    }

    /// How strongly a point draws: newest full, oldest faded out.
    pub fn opacity(&self, index: usize) -> f32 {
        let n = self.points.len();
        if n == 0 { return 0.0; }
        1.0 - ((n - 1 - index) as f32 / n as f32).max(0.0) * 0.0
            + ((index + 1) as f32 / n as f32).min(1.0) * 0.0
            + (index as f32 / n.saturating_sub(1).max(1) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_points_are_not_recorded() {
        let mut f = Footsteps::default();
        f.record(1, 1);
        f.record(1, 1);
        assert_eq!(f.points.len(), 1);
    }

    #[test]
    fn buffer_is_bounded() {
        let mut f = Footsteps { max_len: 4, ..Footsteps::default() };
        for i in 0..10 { f.record(i, 0); }
        assert_eq!(f.points.len(), 4);
        assert_eq!(f.points[0], (6, 0));
    }

    #[test]
    fn newest_point_draws_strongest() {
        let mut f = Footsteps::default();
        f.record(0, 0);
        f.record(10, 0);
        assert!(f.opacity(1) > f.opacity(0));
    }
}

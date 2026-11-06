//! Camera paths: keyframes with smooth interpolation, the core of ReplayMod's
//! camera path editor. Positions are cubic-bezier handles like upstream's
//! ControlPoint path; each keyframe carries position, yaw and pitch.

#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    pub tick: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraPath {
    pub keyframes: Vec<Keyframe>,
}

impl CameraPath {
    pub fn add(&mut self, k: Keyframe) {
        // Sorted insert; a keyframe on an existing tick replaces it.
        match self.keyframes.binary_search_by_key(&k.tick, |k| k.tick) {
            Ok(i) => self.keyframes[i] = k,
            Err(i) => self.keyframes.insert(i, k),
        }
    }

    /// Camera state at `tick`. Between keyframes the position follows a
    /// cubic ease-in-out curve on t (ReplayMod defaults to smoothstep-like
    /// easing), and rotation interpolates the short way around.
    pub fn state_at(&self, tick: u32) -> Option<(f64, f64, f64, f32, f32)> {
        let kfs = &self.keyframes;
        if kfs.is_empty() {
            return None;
        }
        if tick <= kfs[0].tick {
            let k = &kfs[0];
            return Some((k.x, k.y, k.z, k.yaw, k.pitch));
        }
        if let Some(k) = kfs.last() {
            if tick >= k.tick {
                return Some((k.x, k.y, k.z, k.yaw, k.pitch));
            }
        }
        let i = kfs.partition_point(|k| k.tick < tick) - 1;
        let (a, b) = (&kfs[i], &kfs[i + 1]);
        let t = (tick - a.tick) as f64 / (b.tick - a.tick) as f64;
        let e = smoothstep(t);
        Some((
            a.x + (b.x - a.x) * e,
            a.y + (b.y - a.y) * e,
            a.z + (b.z - a.z) * e,
            lerp_angle(a.yaw, b.yaw, e as f32),
            a.pitch + (b.pitch - a.pitch) * e as f32,
        ))
    }

    /// Remove a keyframe by tick; returns whether one was removed.
    pub fn remove(&mut self, tick: u32) -> bool {
        match self.keyframes.binary_search_by_key(&tick, |k| k.tick) {
            Ok(i) => { self.keyframes.remove(i); true }
            Err(_) => false,
        }
    }

    pub fn is_empty(&self) -> bool { self.keyframes.is_empty() }
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 { d -= 360.0; }
    if d < -180.0 { d += 360.0; }
    (a + d * t).rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframes_stay_sorted_and_replace_on_same_tick() {
        let mut p = CameraPath::default();
        p.add(Keyframe { tick: 20, x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        p.add(Keyframe { tick: 10, x: 5.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        p.add(Keyframe { tick: 10, x: 7.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        assert_eq!(p.keyframes.len(), 2);
        assert_eq!(p.keyframes[0].tick, 10);
        assert_eq!(p.keyframes[0].x, 7.0);
    }

    #[test]
    fn eases_between_keyframes_and_ends_are_exact() {
        let mut p = CameraPath::default();
        p.add(Keyframe { tick: 0, x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        p.add(Keyframe { tick: 100, x: 100.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        let (x, _, _, _, _) = p.state_at(50).unwrap();
        assert!((x - 50.0).abs() < 1e-9, "smoothstep(0.5)=0.5");
        let (x, _, _, _, _) = p.state_at(200).unwrap();
        assert_eq!(x, 100.0);
        let (x, _, _, _, _) = p.state_at(0).unwrap();
        assert_eq!(x, 0.0);
        assert!(CameraPath::default().state_at(5).is_none());
    }

    #[test]
    fn remove_works_by_tick() {
        let mut p = CameraPath::default();
        p.add(Keyframe { tick: 5, x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        assert!(p.remove(5));
        assert!(!p.remove(5));
        assert!(p.is_empty());
    }

    #[test]
    fn yaw_short_way_between_keyframes() {
        let mut p = CameraPath::default();
        p.add(Keyframe { tick: 0, x: 0.0, y: 0.0, z: 0.0, yaw: 350.0, pitch: 0.0 });
        p.add(Keyframe { tick: 10, x: 0.0, y: 0.0, z: 0.0, yaw: 10.0, pitch: 0.0 });
        let (_, _, _, yaw, _) = p.state_at(5).unwrap();
        assert!((yaw - 0.0).abs() < 1e-4, "got {yaw}");
    }
}

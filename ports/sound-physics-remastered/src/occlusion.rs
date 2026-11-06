//! Direct-path sound occlusion: raymarch from listener to source the way SPR
//! does, sampling every half block and accumulating the absorption of the
//! blocks the path passes through.

use crate::materials::AIR;

/// A voxel world the physics math can query. Values are the block's
/// absorption; the horizon side supplies this from the live world.
pub trait VoxelWorld {
    fn block_at(&self, x: i32, y: i32, z: i32) -> f32;
}

/// A test/table world; the client-side implementation wraps Horizon's world
/// snapshot (issue #10152 covers thread-safe world snapshots).
pub struct TableWorld(pub std::collections::HashMap<(i32, i32, i32), f32>);

impl VoxelWorld for TableWorld {
    fn block_at(&self, x: i32, y: i32, z: i32) -> f32 {
        *self.0.get(&(x, y, z)).unwrap_or(&0.0)
    }
}

/// The accumulated occlusion of the direct path (sum of each distinct
/// non-air voxel's absorption, at most one hit per voxel like SPR's raymarch).
pub fn direct_occlusion(world: &dyn VoxelWorld, from: (f64, f64, f64), to: (f64, f64, f64)) -> f32 {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let dz = to.2 - from.2;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    if dist == 0.0 {
        return 0.0;
    }
    // One sample per half block, not counting the endpoints' own voxels.
    let steps = (dist / 0.5).ceil() as i32;
    let mut occlusion = 0.0f32;
    let mut last: Option<(i32, i32, i32)> = None;
    for s in 1..steps {
        let t = s as f64 / steps as f64;
        let p = (from.0 + dx * t, from.1 + dy * t, from.2 + dz * t);
        let voxel = (p.0.floor() as i32, p.1.floor() as i32, p.2.floor() as i32);
        if Some(voxel) != last {
            let a = world.block_at(voxel.0, voxel.1, voxel.2);
            if a > 0.0 {
                occlusion += a;
            }
            last = Some(voxel);
        }
    }
    occlusion.min(1.0)
}

/// The final direct-sound factor: distance falloff times what survives the
/// blocks. Falloff is Minecraft-style linear clamp at the audible range.
pub fn direct_sound_factor(distance: f64, audible_range: f64, occlusion: f32) -> f32 {
    if distance >= audible_range {
        return 0.0;
    }
    let distance_factor = (1.0 - distance / audible_range) as f32;
    distance_factor * (1.0 - occlusion)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn world(walls: &[(i32, i32, i32, f32)]) -> TableWorld {
        let mut m = HashMap::new();
        for (x, y, z, a) in walls {
            m.insert((*x, *y, *z), *a);
        }
        TableWorld(m)
    }

    #[test]
    fn clear_path_has_no_occlusion() {
        let w = world(&[]);
        assert_eq!(direct_occlusion(&w, (0.5, 64.0, 0.5), (10.5, 64.0, 0.5)), 0.0);
    }

    #[test]
    fn one_wall_between_listener_and_source() {
        // Stone wall at x=5 on the straight path y=64, z=0.
        let w = world(&[(5, 64, 0, 1.0)]);
        let occ = direct_occlusion(&w, (0.5, 64.0, 0.5), (10.5, 64.0, 0.5));
        assert_eq!(occ, 1.0);
        // A wool wall absorbs everything already at 1.0 per voxel; a thinner
        // absorber accumulates once per voxel, not per sample.
        let w2 = world(&[(5, 64, 0, 0.3)]);
        assert!((direct_occlusion(&w2, (0.5, 64.0, 0.5), (10.5, 64.0, 0.5)) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn path_beside_the_wall_is_clear() {
        let w = world(&[(5, 64, 0, 1.0)]);
        let occ = direct_occlusion(&w, (0.5, 64.0, 2.5), (10.5, 64.0, 2.5));
        assert_eq!(occ, 0.0);
    }

    #[test]
    fn factor_falls_with_distance_and_occlusion() {
        assert!((direct_sound_factor(0.0, 32.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((direct_sound_factor(16.0, 32.0, 0.0) - 0.5).abs() < 1e-6);
        assert_eq!(direct_sound_factor(33.0, 32.0, 0.0), 0.0);
        assert!((direct_sound_factor(16.0, 32.0, 0.5) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn same_point_is_zero() {
        let w = world(&[]);
        assert_eq!(direct_occlusion(&w, (1.0, 1.0, 1.0), (1.0, 1.0, 1.0)), 0.0);
    }
}

//! Raytraced reverb estimation: SPR casts rays from the listener into the
//! world, bounces them off walls up to a bounce budget, and derives the room's
//! reverb decay from how far the energy travels before dying. This port keeps
//! that algorithm with a deterministic LCG instead of a global RNG so tests
//! and replays are reproducible.

use crate::materials::{AIR, Material};
use crate::occlusion::VoxelWorld;
use crate::materials::material_for;

pub struct ReverbSettings {
    /// Rays cast from the listener (SPR's ray count).
    pub ray_count: u32,
    /// Bounces per ray before the energy is spent.
    pub max_bounces: u32,
    /// How far a ray may fly per leg, in blocks.
    pub max_ray_distance: f64,
    /// Fraction of energy kept per wall bounce (roughly the room's mean reflectivity).
    pub bounce_energy: f32,
}

impl Default for ReverbSettings {
    fn default() -> Self {
        Self { ray_count: 64, max_bounces: 6, max_ray_distance: 256.0, bounce_energy: 0.55 }
    }
}

#[derive(Clone, Copy, Debug)]
struct Vec3 { x: f64, y: f64, z: f64 }

impl Vec3 {
    fn normalized(self) -> Vec3 {
        let l = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if l == 0.0 { Vec3 { x: 0.0, y: 1.0, z: 0.0 } } else { Vec3 { x: self.x / l, y: self.y / l, z: self.z / l } }
    }
}

/// One cast ray's travel summary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayResult {
    /// Total distance traveled before the ray's energy ran out.
    pub total_distance: f64,
    /// How many walls it bounced off.
    pub bounces: u32,
}

/// Deterministic LCG in [0, 1).
struct Lcg(u64);
impl Lcg {
    fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / ((1u64 << 31) as f64)
    }
    /// A uniformly random direction on the sphere.
    fn next_dir(&mut self) -> Vec3 {
        // Marsaglia's method on [0,1] cube, rejected inside the sphere.
        loop {
            let (a, b) = (self.next_f64() * 2.0 - 1.0, self.next_f64() * 2.0 - 1.0);
            let r2 = a * a + b * b;
            if r2 < 1.0 {
                let f = (1.0 - r2).sqrt();
                return Vec3 { x: 2.0 * a * f, y: 2.0 * b * f, z: 1.0 - 2.0 * r2 }.normalized();
            }
        }
    }
}

/// The first solid block a ray hits within `max` blocks, with hit position and
/// the surface material.
pub fn cast_ray(
    world: &dyn VoxelWorld,
    blocks: &dyn Fn(i32, i32, i32) -> Material,
    origin: (f64, f64, f64),
    dir: Vec3,
    max: f64,
) -> Option<((f64, f64, f64), f64, Material)> {
    let step = 0.5_f64;
    let mut t = step;
    let mut last: Option<(i32, i32, i32)> = None;
    while t <= max {
        let p = (origin.0 + dir.x * t, origin.1 + dir.y * t, origin.2 + dir.z * t);
        let voxel = (p.0.floor() as i32, p.1.floor() as i32, p.2.floor() as i32);
        if Some(voxel) != last {
            let m = blocks(voxel.0, voxel.1, voxel.2);
            if m.absorption > AIR.absorption {
                return Some((p, t, m));
            }
            last = Some(voxel);
        }
        t += step;
    }
    None
}

/// Reflect a ray off an axis-aligned wall: the dominant component of the hit
/// direction flips. SPR reflects off the face actually hit; the AABB face the
/// ray entered through is the axis it traveled fastest along.
fn reflect(dir: Vec3, hit_dir: Vec3) -> Vec3 {
    let ax = hit_dir.x.abs();
    let ay = hit_dir.y.abs();
    let az = hit_dir.z.abs();
    let mut d = dir;
    if ax >= ay && ax >= az {
        d.x = -d.x;
    } else if ay >= az {
        d.y = -d.y;
    } else {
        d.z = -d.z;
    }
    d
}

/// Cast `settings.ray_count` rays and summarize what the room does to sound:
/// the mean free path (average ray leg) drives the decay time estimate.
pub fn reverb_estimate(
    world: &dyn VoxelWorld,
    blocks: &dyn Fn(i32, i32, i32) -> Material,
    listener: (f64, f64, f64),
    settings: &ReverbSettings,
) -> (f64, u32) {
    let mut rng = Lcg(0x5EED_C0FFEE);
    let mut total_distance = 0.0f64;
    let mut total_bounces = 0u32;
    let mut rays_that_hit = 0u32;
    for _ in 0..settings.ray_count {
        let mut dir = rng.next_dir();
        let mut pos = listener;
        let mut dist = 0.0f64;
        let mut bounces = 0u32;
        let mut energy = 1.0f32;
        for _ in 0..settings.max_bounces {
            if let Some((hit, leg, m)) = cast_ray(world, blocks, pos, dir, settings.max_ray_distance - dist) {
                dist += leg;
                bounces += 1;
                energy *= m.reflectivity * settings.bounce_energy;
                if energy < 0.05 {
                    break;
                }
                pos = hit;
                dir = reflect(dir, dir);
            } else {
                dist += settings.max_ray_distance - dist;
                break;
            }
        }
        total_distance += dist;
        total_bounces += bounces;
        if bounces > 0 {
            rays_that_hit += 1;
        }
    }
    if settings.ray_count == 0 {
        return (0.0, 0);
    }
    (total_distance / settings.ray_count as f64, rays_that_hit)
}

/// The block-material lookup wired to a table world.
pub fn table_blocks(table: &std::collections::HashMap<(i32, i32, i32), String>) -> impl Fn(i32, i32, i32) -> Material + '_ {
    move |x, y, z| material_for(table.get(&(x, y, z)).map(String::as_str).unwrap_or("minecraft:air"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::{AIR, STONE, WOOL};
    use crate::occlusion::TableWorld;
    use std::collections::HashMap;

    fn blocks_of(w: &TableWorld) -> impl Fn(i32, i32, i32) -> Material + '_ {
        move |x, y, z| {
            if w.0.contains_key(&(x, y, z)) {
                STONE
            } else {
                AIR
            }
        }
    }

    #[test]
    fn ray_hits_a_wall_at_the_right_distance() {
        let w = TableWorld(HashMap::new());
        let hit = cast_ray(&w, &blocks_of(&w), (0.5, 64.5, 0.5), Vec3 { x: 1.0, y: 0.0, z: 0.0 }, 100.0);
        assert!(hit.is_none(), "empty world, nothing to hit");

        let mut m = HashMap::new();
        m.insert((10, 64, 0), 1.0);
        let w2 = TableWorld(m);
        let (_, dist, _) = cast_ray(&w2, &blocks_of(&w2), (0.5, 64.5, 0.5), Vec3 { x: 1.0, y: 0.0, z: 0.0 }, 100.0).unwrap();
        assert!((dist - 9.5).abs() < 1.0, "hit ~10 blocks out, got {dist}");
    }

    #[test]
    fn closed_room_bounces_open_world_does_not() {
        // A 5x3x5 stone box with the listener inside.
        let mut m = HashMap::new();
        for x in -2..=2 {
            for z in -2..=2 {
                for y in 63..=66 {
                    let on_wall = x == -2 || x == 2 || z == -2 || z == 2 || y == 63 || y == 66;
                    if on_wall {
                        m.insert((x, y, z), 1.0);
                    }
                }
            }
        }
        let room = TableWorld(m);
        let (_, room_bounces) = reverb_estimate(&room, &blocks_of(&room), (0.5, 64.5, 0.5), &ReverbSettings::default());
        assert!(room_bounces > 0, "rays in a closed room must bounce, got 0");

        let empty = TableWorld(HashMap::new());
        let (_, open_bounces) = reverb_estimate(&empty, &blocks_of(&empty), (0.5, 64.5, 0.5), &ReverbSettings::default());
        assert_eq!(open_bounces, 0, "rays in open air must not bounce");
    }

    #[test]
    fn small_room_has_a_shorter_mean_free_path_than_a_big_one() {
        let build = |half: i32| {
            let mut m = HashMap::new();
            for x in -half..=half {
                for z in -half..=half {
                    for y in 63..=67 {
                        if x == -half || x == half || z == -half || z == half || y == 63 || y == 67 {
                            m.insert((x, y, z), 1.0);
                        }
                    }
                }
            }
            TableWorld(m)
        };
        let s = ReverbSettings { ray_count: 128, ..Default::default() };
        let small = reverb_estimate(&build(2), &blocks_of(&build(2)), (0.5, 64.5, 0.5), &s).0;
        let big = reverb_estimate(&build(12), &blocks_of(&build(12)), (0.5, 64.5, 0.5), &s).0;
        assert!(small < big, "small room {small} should reverb shorter than big room {big}");
    }

    #[test]
    fn estimate_is_deterministic() {
        let empty = TableWorld(HashMap::new());
        let a = reverb_estimate(&empty, &blocks_of(&empty), (1.0, 1.0, 1.0), &ReverbSettings::default());
        let b = reverb_estimate(&empty, &blocks_of(&empty), (1.0, 1.0, 1.0), &ReverbSettings::default());
        assert_eq!(a, b);
    }

    #[test]
    fn wool_walls_kill_reverb_faster_than_stone() {
        let build = |mat: Material| {
            let mut m: HashMap<(i32, i32, i32), f32> = HashMap::new();
            for x in -2..=2 {
                for z in -2..=2 {
                    for y in 63..=66 {
                        if x == -2 || x == 2 || z == -2 || z == 2 || y == 63 || y == 66 {
                            m.insert((x, y, z), 1.0);
                        }
                    }
                }
            }
            let blocks = move |_: i32, _: i32, _: i32| mat;
            (TableWorld(m), blocks)
        };
        let s = ReverbSettings { ray_count: 128, ..Default::default() };
        let (d_stone, _) = { let (w, b) = build(STONE); reverb_estimate(&w, &b, (0.5, 64.5, 0.5), &s) };
        let (d_wool, _) = { let (w, b) = build(WOOL); reverb_estimate(&w, &b, (0.5, 64.5, 0.5), &s) };
        assert!(d_wool < d_stone, "wool room {d_wool} must decay before stone room {d_stone}");
    }
}

// Combat Hitboxes Plus for Horizon: colored entity hitboxes in the world.
//
// A Rust port of Combat Hitboxes Plus (Copyright TrueWulf, Apache-2.0,
// https://github.com/TrueWulf/combat-hitboxes-plus). The original's core
// behavior is kept: a hitbox outline for every mob and player the frame
// draws, red inside the weapon's reach and white outside with the
// out-of-range dimming, an eye-height line and a look-direction line per
// entity, line width scaled by distance and a render distance cap, all
// drawn through walls. Not in this slice yet, with their Horizon gaps:
// dropped-item hitboxes (needs WorldDrawn::motion enumeration, API 371),
// the shield-block color (no client read of another entity's blocking
// state) and the pearl/projectile owner colors (no client enumeration of
// projectiles); the config file and its command come next.
//
// This file is both the standalone Cargo library (its tests) and the module
// the bundle adds to the game.

use std::sync::Mutex;

/// The mod's id.
pub const ID: &str = "combat_hitboxes_plus";

/// Config: the hitbox colour in weapon reach (ARGB, red) and outside it
/// (white, dimmed per the original's out-of-range dimming).
pub const IN_RANGE_COLOR: u32 = 0xFFFF_4444;
pub const OUT_RANGE_COLOR: u32 = 0x70FF_FFFF;
/// The attack reach a red box answers to (the vanilla melee reach).
pub const REACH: f64 = 3.0;
/// The render distance cap, in blocks (the original's renderDistance 72).
pub const RENDER_DISTANCE: f64 = 72.0;
/// Line width at the camera, falling with distance to a floor.
pub const BASE_WIDTH: f32 = 2.0;
pub const MIN_WIDTH: f32 = 0.5;
/// The look-direction and eye-height lines' width factor.
pub const DETAIL_WIDTH: f32 = 0.75;

/// One entity the frame's submit hooks recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub id: u32,
    pub kind: String,
}

static MOBS: Mutex<Vec<Seen>> = Mutex::new(Vec::new());
static PLAYERS: Mutex<Vec<Seen>> = Mutex::new(Vec::new());

/// Record one entity of this frame's draw list, deduplicated.
pub fn record(players: bool, id: u32, kind: &str) {
    let list = if players { &PLAYERS } else { &MOBS };
    if let Ok(mut v) = list.lock() {
        if v.iter().any(|s| s.id == id) {
            return;
        }
        v.push(Seen {
            id,
            kind: kind.to_string(),
        });
    }
}

/// Drain one list for drawing.
pub fn take(players: bool) -> Vec<Seen> {
    let list = if players { &PLAYERS } else { &MOBS };
    if let Ok(mut v) = list.lock() {
        std::mem::take(&mut *v)
    } else {
        Vec::new()
    }
}

/// The line width for a box  blocks from the camera: the base width
/// falling linearly to the floor by the render distance, red boxes a little
/// thicker than dimmed ones.
pub fn width_for(d: f64, in_range: bool) -> f32 {
    let w = BASE_WIDTH - (d / RENDER_DISTANCE) as f32 * BASE_WIDTH;
    let w = w.max(MIN_WIDTH);
    if in_range {
        w + 0.5
    } else {
        w
    }
}

/// The squared distance between two points.
pub fn dist_sqr(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

impl crate::game::Game<'_> {
    /// The submit hooks call this: remember one drawn mob or player.
    pub fn combat_hitboxes_record(&self, players: bool, id: u32, kind: &str) {
        record(players, id, kind);
    }

    /// The player-render gate: this mod listens, so the game poses players.
    pub fn combat_hitboxes_listen(&self, listened: &mut bool) {
        *listened = true;
    }

    /// The world-primitives helper: one box, eye-height line and
    /// look-direction line per recorded entity, colored by whether the
    /// camera's owner could hit it, drawn through walls.
    pub fn combat_hitboxes_draw(
        &self,
        camera: [f64; 3],
        primitives: &mut crate::world_primitives::WorldPrimitives,
    ) {
        if !self.native_on(ID) {
            take(false);
            take(true);
            return;
        }
        for seen in take(false).into_iter().chain(take(true)) {
            let Some(entity) = self.native_entity(seen.id) else {
                continue;
            };
            let Some(minmax) = self.native_entity_bounding_box(entity) else {
                continue;
            };
            let (min, max) = (minmax[0], minmax[1]);
            let centre = [
                (min[0] + max[0]) / 2.0,
                (min[1] + max[1]) / 2.0,
                (min[2] + max[2]) / 2.0,
            ];
            let d = dist_sqr(camera, centre).sqrt();
            if d > RENDER_DISTANCE {
                continue;
            }
            // The nearest point of the box to the camera decides the reach.
            let near = [
                camera[0].clamp(min[0], max[0]),
                camera[1].clamp(min[1], max[1]),
                camera[2].clamp(min[2], max[2]),
            ];
            let dn = dist_sqr(camera, near).sqrt();
            let in_range = dn <= REACH;
            let color = if in_range {
                IN_RANGE_COLOR
            } else {
                OUT_RANGE_COLOR
            };
            let width = width_for(d, in_range);
            primitives
                .box_outline(min, max, color, width)
                .through_walls();
            if let Some(eye_h) = self.native_entity_eye_height(entity) {
                let y = min[1] + eye_h;
                primitives
                    .line(
                        [min[0], y, min[2]],
                        [max[0], y, max[2]],
                        color,
                        width * DETAIL_WIDTH,
                    )
                    .through_walls();
                if let Some(eye) = self.native_entity_eye_position(entity) {
                    if let Some(view) = self.native_entity_view_vector(entity) {
                        let end = [
                            eye[0] + view[0] * 2.0,
                            eye[1] + view[1] * 2.0,
                            eye[2] + view[2] * 2.0,
                        ];
                        primitives
                            .line(eye, end, color, width * DETAIL_WIDTH)
                            .through_walls();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_falls_with_distance_and_holds_a_floor() {
        assert!(width_for(0.0, false) > width_for(40.0, false));
        assert_eq!(width_for(1000.0, false), MIN_WIDTH);
        assert!(width_for(0.0, true) > width_for(0.0, false));
    }

    #[test]
    fn record_deduplicates_and_take_drains() {
        record(false, 7, "minecraft:zombie");
        record(false, 7, "minecraft:zombie");
        let v = take(false);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, 7);
        assert!(take(false).is_empty());
    }
}

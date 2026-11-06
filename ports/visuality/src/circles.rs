//! CirclesOnWaterEvent.onTick, 1:1 gates, rolls and spawn conditions.
//! The world side is a trait; Horizon glue implements it, tests stub it.

use crate::config::VisualityConfig;
use crate::particles;
use crate::rng::JavaRandom;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub fn below(self) -> Self {
        BlockPos { y: self.y - 1, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precipitation {
    None,
    Rain,
    Snow,
}

/// Test model of a biome value: fixed precipitation plus the warmth gate.
#[derive(Clone, Copy, Debug)]
pub struct Biome {
    pub precipitation: Precipitation,
    pub warm_enough_to_rain: bool,
}

impl Biome {
    pub fn precipitation_at(&self, _pos: BlockPos, _sea_level: i32) -> Precipitation {
        self.precipitation
    }
    pub fn warm_enough(&self, _pos: BlockPos, _sea_level: i32) -> bool {
        self.warm_enough_to_rain
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockRef {
    Air,
    Water,
    Solid,
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerSnapshot {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub under_water: bool,
}

pub trait CirclesWorld {
    fn is_paused(&self) -> bool;
    fn particles_minimal(&self) -> bool;
    fn player(&self) -> Option<PlayerSnapshot>;
    fn is_raining(&self) -> bool;
    fn is_overworld(&self) -> bool;
    fn sea_level(&self) -> i32;
    fn biome_at(&self, pos: BlockPos) -> Option<Biome>;
    fn heightmap_top_motion_blocking(&self, pos: BlockPos) -> BlockPos;
    fn block_state(&self, pos: BlockPos) -> BlockRef;
    fn fluid_amount(&self, pos: BlockPos) -> i32;
    fn add_particle(&mut self, particle: &str, x: f64, y: f64, z: f64);
}

/// CirclesOnWaterEvent.onTick(world): shown() gate first, then the density roll
/// and the per-candidate water check.
pub fn on_tick<W: CirclesWorld>(world: &mut W, config: &VisualityConfig, rng: &mut JavaRandom) {
    let Some((player, biome)) = shown(world, config) else {
        return;
    };
    let density = config.water_circles.density;
    let radius = config.water_circles.radius;
    if density <= 0 || radius <= 0 {
        return;
    }
    let random_density = rng.next_int_bounded(density as u32) + (density / 2);
    let sea_level = world.sea_level();
    for _i in 0..=random_density {
        let x = rng.next_int_bounded(radius as u32) - (radius / 2);
        let z = rng.next_int_bounded(radius as u32) - (radius / 2);
        let player_pos = BlockPos { x: player.x + x, y: player.y, z: player.z + z };
        let top_pos = world.heightmap_top_motion_blocking(player_pos);
        let on_pos = BlockPos { x: player.x, y: player.y - 1, z: player.z };
        if biome.precipitation_at(top_pos, sea_level) != Precipitation::Rain {
            continue;
        }
        if !biome.warm_enough(on_pos, sea_level) {
            continue;
        }
        if world.block_state(top_pos.below()) == BlockRef::Water
            && world.block_state(top_pos) == BlockRef::Air
        {
            if world.fluid_amount(top_pos.below()) == 8 {
                world.add_particle(
                    particles::WATER_CIRCLE,
                    top_pos.x as f64 + rng.next_double(),
                    top_pos.y as f64 + 0.05,
                    top_pos.z as f64 + rng.next_double(),
                );
            }
        }
    }
}

/// CirclesOnWaterEvent.shown(world), same gate order; returns the player and
/// biome it latched as statics upstream.
fn shown<W: CirclesWorld>(world: &W, config: &VisualityConfig) -> Option<(PlayerSnapshot, Biome)> {
    if !config.water_circles.enabled {
        return None;
    }
    if world.is_paused() {
        return None;
    }
    if world.particles_minimal() {
        return None;
    }
    let player = world.player()?;
    if player.under_water {
        return None;
    }
    if !world.is_overworld() {
        return None;
    }
    if !world.is_raining() {
        return None;
    }
    let on_pos = BlockPos { x: player.x, y: player.y - 1, z: player.z };
    let biome = world.biome_at(on_pos)?;
    Some((player, biome))
}

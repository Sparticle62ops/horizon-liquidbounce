//! EntityMixin / BlockMixin / AmethystClusterMixin / CreeperEntityMixin spawn rules.

use crate::circles::BlockPos;
use crate::config::VisualityConfig;
use crate::particles;
use crate::rng::JavaRandom;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Direction {
    pub const VALUES: [Direction; 6] =
        [Direction::Down, Direction::Up, Direction::North, Direction::South, Direction::West, Direction::East];

    pub fn axis(self) -> Axis {
        match self {
            Direction::Down | Direction::Up => Axis::Y,
            Direction::North | Direction::South => Axis::Z,
            Direction::West | Direction::East => Axis::X,
        }
    }

    pub fn step(self) -> (i32, i32, i32) {
        match self {
            Direction::Down => (0, -1, 0),
            Direction::Up => (0, 1, 0),
            Direction::North => (0, 0, -1),
            Direction::South => (0, 0, 1),
            Direction::West => (-1, 0, 0),
            Direction::East => (1, 0, 0),
        }
    }
}

/// EntityMixin.spawnSprintingParticles: soul puff when sprinting over
/// wither-summon base blocks, 1 in 5.
pub fn sprint_soul(
    config: &VisualityConfig,
    rng: &mut JavaRandom,
    state_is_wither_base: bool,
    x: f64,
    y: f64,
    z: f64,
    mut spawn: impl FnMut(&str, f64, f64, f64),
) {
    if config.soul_enabled && state_is_wither_base && rng.next_int_bounded(5) == 0 {
        spawn(particles::SOUL, x, y + 0.1, z);
    }
}

/// BlockMixin.randomDisplayTick: soul above wither-summon base blocks and
/// sparkles on shiny blocks, drawn only on exposed faces.
pub fn block_random_display_tick(
    config: &VisualityConfig,
    rng: &mut JavaRandom,
    state_wither_base: bool,
    above_air: bool,
    pos: BlockPos,
    is_shiny_block: bool,
    solid_render: impl Fn(Direction) -> bool,
    mut spawn: impl FnMut(&str, f64, f64, f64),
) {
    if config.soul_enabled && state_wither_base {
        if above_air && rng.next_f32() > 0.995 {
            spawn(
                particles::SOUL,
                pos.x as f64 + rng.next_double(),
                pos.y as f64 + 1.1,
                pos.z as f64 + rng.next_double(),
            );
        }
    }
    if config.shiny_blocks_enabled && is_shiny_block {
        for dir in Direction::VALUES {
            if !solid_render(dir) {
                if rng.next_f32() > 0.8 {
                    let (sx, sy, sz) = dir.step();
                    let x = if dir.axis() == Axis::X { 0.5 + 0.5625 * sx as f64 } else { rng.next_f32() as f64 };
                    let y = if dir.axis() == Axis::Y { 0.5 + 0.5625 * sy as f64 } else { rng.next_f32() as f64 };
                    let z = if dir.axis() == Axis::Z { 0.5 + 0.5625 * sz as f64 } else { rng.next_f32() as f64 };
                    spawn(particles::SPARKLE, pos.x as f64 + x, pos.y as f64 + y, pos.z as f64 + z);
                }
            }
        }
    }
}

/// AmethystClusterMixin.animateTick: sparkle from large clusters.
pub fn amethyst_cluster_animate_tick(
    config: &VisualityConfig,
    rng: &mut JavaRandom,
    is_cluster: bool,
    cluster_height: f32,
    pos: BlockPos,
    mut spawn: impl FnMut(&str, f64, f64, f64),
) {
    if config.sparkle_enabled && is_cluster && cluster_height > 5.0 && rng.next_f32() > 0.5 {
        spawn(
            particles::SPARKLE,
            pos.x as f64 + rng.next_double(),
            pos.y as f64 + rng.next_double(),
            pos.z as f64 + rng.next_double(),
        );
    }
}

/// CreeperEntityMixin.tick: charge particles around a powered creeper.
pub fn creeper_tick(
    config: &VisualityConfig,
    rng: &mut JavaRandom,
    client_side: bool,
    is_alive: bool,
    powered: bool,
    x: f64,
    y: f64,
    z: f64,
    mut spawn: impl FnMut(&str, f64, f64, f64),
) {
    if client_side && is_alive && powered {
        if config.charge_enabled && rng.next_int_bounded(20) == 0 {
            let dx = rng.next_f32() as f64 * 2.0 - 1.0;
            let dy = rng.next_f32() as f64;
            let dz = rng.next_f32() as f64 * 2.0 - 1.0;
            spawn(particles::CHARGE, x + dx, y + dy + 1.0, z + dz);
        }
    }
}

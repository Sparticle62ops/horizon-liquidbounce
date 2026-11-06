pub mod circles {
// CirclesOnWaterEvent.onTick, 1:1 gates, rolls and spawn conditions.
// The world side is a trait; Horizon glue implements it, tests stub it.

use super::config::VisualityConfig;
use super::particles;
use super::rng::JavaRandom;

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
}
pub mod colors {
// ColorsUtils.java and SlimeColors.java, verbatim constants.

pub const RED: u32 = 16735855;
pub const ORANGE: u32 = 16751185;
pub const YELLOW: u32 = 16318324;
pub const GREEN: u32 = 7077755;
pub const BLUE: u32 = 8245247;
pub const PURPLE: u32 = 16750831;

pub const RAINBOW: [u32; 6] = [RED, ORANGE, YELLOW, GREEN, BLUE, PURPLE];

pub const SLIME_VANILLA: u32 = 8978297;

pub fn get_random_color_of_rainbow(rng: &mut super::rng::JavaRandom) -> u32 {
    RAINBOW[rng.next_int_bounded(RAINBOW.len() as u32) as usize]
}
}
pub mod config {
// VisualityConfig.java + WaterCirclesConfigOption, defaults verbatim.
// TextColor becomes a raw rgb u32.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaterCirclesConfigOption {
    pub density: i32,
    pub radius: i32,
    pub enabled: bool,
    pub colored: bool,
}

impl Default for WaterCirclesConfigOption {
    fn default() -> Self {
        WaterCirclesConfigOption { density: 10, radius: 16, enabled: true, colored: true }
    }
}

pub const DEFAULT_HIT_PARTICLES: [&str; 6] = [
    "minecraft:skeleton/visuality:bone",
    "minecraft:skeleton_horse/visuality:bone",
    "minecraft:stray/visuality:bone",
    "minecraft:wither_skeleton/visuality:wither_bone",
    "minecraft:chicken/visuality:feather",
    "minecraft:villager/visuality:emerald",
];

pub const DEFAULT_SHINY_ARMOR: [&str; 8] = [
    "minecraft:golden_helmet",
    "minecraft:golden_chestplate",
    "minecraft:golden_leggings",
    "minecraft:golden_boots",
    "minecraft:diamond_helmet",
    "minecraft:diamond_chestplate",
    "minecraft:diamond_leggings",
    "minecraft:diamond_boots",
];

pub const DEFAULT_SHINY_BLOCKS: [&str; 3] =
    ["#minecraft:gold_ores", "#minecraft:diamond_ores", "#minecraft:emerald_ores"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualityConfig {
    pub slime_enabled: bool,
    pub slime_color: u32,
    pub charge_enabled: bool,
    pub sparkle_enabled: bool,
    pub soul_enabled: bool,
    pub water_circles: WaterCirclesConfigOption,
    pub hit_particles_enabled: bool,
    pub hit_particle_entries: Vec<String>,
    pub shiny_armor_enabled: bool,
    pub shiny_armor_entries: Vec<String>,
    pub shiny_blocks_enabled: bool,
    pub shiny_block_entries: Vec<String>,
}

impl Default for VisualityConfig {
    fn default() -> Self {
        VisualityConfig {
            slime_enabled: true,
            slime_color: 8978297,
            charge_enabled: true,
            sparkle_enabled: true,
            soul_enabled: true,
            water_circles: WaterCirclesConfigOption::default(),
            hit_particles_enabled: true,
            hit_particle_entries: DEFAULT_HIT_PARTICLES.iter().map(|s| s.to_string()).collect(),
            shiny_armor_enabled: true,
            shiny_armor_entries: DEFAULT_SHINY_ARMOR.iter().map(|s| s.to_string()).collect(),
            shiny_blocks_enabled: true,
            shiny_block_entries: DEFAULT_SHINY_BLOCKS.iter().map(|s| s.to_string()).collect(),
        }
    }
}
}
pub mod ids {
// Resource identifiers (net.minecraft.resources.Identifier analog).

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId {
    pub namespace: String,
    pub path: String,
}

fn valid(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.' || c == '-' || c == '/')
}

impl ResourceId {
    /// Bare ids get the minecraft namespace, matching Identifier.parse defaults.
    pub fn parse(s: &str) -> Option<ResourceId> {
        match s.split_once(':') {
            Some((ns, path)) if valid(ns) && valid(path) => {
                Some(ResourceId { namespace: ns.to_string(), path: path.to_string() })
            }
            None if valid(s) => {
                Some(ResourceId { namespace: "minecraft".to_string(), path: s.to_string() })
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}
}
pub mod particles {
// The 13 custom particle types registered by VisualityParticles.java
// (visuality:sparkle ... visuality:soul).

pub const MOD_ID: &str = "visuality";

pub const SPARKLE: &str = "visuality:sparkle";
pub const BONE: &str = "visuality:bone";
pub const WITHER_BONE: &str = "visuality:wither_bone";
pub const FEATHER: &str = "visuality:feather";
pub const COLD_FEATHER: &str = "visuality:cold_feather";
pub const WARM_FEATHER: &str = "visuality:warm_feather";
pub const SMALL_SLIME_BLOB: &str = "visuality:small_slime_blob";
pub const MEDIUM_SLIME_BLOB: &str = "visuality:medium_slime_blob";
pub const BIG_SLIME_BLOB: &str = "visuality:big_slime_blob";
pub const CHARGE: &str = "visuality:charge";
pub const WATER_CIRCLE: &str = "visuality:water_circle";
pub const EMERALD: &str = "visuality:emerald";
pub const SOUL: &str = "visuality:soul";

pub const ALL: [&str; 13] = [
    SPARKLE,
    BONE,
    WITHER_BONE,
    FEATHER,
    COLD_FEATHER,
    WARM_FEATHER,
    SMALL_SLIME_BLOB,
    MEDIUM_SLIME_BLOB,
    BIG_SLIME_BLOB,
    CHARGE,
    WATER_CIRCLE,
    EMERALD,
    SOUL,
];
}
pub mod registries {
// HitParticleRegistry / ShinyArmorRegistry / ShinyBlockRegistry logic.
// Registry lookups stay outside the core: callers pass resolvers, tests stub them.

use super::ids::ResourceId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitEntry {
    pub entity: ResourceId,
    pub particle: ResourceId,
}

/// HitParticleRegistry.reload(): entry shape "entity_id/particle_id", both sides
/// must resolve. Upstream would throw on an entry without a "/"; that is skipped here.
pub fn reload_hit_particles(
    entries_src: &[String],
    entity_exists: &dyn Fn(&ResourceId) -> bool,
    particle_exists: &dyn Fn(&ResourceId) -> bool,
) -> Vec<HitEntry> {
    let mut out = Vec::new();
    for entry in entries_src {
        if let Some((entity_part, particle_part)) = entry.split_once('/') {
            if let (Some(entity), Some(particle)) =
                (ResourceId::parse(entity_part), ResourceId::parse(particle_part))
            {
                if entity_exists(&entity) && particle_exists(&particle) {
                    out.push(HitEntry { entity, particle });
                }
            }
        }
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShinyLists {
    pub items: Vec<ResourceId>,
    pub tags: Vec<ResourceId>,
}

fn reload_shiny(entries_src: &[String]) -> ShinyLists {
    let mut lists = ShinyLists::default();
    for entry in entries_src {
        if let Some(tag) = entry.strip_prefix('#') {
            if let Some(id) = ResourceId::parse(tag) {
                lists.tags.push(id);
            }
        } else if let Some(id) = ResourceId::parse(entry) {
            lists.items.push(id);
        }
    }
    lists
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShinyArmorRegistry(pub ShinyLists);

impl ShinyArmorRegistry {
    pub fn reload(entries_src: &[String]) -> Self {
        ShinyArmorRegistry(reload_shiny(entries_src))
    }
    pub fn is_shiny(&self, item: &ResourceId, tag_matches: &dyn Fn(&ResourceId) -> bool) -> bool {
        self.0.items.contains(item) || self.0.tags.iter().any(|t| tag_matches(t))
    }
}

/// ShinyBlockRegistry.java is the same parse + match over blocks/tags.
pub type ShinyBlockRegistry = ShinyArmorRegistry;
}
pub mod rng {
// java.util.Random on a 48-bit LCG, so port logic keeps Java's draw order and
// modulo/rejection semantics for tests.

const MASK: u64 = (1u64 << 48) - 1;
const MUL: u64 = 0x5DEECE66D;
const ADD: u64 = 0xB;

#[derive(Clone, Debug)]
pub struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    pub fn new(seed: u64) -> Self {
        JavaRandom { seed: (seed ^ MUL) & MASK }
    }

    fn next_bits(&mut self, bits: u32) -> i32 {
        self.seed = (self.seed.wrapping_mul(MUL).wrapping_add(ADD)) & MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// java.util.Random.nextInt(bound) including the rejection path.
    pub fn next_int_bounded(&mut self, bound: u32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if bound & bound.wrapping_neg() == bound {
            (((bound as i64) * (self.next_bits(31) as i64)) >> 31) as i32
        } else {
            let b = bound as i32;
            loop {
                let bits = self.next_bits(31);
                let val = bits % b;
                if bits.wrapping_sub(val).wrapping_add(b - 1) >= 0 {
                    return val;
                }
            }
        }
    }

    pub fn next_f32(&mut self) -> f32 {
        self.next_bits(24) as f32 / (1u32 << 24) as f32
    }

    pub fn next_double(&mut self) -> f64 {
        (((self.next_bits(26) as i64) << 27) + self.next_bits(27) as i64) as f64
            / (1u64 << 53) as f64
    }
}
}
pub mod rules {
// EntityMixin / BlockMixin / AmethystClusterMixin / CreeperEntityMixin spawn rules.

use super::circles::BlockPos;
use super::config::VisualityConfig;
use super::particles;
use super::rng::JavaRandom;

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
}
pub mod sparkles {
// LivingEntityMixin: hit particles on hurt and shiny-armor sparkles on tick.

use super::config::VisualityConfig;
use super::ids::ResourceId;
use super::particles;
use super::registries::{HitEntry, ShinyArmorRegistry};
use super::rng::JavaRandom;

pub const SLOT_FEET: usize = 0;
pub const SLOT_LEGS: usize = 1;
pub const SLOT_CHEST: usize = 2;
pub const SLOT_HEAD: usize = 3;

#[derive(Clone, Debug, Default)]
pub struct LivingSnapshot {
    pub entity_type: Option<ResourceId>,
    pub is_chicken: bool,
    pub chicken_variant_cold: bool,
    pub chicken_variant_warm: bool,
    pub is_baby_zombie: bool,
    pub is_alive: bool,
    pub client_side: bool,
    pub is_self_player: bool,
    pub camera_first_person: bool,
    pub sneaking: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub bb_height: f32,
    pub armor: [Option<ResourceId>; 4],
}

#[derive(Clone, Debug)]
pub struct Attacker {
    pub is_living: bool,
    /// Some(amount) only when the main-hand stack has the attribute-modifiers
    /// component and it carries an ATTACK_DAMAGE modifier.
    pub main_hand_attack_damage: Option<f64>,
}

/// LivingEntityMixin.hurtClient, gate order as upstream. ticksDelay latches 10
/// ticks on the first matching entry.
pub fn hurt_client(
    state: &LivingSnapshot,
    attacker: Option<&Attacker>,
    hit_entries: &[HitEntry],
    config: &VisualityConfig,
    rng: &mut JavaRandom,
    ticks_delay: &mut i32,
    mut spawn: impl FnMut(&str, f64, f64, f64),
) {
    let Some(attacker) = attacker.filter(|a| a.is_living) else {
        return;
    };
    if !state.client_side || *ticks_delay != 0 || !state.is_alive || !config.hit_particles_enabled {
        return;
    }
    for entry in hit_entries {
        if state.entity_type.as_ref() == Some(&entry.entity) {
            *ticks_delay = 10;
            let mut count: i32 = rng.next_int_bounded(3);
            if let Some(amount) = attacker.main_hand_attack_damage {
                // Java (int)((float)amount * 0.5F): float math then truncate.
                count = ((amount as f32) * 0.5) as i32;
            }
            let mut particle = entry.particle.clone();
            if state.is_chicken && particle == ResourceId::parse(particles::FEATHER).unwrap() {
                if state.chicken_variant_cold {
                    particle = ResourceId::parse(particles::COLD_FEATHER).unwrap();
                } else if state.chicken_variant_warm {
                    particle = ResourceId::parse(particles::WARM_FEATHER).unwrap();
                }
            }
            spawn_hit_particles(state, &particle, count, rng, &mut spawn);
        }
    }
}

/// LivingEntityMixin.spawnHitParticles.
pub fn spawn_hit_particles(
    state: &LivingSnapshot,
    particle: &ResourceId,
    count: i32,
    rng: &mut JavaRandom,
    spawn: &mut dyn FnMut(&str, f64, f64, f64),
) {
    let mut height = state.bb_height;
    if height * 100.0 < 100.0 {
        height = 1.0;
    } else {
        height += 0.5;
    }
    for _i in 0..=count.min(10) {
        let bound = (height as i32) * 10;
        if bound <= 0 {
            break; // nextInt(0) would throw upstream; height >= 1.0 keeps this dead
        }
        let random_height = rng.next_int_bounded(bound as u32) as f64 / 10.0;
        spawn(&particle.to_string(), state.x, state.y + 0.2 + random_height, state.z);
    }
}

/// LivingEntityMixin.tick: delay countdown plus the sparkle pass.
pub fn tick(
    state: &LivingSnapshot,
    client_player_present: bool,
    config: &VisualityConfig,
    shiny_armor: &ShinyArmorRegistry,
    tag_matches: &dyn Fn(&ResourceId) -> bool,
    rng: &mut JavaRandom,
    ticks_delay: &mut i32,
    spawn: &mut dyn FnMut(&str, f64, f64, f64),
) {
    if state.client_side && *ticks_delay != 0 {
        *ticks_delay -= 1;
    }
    if state.client_side && state.is_alive && client_player_present && config.shiny_armor_enabled {
        if state.is_self_player {
            if !state.camera_first_person {
                spawn_sparkles(state, shiny_armor, tag_matches, rng, spawn);
            }
        } else {
            spawn_sparkles(state, shiny_armor, tag_matches, rng, spawn);
        }
    }
}

/// LivingEntityMixin.spawnSparkles.
pub fn spawn_sparkles(
    state: &LivingSnapshot,
    shiny_armor: &ShinyArmorRegistry,
    tag_matches: &dyn Fn(&ResourceId) -> bool,
    rng: &mut JavaRandom,
    mut spawn: impl FnMut(&str, f64, f64, f64),
) {
    if state.is_baby_zombie {
        return;
    }
    let shiny = |slot: usize| {
        state.armor[slot]
            .as_ref()
            .map(|id| shiny_armor.is_shiny(id, tag_matches))
            .unwrap_or(false)
    };
    if !(shiny(SLOT_FEET) || shiny(SLOT_LEGS) || shiny(SLOT_CHEST) || shiny(SLOT_HEAD)) {
        return;
    }
    let mut heights: Vec<f32> = Vec::new();
    if shiny(SLOT_FEET) {
        heights.push(0.25);
    }
    if shiny(SLOT_LEGS) {
        heights.push(0.65);
    }
    if shiny(SLOT_CHEST) {
        heights.push(1.1);
    }
    let head = shiny(SLOT_HEAD);
    if head {
        if state.sneaking {
            heights.push(1.65);
        } else {
            heights.push(1.85);
        }
    }
    if heights.is_empty() {
        return;
    }
    let height = heights[rng.next_int_bounded(heights.len() as u32) as usize];
    if rng.next_int_bounded((20 - (heights.len() as i32) * 2) as u32) == 0 {
        let rand_x = rng.next_f32() as f64 - 0.5;
        let rand_y = ((rng.next_f32() * 2.0 - 1.0) / 5.0) as f64;
        let rand_z = rng.next_f32() as f64 - 0.5;
        spawn(
            particles::SPARKLE,
            state.x + rand_x,
            state.y + rand_y + height as f64,
            state.z + rand_z,
        );
    }
}
}
// Visuality for Horizon: logic core of PinkGoosik/visuality (MIT), ported 1:1.
// Spawn rules, registries, config and event gates live here; particle
// rendering waits on a Horizon client particle surface.



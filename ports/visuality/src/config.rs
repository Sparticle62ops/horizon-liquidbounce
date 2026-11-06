//! VisualityConfig.java + WaterCirclesConfigOption, defaults verbatim.
//! TextColor becomes a raw rgb u32.

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

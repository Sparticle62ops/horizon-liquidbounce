//! Per-block sound behavior. SPR derives occlusion, absorption and sound
//! diffusivity from each block's material; this port keeps the same shape:
//! every block gets an absorption in 0.0 (air) .. 1.0 (fully solid) plus a
//! reverb-reflectivity.

/// Sound behavior of one block type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// How much direct sound the block absorbs (0 = air, 1 = fully absorbing).
    pub absorption: f32,
    /// Fraction of reflected energy the block returns (stone echoes, wool does not).
    pub reflectivity: f32,
}

pub const AIR: Material = Material { absorption: 0.0, reflectivity: 0.0 };
pub const STONE: Material = Material { absorption: 0.2, reflectivity: 0.8 };
pub const WOOL: Material = Material { absorption: 1.0, reflectivity: 0.1 };
pub const WOOD: Material = Material { absorption: 0.5, reflectivity: 0.4 };
pub const GLASS: Material = Material { absorption: 0.1, reflectivity: 0.7 };
pub const DIRT: Material = Material { absorption: 0.7, reflectivity: 0.3 };
pub const WATER: Material = Material { absorption: 0.3, reflectivity: 0.5 };

/// Map a block name to its material, the lookup SPR does from the block's
/// sound type. Names are the `minecraft:` ids the client reports.
pub fn material_for(block: &str) -> Material {
    match block {
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" => AIR,
        "minecraft:stone" | "minecraft:cobblestone" | "minecraft:obsidian" | "minecraft:bricks" => STONE,
        "minecraft:white_wool" | "minecraft:orange_wool" | "minecraft:black_wool" | "minecraft:carpet" => WOOL,
        "minecraft:oak_planks" | "minecraft:spruce_planks" | "minecraft:oak_log" => WOOD,
        "minecraft:glass" | "minecraft:glass_pane" => GLASS,
        "minecraft:dirt" | "minecraft:grass_block" | "minecraft:sand" => DIRT,
        "minecraft:water" => WATER,
        // Unknown solid blocks default to stone-like, matching SPR's fallback.
        _ => STONE,
    }
}

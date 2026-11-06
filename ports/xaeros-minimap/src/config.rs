//! The mod's documented settings model (behavioral port; Xaero's Minimap is
//! All Rights Reserved, no source published). Only settings the mod's page and
//! in-game options document are modeled here.

/// Minimap shape: the two documented options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape { Square, Circle }

/// Minimap size options, including the automatic one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapSize { Small, Medium, Large, Automatic }

/// Block color mode: vanilla map colors, or accurate block-texture and biome colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode { Vanilla, Accurate }

/// Entity radar rendering: dots with a custom color, or icons (usually heads).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityDisplay { Off, Dots, Icons }

/// The config file model with its defaults.
#[derive(Clone, Debug)]
pub struct Config {
    pub enabled: bool,
    pub shape: Shape,
    pub size: MapSize,
    /// Zoom level: multiple zoom options are documented; higher = farther out.
    pub zoom: u8,
    /// Pixel offset on screen; the page notes 1-pixel shifts are possible.
    pub offset_x: i32,
    pub offset_y: i32,
    /// "Lock north": stop rotation, show the facing direction as an arrow instead.
    pub lock_north: bool,
    /// Automatic cave mode with the documented "solid roof" size (odd number, 1, 3, 5...).
    pub cave_mode: bool,
    pub cave_roof_size: u8,
    pub color_mode: ColorMode,
    /// Biome colors inside the vanilla color mode.
    pub biome_colors: bool,
    /// Shading customization.
    pub terrain_depth: bool,
    pub terrain_slopes: bool,
    /// The enlarge keybind (Z by default) and its documented behaviors.
    pub enlarge_centered: bool,
    pub enlarge_full_zoom_out: bool,
    pub entity_radar: EntityDisplay,
    pub radar_players: bool,
    pub radar_mobs: bool,
    pub radar_items: bool,
    /// Optional: bind the minimap to an item required in the hotbar or equipped.
    pub required_item_id: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            shape: Shape::Square,
            size: MapSize::Automatic,
            zoom: 0,
            offset_x: 0,
            offset_y: 0,
            lock_north: false,
            cave_mode: true,
            cave_roof_size: 3,
            color_mode: ColorMode::Vanilla,
            biome_colors: false,
            terrain_depth: true,
            terrain_slopes: false,
            enlarge_centered: false,
            enlarge_full_zoom_out: false,
            entity_radar: EntityDisplay::Dots,
            radar_players: true,
            radar_mobs: true,
            radar_items: true,
            required_item_id: None,
        }
    }
}

/// The map size in pixels the widget renders at, mirroring the size options;
/// Automatic sizes to a fraction of the screen's shorter side.
pub fn map_size_pixels(cfg: &Config, screen_w: u32, screen_h: u32) -> u32 {
    match cfg.size {
        MapSize::Small => 128,
        MapSize::Medium => 192,
        MapSize::Large => 256,
        MapSize::Automatic => screen_w.min(screen_h) * 2 / 5,
    }
}

/// Whether the minimap may be shown at all, applying the optional item bind:
/// with a bind set, the minimap only shows when the required item is in the
/// hotbar or equipped.
pub fn may_show(cfg: &Config, hotbar_or_equipped_has_required_item: bool) -> bool {
    cfg.enabled && (cfg.required_item_id.is_none() || hotbar_or_equipped_has_required_item)
}

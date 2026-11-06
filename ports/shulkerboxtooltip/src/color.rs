//! Player-customizable colors, ported from api/color/ColorKey.java,
//! impl/color/ColorKeyImpl.java and impl/color/ColorRegistryImpl.java
//! (MIT, MisterPeModder/ShulkerBoxTooltip @68e7f8e). Components are stored as
//! floats exactly upstream; the config-file codec and config GUI are host seams.

use std::collections::HashMap;

/// `ShulkerBoxTooltipUtil.rgbToComponents`.
pub fn rgb_to_components(rgb: i32) -> [f32; 3] {
    [(rgb >> 16 & 0xFF) as f32 / 255.0, (rgb >> 8 & 0xFF) as f32 / 255.0, (rgb & 0xFF) as f32 / 255.0]
}

/// `ShulkerBoxTooltipUtil.componentsToRgb` (truncating cast, as upstream).
pub fn components_to_rgb(c: [f32; 3]) -> i32 {
    (((255.0 * c[0]) as i32) << 16) | (((255.0 * c[1]) as i32) << 8) | (255.0 * c[2]) as i32
}

/// `ColorKeyImpl`: current and default rgb as float components.
#[derive(Clone, Debug)]
pub struct ColorKey {
    pub category: String,
    pub id: String,
    rgb_components: [f32; 3],
    default_rgb_components: [f32; 3],
}

impl ColorKey {
    /// `ColorKey.ofRgb`.
    pub fn of_rgb(category: &str, id: &str, rgb: i32) -> Self {
        let c = rgb_to_components(rgb);
        Self { category: category.to_string(), id: id.to_string(), rgb_components: c, default_rgb_components: c }
    }

    /// `ColorKey.ofDye`: each channel clamped to at least 0.15.
    pub fn of_dye(category: &str, id: &str, dye: crate::item::DyeColor) -> Self {
        let color = dye.texture_diffuse_color();
        let clamped = [
            0.15f32.max(((color >> 16) & 0xff) as f32 / 255.0),
            0.15f32.max(((color >> 8) & 0xff) as f32 / 255.0),
            0.15f32.max((color & 0xff) as f32 / 255.0),
        ];
        Self { category: category.to_string(), id: id.to_string(), rgb_components: clamped, default_rgb_components: clamped }
    }

    pub fn rgb(&self) -> i32 {
        components_to_rgb(self.rgb_components)
    }

    pub fn default_rgb(&self) -> i32 {
        components_to_rgb(self.default_rgb_components)
    }

    pub fn set_rgb(&mut self, rgb: i32) {
        self.rgb_components = rgb_to_components(rgb);
    }
}

/// `ColorRegistryImpl`: categories of named color keys.
#[derive(Default)]
pub struct ColorRegistry {
    categories: HashMap<String, HashMap<String, ColorKey>>,
}

impl ColorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn category(&mut self, category_id: &str) -> &mut HashMap<String, ColorKey> {
        self.categories.entry(category_id.to_string()).or_default()
    }

    pub fn register(&mut self, category_id: &str, id: &str, key: ColorKey) {
        self.category(category_id).insert(id.to_string(), key);
    }

    pub fn key(&self, category_id: &str, id: &str) -> Option<&ColorKey> {
        self.categories.get(category_id).and_then(|c| c.get(id))
    }
}

use crate::item::DyeColor;

pub const DEFAULT_ID: &str = "default";
pub const ENDER_CHEST_ID: &str = "ender_chest";
pub const GENERIC_CONTAINER_ID: &str = "generic_container";
pub const SHULKER_BOX_ID: &str = "shulker_box";
pub const BUNDLE_ID: &str = "bundle";
pub const DEFAULT_CATEGORY: &str = "default";

/// Registers the mod's default color keys (upstream registers the same set on
/// client init: generic defaults plus one dyed key per shulker box / bundle).
pub fn register_defaults(registry: &mut ColorRegistry) {
    registry.register(DEFAULT_CATEGORY, DEFAULT_ID, ColorKey::of_rgb(DEFAULT_CATEGORY, DEFAULT_ID, 0xffffff));
    registry.register(DEFAULT_CATEGORY, ENDER_CHEST_ID, ColorKey::of_rgb(DEFAULT_CATEGORY, ENDER_CHEST_ID, 0x0b4b41));
    registry.register(DEFAULT_CATEGORY, GENERIC_CONTAINER_ID, ColorKey::of_rgb(DEFAULT_CATEGORY, GENERIC_CONTAINER_ID, 0xffffff));
    registry.register(DEFAULT_CATEGORY, SHULKER_BOX_ID, ColorKey::of_rgb(DEFAULT_CATEGORY, SHULKER_BOX_ID, 0x976797));
    registry.register(DEFAULT_CATEGORY, BUNDLE_ID, ColorKey::of_rgb(DEFAULT_CATEGORY, BUNDLE_ID, 0x844b2b));
    let dyed: [(&str, DyeColor); 16] = [
        ("white", DyeColor::White),
        ("orange", DyeColor::Orange),
        ("magenta", DyeColor::Magenta),
        ("light_blue", DyeColor::LightBlue),
        ("yellow", DyeColor::Yellow),
        ("lime", DyeColor::Lime),
        ("pink", DyeColor::Pink),
        ("gray", DyeColor::Gray),
        ("light_gray", DyeColor::LightGray),
        ("cyan", DyeColor::Cyan),
        ("purple", DyeColor::Purple),
        ("blue", DyeColor::Blue),
        ("brown", DyeColor::Brown),
        ("green", DyeColor::Green),
        ("red", DyeColor::Red),
        ("black", DyeColor::Black),
    ];
    for (name, dye) in dyed {
        registry.register(
            DEFAULT_CATEGORY,
            &format!("{name}_shulker_box"),
            ColorKey::of_dye(DEFAULT_CATEGORY, &format!("{name}_shulker_box"), dye),
        );
        registry.register(
            DEFAULT_CATEGORY,
            &format!("{name}_bundle"),
            ColorKey::of_dye(DEFAULT_CATEGORY, &format!("{name}_bundle"), dye),
        );
    }
}

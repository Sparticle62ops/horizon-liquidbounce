//! Preview providers, ported from api/provider/* and impl/provider/*
//! (MIT, MisterPeModder/ShulkerBoxTooltip @68e7f8e). The tooltip text and the
//! renderer are host seams (see README); the display/slot/row-size logic is exact.

use crate::color::{ColorKey, DEFAULT_CATEGORY, DEFAULT_ID, ENDER_CHEST_ID, GENERIC_CONTAINER_ID, SHULKER_BOX_ID};
use crate::config::{BundleTheme, ItemStackMergingStrategy};
use crate::item::{DyeColor, Stack, EMPTY};

/// `api/PreviewContext`. Upstream carries stack, owner, config view and
/// registry lookup; the owner/registry halves are host seams.
pub struct PreviewContext<'a> {
    pub stack: &'a Stack,
}

/// `api/provider/PreviewProvider`, with the upstream defaults.
pub trait PreviewProvider {
    fn should_display(&self, _ctx: &PreviewContext) -> bool {
        false
    }

    fn get_inventory(&self, _ctx: &PreviewContext) -> Vec<Stack> {
        Vec::new()
    }

    /// The maximum amount of slots this preview can display.
    fn get_inventory_max_size(&self, _ctx: &PreviewContext) -> i32 {
        0
    }

    /// The number of actively usable slots in the preview.
    fn get_active_slot_count(&self, ctx: &PreviewContext) -> i32 {
        self.get_inventory_max_size(ctx)
    }

    /// Slot to force-highlight, or -1 for none.
    fn get_selected_slot(&self, _ctx: &PreviewContext) -> i32 {
        -1
    }

    /// Max stacks per row in full preview mode; 0 means use the config default.
    fn get_max_row_size(&self, _ctx: &PreviewContext) -> i32 {
        0
    }

    /// Max stacks per row in compact preview mode; 0 means use the config default.
    fn get_compact_max_row_size(&self, _ctx: &PreviewContext) -> i32 {
        0
    }

    fn is_full_preview_available(&self, _ctx: &PreviewContext) -> bool {
        true
    }

    fn show_tooltip_hints(&self, _ctx: &PreviewContext) -> bool {
        false
    }

    fn get_window_color_key_id(&self, _ctx: &PreviewContext) -> &str {
        DEFAULT_ID
    }

    fn get_priority(&self) -> i32 {
        0
    }

    fn can_use_loot_tables(&self) -> bool {
        false
    }
}

/// `api/provider/BlockEntityPreviewProvider`: works on items carrying the
/// `minecraft:container` data component.
pub struct BlockEntityPreviewProvider {
    pub default_max_inv_size: i32,
    pub default_can_use_loot_tables: bool,
    pub default_max_row_size: i32,
    pub default_compact_max_row_size: i32,
}

impl BlockEntityPreviewProvider {
    pub fn new(default_max_inv_size: i32, default_can_use_loot_tables: bool, default_max_row_size: i32, default_compact_max_row_size: i32) -> Self {
        Self {
            default_max_inv_size,
            default_can_use_loot_tables,
            default_max_row_size: if default_max_row_size <= 0 { 9 } else { default_max_row_size },
            default_compact_max_row_size,
        }
    }
}

fn get_inventory_from_container(context: &PreviewContext, inv_max_size: i32) -> Vec<Stack> {
    let mut inv: Vec<Stack> = vec![EMPTY.clone(); inv_max_size.max(0) as usize];
    if let Some(container) = &context.stack.container {
        for (slot, stack) in container.iter().enumerate() {
            if (slot as i32) < inv_max_size {
                if let Some(stack) = stack {
                    inv[slot] = stack.clone();
                }
            }
        }
    }
    inv
}

fn item_count(items: &[Stack]) -> usize {
    items.iter().filter(|s| !s.is_empty()).count()
}

impl PreviewProvider for BlockEntityPreviewProvider {
    fn should_display(&self, context: &PreviewContext) -> bool {
        if self.can_use_loot_tables() && context.stack.has_container_loot {
            return false;
        }
        item_count(&self.get_inventory(context)) > 0
    }

    fn show_tooltip_hints(&self, context: &PreviewContext) -> bool {
        context.stack.container.is_some()
    }

    fn get_inventory(&self, context: &PreviewContext) -> Vec<Stack> {
        get_inventory_from_container(context, self.get_inventory_max_size(context))
    }

    fn get_inventory_max_size(&self, _context: &PreviewContext) -> i32 {
        self.default_max_inv_size
    }

    fn get_max_row_size(&self, _context: &PreviewContext) -> i32 {
        self.default_max_row_size
    }

    fn get_compact_max_row_size(&self, _context: &PreviewContext) -> i32 {
        self.default_compact_max_row_size
    }

    fn can_use_loot_tables(&self) -> bool {
        self.default_can_use_loot_tables
    }
}

/// `impl/provider/GenericContainerPreviewProvider`: any item with the
/// `minecraft:container` data component.
pub struct GenericContainerPreviewProvider;

impl Default for GenericContainerPreviewProvider {
    fn default() -> Self {
        Self
    }
}

impl PreviewProvider for GenericContainerPreviewProvider {
    fn should_display(&self, context: &PreviewContext) -> bool {
        if self.can_use_loot_tables() && context.stack.has_container_loot {
            return false;
        }
        item_count(&self.get_inventory(context)) > 0
    }

    fn get_inventory(&self, context: &PreviewContext) -> Vec<Stack> {
        get_inventory_from_container(context, self.get_inventory_max_size(context))
    }

    fn get_inventory_max_size(&self, _context: &PreviewContext) -> i32 {
        27
    }

    fn get_max_row_size(&self, _context: &PreviewContext) -> i32 {
        9
    }

    fn get_compact_max_row_size(&self, _context: &PreviewContext) -> i32 {
        9
    }

    fn get_window_color_key_id(&self, _context: &PreviewContext) -> &str {
        GENERIC_CONTAINER_ID
    }

    fn get_priority(&self) -> i32 {
        0
    }

    fn can_use_loot_tables(&self) -> bool {
        true
    }
}

/// `impl/provider/ShulkerBoxPreviewProvider` (an `InventoryAwarePreviewProvider`
/// with max row size 9): window color follows the box's dye color.
pub struct ShulkerBoxPreviewProvider;

fn shulker_color_key_id(dye: Option<DyeColor>) -> &'static str {
    match dye {
        None => SHULKER_BOX_ID,
        Some(d) => match d {
            DyeColor::Orange => "orange_shulker_box",
            DyeColor::Magenta => "magenta_shulker_box",
            DyeColor::LightBlue => "light_blue_shulker_box",
            DyeColor::Yellow => "yellow_shulker_box",
            DyeColor::Lime => "lime_shulker_box",
            DyeColor::Pink => "pink_shulker_box",
            DyeColor::Gray => "gray_shulker_box",
            DyeColor::LightGray => "light_gray_shulker_box",
            DyeColor::Cyan => "cyan_shulker_box",
            DyeColor::Purple => "purple_shulker_box",
            DyeColor::Blue => "blue_shulker_box",
            DyeColor::Brown => "brown_shulker_box",
            DyeColor::Green => "green_shulker_box",
            DyeColor::Red => "red_shulker_box",
            DyeColor::Black => "black_shulker_box",
            DyeColor::White => "white_shulker_box",
        },
    }
}

impl PreviewProvider for ShulkerBoxPreviewProvider {
    fn should_display(&self, context: &PreviewContext) -> bool {
        // canUseLootTables() is true (shulker box block entities are
        // RandomizableContainers) and shulker stacks don't carry container loot.
        if context.stack.has_container_loot {
            return false;
        }
        item_count(&self.get_inventory(context)) > 0
    }

    fn show_tooltip_hints(&self, _context: &PreviewContext) -> bool {
        true
    }

    fn get_inventory(&self, context: &PreviewContext) -> Vec<Stack> {
        get_inventory_from_container(context, 27)
    }

    fn get_inventory_max_size(&self, _context: &PreviewContext) -> i32 {
        27
    }

    fn get_max_row_size(&self, _context: &PreviewContext) -> i32 {
        9
    }

    fn get_compact_max_row_size(&self, _context: &PreviewContext) -> i32 {
        9
    }

    fn get_window_color_key_id(&self, context: &PreviewContext) -> &str {
        shulker_color_key_id(context.stack.dye)
    }

    fn can_use_loot_tables(&self) -> bool {
        true
    }
}

/// `impl/provider/BundlePreviewProvider` (theme gating + slot selection).
pub struct BundlePreviewProvider;

impl PreviewProvider for BundlePreviewProvider {
    fn should_display(&self, context: &PreviewContext) -> bool {
        let config = crate::config::Configuration::default();
        if config.preview.theme_bundle == BundleTheme::Vanilla {
            return false;
        }
        context.stack.container.as_ref().is_some_and(|c| c.iter().any(|s| s.as_ref().is_some_and(|s| !s.is_empty())))
    }

    fn get_inventory_max_size(&self, _context: &PreviewContext) -> i32 {
        64
    }

    fn get_active_slot_count(&self, context: &PreviewContext) -> i32 {
        context.stack.container.as_ref().map_or(0, |c| c.len() as i32)
    }

    fn get_selected_slot(&self, context: &PreviewContext) -> i32 {
        // Upstream maps the bundle's selected item index into the tail of the
        // 64-slot inventory; the selected index itself is a host seam.
        -1
    }

    fn get_inventory(&self, context: &PreviewContext) -> Vec<Stack> {
        let size = self.get_inventory_max_size(context);
        let mut inv: Vec<Stack> = vec![EMPTY.clone(); size as usize];
        if let Some(container) = &context.stack.container {
            let count = container.len() as i32;
            let start_slot = (size - count).max(0);
            for (i, stack) in container.iter().enumerate() {
                if let Some(stack) = stack {
                    inv[(start_slot + i as i32) as usize] = stack.clone();
                }
            }
        }
        inv
    }
}

/// `impl/provider/EnderChestPreviewProvider`: server-synced inventory when
/// available (host seam), else the client-side cache.
pub struct EnderChestPreviewProvider<'a> {
    pub cache: &'a crate::ender_cache::EnderChestCache,
}

impl<'a> PreviewProvider for EnderChestPreviewProvider<'a> {
    fn get_inventory(&self, _context: &PreviewContext) -> Vec<Stack> {
        self.cache
            .get_items()
            .into_iter()
            .map(|s| s.unwrap_or_else(|| EMPTY.clone()))
            .collect()
    }

    fn get_inventory_max_size(&self, _context: &PreviewContext) -> i32 {
        27
    }

    fn get_window_color_key_id(&self, _context: &PreviewContext) -> &str {
        ENDER_CHEST_ID
    }
}

/// `api/provider/EmptyPreviewProvider`: does nothing.
pub struct EmptyPreviewProvider;

impl PreviewProvider for EmptyPreviewProvider {
    fn get_inventory_max_size(&self, _context: &PreviewContext) -> i32 {
        0
    }

    fn should_display(&self, _context: &PreviewContext) -> bool {
        false
    }

    fn get_inventory(&self, _context: &PreviewContext) -> Vec<Stack> {
        Vec::new()
    }
}

/// Registry lookup by highest priority, mirroring
/// `PreviewProviderRegistryImpl`'s priority-ordered scan (the item-id keyed
/// map is a host seam; callers pass the candidate chain).
pub fn best_provider<'a>(candidates: &[&'a dyn PreviewProvider], _ctx: &PreviewContext) -> Option<&'a dyn PreviewProvider> {
    candidates
        .iter()
        .copied()
        .max_by_key(|p| p.get_priority())
}

impl BundlePreviewProvider {
    /// Upstream `getTheme()`: whether the ShulkerBoxTooltip theme applies.
    pub fn theme_is_mod(&self) -> bool {
        let config = crate::config::Configuration::default();
        config.preview.theme_bundle == BundleTheme::ShulkerBoxTooltip
    }
}

pub mod color {
// Player-customizable colors, ported from api/color/ColorKey.java,
// impl/color/ColorKeyImpl.java and impl/color/ColorRegistryImpl.java
// (MIT, MisterPeModder/ShulkerBoxTooltip @68e7f8e). Components are stored as
// floats exactly upstream; the config-file codec and config GUI are host seams.

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
    pub fn of_dye(category: &str, id: &str, dye: super::item::DyeColor) -> Self {
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

use super::item::DyeColor;

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
}
pub mod config {
// Configuration model, ported 1:1 from
// com.misterpemodder.shulkerboxtooltip.impl.config.Configuration (MIT,
// MisterPeModder/ShulkerBoxTooltip @68e7f8e). Field names and defaults match
// upstream; the jankson JSON serializer and the config GUI are host seams.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemStackMergingStrategy {
    /// Ignore component data when merging item stacks.
    Ignore,
    /// Merge regardless of component data but use the components of the first item stack.
    FirstItem,
    /// Do not merge stacks that differ in component data.
    Separate,
}

impl core::fmt::Display for ItemStackMergingStrategy {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Self::Ignore => "ignore",
            Self::FirstItem => "first_item",
            Self::Separate => "separate",
        };
        write!(f, "shulkerboxtooltip.config.item_stack_merging_strategy.{name}")
    }
}

/// The ordering strategy used for compact preview rendering.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompactPreviewOrder {
    /// Sort compact preview items by stack size, largest first.
    StackSize,
    /// Preserve the order of items as they appear in the container preview.
    PreviewOrder,
}

impl core::fmt::Display for CompactPreviewOrder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Self::StackSize => "stack_size",
            Self::PreviewOrder => "preview_order",
        };
        write!(f, "shulkerboxtooltip.config.compact_preview_order.{name}")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PreviewPosition {
    Inside,
    Outside,
    OutsideTop,
    OutsideBottom,
}

impl core::fmt::Display for PreviewPosition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Self::Inside => "inside",
            Self::Outside => "outside",
            Self::OutsideTop => "outside_top",
            Self::OutsideBottom => "outside_bottom",
        };
        write!(f, "shulkerboxtooltip.config.preview_position.{name}")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShulkerBoxTooltipType {
    Vanilla,
    Mod,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LootTableInfoType {
    Hide,
    Simple,
    Advanced,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnderChestSyncType {
    /// No synchronization, prevents clients from seeing a preview of their ender chest.
    None,
    /// Ender chest contents are synchronized when changed.
    Active,
    /// Ender chest contents are synchronized when the client opens a preview.
    Passive,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    ShulkerBoxTooltip,
    Vanilla,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BundleTheme {
    /// Fully disable ShulkerBoxTooltip's changes to bundles.
    Vanilla,
    /// Mimics vanilla theme, with enhancements like no size limit and preview locking.
    VanillaPlus,
    /// ShulkerBoxTooltip's custom look and feel.
    ShulkerBoxTooltip,
}

/// The type of preview to draw.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PreviewType {
    /// Preview is not present.
    NoPreview,
    /// Compact mode: similar items are grouped together and empty slots are not shown.
    Compact,
    /// Full mode: all stacks are shown in their respective slots, empty slots are also displayed.
    Full,
}

/// A key name as stored in the controls category, e.g. `key.keyboard.left.shift`.
pub type Key = &'static str;

pub const DEFAULT_PREVIEW_KEY: Key = "key.keyboard.left.shift";
pub const DEFAULT_FULL_PREVIEW_KEY: Key = "key.keyboard.left.alt";
pub const DEFAULT_LOCK_TOOLTIP_KEY: Key = "key.keyboard.left.control";

#[derive(Clone, Debug)]
pub struct PreviewCategory {
    /// Toggles the shulker box preview.
    pub enable: bool,
    /// Swaps the preview modes: if true, pressing the preview key shows the full preview.
    pub swap_modes: bool,
    /// If on, the preview is always displayed, regardless of the preview key being pressed.
    pub always_on: bool,
    /// How stacks with the same id but different component data are compacted.
    pub compact_preview_nbt_behavior: ItemStackMergingStrategy,
    /// How compact preview items are ordered.
    pub compact_preview_order: CompactPreviewOrder,
    /// The max number of items in a row.
    pub default_max_row_size: i32,
    /// If on, the client sends packets to servers for extra preview info (ender chest previews).
    pub server_integration: bool,
    /// The theme used for preview windows.
    pub theme: Theme,
    /// The theme used for bundle preview windows.
    pub theme_bundle: BundleTheme,
    /// The position of the preview window.
    pub position: PreviewPosition,
    /// If on, large item counts in compact previews are shortened.
    pub short_item_counts: bool,
    /// If on, unsupported items with the container data component still show a preview.
    pub generic_container_preview: bool,
}

#[derive(Clone, Debug)]
pub struct TooltipCategory {
    /// Controls whether the key hints in the container's tooltip are displayed.
    pub show_key_hints: bool,
    /// The tooltip to use: vanilla, the mod's, or none.
    pub r#type: ShulkerBoxTooltipType,
    /// How info about the stack's loot table shows up (vanilla shows `???????`).
    pub loot_table_info_type: LootTableInfoType,
    /// If on, the mod hides the custom text on shulker box tooltips.
    pub hide_shulker_box_lore: bool,
}

#[derive(Clone, Debug)]
pub struct ServerCategory {
    /// If on, the server provides extra information about containers to clients.
    pub client_integration: bool,
    /// How ender chest content previews are synchronized.
    pub ender_chest_sync_type: EnderChestSyncType,
}

#[derive(Clone, Debug)]
pub struct ColorsCategory {
    /// Controls whether the preview window is colored.
    pub colored_preview: bool,
}

#[derive(Clone, Debug)]
pub struct ControlsCategory {
    /// Press when hovering a container stack to open the preview window.
    pub preview_key: Key,
    /// Press when hovering a container stack to open the full preview window.
    pub full_preview_key: Key,
    /// Hold when previewing a stack to lock the tooltip.
    pub lock_tooltip_key: Key,
}

#[derive(Clone, Debug)]
pub struct Configuration {
    pub preview: PreviewCategory,
    pub tooltip: TooltipCategory,
    pub server: ServerCategory,
    pub colors: ColorsCategory,
    pub controls: ControlsCategory,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            preview: PreviewCategory {
                enable: true,
                swap_modes: false,
                always_on: false,
                compact_preview_nbt_behavior: ItemStackMergingStrategy::Separate,
                compact_preview_order: CompactPreviewOrder::StackSize,
                default_max_row_size: 9,
                server_integration: true,
                theme: Theme::ShulkerBoxTooltip,
                theme_bundle: BundleTheme::Vanilla,
                position: PreviewPosition::Inside,
                short_item_counts: true,
                generic_container_preview: true,
            },
            tooltip: TooltipCategory {
                show_key_hints: true,
                r#type: ShulkerBoxTooltipType::Mod,
                loot_table_info_type: LootTableInfoType::Hide,
                hide_shulker_box_lore: false,
            },
            server: ServerCategory {
                client_integration: true,
                ender_chest_sync_type: EnderChestSyncType::Active,
            },
            colors: ColorsCategory { colored_preview: true },
            controls: ControlsCategory {
                preview_key: DEFAULT_PREVIEW_KEY,
                full_preview_key: DEFAULT_FULL_PREVIEW_KEY,
                lock_tooltip_key: DEFAULT_LOCK_TOOLTIP_KEY,
            },
        }
    }
}
}
pub mod ender_cache {
// Client-side ender chest cache, ported from impl/provider/EnderChestCache.java
// (MIT, MisterPeModder/ShulkerBoxTooltip @68e7f8e). The per-world disk
// persistence (singleplayer `.dat` / multiplayer entries file keyed by server
// ip) is a host seam; the in-memory cache logic is exact.

use super::item::Stack;

#[derive(Default)]
pub struct EnderChestCache {
    /// The cached ender chest inventory for the current world/server, as
    /// sparse (slot, stack) entries in upstream `ItemStackWithSlot` form.
    cached_inventory: Option<Vec<(u32, Stack)>>,
}

impl EnderChestCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// True if there is a cached inventory for the current world/server.
    pub fn has_cached_inventory(&self) -> bool {
        self.cached_inventory.is_some()
    }

    /// True if the cache exists and contains at least one non-empty stack.
    pub fn has_cached_items(&self) -> bool {
        self.cached_inventory.as_ref().is_some_and(|entries| entries.iter().any(|(_, s)| !s.is_empty()))
    }

    /// The cached ender chest items for the current world/server, sized from
    /// the maximum slot index, or an empty list if there is no cache entry.
    pub fn get_items(&self) -> Vec<Option<Stack>> {
        let Some(entries) = &self.cached_inventory else {
            return Vec::new();
        };
        let mut size = 0usize;
        for (slot, _) in entries {
            if (*slot as usize) >= size {
                size = *slot as usize + 1;
            }
        }
        let mut items: Vec<Option<Stack>> = vec![None; size];
        for (slot, stack) in entries {
            items[*slot as usize] = Some(stack.clone());
        }
        items
    }

    /// Updates the cached inventory (upstream also saves it to disk).
    pub fn put_items(&mut self, inventory: Vec<(u32, Stack)>) {
        self.cached_inventory = Some(inventory);
    }

    /// Clears the cache (upstream `loadFromDisk` starts by dropping it).
    pub fn clear(&mut self) {
        self.cached_inventory = None;
    }
}
}
pub mod item {
// Item stacks and the compact-preview merge algorithm, ported 1:1 from
// impl/util/MergedItemStack.java + impl/util/ItemKey.java (MIT,
// MisterPeModder/ShulkerBoxTooltip @68e7f8e).
//
// The real item stack / component map lives in Horizon's registry, so this
// port models an opaque components blob; the merge math is exactly upstream.

use super::config::ItemStackMergingStrategy;

/// A dye color from `net.minecraft.world.item.DyeColor`, with the vanilla
/// texture diffuse colors (the source of `ColorKey::of_dye` inputs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DyeColor {
    White,
    Orange,
    Magenta,
    LightBlue,
    Yellow,
    Lime,
    Pink,
    Gray,
    LightGray,
    Cyan,
    Purple,
    Blue,
    Brown,
    Green,
    Red,
    Black,
}

impl DyeColor {
    /// Vanilla `DyeColor.getTextureDiffuseColor()`.
    pub fn texture_diffuse_color(self) -> i32 {
        match self {
            Self::White => 0xf9fffe,
            Self::Orange => 0xf9801d,
            Self::Magenta => 0xc74ebd,
            Self::LightBlue => 0x3ab3da,
            Self::Yellow => 0xfed83d,
            Self::Lime => 0x80c71f,
            Self::Pink => 0xf38baa,
            Self::Gray => 0x474f52,
            Self::LightGray => 0x9d9d97,
            Self::Cyan => 0x169c9c,
            Self::Purple => 0x8932b8,
            Self::Blue => 0x3c44aa,
            Self::Brown => 0x835432,
            Self::Green => 0x5e7c16,
            Self::Red => 0xb02e26,
            Self::Black => 0x1d1d21,
        }
    }
}

/// A stub of `net.minecraft.world.item.ItemStack` carrying exactly the fields
/// the ported logic reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stack {
    pub id: String,
    pub count: u32,
    /// Opaque component map (nbt/components), `None` when the stack has none.
    pub components: Option<Vec<u8>>,
    /// Contents of the `minecraft:container` data component, slot-indexed.
    pub container: Option<Vec<Option<Stack>>>,
    /// Dye color of shulker box / bundle stacks.
    pub dye: Option<DyeColor>,
    /// Whether the `minecraft:container_loot` data component is present.
    pub has_container_loot: bool,
}

impl Stack {
    pub fn new(id: &str, count: u32) -> Self {
        Self { id: id.to_string(), count, ..Default::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty() || self.count == 0
    }
}

/// Upstream `ItemStack.EMPTY`.
pub static EMPTY: Stack = Stack { id: String::new(), count: 0, components: None, container: None, dye: None, has_container_loot: false };

/// Used as a key in maps, ported from `impl/util/ItemKey.java`.
#[derive(Debug)]
pub struct ItemKey {
    item_id: String,
    /// `None` when components are ignored or the stack has none.
    components: Option<Vec<u8>>,
    ignore_components: bool,
}

impl PartialEq for ItemKey {
    fn eq(&self, other: &Self) -> bool {
        self.item_id == other.item_id
            && (self.ignore_components || self.components == other.components)
    }
}
impl Eq for ItemKey {}
impl core::hash::Hash for ItemKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.item_id.hash(state);
        if !(self.ignore_components || self.components.is_none()) {
            self.components.hash(state);
        }
    }
}

impl ItemKey {
    pub fn new(stack: &Stack, ignore_components: bool) -> Self {
        Self {
            item_id: stack.id.clone(),
            components: stack.components.clone(),
            ignore_components,
        }
    }
}

/// A group of equal stacks merged for the compact preview.
#[derive(Clone, Debug)]
pub struct MergedItemStack {
    merged: Stack,
    sub_items: Vec<Stack>,
    first_slot: i32,
}

impl MergedItemStack {
    pub fn new(slot_count: usize) -> Self {
        Self { merged: EMPTY.clone(), sub_items: vec![EMPTY.clone(); slot_count], first_slot: i32::MAX }
    }

    pub fn get(&self) -> &Stack {
        &self.merged
    }

    /// Add the passed stack into the item list. Does not check if items are equal.
    pub fn add(&mut self, stack: &Stack, slot: i32, merging_strategy: ItemStackMergingStrategy) {
        if slot < 0 || slot as usize >= self.sub_items.len() {
            return;
        }
        self.sub_items[slot as usize] = stack.clone();
        if slot < self.first_slot {
            self.first_slot = slot;
        }
        if self.merged.is_empty() {
            self.merged = if merging_strategy == ItemStackMergingStrategy::Ignore {
                copy_stack_without_components(stack)
            } else {
                stack.clone()
            };
        } else {
            self.merged.count += stack.count;
        }
    }

    pub fn get_sub_stack(&self, slot: i32) -> &Stack {
        self.sub_items.get(slot as usize).unwrap_or(&EMPTY)
    }

    pub fn size(&self) -> usize {
        self.sub_items.len()
    }

    pub fn get_first_slot(&self) -> i32 {
        self.first_slot
    }

    /// Upstream `compareTo`: stack count ascending, then larger first slot first.
    pub fn compare(&self, other: &Self) -> core::cmp::Ordering {
        use core::cmp::Ordering;
        let ret = self.merged.count as i64 - other.merged.count as i64;
        if ret != 0 {
            return ret.cmp(&0);
        }
        (other.first_slot).cmp(&self.first_slot).reverse()
    }
}

fn copy_stack_without_components(stack: &Stack) -> Stack {
    if stack.is_empty() {
        EMPTY.clone()
    } else {
        Stack { components: None, ..stack.clone() }
    }
}

/// Port of `MergedItemStack.mergeInventory`: groups equal stacks, keeps
/// first-encounter order, then sorts with the given comparator.
pub fn merge_inventory(
    inventory: &[Stack],
    max_size: usize,
    merging_strategy: ItemStackMergingStrategy,
    sort_by_stack_size: bool,
) -> Vec<MergedItemStack> {
    let mut items: Vec<MergedItemStack> = Vec::new();
    if inventory.is_empty() {
        return items;
    }

    let mut merged_stacks: std::collections::HashMap<ItemKey, usize> = std::collections::HashMap::new();

    for (i, s) in inventory.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let k = ItemKey::new(s, merging_strategy != ItemStackMergingStrategy::Separate);
        let merged = match merged_stacks.get(&k) {
            Some(&idx) => &mut items[idx],
            None => {
                items.push(MergedItemStack::new(max_size));
                merged_stacks.insert(k, items.len() - 1);
                items.last_mut().unwrap()
            }
        };
        merged.add(s, i as i32, merging_strategy);
    }

    if sort_by_stack_size {
        // STACK_SIZE order: Comparator.reverseOrder() over the natural order.
        items.sort_by(|a, b| b.compare(a));
    } else {
        // PREVIEW_ORDER: comparingInt(getFirstSlot).
        items.sort_by_key(|m| m.get_first_slot());
    }
    items
}
}
pub mod provider {
// Preview providers, ported from api/provider/* and impl/provider/*
// (MIT, MisterPeModder/ShulkerBoxTooltip @68e7f8e). The tooltip text and the
// renderer are host seams (see README); the display/slot/row-size logic is exact.

use super::color::{ColorKey, DEFAULT_CATEGORY, DEFAULT_ID, ENDER_CHEST_ID, GENERIC_CONTAINER_ID, SHULKER_BOX_ID};
use super::config::{BundleTheme, ItemStackMergingStrategy};
use super::item::{DyeColor, Stack, EMPTY};

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
        let config = super::config::Configuration::default();
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
    pub cache: &'a super::ender_cache::EnderChestCache,
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
        let config = super::config::Configuration::default();
        config.preview.theme_bundle == BundleTheme::ShulkerBoxTooltip
    }
}
}
#[cfg(test)]
mod tests {
use super::color::{self, components_to_rgb, rgb_to_components, ColorKey, ColorRegistry, DEFAULT_CATEGORY};
use super::config::*;
use super::ender_cache::EnderChestCache;
use super::item::*;
use super::provider::*;

#[test]
fn config_defaults_match_upstream() {
    let c = Configuration::default();
    assert!(c.preview.enable);
    assert!(!c.preview.swap_modes);
    assert!(!c.preview.always_on);
    assert_eq!(c.preview.compact_preview_nbt_behavior, ItemStackMergingStrategy::Separate);
    assert_eq!(c.preview.compact_preview_order, CompactPreviewOrder::StackSize);
    assert_eq!(c.preview.default_max_row_size, 9);
    assert!(c.preview.server_integration);
    assert_eq!(c.preview.theme, Theme::ShulkerBoxTooltip);
    assert_eq!(c.preview.theme_bundle, BundleTheme::Vanilla);
    assert_eq!(c.preview.position, PreviewPosition::Inside);
    assert!(c.preview.short_item_counts);
    assert!(c.preview.generic_container_preview);
    assert!(c.tooltip.show_key_hints);
    assert_eq!(c.tooltip.r#type, ShulkerBoxTooltipType::Mod);
    assert_eq!(c.tooltip.loot_table_info_type, LootTableInfoType::Hide);
    assert!(!c.tooltip.hide_shulker_box_lore);
    assert!(c.server.client_integration);
    assert_eq!(c.server.ender_chest_sync_type, EnderChestSyncType::Active);
    assert!(c.colors.colored_preview);
    assert_eq!(c.controls.preview_key, "key.keyboard.left.shift");
    assert_eq!(c.controls.full_preview_key, "key.keyboard.left.alt");
    assert_eq!(c.controls.lock_tooltip_key, "key.keyboard.left.control");
    // Upstream lang-key strings are carried exactly.
    assert_eq!(ItemStackMergingStrategy::FirstItem.to_string(), "shulkerboxtooltip.config.item_stack_merging_strategy.first_item");
    assert_eq!(CompactPreviewOrder::PreviewOrder.to_string(), "shulkerboxtooltip.config.compact_preview_order.preview_order");
    assert_eq!(PreviewPosition::OutsideTop.to_string(), "shulkerboxtooltip.config.preview_position.outside_top");
}

fn stacks() -> Vec<Stack> {
    vec![
        Stack::new("minecraft:dirt", 32),
        Stack::new("minecraft:stone", 3),
        Stack::new("minecraft:dirt", 5),
        EMPTY.clone(),
        Stack::new("minecraft:stone", 1),
    ]
}

#[test]
fn merge_inventory_separate_keeps_component_variants_apart() {
    let mut a = Stack::new("minecraft:dirt", 2);
    a.components = Some(vec![1]);
    let b = Stack::new("minecraft:dirt", 3);
    let inv = vec![a, b];
    let merged = merge_inventory(&inv, 27, ItemStackMergingStrategy::Separate, true);
    assert_eq!(merged.len(), 2);
}

#[test]
fn merge_inventory_ignore_and_first_item_merge_components() {
    let mut a = Stack::new("minecraft:dirt", 2);
    a.components = Some(vec![1]);
    let b = Stack::new("minecraft:dirt", 3);
    let merged = merge_inventory(&[a, b], 27, ItemStackMergingStrategy::Ignore, true);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].get().count, 5);
    // IGNORE drops the components entirely.
    assert!(merged[0].get().components.is_none());
    // FIRST_ITEM keeps the first item's components but still merges.
    let mut c = Stack::new("minecraft:dirt", 2);
    c.components = Some(vec![7]);
    let d = Stack::new("minecraft:dirt", 3);
    let merged = merge_inventory(&[c, d], 27, ItemStackMergingStrategy::FirstItem, true);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].get().components, Some(vec![7]));
}

#[test]
fn merge_inventory_compact_order_and_slots() {
    let merged = merge_inventory(&stacks(), 27, ItemStackMergingStrategy::Separate, true);
    // STACK_SIZE: dirt (37) first, then stone (4).
    assert_eq!(merged[0].get().id, "minecraft:dirt");
    assert_eq!(merged[0].get().count, 37);
    assert_eq!(merged[1].get().id, "minecraft:stone");
    assert_eq!(merged[1].get().count, 4);
    assert_eq!(merged[1].get_first_slot(), 1);
    assert_eq!(merged[1].get_sub_stack(1).id, "minecraft:stone");
    assert_eq!(merged[1].get_sub_stack(4).id, "minecraft:stone");
    assert_eq!(merged[1].get_sub_stack(3).id, "");
    // PREVIEW_ORDER: by first slot.
    let merged = merge_inventory(&stacks(), 27, ItemStackMergingStrategy::Separate, false);
    assert_eq!(merged[0].get().id, "minecraft:dirt");
    assert_eq!(merged[1].get().id, "minecraft:stone");
}

#[test]
fn dye_clamps_to_15_percent() {
    // Black 0x1d1d21: each channel < 0.15 clamps up to 0.15 -> 38/255 -> 0x26.
    let key = ColorKey::of_dye(DEFAULT_CATEGORY, "black_shulker_box", DyeColor::Black);
    assert_eq!(key.rgb(), 0x262626);
    // Orange 0xf9801d: r and g survive, the blue channel (29/255 = 0.114)
    // clamps up to 0.15 -> 38/255.
    let key = ColorKey::of_dye(DEFAULT_CATEGORY, "orange_shulker_box", DyeColor::Orange);
    assert_eq!(key.rgb(), 0xf98026);
    // Component conversion of a float-exact input.
    assert_eq!(components_to_rgb([0.0, 0.5, 1.0]), 0x007fff);
    assert_eq!(rgb_to_components(0x976797)[0], 151.0 / 255.0);
}

#[test]
fn color_registry_holds_defaults() {
    let mut reg = ColorRegistry::new();
    color::register_defaults(&mut reg);
    assert_eq!(reg.key(DEFAULT_CATEGORY, "default").unwrap().rgb(), 0xffffff);
    assert_eq!(reg.key(DEFAULT_CATEGORY, "ender_chest").unwrap().rgb(), 0x0b4b41);
    assert_eq!(reg.key(DEFAULT_CATEGORY, "shulker_box").unwrap().rgb(), 0x976797);
    assert_eq!(reg.key(DEFAULT_CATEGORY, "bundle").unwrap().rgb(), 0x844b2b);
    assert!(reg.key(DEFAULT_CATEGORY, "lime_shulker_box").is_some());
    assert!(reg.key(DEFAULT_CATEGORY, "lime_bundle").is_some());
    assert!(reg.key(DEFAULT_CATEGORY, "nope").is_none());
}

#[test]
fn generic_container_and_shulker_providers() {
    let mut stack = Stack::new("minecraft:shulker_box", 1);
    let mut dirt = Stack::new("minecraft:dirt", 10);
    dirt.components = Some(vec![9]);
    stack.container = Some(vec![Some(dirt), None, None]);
    let ctx = PreviewContext { stack: &stack };

    let generic = GenericContainerPreviewProvider;
    let shulker = ShulkerBoxPreviewProvider;
    assert!(generic.should_display(&ctx));
    assert!(shulker.should_display(&ctx));
    assert_eq!(generic.get_inventory_max_size(&ctx), 27);
    assert_eq!(shulker.get_max_row_size(&ctx), 9);
    assert_eq!(shulker.get_window_color_key_id(&ctx), "shulker_box");
    // Dyed shulker boxes map their dye color.
    stack.dye = Some(DyeColor::Cyan);
    let ctx = PreviewContext { stack: &stack };
    assert_eq!(shulker.get_window_color_key_id(&ctx), "cyan_shulker_box");
    // Container loot hides the preview.
    stack.dye = None;
    stack.has_container_loot = true;
    let ctx = PreviewContext { stack: &stack };
    assert!(!generic.should_display(&ctx));
    assert!(!shulker.should_display(&ctx));
}

#[test]
fn empty_container_stack_does_not_display() {
    let stack = Stack::new("minecraft:barrel", 1);
    let ctx = PreviewContext { stack: &stack };
    assert!(!GenericContainerPreviewProvider.should_display(&ctx));
    assert!(EmptyPreviewProvider.should_display(&ctx) == false);
}

#[test]
fn bundle_provider_follows_theme_gate() {
    let mut stack = Stack::new("minecraft:bundle", 1);
    stack.container = Some(vec![Some(Stack::new("minecraft:stick", 4))]);
    let ctx = PreviewContext { stack: &stack };
    let bundle = BundlePreviewProvider;
    // Default theme for bundles is VANILLA: no preview at all.
    assert!(!bundle.should_display(&ctx));
    assert!(!bundle.theme_is_mod());
}

#[test]
fn ender_chest_cache_sizes_from_max_slot() {
    let mut cache = EnderChestCache::new();
    assert!(!cache.has_cached_inventory());
    assert!(!cache.has_cached_items());
    cache.put_items(vec![(5, Stack::new("minecraft:diamond", 1))]);
    assert!(cache.has_cached_inventory());
    assert!(cache.has_cached_items());
    let items = cache.get_items();
    assert_eq!(items.len(), 6);
    assert_eq!(items[5].as_ref().unwrap().id, "minecraft:diamond");
    assert!(items[0].is_none());
    cache.clear();
    assert!(!cache.has_cached_inventory());
}

#[test]
fn registry_picks_highest_priority() {
    let stack = Stack::new("minecraft:x", 1);
    let ctx = PreviewContext { stack: &stack };
    let generic = GenericContainerPreviewProvider;
    let empty = EmptyPreviewProvider;
    let best = best_provider(&[&empty as &dyn PreviewProvider, &generic], &ctx).unwrap();
    assert_eq!(best.get_priority(), 0);
}
}
// ShulkerBoxTooltip for Horizon: shows a preview of a container's contents
// when hovering the container item in an inventory.
//
// Ported 1:1 where possible from MisterPeModder/ShulkerBoxTooltip (MIT,
// Yanis Guaye) @68e7f8e. The full logic core is here: the configuration
// model with upstream defaults, the color registry with the dyed
// shulker-box/bundle keys, the compact-preview merge algorithm
// (ItemStackMergingStrategy + CompactPreviewOrder), the preview providers
// (generic container, shulker box, bundle, ender chest via the client cache)
// and the ender chest cache. The tooltip rendering, the preview keybinds and
// the ender-chest sync packets are host seams, documented in the README.


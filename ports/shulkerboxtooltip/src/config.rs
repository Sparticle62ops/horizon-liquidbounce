//! Configuration model, ported 1:1 from
//! com.misterpemodder.shulkerboxtooltip.impl.config.Configuration (MIT,
//! MisterPeModder/ShulkerBoxTooltip @68e7f8e). Field names and defaults match
//! upstream; the jankson JSON serializer and the config GUI are host seams.

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

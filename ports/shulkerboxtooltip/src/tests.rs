use crate::color::{self, components_to_rgb, rgb_to_components, ColorKey, ColorRegistry, DEFAULT_CATEGORY};
use crate::config::*;
use crate::ender_cache::EnderChestCache;
use crate::item::*;
use crate::provider::*;

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

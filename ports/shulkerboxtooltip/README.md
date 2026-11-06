# ShulkerBoxTooltip for Horizon (Rust port)

Ported 1:1 from [MisterPeModder/ShulkerBoxTooltip](https://github.com/MisterPeModder/ShulkerBoxTooltip) (MIT, (c) 2019 Yanis Guaye) @ 68e7f8e.
Shows a preview of a container's contents when hovering the container item in an inventory.

## What is ported (the full logic core)

- `src/config.rs` - the complete configuration model with upstream defaults: preview category (enable, swap_modes, always_on, compact_preview_nbt_behavior = Separate, compact_preview_order = StackSize, default_max_row_size = 9, server_integration, theme, theme_bundle, position, short_item_counts, generic_container_preview), tooltip category (show_key_hints, type = Mod, loot_table_info_type = Hide, hide_shulker_box_lore), server category (client_integration, ender_chest_sync_type = Active), colors (colored_preview) and controls (preview/full-preview/lock-tooltip keys). Enum Display strings are the upstream lang keys, carried verbatim.
- `src/color.rs` - ColorKey/ColorKeyImpl (float rgb components + default components), ColorRegistryImpl categories, rgbToComponents/componentsToRgb, ColorKey.ofDye with the 0.15 per-channel clamp, and the default key set (default, ender_chest, generic_container, shulker_box, bundle + all 16 dyed shulker box / bundle keys).
- `src/item.rs` - ItemKey (item id + component map, with the upstream asymmetric hashing rule), MergedItemStack (add/get_sub_stack/first_slot/compareTo) and mergeInventory with all three ItemStackMergingStrategy values and both CompactPreviewOrder sorts.
- `src/provider.rs` - the PreviewProvider trait with upstream defaults, BlockEntityPreviewProvider (loot-table gating, container component read, row sizes), GenericContainerPreviewProvider (27 slots, priority 0), ShulkerBoxPreviewProvider (dye -> ColorKey mapping), BundlePreviewProvider (theme gating, 64-slot tail placement, selected-slot mapping), EnderChestPreviewProvider (cache-backed), EmptyPreviewProvider, and priority-ordered registry lookup.
- `src/ender_cache.rs` - EnderChestCache: sparse (slot, stack) entries, has_cached_inventory/has_cached_items, slot-indexed get_items, put_items.

Tests: 11 tests (`cargo test -p horizon-mixin-shulkerboxtooltip` inside the Horizon workspace), all passing.

## Host seams (need Horizon native-mod API support; issues filed)

1. **Tooltip / preview-window rendering** - upstream impl/gui/* + impl/tooltip/* draws the preview window in the item tooltip, handles compact/full modes, key hint lines and the theme. Horizon has no client item-tooltip render hook; the render half is not ported.
2. **Preview keybinds** - the preview / full-preview / lock-tooltip keys (Controls category) need client keybind registration.
3. **Ender chest sync packets** - upstream syncs ender chest contents C2S (EnderChestSyncType Active/Passive) so previews stay fresh on servers; needs Horizon client/server networking. The client-side cache itself is ported.
4. **Item component maps** - real `minecraft:container` / `minecraft:container_loot` data components come from the host registry; the port models them as opaque blobs. A matching record-level API is required.

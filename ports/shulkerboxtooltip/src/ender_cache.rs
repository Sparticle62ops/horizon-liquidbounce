//! Client-side ender chest cache, ported from impl/provider/EnderChestCache.java
//! (MIT, MisterPeModder/ShulkerBoxTooltip @68e7f8e). The per-world disk
//! persistence (singleplayer `.dat` / multiplayer entries file keyed by server
//! ip) is a host seam; the in-memory cache logic is exact.

use crate::item::Stack;

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

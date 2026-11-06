//! Item stacks and the compact-preview merge algorithm, ported 1:1 from
//! impl/util/MergedItemStack.java + impl/util/ItemKey.java (MIT,
//! MisterPeModder/ShulkerBoxTooltip @68e7f8e).
//!
//! The real item stack / component map lives in Horizon's registry, so this
//! port models an opaque components blob; the merge math is exactly upstream.

use crate::config::ItemStackMergingStrategy;

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

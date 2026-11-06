//! ShulkerBoxTooltip for Horizon: shows a preview of a container's contents
//! when hovering the container item in an inventory.
//!
//! Ported 1:1 where possible from MisterPeModder/ShulkerBoxTooltip (MIT,
//! Yanis Guaye) @68e7f8e. The full logic core is here: the configuration
//! model with upstream defaults, the color registry with the dyed
//! shulker-box/bundle keys, the compact-preview merge algorithm
//! (ItemStackMergingStrategy + CompactPreviewOrder), the preview providers
//! (generic container, shulker box, bundle, ender chest via the client cache)
//! and the ender chest cache. The tooltip rendering, the preview keybinds and
//! the ender-chest sync packets are host seams, documented in the README.

pub mod color;
pub mod config;
pub mod ender_cache;
pub mod item;
pub mod provider;
pub mod tests;

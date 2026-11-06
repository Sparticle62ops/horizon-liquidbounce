//! LiquidBounce (Horizon Rust port), slice 1: the feature core.
//!
//! 1:1 port of the upstream module-system core (nextgen branch, GPL-3.0):
//! ModuleCategory, the Value/settings system, ClientModule and ModuleManager.
//! Game-facing calls (world access, rendering) land in later slices behind the
//! same seams upstream uses; the logic model here is exact.
pub mod category;
pub mod module;
pub mod module_manager;
pub mod value;

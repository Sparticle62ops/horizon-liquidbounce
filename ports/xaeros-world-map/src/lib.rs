//! xaeros-world-map runtime (behavioral port of xaero96/Xaero's World Map,
//! All Rights Reserved; no upstream code is included).
//!
//! Ported 1:1 against the behavior the mod's page documents: the self-writing
//! fullscreen map (M to open, mouse pan, wheel zoom), multiple maps per server
//! address with manual or automatic switching, cave-dimension layers, the
//! vanilla-maps mode and terrain shading flags, and "footsteps" showing the
//! most recent path taken. The waypoint overlay rides the xaeros-minimap
//! port's waypoint store; rendering and world reads ride Horizon (seams in the
//! README).

pub mod footsteps;
pub mod view;
pub mod world;

/// The mod's client settings model with the page's documented toggles.
#[derive(Clone, Debug)]
pub struct Config {
    pub vanilla_maps_mode: bool,
    pub terrain_shading: bool,
    pub auto_world_detection: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { vanilla_maps_mode: false, terrain_shading: true, auto_world_detection: false }
    }
}

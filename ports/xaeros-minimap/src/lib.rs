//! xaeros-minimap runtime (behavioral port of xaero96/Xaero's Minimap,
//! All Rights Reserved; no upstream code is included).
//!
//! Ported 1:1 against the behavior the mod's page documents: the vanilla-styled
//! widget (square or circle shape), multiple sizes including automatic, zoom
//! levels, any-pixel positioning, lock-north with the direction arrow,
//! automatic cave mode with the "solid roof" search, waypoints per world and
//! dimension, the entity radar, the vanilla/accurate color modes, and the
//! enlarge keybind's behaviors. The rendering and world reads ride Horizon;
//! the seams are in the README.

pub mod cave;
pub mod config;
pub mod map;
pub mod radar;
pub mod waypoint;

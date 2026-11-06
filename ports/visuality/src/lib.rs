//! Visuality for Horizon: logic core of PinkGoosik/visuality (MIT), ported 1:1.
//! Spawn rules, registries, config and event gates live here; particle
//! rendering waits on a Horizon client particle surface.

pub mod circles;
pub mod colors;
pub mod config;
pub mod ids;
pub mod particles;
pub mod registries;
pub mod rng;
pub mod rules;
pub mod sparkles;

#[cfg(test)]
mod tests;

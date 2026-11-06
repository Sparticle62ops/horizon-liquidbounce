//! sound-physics-remastered runtime for Horizon (slice 1): the pure sound
//! physics, per-block materials, direct-path occlusion raymarching and
//! raytraced reverb estimation. Ported behaviorally from Sound Physics
//! Remastered (GPL-3.0-only). The client audio-path glue lands with the
//! Horizon sound surface; world access rides issue #10152's snapshots.

pub mod materials;
pub mod occlusion;
pub mod reverb;

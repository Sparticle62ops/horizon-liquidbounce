//! replaymod runtime for Horizon (slice 1): the recording data model, the
//! playback state machine, and camera paths. Ported behaviorally from
//! ReplayMod (ReplayMod/ReplayMod, GPL-3.0-or-later); render-to-video and the
//! GUI surface ride upstream issue #10025.

pub mod camera;
pub mod playback;
pub mod recording;

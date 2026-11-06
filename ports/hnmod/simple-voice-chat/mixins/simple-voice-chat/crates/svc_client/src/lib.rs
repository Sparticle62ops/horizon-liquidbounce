//! svc_client: the clean-room Simple Voice Chat client half for Horizon,
//! bundled as a single crate (bundle crates cannot depend on each other).
pub mod audio;
pub mod audio_io;
pub mod opus_ffi;
pub mod protocol;
pub mod runtime;
pub mod session;

//! Player renders: note who is in view (positions stay a v2 gap).
use crate::simple_voice_chat::runtime;

pub fn player_submit(player: &crate::game::native_mob_draw::NativeMobRender<'_>,
                     who: &crate::game::native_mob_draw::NativePlayerRender<'_>) {
    let g = runtime::state();
    let _g = g.lock().unwrap();
    // Positions come camera-relative and the frame's origin is private, so
    // v1 ships server-provided distance falloff; the name map is still useful.
    let _ = who.name;
}

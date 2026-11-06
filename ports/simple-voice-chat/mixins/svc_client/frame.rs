//! Every frame: drive the session, surface notes in chat, send pending
//! mod-channel packets, and re-handshake after a dropped connection.
use crate::game::native_hooks::Game;
use crate::simple_voice_chat::runtime;

pub fn frame(game: &mut Game<'_>, _elapsed: &std::time::Duration) {
    let g = runtime::state();
    let mut g = g.lock().unwrap();
    let _ = g.on_frame(std::time::Instant::now());
    let pending = std::mem::take(&mut g.pending_sends);
    for (channel, body) in pending {
        let _ = game.native_net_send_to_server(&channel, &body);
    }
    if g.reconnect_pending {
        g.reconnect_pending = false;
        let (channel, body) = runtime::Glue::handshake();
        let _ = game.native_net_send_to_server(&channel, &body);
    }
    // Notes surface through the HUD layer hook (v2); they stay queued here.
}

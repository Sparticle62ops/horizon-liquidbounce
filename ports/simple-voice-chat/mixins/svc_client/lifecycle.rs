//! World lifecycle: open sends the handshake, close tears the session down.
use crate::game::native_hooks::Game;
use crate::simple_voice_chat::runtime;

pub fn world_lifecycle(game: &mut Game<'_>, phase: &crate::game::native_hooks::WorldPhase) {
    let g = runtime::state();
    let mut g = g.lock().unwrap();
    match phase {
        crate::game::native_hooks::WorldPhase::WorldOpenReady => {
            g.world_open();
            let (channel, body) = runtime::Glue::handshake();
            let _ = game.native_net_send_to_server(&channel, &body);
        }
        crate::game::native_hooks::WorldPhase::WorldOpenFailed
        | crate::game::native_hooks::WorldPhase::WorldCloseFailed
        | crate::game::native_hooks::WorldPhase::WorldCloseFinal => g.world_close(),
    }
}

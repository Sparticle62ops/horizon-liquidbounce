// Game-side module for the Simple Voice Chat port. Thin shell over the
// svc_client bundle crate: protocol, session and audio live there; this
// module bridges Horizon's hooks to it and queues outbound packets for
// the net send path.

use std::sync::Mutex;
use std::time::Instant;

pub use svc_client::runtime::{state, Glue};

static OUT: Mutex<Vec<(String, Vec<u8>)>> = Mutex::new(Vec::new());

fn locked() -> std::sync::MutexGuard<'static, Glue> {
    state().lock().unwrap_or_else(|e| e.into_inner())
}

pub fn queue_outbound(channel: String, body: Vec<u8>) {
    OUT.lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((channel, body));
}

pub fn drain_outbound() -> Vec<(String, Vec<u8>)> {
    std::mem::take(&mut *OUT.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn world_open_ready() {
    let mut g = locked();
    g.world_open();
    let (channel, body) = Glue::handshake();
    queue_outbound(channel, body);
    let _ = g.start_audio();
}

pub fn world_closed() {
    locked().world_close();
}

pub fn world_lifecycle(
    _game: &mut crate::game::Game<'_>,
    phase: &crate::game::native_lifecycle::WorldPhase,
) {
    use crate::game::native_lifecycle::WorldPhase::{WorldCloseFailed, WorldCloseFinal, WorldOpenReady};
    match phase {
        WorldOpenReady => world_open_ready(),
        WorldCloseFinal | WorldCloseFailed => world_closed(),
        _ => {}
    }
}

pub fn on_net(channel: &str, data: &[u8]) {
    locked().on_net(channel, data, None, Instant::now());
}

pub fn on_player_submit(
    _player: &crate::game::native_mob_draw::NativeMobRender<'_>,
    _who: &crate::game::native_mob_draw::NativePlayerRender<'_>,
) {
    // Per-player voice indicators are wired in a later stage.
}

pub fn on_key(event: &winit::event::KeyEvent, _modifiers: u32) {
    let code = match event.physical_key {
        winit::keyboard::PhysicalKey::Code(k) => format!("{k:?}"),
        _ => return,
    };
    let pressed = event.state == winit::event::ElementState::Pressed;
    locked().on_key(&code, pressed);
}

pub fn on_command(command: &str, ran: &mut bool) {
    let (feedback, outbound) = locked().on_command(command);
    if !feedback.is_empty() {
        eprintln!("{feedback}");
    }
    for (channel, body) in outbound {
        queue_outbound(channel, body);
    }
    *ran = true;
}

pub fn on_frame(_game: &mut crate::game::Game<'_>, _elapsed: std::time::Duration) {
    let _ = locked().on_frame(Instant::now());
}

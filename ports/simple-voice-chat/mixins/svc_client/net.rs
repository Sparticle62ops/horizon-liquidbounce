//! Mod messages from the server: the handshake reply, states, group updates.
use crate::simple_voice_chat::runtime;

pub fn net_from_server(channel: &str, data: &[u8]) {
    let g = runtime::state();
    let mut g = g.lock().unwrap();
    let hint = g.server_addr.map(|a| a.ip().to_string());
    g.on_net(channel, data, hint.as_deref(), std::time::Instant::now());
}

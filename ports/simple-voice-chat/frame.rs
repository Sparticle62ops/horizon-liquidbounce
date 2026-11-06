// Runs at the head of the frame hook (listen). Drains the module's outbound
// mod-channel queue onto the server (the 32-a-tick cap lives in the send path;
// send errors like "no mod messages" on vanilla servers are dropped).
fn frame(game: &mut crate::game::Game<'_>, elapsed: std::time::Duration) {
    crate::game::simple_voice_chat::on_frame(game, elapsed);
    for (channel, data) in crate::game::simple_voice_chat::drain_outbound() {
        let _ = game.native_net_send_to_server(&channel, &data);
    }
}

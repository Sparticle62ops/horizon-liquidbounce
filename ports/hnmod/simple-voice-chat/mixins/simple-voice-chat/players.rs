// Runs at the head of the player submit hook (cancel contract, out drawn).
fn player_submit(
    player: &crate::game::native_mob_draw::NativeMobRender<'_>,
    who: &crate::game::native_mob_draw::NativePlayerRender<'_>,
) {
    crate::game::simple_voice_chat::on_player_submit(player, who);
}

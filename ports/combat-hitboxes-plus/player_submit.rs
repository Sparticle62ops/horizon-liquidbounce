fn combat_hitboxes_on_player_submit(
    game: &crate::game::Game<'_>,
    player: &crate::mod_api::NativeMobRender<'_>,
) {
    game.combat_hitboxes_record(true, player.id, player.kind);
}

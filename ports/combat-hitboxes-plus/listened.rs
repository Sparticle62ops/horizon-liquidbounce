fn combat_hitboxes_on_player_render_listened(
    game: &crate::game::Game<'_>,
    listened: &mut bool,
) {
    game.combat_hitboxes_listen(listened);
}

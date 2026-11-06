fn litematica_on_screen_render_post(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client_render::hud::HudBatch,
    screen: &crate::mod_api::NativeScreenView<'_>,
) {
    game.litematica_menu_draw(batch, screen);
}

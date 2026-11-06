// The speed trade button on the merchant panel.
fn fast_trading_draw(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    screen: &crate::mod_api::NativeScreenView<'_>,
) {
    crate::game::fast_trading::draw(game, batch, screen)
}

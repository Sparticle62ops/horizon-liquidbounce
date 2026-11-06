// A press on the speed trade button starts the repeat.
fn fast_trading_click(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    crate::game::fast_trading::click(game, screen, button, canceled)
}

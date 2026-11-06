// The screen release hook: cancels the one release after a right drag.
fn mouse_tweaks_released(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    button: i32,
    modifiers: u32,
    canceled: &mut bool,
) {
    crate::game::mouse_tweaks::on_release_ev(game, screen, button, canceled)
}

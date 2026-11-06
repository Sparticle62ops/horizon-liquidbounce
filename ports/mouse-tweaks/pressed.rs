// The screen press hook: records the press for the frame executor.
fn mouse_tweaks_pressed(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    button: i32,
    modifiers: u32,
    canceled: &mut bool,
) {
    crate::game::mouse_tweaks::on_press_ev(game, screen, button, modifiers, canceled)
}

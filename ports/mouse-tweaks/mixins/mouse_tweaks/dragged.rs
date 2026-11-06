// The screen drag hook: records the drag for the frame executor.
fn mouse_tweaks_dragged(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    button: i32,
    delta: [f32; 2],
    modifiers: u32,
    canceled: &mut bool,
) {
    crate::game::mouse_tweaks::on_drag_ev(game, screen, button, delta, modifiers, canceled)
}

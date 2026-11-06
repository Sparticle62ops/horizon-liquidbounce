// The screen scroll hook: records the wheel for the frame executor.
fn mouse_tweaks_scrolled(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    scroll: [f64; 2],
    modifiers: u32,
    canceled: &mut bool,
) {
    crate::game::mouse_tweaks::on_scroll_ev(game, screen, scroll, modifiers, canceled)
}

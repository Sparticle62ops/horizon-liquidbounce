// The frame hook: drains the recorded events and runs the tweak logic.
fn mouse_tweaks_frame(game: &mut crate::game::Game<'_>, elapsed: std::time::Duration) {
    crate::game::mouse_tweaks::on_frame(game, elapsed)
}

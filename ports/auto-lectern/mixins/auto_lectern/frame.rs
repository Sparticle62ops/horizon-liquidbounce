// The farm's tick: advances with the frame clock.
fn auto_lectern_frame(game: &mut crate::game::Game<'_>, elapsed: std::time::Duration) {
    crate::game::auto_lectern::frame(game, elapsed)
}

// The speed trade timer: advances with the frame clock and repeats the
// selected trade while the button's phase runs.
fn fast_trading_frame(game: &mut crate::game::Game<'_>, elapsed: std::time::Duration) {
    crate::game::fast_trading::frame(game, &elapsed)
}

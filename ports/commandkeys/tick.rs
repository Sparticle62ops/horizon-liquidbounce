// The mod tick: drains the ratelimiter, sends due scheduled messages and
// keeps repeating macros going, like the original's afterClientTick.
fn tick(game: &mut Game<'_>) {
    crate::game::commandkeys::on_tick(game);
}

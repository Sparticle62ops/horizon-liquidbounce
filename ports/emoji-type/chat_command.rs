// The command hook: replaces `:code:` shortcodes with their emoji just before
// a `/` command is dispatched, matching the original's edit-box replacement.
fn chat_command(
    game: &mut Game<'_>,
    command: &str,
    original: impl FnOnce(&mut Game<'_>, &str) -> (),
) {
    let transformed = crate::game::emoji_type::transform(game, command);
    original(game, &transformed);
}

// The chat hook: replaces `:code:` shortcodes with their emoji just before the
// chat line is signed and sent to the server.
fn chat_send(
    game: &mut Game<'_>,
    message: &str,
    original: impl FnOnce(&mut Game<'_>, &str) -> (),
) {
    let transformed = crate::game::emoji_type::transform(game, message);
    original(game, &transformed);
}

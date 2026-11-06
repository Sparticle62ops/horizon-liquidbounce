// The key hook: records key state and triggers macro activation on press
// and action on release, like the original's KeyboardHandler.handleKey.
// The key hook: renamed so the helper does not shadow the native_key "key" parameter
fn commandkeys_key(game: &mut Game<'_>, key_name: &str, pressed: bool) {
    crate::game::commandkeys::on_key(game, key_name, pressed);
}

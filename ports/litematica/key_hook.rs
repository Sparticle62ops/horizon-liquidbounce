// Runs at the head of the input-key hook (after a real window key event
// finished handling, screens included).
fn litematica_on_input_key(
    game: &mut crate::game::Game<'_>,
    event: &winit::event::KeyEvent,
    modifiers: u32,
) {
    game.litematica_input_key(event, modifiers);
}

// Runs at the head of the world lifecycle hook (listen).
fn world_lifecycle(
    game: &mut crate::game::Game<'_>,
    phase: &crate::game::native_lifecycle::WorldPhase,
) {
    crate::game::simple_voice_chat::world_lifecycle(game, phase);
}

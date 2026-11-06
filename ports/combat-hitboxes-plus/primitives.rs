fn combat_hitboxes_on_world_primitives(
    game: &crate::game::Game<'_>,
    camera: [f64; 3],
    primitives: &mut crate::world_primitives::WorldPrimitives,
) {
    game.combat_hitboxes_draw(camera, primitives);
}

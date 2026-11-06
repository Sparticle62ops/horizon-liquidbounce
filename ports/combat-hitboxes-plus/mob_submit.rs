fn combat_hitboxes_on_mob_submit(
    game: &crate::game::Game<'_>,
    mob: &crate::mod_api::NativeMobRender<'_>,
) {
    game.combat_hitboxes_record(false, mob.id, mob.kind);
}

// The movement lock: the farm zeroes the walk inputs while it holds the
// player in place (the original's DummyInput).
fn auto_lectern_movement(
    forward: &mut f32,
    strafe: &mut f32,
    jump: &mut bool,
    sneak: &mut bool,
) {
    crate::game::auto_lectern::movement(forward, strafe, jump, sneak)
}

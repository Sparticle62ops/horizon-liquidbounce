// `/autolec` and `/autolectern`: the settings the original keeps in
// autolec.txt. The parsing lives in the runtime module (`command_entry`).
fn auto_lectern_command(
    game: &mut crate::game::Game<'_>,
    command: &str,
    ran: &mut bool,
) {
    crate::game::auto_lectern::command_entry(game, command, ran)
}

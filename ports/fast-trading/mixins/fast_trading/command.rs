// `/fasttrading` and `/ftrade`: the settings the original keeps in its
// config screen. The parsing lives in the runtime module (`command_entry`).
fn fast_trading_command(
    game: &mut crate::game::Game<'_>,
    command: &str,
    ran: &mut bool,
) {
    crate::game::fast_trading::command_entry(game, command, ran)
}

// `/autoreauth login | logout | status | reauth | auto <on|off>`, and
// `/reauth` as the short alias. Login runs in its own thread so the browser
// round-trip never freezes the game; the other actions answer in chat, which
// stands in for the original's config screen, overlay and toasts. The
// parsing lives in the runtime module (`command_entry`).
fn auto_reauth_command(
    game: &mut crate::game::Game<'_>,
    command: &str,
    ran: &mut bool,
) {
    crate::game::auto_reauth::command_entry(game, command, ran)
}

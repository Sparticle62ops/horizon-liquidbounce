// Runs at the head of the command hook (cancel contract, out ran).
fn command_handle(command: &str, ran: &mut bool) {
    crate::game::simple_voice_chat::on_command(command, ran);
}

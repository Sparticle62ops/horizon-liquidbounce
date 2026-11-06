//! /svc and /vcgroup commands.
use crate::simple_voice_chat::runtime;

pub fn command(command: &str, ran: &mut bool) {
    if !command.starts_with("svc") && !command.starts_with("vcgroup") {
        return;
    }
    *ran = true;
    let g = runtime::state();
    let mut g = g.lock().unwrap();
    let (reply, sends) = g.on_command(command);
    if !reply.is_empty() {
        g.notes.push(reply);
    }
    g.pending_sends.extend(sends); // the frame helper sends these next frame
}

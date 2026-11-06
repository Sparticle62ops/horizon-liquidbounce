//! Keys: push-to-talk and the mute toggle.
use crate::simple_voice_chat::runtime;
use winit::keyboard::{KeyCode, PhysicalKey};

fn code_name(code: PhysicalKey) -> &'static str {
    match code {
        PhysicalKey::Code(KeyCode::KeyV) => "KeyV",
        PhysicalKey::Code(KeyCode::KeyB) => "KeyB",
        PhysicalKey::Code(KeyCode::KeyM) => "KeyM",
        PhysicalKey::Code(KeyCode::KeyG) => "KeyG",
        _ => "",
    }
}

pub fn input_key(event: &winit::event::KeyEvent, _modifiers: &winit::keyboard::ModifiersState) {
    let name = code_name(event.physical_key);
    if name.is_empty() {
        return;
    }
    let g = runtime::state();
    let mut g = g.lock().unwrap();
    if let Some(note) = g.on_key(name, event.state.is_pressed()) {
        // The command helper surfaces the reply; a key toggle just notes it.
        g.notes.push(format!("[voice] {}", note));
    }
}

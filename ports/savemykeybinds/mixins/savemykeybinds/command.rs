// `/savemykeybinds save|load|delete <name>`, `/savemykeybinds list`, and
// `/smkb` as a shorter alias. The hook runs before the server and Horizon's
// commands see the line, so a preset load applies to the keybinds at once.
fn savemykeybinds_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    use crate::game::savemykeybinds as m;
    let mut words = command.split_whitespace();
    let head = words.next().unwrap_or("");
    if !game.native_on(m::ID) || !m::HEADS.contains(&head) {
        return;
    }
    let args: Vec<&str> = words.collect();
    let Some(action) = m::parse(head, &args) else {
        *ran = true;
        game.native_chat(m::USAGE);
        return;
    };
    *ran = true;
    let dir = match crate::game::Game::native_config_dir() {
        Ok(dir) => m::preset_dir(&dir),
        Err(_) => {
            game.native_chat("No config folder for keybind presets");
            return;
        }
    };
    match action {
        m::Action::List => game.native_chat(&m::list_line(&m::list_presets(&dir))),
        m::Action::Save(name) => {
            let bindings = game.settings.keys.saved();
            match m::save_preset(&dir, &name, &bindings) {
                Ok(()) => game.native_chat(&m::save_line(&name)),
                Err(_) => game.native_chat(&m::failed("save", &name)),
            }
        }
        m::Action::Load(name) => match m::load_preset(&dir, &name) {
            Ok(preset) => {
                let current = game.settings.keys.saved();
                for (binding, key) in m::apply(&current, &preset) {
                    let key = if known_key(&key) { key } else { m::UNKNOWN_KEY.to_owned() };
                    game.settings.keys.set(binding, &key);
                }
                game.native_chat(&m::load_line(&name));
            }
            Err(_) => game.native_chat(&m::failed("load", &name)),
        },
        m::Action::Delete(name) => match m::delete_preset(&dir, &name) {
            Ok(()) => game.native_chat(&m::delete_line(&name)),
            Err(_) => game.native_chat(&m::failed("delete", &name)),
        },
    }
}

/// A key the game knows: a keyboard key, a mouse button, or unbound.
fn known_key(key: &str) -> bool {
    horizon_client::keybinds::key_code(key).is_some()
        || horizon_client::keybinds::mouse_button(key).is_some()
        || key == crate::game::savemykeybinds::UNKNOWN_KEY
}

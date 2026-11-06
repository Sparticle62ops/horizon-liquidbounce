// Save My Keybinds for Horizon: save and load keybind presets.
//
// A Rust port of SaveMyKeybinds (Copyright (C) 2024-2026 MisterCheezeCake,
// LGPL-3.0-or-later, https://github.com/MisterCheezeCake/SaveMyKeybinds).
// The original's behavior is kept: presets live in a `savemykeybinds` folder
// in the config dir as `<name>.preset.json` and keep the original's JSON
// shape, so presets made with the original load here unchanged. Horizon
// drives it with chat commands instead of the options screen's buttons.
//
// This file is both the standalone Cargo library (its tests) and the module
// the bundle adds to the game.

use std::fs;
use std::path::Path;

/// The mod's id (its settings folder).
pub const ID: &str = "savemykeybinds";
pub const NAME: &str = "Save My Keybinds";
pub const VERSION: &str = "1.0.0";
/// The presets' folder under the config dir, as the original names it.
pub const FOLDER: &str = "savemykeybinds";
/// What an unbound key reads as.
pub const UNKNOWN_KEY: &str = "key.keyboard.unknown";

/// The command words the mod answers to.
pub const HEADS: [&str; 2] = ["savemykeybinds", "smkb"];

/// One line of how the command reads, shown when it is not understood.
pub const USAGE: &str = "Usage: /savemykeybinds save|load|delete <name>, list";

/// One `/savemykeybinds` action.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Save(String),
    Load(String),
    Delete(String),
    List,
}

/// Parse a command line without its slash into an action. `head` is the
/// command's first word, `args` the rest.
pub fn parse(head: &str, args: &[&str]) -> Option<Action> {
    if !HEADS.contains(&head) {
        return None;
    }
    match args.first().copied() {
        None => Some(Action::List),
        Some("save") => name(args.get(1)).map(Action::Save),
        Some("load") => name(args.get(1)).map(Action::Load),
        Some("delete") => name(args.get(1)).map(Action::Delete),
        Some("list") => Some(Action::List),
        _ => None,
    }
}

/// A preset name from an argument: plain text, safe as one file name.
fn name(args: Option<&&str>) -> Option<String> {
    let name = args?.trim();
    valid_name(name).then(|| name.to_owned())
}

/// Whether a name works as a preset file name on its own: no path parts, no
/// control characters, short.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
        && name.chars().all(|c| !c.is_control())
}

/// The file a preset name writes to.
pub fn preset_file(name: &str) -> String {
    format!("{name}.preset.json")
}

/// A preset as the original writes it: a version, whether it unbinds keys it
/// does not name, and one key per binding's id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub version: u32,
    pub strict: bool,
    pub keybinds: Vec<(String, String)>,
}

impl Default for Preset {
    fn default() -> Self {
        Self { version: 2, strict: false, keybinds: Vec::new() }
    }
}

/// The preset's JSON, the original's V2 shape, pretty-printed the way the
/// original's Gson does.
pub fn save_json(bindings: &[(&str, String)]) -> String {
    let mut keybinds = serde_json::Map::new();
    for (name, key) in bindings {
        keybinds.insert(
            (*name).to_owned(),
            serde_json::json!({ "key": key, "modifiers": [] }),
        );
    }
    serde_json::to_string_pretty(&serde_json::json!({
        "version": 2,
        "strict": false,
        "keybinds": keybinds,
    }))
    .unwrap_or_else(|_| "{}".to_owned())
}

/// A preset read from its JSON. An entry whose value is a bare string is the
/// original's V1 shape, read as its key; modifiers are not kept, like the
/// original's 1.0.0 read of a V2 file.
pub fn parse_preset(text: &str) -> Result<Preset, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| e.to_string())?;
    let keybinds = value
        .get("keybinds")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "no keybinds object".to_owned())?;
    let mut out = Vec::with_capacity(keybinds.len());
    for (name, entry) in keybinds {
        let key = match entry {
            serde_json::Value::String(key) => key,
            entry => entry
                .get("key")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(UNKNOWN_KEY),
        };
        out.push((name.clone(), key.to_owned()));
    }
    Ok(Preset {
        version: value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as u32,
        strict: value
            .get("strict")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        keybinds: out,
    })
}

/// What the bindings become once a preset applies: each binding whose id the
/// preset names takes the preset's key; when the preset is strict the rest
/// unbind, and when it is not they keep their key (the original's `strict`).
pub fn apply<'a>(current: &[(&'a str, String)], preset: &Preset) -> Vec<(&'a str, String)> {
    current
        .iter()
        .map(|&(name, _)| {
            let key = preset
                .keybinds
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, k)| k.clone())
                .unwrap_or_else(|| {
                    if preset.strict {
                        UNKNOWN_KEY.to_owned()
                    } else {
                        current
                            .iter()
                            .find(|(n, _)| *n == name)
                            .map(|(_, k)| k.clone())
                            .unwrap_or_else(|| UNKNOWN_KEY.to_owned())
                    }
                });
            (name, key)
        })
        .collect()
}

/// The presets' folder under `config_dir`.
pub fn preset_dir(config_dir: &Path) -> std::path::PathBuf {
    config_dir.join(FOLDER)
}

/// Save a preset file. Makes the folder if it is missing.
pub fn save_preset(dir: &Path, name: &str, bindings: &[(&str, String)]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = save_json(bindings);
    fs::write(dir.join(preset_file(name)), json).map_err(|e| e.to_string())
}

/// Read a preset file.
pub fn load_preset(dir: &Path, name: &str) -> Result<Preset, String> {
    let text = fs::read_to_string(dir.join(preset_file(name)))
        .map_err(|e| e.to_string())?;
    parse_preset(&text)
}

/// Delete a preset file.
pub fn delete_preset(dir: &Path, name: &str) -> Result<(), String> {
    fs::remove_file(dir.join(preset_file(name))).map_err(|e| e.to_string())
}

/// The preset names in the folder, sorted, extensions off.
pub fn list_presets(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|file| file.ends_with(".preset.json"))
                .map(|file| file.trim_end_matches(".preset.json").to_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The lines feedback takes: the original shows a toast, Horizon chats.
pub fn save_line(name: &str) -> String {
    format!("Saved keybind preset {name}")
}
pub fn load_line(name: &str) -> String {
    format!("Loaded keybind preset {name}")
}
pub fn delete_line(name: &str) -> String {
    format!("Deleted keybind preset {name}")
}
pub fn failed(what: &str, name: &str) -> String {
    format!("Failed to {what} keybind preset {name}")
}
pub fn list_line(names: &[String]) -> String {
    if names.is_empty() {
        "No keybind presets yet. Save one with /savemykeybinds save <name>".to_owned()
    } else {
        format!("Keybind presets: {}", names.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_each_action() {
        assert_eq!(parse("savemykeybinds", &[]), Some(Action::List));
        assert_eq!(parse("smkb", &["save", "pvp"]), Some(Action::Save("pvp".to_owned())));
        assert_eq!(parse("smkb", &["load", "pvp"]), Some(Action::Load("pvp".to_owned())));
        assert_eq!(parse("smkb", &["delete", "pvp"]), Some(Action::Delete("pvp".to_owned())));
        assert_eq!(parse("smkb", &["list"]), Some(Action::List));
        assert_eq!(parse("say", &["hi"]), None);
        assert_eq!(parse("smkb", &["save"]), None);
        assert_eq!(parse("smkb", &["teleport"]), None);
    }

    #[test]
    fn parse_rejects_unsafe_names() {
        assert_eq!(parse("smkb", &["save", "../etc/passwd"]), None);
        assert_eq!(parse("smkb", &["save", "a/b"]), None);
        assert_eq!(parse("smkb", &["save", ""]), None);
        assert_eq!(parse("smkb", &["load", "ok name"]), Some(Action::Load("ok name".to_owned())));
    }

    #[test]
    fn save_json_keeps_the_original_shape() {
        let json = save_json(&[("key.jump", "key.keyboard.space".to_owned())]);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], 2);
        assert_eq!(value["strict"], false);
        assert_eq!(value["keybinds"]["key.jump"]["key"], "key.keyboard.space");
        assert_eq!(value["keybinds"]["key.jump"]["modifiers"], serde_json::json!([]));
    }

    #[test]
    fn parse_reads_v2_and_v1_shapes() {
        let v2 = r#"{"version":2,"strict":true,"keybinds":{"key.jump":{"key":"key.keyboard.space","modifiers":[]}}}"#;
        let preset = parse_preset(v2).unwrap();
        assert_eq!(preset.version, 2);
        assert!(preset.strict);
        assert_eq!(preset.keybinds, vec![("key.jump".to_owned(), "key.keyboard.space".to_owned())]);

        let v1 = r#"{"keybinds":{"key.jump":"key.keyboard.space"}}"#;
        let preset = parse_preset(v1).unwrap();
        assert_eq!(preset.version, 1);
        assert!(!preset.strict);
        assert_eq!(preset.keybinds, vec![("key.jump".to_owned(), "key.keyboard.space".to_owned())]);
    }

    #[test]
    fn apply_applies_named_and_strict() {
        let current = [("key.jump", "key.keyboard.space".to_owned()), ("key.sneak", "key.keyboard.left.shift".to_owned())];
        let preset = Preset {
            version: 2,
            strict: false,
            keybinds: vec![("key.jump".to_owned(), "key.keyboard.w".to_owned())],
        };
        let after = apply(&current, &preset);
        assert_eq!(after[0].1, "key.keyboard.w");
        assert_eq!(after[1].1, "key.keyboard.left.shift");

        let strict = Preset { strict: true, ..preset };
        let after = apply(&current, &strict);
        assert_eq!(after[1].1, UNKNOWN_KEY);
    }

    #[test]
    fn lines_cover_the_feedback() {
        assert_eq!(save_line("pvp"), "Saved keybind preset pvp");
        assert_eq!(load_line("pvp"), "Loaded keybind preset pvp");
        assert_eq!(delete_line("pvp"), "Deleted keybind preset pvp");
        assert_eq!(failed("load", "gone"), "Failed to load keybind preset gone");
        assert_eq!(list_line(&[]), "No keybind presets yet. Save one with /savemykeybinds save <name>");
        assert_eq!(list_line(&["a".to_owned(), "b".to_owned()]), "Keybind presets: a, b");
    }

    #[test]
    fn preset_files_round_trip() {
        let dir = std::env::temp_dir().join("smkb-test");
        let _ = fs::remove_dir_all(&dir);
        let bindings = [("key.jump", "key.keyboard.space".to_owned())];
        save_preset(&dir, "pvp", &bindings).unwrap();
        assert_eq!(list_presets(&dir), vec!["pvp".to_owned()]);
        let preset = load_preset(&dir, "pvp").unwrap();
        assert_eq!(preset.keybinds, vec![("key.jump".to_owned(), "key.keyboard.space".to_owned())]);
        delete_preset(&dir, "pvp").unwrap();
        assert!(list_presets(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}

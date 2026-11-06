//! Litematica hotkeys: upstream's default keybinds (litematica Hotkeys.java)
//! matched against Horizon's native_input_key events.
//! Port of litematica (Copyright maruohon, LGPL-3.0).

/// An action a bound key runs.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum HotkeyAction {
    /// renderOverlayThroughBlocks (upstream default R_CTRL; upstream holds it
    /// as a modifier, the port toggles it on press until a modifier-state
    /// read exists).
    ToggleThroughWalls,
    /// invertSchematicRenderState (upstream default is unbound; the port adds
    /// /litematica render and leaves the key unbound to match upstream).
    ToggleRender,
    /// openMainMenuScreen (upstream default M; the port opens Horizon's
    /// native mod screen with a status panel).
    MainMenu,
    /// saveSchematicToFile (upstream default L_CTRL,L_ALT,S; upstream opens a
    /// naming GUI, the port points at /litematica savearea <path>).
    SaveSchematic,
}

/// The action upstream's default keybinds give a key event with these parts,
/// or None when the key is unbound (matching upstream's own defaults).
pub fn action_for(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<HotkeyAction> {
    match (key, ctrl, alt, shift) {
        ("M", false, false, false) => Some(HotkeyAction::MainMenu),
        ("R_CTRL", false, false, false) => Some(HotkeyAction::ToggleThroughWalls),
        ("S", true, true, false) => Some(HotkeyAction::SaveSchematic),
        _ => None,
    }
}

impl HotkeyAction {
    /// The menu-facing label for this action.
    pub fn label(self) -> &'static str {
        match self {
            HotkeyAction::ToggleThroughWalls => "toggle through walls",
            HotkeyAction::ToggleRender => "toggle render",
            HotkeyAction::MainMenu => "open main menu",
            HotkeyAction::SaveSchematic => "save schematic",
        }
    }
}

/// The default keybinds as (action, combo) pairs, upstream's defaults. An
/// "unbound" combo never matches a key, so ToggleRender stays unbound.
pub fn default_binds() -> Vec<(HotkeyAction, String)> {
    vec![
        (HotkeyAction::MainMenu, "M".to_string()),
        (HotkeyAction::ToggleThroughWalls, "R_CTRL".to_string()),
        (HotkeyAction::SaveSchematic, "L_CTRL,L_ALT,S".to_string()),
        (HotkeyAction::ToggleRender, "unbound".to_string()),
    ]
}

/// The upstream-style combo string for a key event's modifiers and key label.
pub fn combo_label(ctrl: bool, alt: bool, shift: bool, key: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if ctrl {
        parts.push("L_CTRL");
    }
    if alt {
        parts.push("L_ALT");
    }
    if shift {
        parts.push("L_SHIFT");
    }
    parts.push(key);
    parts.join(",")
}

/// The action a bound combo list gives a key event, or None when unbound.
pub fn action_for_binds(
    binds: &[(HotkeyAction, String)],
    key: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> Option<HotkeyAction> {
    let combo = combo_label(ctrl, alt, shift, key);
    binds
        .iter()
        .find(|(_, bound)| *bound == combo)
        .map(|(action, _)| *action)
}

/// The upstream-style label for any winit key code, for rebinding: known
/// labels first, single letters without the Key prefix, digits bare, and the
/// raw code name for everything else.
pub fn generic_label(code: &str) -> String {
    if let Some(label) = label_for_code(code) {
        return label.to_string();
    }
    if let Some(rest) = code.strip_prefix("Key") {
        let chars = rest.chars().collect::<Vec<_>>();
        if chars.len() == 1 && chars[0].is_ascii_alphabetic() {
            return rest.to_string();
        }
    }
    if let Some(rest) = code.strip_prefix("Digit") {
        if rest.chars().all(|c| c.is_ascii_digit()) && !rest.is_empty() {
            return rest.to_string();
        }
    }
    code.to_string()
}

/// The rebind slot a digit key picks, or None (the menu binds 1-4 to its four
/// actions).
pub fn digit_index(key: &str) -> Option<usize> {
    match key {
        "1" => Some(0),
        "2" => Some(1),
        "3" => Some(2),
        "4" => Some(3),
        _ => None,
    }
}

/// The upstream-style label for a winit physical key code name (its Debug
/// spelling, e.g. "KeyM"), or None for keys no binding uses.
pub fn label_for_code(code: &str) -> Option<&'static str> {
    Some(match code {
        "KeyM" => "M",
        "KeyS" => "S",
        "PageUp" => "PAGE_UP",
        "PageDown" => "PAGE_DOWN",
        "ControlLeft" => "L_CTRL",
        "ControlRight" => "R_CTRL",
        "AltLeft" => "L_ALT",
        "AltRight" => "R_ALT",
        "ShiftLeft" => "L_SHIFT",
        "ShiftRight" => "R_SHIFT",
        "NumpadMultiply" => "MULTIPLY",
        "NumpadSubtract" => "SUBTRACT",
        "F3" => "F3",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_defaults_match() {
        assert_eq!(action_for("M", false, false, false), Some(HotkeyAction::MainMenu));
        assert_eq!(
            action_for("R_CTRL", false, false, false),
            Some(HotkeyAction::ToggleThroughWalls)
        );
        assert_eq!(
            action_for("S", true, true, false),
            Some(HotkeyAction::SaveSchematic)
        );
        assert_eq!(action_for("M", true, false, false), None);
        assert_eq!(action_for("S", true, false, false), None);
        assert_eq!(action_for("Q", false, false, false), None);
    }

    #[test]
    fn defaults_and_labels() {
        let d = default_binds();
        assert_eq!(d[0], (HotkeyAction::MainMenu, "M".to_string()));
        assert_eq!(d[2], (HotkeyAction::SaveSchematic, "L_CTRL,L_ALT,S".to_string()));
        assert_eq!(HotkeyAction::MainMenu.label(), "open main menu");
        assert_eq!(HotkeyAction::ToggleRender.label(), "toggle render");
    }

    #[test]
    fn action_for_binds_matches_and_rebinds() {
        let d = default_binds();
        assert_eq!(
            action_for_binds(&d, "M", false, false, false),
            Some(HotkeyAction::MainMenu)
        );
        assert_eq!(
            action_for_binds(&d, "S", true, true, false),
            Some(HotkeyAction::SaveSchematic)
        );
        assert_eq!(action_for_binds(&d, "G", false, false, false), None);
        let mut rebound = d;
        rebound[0].1 = "G".to_string();
        assert_eq!(
            action_for_binds(&rebound, "G", false, false, false),
            Some(HotkeyAction::MainMenu)
        );
        assert_eq!(action_for_binds(&rebound, "M", false, false, false), None);
    }

    #[test]
    fn combo_and_generic_labels() {
        assert_eq!(combo_label(true, true, false, "S"), "L_CTRL,L_ALT,S");
        assert_eq!(combo_label(false, false, true, "M"), "L_SHIFT,M");
        assert_eq!(combo_label(false, false, false, "G"), "G");
        assert_eq!(generic_label("KeyM"), "M");
        assert_eq!(generic_label("KeyG"), "G");
        assert_eq!(generic_label("Digit4"), "4");
        assert_eq!(generic_label("ControlRight"), "R_CTRL");
        assert_eq!(generic_label("Escape"), "Escape");
        assert_eq!(digit_index("1"), Some(0));
        assert_eq!(digit_index("4"), Some(3));
        assert_eq!(digit_index("5"), None);
    }

    #[test]
    fn labels() {
        assert_eq!(label_for_code("KeyM"), Some("M"));
        assert_eq!(label_for_code("ControlRight"), Some("R_CTRL"));
        assert_eq!(label_for_code("NumpadMultiply"), Some("MULTIPLY"));
        assert_eq!(label_for_code("KeyQ"), None);
    }
}

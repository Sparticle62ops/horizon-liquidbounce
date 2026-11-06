//! The main menu's panel content: upstream's LitematicaMainMenu as a text
//! panel over the world (a degradation note tracks the full widget GUI).

/// The menu panel's lines from the current state.
pub fn panel_lines(
    schematic: Option<&str>,
    render: bool,
    walls: bool,
    ghosts: usize,
    highlights: usize,
    binds: &[(String, String)],
    rebinding: Option<&str>,
) -> Vec<String> {
    let mut lines = vec![
        "Litematica".to_string(),
        match schematic {
            Some(s) => format!("schematic: {}", s),
            None => "schematic: none loaded".to_string(),
        },
        format!("ghosts: {}", ghosts),
        format!("highlights: {}", highlights),
        format!("render: {}", if render { "on" } else { "off" }),
        format!("through walls: {}", if walls { "on" } else { "off" }),
        "keybinds:".to_string(),
    ];
    for (label, keys) in binds {
        lines.push(format!("  {}: {}", label, keys));
    }
    lines.push("press 1-4 to rebind".to_string());
    lines.push(String::new());
    if let Some(label) = rebinding {
        lines.push(format!("rebinding {}: press a key", label));
    }
    lines.push("esc to close".to_string());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_shows_none_when_unloaded() {
        let lines = panel_lines(None, true, false, 5, 2, &[], None);
        assert_eq!(lines[0], "Litematica");
        assert_eq!(lines[1], "schematic: none loaded");
        assert_eq!(lines[2], "ghosts: 5");
        assert_eq!(lines[3], "highlights: 2");
        assert_eq!(lines[4], "render: on");
        assert_eq!(lines[5], "through walls: off");
        assert_eq!(lines.last().unwrap(), "esc to close");
    }

    #[test]
    fn panel_lists_keybinds() {
        let binds = vec![
            ("open main menu".to_string(), "M".to_string()),
            ("toggle render".to_string(), "unbound".to_string()),
        ];
        let lines = panel_lines(None, true, false, 0, 0, &binds, None);
        let i = lines.iter().position(|l| l == "keybinds:").unwrap();
        assert_eq!(lines[i + 1], "  open main menu: M");
        assert_eq!(lines[i + 2], "  toggle render: unbound");
        assert!(lines.contains(&"press 1-4 to rebind".to_string()));
        assert_eq!(lines.last().unwrap(), "esc to close");
    }

    #[test]
    fn panel_shows_the_rebind_prompt() {
        let lines = panel_lines(None, true, false, 0, 0, &[], Some("open main menu"));
        assert!(lines.contains(&"rebinding open main menu: press a key".to_string()));
        assert_eq!(lines.last().unwrap(), "esc to close");
    }

    #[test]
    fn panel_shows_the_schematic_summary() {
        let lines = panel_lines(Some("3 region(s), 4x2x6"), false, true, 0, 0, &[], None);
        assert_eq!(lines[1], "schematic: 3 region(s), 4x2x6");
        assert_eq!(lines[4], "render: off");
        assert_eq!(lines[5], "through walls: on");
    }
}

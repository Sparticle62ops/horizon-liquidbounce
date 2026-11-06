//! Port of `features/module/ClientModule.kt`: the module base.
//!
//! Upstream fields kept 1:1: category, notActivatable, disableActivation,
//! disableOnQuit, hidden, bind, tag, settings map, and the toggle state
//! machine (onToggled returns whether the switch happened, with the
//! notification title/severity pair it feeds).
use crate::category::ModuleCategory;
use crate::value::{boolean, Value, ValueKind};
use std::collections::BTreeMap;

pub type TickFn = fn(&mut Module);

/// A module definition. In upstream each module is a class instance; here a
/// registered definition plus its per-instance runtime state in ModuleManager.
pub struct Module {
    pub name: &'static str,
    pub category: ModuleCategory,
    pub not_activatable: bool,
    /// Upstream: disableActivation defaults to notActivatable.
    pub disable_activation: bool,
    pub disable_on_quit: bool,
    pub hidden_default: bool,
    pub tag: Option<&'static str>,
    pub bind_default: String,
    pub settings: Vec<Value>,
    /// Per-module tick body; game seams arrive in later slices.
    pub on_tick: Option<TickFn>,
    /// Runtime state.
    pub running: bool,
    pub hidden: bool,
    pub bind: String,
    pub called_since_startup: bool,
}

impl Module {
    pub fn new(name: &'static str, category: ModuleCategory, settings: Vec<Value>) -> Module {
        Module {
            name,
            category,
            not_activatable: false,
            disable_activation: false,
            disable_on_quit: false,
            hidden_default: false,
            tag: None,
            bind_default: "keyboard:key.unknown:press".into(),
            settings,
            on_tick: None,
            running: false,
            hidden: false,
            bind: "keyboard:key.unknown:press".into(),
            called_since_startup: false,
        }
    }

    /// Upstream `baseKey`: ConfigSystem prefix + module.<lowerCamelCase name>.
    pub fn base_key(&self) -> String {
        format!("liquidbounce.module.{}", to_lower_camel_case(self.name))
    }

    pub fn setting(&self, name: &str) -> Option<&Value> {
        self.settings.iter().find(|v| v.name == name)
    }

    pub fn setting_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.settings.iter_mut().find(|v| v.name == name)
    }

    pub fn settings_by_name(&self) -> BTreeMap<&str, &Value> {
        self.settings.iter().map(|v| (v.name.as_str(), v)).collect()
    }

    /// Upstream `onToggled`: notActivatable/disableActivation modules refuse
    /// (returns false, no switch); otherwise flip and return the new state.
    pub fn on_toggled(&mut self, requested: bool) -> bool {
        if self.disable_activation || (requested && self.not_activatable) {
            return false;
        }
        self.running = requested;
        requested
    }

    /// The notification pair upstream computes in onToggled.
    pub fn toggle_notification(&self, new_state: bool) -> (&'static str, &'static str) {
        if new_state { ("Enabled", "info") } else { ("Disabled", "info") }
    }

    /// Upstream `disableOnQuit`: called when the player leaves the world.
    pub fn on_quit(&mut self) {
        if self.disable_on_quit {
            self.running = false;
        }
    }

    /// The enabled toggle lives in the settings list like upstream's
    /// `boolean("Enabled", ...)` value registration.
    pub fn enabled_value(&self) -> Value {
        let mut v = boolean("Enabled", self.running);
        v.is_enabled_toggle = true;
        v
    }

    pub fn visible_settings(&self) -> Vec<&Value> {
        let mut scope = BTreeMap::new();
        for v in &self.settings {
            scope.insert(v.name.clone(), v.to_json());
        }
        self.settings
            .iter()
            .filter(|v| match v.kind {
                ValueKind::Bool(_) if v.is_enabled_toggle => true,
                _ => (v.visible)(&scope),
            })
            .collect()
    }
}

/// Upstream `String.toLowerCamelCase()` for module config keys.
pub fn to_lower_camel_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = false;
    for (i, ch) in name.chars().enumerate() {
        match ch {
            ' ' | '-' | '_' | '.' => upper_next = i > 0,
            _ => {
                if i == 0 || upper_next {
                    out.extend(ch.to_uppercase());
                    upper_next = false;
                } else {
                    out.extend(ch.to_lowercase());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::{COMBAT, MOVEMENT};

    fn speed() -> Module {
        Module::new(
            "Speed",
            MOVEMENT,
            vec![float("Horizontal Speed", 1.0, 0.1, 5.0)],
        )
    }

    #[test]
    fn toggle_flips_state_and_reports() {
        let mut m = speed();
        assert!(m.on_toggled(true));
        assert!(m.running);
        assert_eq!(m.toggle_notification(true).0, "Enabled");
        assert!(m.on_toggled(false));
        assert!(!m.running);
    }

    #[test]
    fn not_activatable_refuses_enable_but_allows_disable() {
        let mut m = speed();
        m.not_activatable = true;
        m.disable_activation = true;
        assert!(!m.on_toggled(true));
        m.running = true;
        assert!(!m.on_toggled(true), "disableActivation refuses any request");
        m.disable_activation = false;
        m.not_activatable = true;
        assert!(!m.on_toggled(true), "notActivatable refuses enable");
        assert!(m.on_toggled(false), "disable always allowed");
        assert!(!m.running);
    }

    #[test]
    fn disable_on_quit_clears_running() {
        let mut m = speed();
        m.running = true;
        m.on_quit();
        assert!(m.running, "no flag set");
        m.disable_on_quit = true;
        m.on_quit();
        assert!(!m.running);
    }

    #[test]
    fn base_key_uses_lower_camel_case() {
        assert_eq!(speed().base_key(), "liquidbounce.module.speed");
        let mut m = Module::new("Auto Clicker", COMBAT, vec![]);
        assert_eq!(m.base_key(), "liquidbounce.module.autoClicker");
    }

    #[test]
    fn settings_lookup_by_name() {
        let m = speed();
        assert!(m.setting("Horizontal Speed").is_some());
        assert!(m.setting("Nope").is_none());
        assert_eq!(m.settings_by_name().len(), 1);
    }
}

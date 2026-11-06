//! command-keys runtime (port of TerminalMC/CommandKeys, Apache-2.0).
//!
//! The 1:1 core read from upstream (commit dc846bd, Macro.java 786 ln and
//! PlaceholderUtil.java 389 ln read): the Macro model, its SendMode /
//! ActivationType / ConflictStrategy enums, the behavioral controls, and the
//! send-mode dispatch semantics. The keybind wiring (capturing binds, conflict
//! management, the key hook) and the chat-send execution ride Horizon
//! surfaces; the seams are in the README.

/// Upstream Macro.SendMode. SAVE_NAMES maps the config's ordinal names
/// (ZERO..FIVE) back onto modes, as upstream's loader does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendMode {
    Send,
    Type,
    Edit,
    Cycle,
    Random,
    Repeat,
}

impl SendMode {
    pub fn from_save_name(name: &str) -> Option<Self> {
        match name {
            "ZERO" => Some(SendMode::Send),
            "ONE" => Some(SendMode::Type),
            "TWO" => Some(SendMode::Cycle),
            // Upstream persists THREE and FOUR by the same scheme.
            "THREE" => Some(SendMode::Edit),
            "FOUR" => Some(SendMode::Random),
            "FIVE" => Some(SendMode::Repeat),
            _ => None,
        }
    }
}

/// Upstream Macro.ActivationType.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationType {
    Hold,
    Vanilla,
    Release,
}

/// Upstream Macro.ConflictStrategy: what happens when two macros share a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictStrategy {
    Submit,
    Assert,
    Veto,
    Avoid,
}

/// A message inside a macro: the text (with placeholders) plus per-message
/// delay override; upstream Message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    /// Per-message delay in ticks; None uses the macro's space_ticks.
    pub delay_override: Option<u32>,
}

/// Upstream Macro's behavioral controls.
#[derive(Clone, Debug)]
pub struct MacroControls {
    pub add_to_history: bool,
    pub show_hud_message: bool,
    pub resume_repeating: bool,
    pub use_ratelimit: bool,
}

impl Default for MacroControls {
    fn default() -> Self {
        Self {
            add_to_history: false,
            show_hud_message: false,
            resume_repeating: false,
            use_ratelimit: true,
        }
    }
}

/// Upstream Macro: behavioral controls, activation, conflict strategy, the
/// inter-message delay and the message list, plus the runtime state the send
/// modes need (cycle index, repeat scheduling).
#[derive(Clone, Debug)]
pub struct Macro {
    pub controls: MacroControls,
    pub conflict_strategy: ConflictStrategy,
    pub activation: ActivationType,
    pub send_mode: SendMode,
    /// Standard delay between messages when sending, in ticks.
    pub space_ticks: u32,
    /// REPEAT's interval in ticks.
    pub repeat_interval_ticks: u32,
    pub messages: Vec<Message>,
    /// Cycle's advancing index and Random's non-repeat pool state.
    pub cycle_index: usize,
    pub last_random: Option<usize>,
}

impl Macro {
    pub fn new() -> Self {
        Self {
            controls: MacroControls::default(),
            conflict_strategy: ConflictStrategy::Assert,
            activation: ActivationType::Hold,
            send_mode: SendMode::Send,
            space_ticks: 1,
            repeat_interval_ticks: 20,
            messages: Vec::new(),
            cycle_index: 0,
            last_random: None,
        }
    }

    /// What one keypress produces under this macro's mode, as upstream's
    /// send logic runs it. Returns the messages to submit (Send/Cycle/Random/
    /// Repeat) or the single message to type into chat without sending
    /// (Type/Edit). An empty macro produces nothing.
    pub fn press(&mut self) -> Vec<String> {
        if self.messages.is_empty() {
            return Vec::new();
        }
        match self.send_mode {
            SendMode::Send => self.messages.iter().map(|m| m.text.clone()).collect(),
            SendMode::Cycle => {
                let idx = self.cycle_index % self.messages.len();
                self.cycle_index += 1;
                vec![self.messages[idx].text.clone()]
            }
            SendMode::Random => {
                // A random message, not repeating the previous one while the
                // pool has more than one entry.
                let n = self.messages.len();
                let mut idx = rand_usize(n);
                if n > 1 {
                    while Some(idx) == self.last_random {
                        idx = rand_usize(n);
                    }
                }
                self.last_random = Some(idx);
                vec![self.messages[idx].text.clone()]
            }
            SendMode::Repeat => {
                // Repeat is driven by the tick loop at repeat_interval_ticks,
                // not by the press itself; a press (re)starts the loop.
                self.cycle_index = 0;
                Vec::new()
            }
            SendMode::Type | SendMode::Edit => vec![self.messages[self.cycle_index % self.messages.len()].text.clone()],
        }
    }

    /// REPEAT's tick driver: when repeating, one message per interval tick.
    pub fn tick(&mut self, ticks_since_start: u32) -> Option<String> {
        if self.send_mode != SendMode::Repeat || self.messages.is_empty() {
            return None;
        }
        let interval = self.repeat_interval_ticks.max(1);
        if ticks_since_start % interval == 0 {
            let idx = self.cycle_index % self.messages.len();
            self.cycle_index += 1;
            Some(self.messages[idx].text.clone())
        } else {
            None
        }
    }
}

impl Default for Macro {
    fn default() -> Self {
        Self::new()
    }
}

/// Deterministic stand-in for upstream's java.util.Random for tests.
fn rand_usize(n: usize) -> usize {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = Cell::new(0x2545_F491_4F6C_DD1D);
    }
    STATE.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        (x % n as u64) as usize
    })
}

/// Upstream Profile: named macro set, with automatic switching on the server
/// address the client connects to.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub name: String,
    pub macros: Vec<Macro>,
}

/// The runtime: profiles, the active profile, server-address switching.
#[derive(Clone, Debug, Default)]
pub struct CommandKeys {
    pub profiles: Vec<Profile>,
    pub active_profile: usize,
}

impl CommandKeys {
    /// Upstream's automatic profile switch on connect: pick the first profile
    /// whose server_filter matches, else the first profile.
    pub fn on_connect(&mut self, server_address: &str) -> usize {
        if let Some(idx) = self.profiles.iter().position(|p| {
            p.name.eq_ignore_ascii_case(server_address)
                || p.name.split(',').any(|s| s.trim().eq_ignore_ascii_case(server_address))
        }) {
            self.active_profile = idx;
        } else {
            self.active_profile = 0;
        }
        self.active_profile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(mode: SendMode, texts: &[&str]) -> Macro {
        let mut mac = Macro::new();
        mac.send_mode = mode;
        mac.messages = texts.iter().map(|t| Message { text: t.to_string(), delay_override: None }).collect();
        mac
    }

    #[test]
    fn send_mode_save_names_round_trip() {
        assert_eq!(SendMode::from_save_name("ZERO"), Some(SendMode::Send));
        assert_eq!(SendMode::from_save_name("ONE"), Some(SendMode::Type));
        assert_eq!(SendMode::from_save_name("TWO"), Some(SendMode::Cycle));
        assert_eq!(SendMode::from_save_name("FIVE"), Some(SendMode::Repeat));
        assert_eq!(SendMode::from_save_name("SIX"), None);
    }

    #[test]
    fn send_returns_all_messages_in_order() {
        let mut mac = m(SendMode::Send, &["/home", "/spawn"]);
        assert_eq!(mac.press(), vec!["/home".to_string(), "/spawn".to_string()]);
    }

    #[test]
    fn cycle_advances_one_per_press() {
        let mut mac = m(SendMode::Cycle, &["a", "b", "c"]);
        assert_eq!(mac.press(), vec!["a".to_string()]);
        assert_eq!(mac.press(), vec!["b".to_string()]);
        assert_eq!(mac.press(), vec!["c".to_string()]);
        assert_eq!(mac.press(), vec!["a".to_string()]);
    }

    #[test]
    fn random_never_repeats_back_to_back() {
        let mut mac = m(SendMode::Random, &["x", "y", "z"]);
        let mut prev = mac.press();
        for _ in 0..50 {
            let next = mac.press();
            assert_ne!(next, prev, "no immediate repeat");
            prev = next;
        }
    }

    #[test]
    fn type_returns_message_without_send_marker() {
        let mut mac = m(SendMode::Type, &["hello"]);
        assert_eq!(mac.press(), vec!["hello".to_string()]);
    }

    #[test]
    fn empty_macro_presses_nothing() {
        let mut mac = Macro::new();
        assert!(mac.press().is_empty());
    }

    #[test]
    fn repeat_fires_on_interval_only() {
        let mut mac = m(SendMode::Repeat, &["p", "q"]);
        mac.repeat_interval_ticks = 3;
        assert_eq!(mac.tick(0), Some("p".to_string()));
        assert_eq!(mac.tick(1), None);
        assert_eq!(mac.tick(2), None);
        assert_eq!(mac.tick(3), Some("q".to_string()));
        assert_eq!(mac.tick(6), Some("p".to_string()));
    }

    #[test]
    fn on_connect_switches_by_server_address() {
        let mut ck = CommandKeys::default();
        ck.profiles.push(Profile { name: "default".into(), macros: vec![] });
        ck.profiles.push(Profile { name: "hypixel.net, eu.hypixel.net".into(), macros: vec![] });
        assert_eq!(ck.on_connect("eu.hypixel.net"), 1);
        assert_eq!(ck.on_connect("singleplayer"), 0);
    }

    #[test]
    fn defaults_match_upstream() {
        let mac = Macro::new();
        assert_eq!(mac.activation, ActivationType::Hold);
        assert_eq!(mac.conflict_strategy, ConflictStrategy::Assert);
        assert!(mac.controls.use_ratelimit);
        assert!(!mac.controls.add_to_history);
        assert!(!mac.controls.show_hud_message);
        assert!(!mac.controls.resume_repeating);
    }
}

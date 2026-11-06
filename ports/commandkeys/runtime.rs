// CommandKeys for Horizon: bind keys to macros (chat messages and commands
// with per-message delays, spacing-based repeats, cycling, random pick, a
// global ratelimit and the chat length limit). A Rust port of CommandKeys by
// TerminalMC (github.com/TerminalMC/CommandKeys, Apache-2.0); this port keeps
// Apache-2.0. Degradations vs the original are listed in README.md.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::game::{Game, Screen};

#[derive(Clone)]
struct Keybind {
    name: String,
    limit: String,
}

impl Keybind {
    fn is_unknown(&self) -> bool {
        self.name.is_empty() || self.name == "unknown"
    }
    // The original fires when the key matches and its limit key (a modifier
    // whose release must not be swallowed by a screen) is down, or no limit
    // key is set ("unknown").
    fn matches(&self, key: &str, down: &HashMap<String, bool>) -> bool {
        if self.is_unknown() || self.name != key {
            return false;
        }
        if self.limit.is_empty() || self.limit == "unknown" {
            return true;
        }
        down.get(&self.limit).copied().unwrap_or(false)
    }
}

struct Message {
    text: String,
    delay: i32,
}

#[derive(Clone, Copy, PartialEq)]
enum SendMode {
    Send,
    Repeat,
    Type,
    Edit,
    Cycle,
    Random,
}

#[derive(Clone, Copy, PartialEq)]
enum ActivationType {
    Hold,
    Vanilla,
    Release,
}

#[derive(Clone, Copy, PartialEq)]
enum Strategy {
    Assert,
    Submit,
    Veto,
    Avoid,
}

struct Macro {
    keybind: Keybind,
    alt: Keybind,
    messages: Vec<Message>,
    mode: SendMode,
    activation: ActivationType,
    strategy: Strategy,
    space_ticks: i32,
    max_repeats: i32,
    cycle_index: i32,
    use_ratelimit: bool,
    active: bool,
    waiting_release: bool,
    active_ticks: i32,
    repetitions: i32,
    scheduled: Vec<(i32, String)>,
}

#[allow(dead_code)]
struct Profile {
    name: String,
    ratelimit_ctl: Option<bool>,
    macros: Vec<Macro>,
}

#[allow(dead_code)]
struct State {
    profiles: Vec<Profile>,
    default_profile: usize,
    ratelimit_count: i32,
    ratelimit_ticks: i32,
    length_limit: usize,
    limiter: Vec<i32>,
    down: HashMap<String, bool>,
    rng: u64,
}

static STATE: OnceLock<Mutex<Option<State>>> = OnceLock::new();

fn with_state<R>(game: &mut Game, f: impl FnOnce(&mut State, &mut Game) -> R) {
    let cell = STATE.get_or_init(|| Mutex::new(None));
    let Ok(mut guard) = cell.lock() else {
        return;
    };
    if guard.is_none() {
        *guard = Some(State::load(game));
    }
    let Some(st) = guard.as_mut() else {
        return;
    };
    f(st, game);
}

// A small JSON reader, enough for the original's Gson config: objects,
// arrays, strings, numbers, booleans and null.
enum Jv {
    Obj(Vec<(String, Jv)>),
    Arr(Vec<Jv>),
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

struct Jp<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Jp<'a> {
    fn ws(&mut self) {
        while let Some(c) = self.b.get(self.i) {
            match c {
                b' ' | b'\t' | b'\n' | b'\r' => self.i += 1,
                _ => break,
            }
        }
    }

    fn val(&mut self) -> Option<Jv> {
        self.ws();
        match *self.b.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut m: Vec<(String, Jv)> = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Some(Jv::Obj(m));
                }
                loop {
                    self.ws();
                    let k = match self.val()? {
                        Jv::Str(s) => s,
                        _ => return None,
                    };
                    self.ws();
                    if self.b.get(self.i) != Some(&b':') {
                        return None;
                    }
                    self.i += 1;
                    let v = self.val()?;
                    m.push((k, v));
                    self.ws();
                    match self.b.get(self.i) {
                        Some(&b',') => self.i += 1,
                        Some(&b'}') => {
                            self.i += 1;
                            break;
                        }
                        _ => return None,
                    }
                }
                Some(Jv::Obj(m))
            }
            b'[' => {
                self.i += 1;
                let mut a: Vec<Jv> = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Some(Jv::Arr(a));
                }
                loop {
                    a.push(self.val()?);
                    self.ws();
                    match self.b.get(self.i) {
                        Some(&b',') => self.i += 1,
                        Some(&b']') => {
                            self.i += 1;
                            break;
                        }
                        _ => return None,
                    }
                }
                Some(Jv::Arr(a))
            }
            b'"' => {
                self.i += 1;
                let mut s = String::new();
                loop {
                    let c = *self.b.get(self.i)?;
                    self.i += 1;
                    match c {
                        b'"' => break,
                        b'\\' => {
                            let e = *self.b.get(self.i)?;
                            self.i += 1;
                            match e {
                                b'n' => s.push('\n'),
                                b't' => s.push('\t'),
                                b'r' => s.push('\r'),
                                b'u' => {
                                    let hex =
                                        std::str::from_utf8(self.b.get(self.i..self.i + 4)?)
                                            .ok()?;
                                    let cp = u32::from_str_radix(hex, 16).ok()?;
                                    self.i += 4;
                                    s.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                                }
                                other => s.push(other as char),
                            }
                        }
                        other => s.push(other as char),
                    }
                }
                Some(Jv::Str(s))
            }
            b't' => {
                self.i += 4;
                Some(Jv::Bool(true))
            }
            b'f' => {
                self.i += 5;
                Some(Jv::Bool(false))
            }
            b'n' => {
                self.i += 4;
                Some(Jv::Null)
            }
            _ => {
                let st = self.i;
                while let Some(c) = self.b.get(self.i) {
                    if c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E') {
                        self.i += 1;
                    } else {
                        break;
                    }
                }
                if self.i == st {
                    return None;
                }
                std::str::from_utf8(&self.b[st..self.i])
                    .ok()?
                    .parse::<f64>()
                    .ok()
                    .map(Jv::Num)
            }
        }
    }
}

fn m_of(v: &Jv) -> Option<&Vec<(String, Jv)>> {
    match v {
        Jv::Obj(m) => Some(m),
        _ => None,
    }
}

fn fld<'v>(m: &'v [(String, Jv)], k: &str) -> Option<&'v Jv> {
    m.iter().find(|e| e.0 == k).map(|e| &e.1)
}

fn j_str(v: Option<&Jv>, d: &str) -> String {
    match v {
        Some(Jv::Str(s)) => s.clone(),
        _ => d.to_string(),
    }
}

fn j_i32(v: Option<&Jv>, d: i32) -> i32 {
    match v {
        Some(Jv::Num(n)) => *n as i32,
        _ => d,
    }
}

fn j_bool(v: Option<&Jv>, d: bool) -> bool {
    match v {
        Some(Jv::Bool(b)) => *b,
        _ => d,
    }
}

/// Profile-level managed controls are Gson enums: "ON", "OFF" or "DEFAULT".
fn j_ctl(v: Option<&Jv>) -> Option<bool> {
    match v {
        Some(Jv::Str(s)) => match s.trim().to_ascii_uppercase().as_str() {
            "ON" => Some(true),
            "OFF" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn parse_key(v: Option<&Jv>) -> Keybind {
    match v.and_then(m_of) {
        Some(m) => Keybind {
            name: j_str(fld(m, "keyName"), "unknown"),
            limit: j_str(fld(m, "limitKeyName"), "unknown"),
        },
        None => Keybind {
            name: "unknown".into(),
            limit: "unknown".into(),
        },
    }
}

fn parse_macro(m: &[(String, Jv)]) -> Macro {
    let mut messages: Vec<Message> = Vec::new();
    if let Some(Jv::Arr(items)) = fld(m, "messages") {
        for it in items {
            if let Some(mo) = m_of(it) {
                messages.push(Message {
                    text: j_str(fld(mo, "string"), ""),
                    delay: j_i32(fld(mo, "delayTicks"), 0).max(0),
                });
            }
        }
    }
    Macro {
        keybind: parse_key(fld(m, "keybind")),
        alt: parse_key(fld(m, "altKeybind")),
        messages,
        mode: match j_str(fld(m, "sendMode"), "SEND").as_str() {
            "REPEAT" => SendMode::Repeat,
            "TYPE" => SendMode::Type,
            "EDIT" => SendMode::Edit,
            "CYCLE" => SendMode::Cycle,
            "RANDOM" => SendMode::Random,
            _ => SendMode::Send,
        },
        activation: match j_str(fld(m, "activationType"), "HOLD").as_str() {
            "VANILLA" => ActivationType::Vanilla,
            "RELEASE" => ActivationType::Release,
            _ => ActivationType::Hold,
        },
        strategy: match j_str(fld(m, "conflictStrategy"), "ASSERT").as_str() {
            "SUBMIT" => Strategy::Submit,
            "VETO" => Strategy::Veto,
            "AVOID" => Strategy::Avoid,
            _ => Strategy::Assert,
        },
        space_ticks: j_i32(fld(m, "spaceTicks"), 0).max(0),
        max_repeats: j_i32(fld(m, "maxRepeats"), 0).max(0),
        cycle_index: j_i32(fld(m, "cycleIndex"), 0).max(0),
        use_ratelimit: j_bool(fld(m, "useRatelimit"), true),
        active: false,
        waiting_release: false,
        active_ticks: -1,
        repetitions: 1,
        scheduled: Vec::new(),
    }
}

impl State {
    // The original's defaults when no config file exists yet.
    fn default_cfg() -> State {
        State {
            profiles: vec![Profile {
                name: "default".to_string(),
                ratelimit_ctl: None,
                macros: Vec::new(),
            }],
            default_profile: 0,
            ratelimit_count: 4,
            ratelimit_ticks: 20,
            length_limit: 256,
            limiter: Vec::new(),
            down: HashMap::new(),
            rng: 0,
        }
    }
    fn load(game: &Game) -> State {
        let mut st = Self::default_cfg();
        let path = game.native_data_dir("commandkeys").join("commandkeys.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return st;
        };
        let mut p = Jp {
            b: text.as_bytes(),
            i: 0,
        };
        let Some(root) = p.val() else { return st };
        let Some(top) = m_of(&root) else { return st };
        st.ratelimit_count = j_i32(fld(top, "ratelimitCount"), 4).max(1);
        st.ratelimit_ticks = j_i32(fld(top, "ratelimitTicks"), 20).max(1);
        st.length_limit = j_i32(fld(top, "lengthLimitLength"), 256).max(1) as usize;
        if let Some(Jv::Arr(items)) = fld(top, "profiles") {
            let mut profiles: Vec<Profile> = Vec::new();
            for it in items {
                let Some(po) = m_of(it) else { continue };
                let mut macros: Vec<Macro> = Vec::new();
                let keys = fld(po, "macros").or_else(|| fld(po, "commandKeys"));
                if let Some(Jv::Arr(items)) = keys {
                    for mv in items {
                        if let Some(mo) = m_of(mv) {
                            macros.push(parse_macro(mo));
                        }
                    }
                }
                profiles.push(Profile {
                    name: j_str(fld(po, "name"), ""),
                    ratelimit_ctl: j_ctl(fld(po, "useRatelimit")),
                    macros,
                });
            }
            if !profiles.is_empty() {
                st.profiles = profiles;
            }
        }
        let last = st.profiles.len() - 1;
        st.default_profile = (j_i32(fld(top, "mpDefault"), 0).max(0) as usize).min(last);
        // Only CYCLE/TYPE macros keep blank messages; SEND/REPEAT drop them
        // and all messages get trimmed like in the original's Macro.validate.
        let mp = st.default_profile;
        for m in &mut st.profiles[mp].macros {
            let keep_blank = matches!(m.mode, SendMode::Cycle | SendMode::Type);
            m.messages.retain(|msg| keep_blank || !msg.text.trim().is_empty());
            for msg in &mut m.messages {
                if !matches!(m.mode, SendMode::Type | SendMode::Edit) {
                    msg.text = msg.text.trim().to_string();
                }
            }
        }
        st
    }

    fn pi(&self) -> usize {
        self.default_profile
    }

    fn down(&self, name: &str) -> bool {
        *self.down.get(name).unwrap_or(&false)
    }

    // The original's trigger: an active macro completes (non-HOLD
    // deactivates), a new one is ratified and armed. Returns true when the
    // caller should run the macro's action this tick.
    fn trigger(&mut self, mp: usize, i: usize) -> Option<bool> {
        let m = &mut self.profiles[mp].macros[i];
        if m.active {
            if m.activation != ActivationType::Release || !m.waiting_release {
                if m.activation != ActivationType::Hold {
                    m.active = false;
                }
            }
            return None;
        }
        let use_rl = m.use_ratelimit;
        if use_rl {
            if self.limiter.len() >= self.ratelimit_count as usize {
                return None;
            }
            self.limiter.push(self.ratelimit_ticks);
        }
        m.active = true;
        m.active_ticks = -1;
        m.repetitions = 1;
        if m.activation == ActivationType::Release {
            m.waiting_release = true;
            None
        } else {
            Some(true)
        }
    }

    // The original's Macro.doAction: SEND/REPEAT send the whole chain (ALT
    // reverses the order), CYCLE sends one slice of the ",,"-split, RANDOM
    // one random message, TYPE/EDIT degrade to sending the first message.
    fn do_action(&mut self, game: &mut Game, mp: usize, i: usize, alt_used: bool) {
        let mut out: Vec<(i32, String)> = Vec::new();
        let mut cycle_next: Option<i32> = None;
        let mut rng_step: Option<u64> = None;
        {
            let m = &self.profiles[mp].macros[i];
            match m.mode {
                SendMode::Send | SendMode::Repeat => {
                    let standard = m.mode == SendMode::Send && m.space_ticks != 0;
                    let mut total = if standard { -m.space_ticks } else { 0 };
                    for msg in &m.messages {
                        total += if standard { m.space_ticks } else { msg.delay };
                        if !msg.text.trim().is_empty() {
                            out.push((total, msg.text.clone()));
                        }
                    }
                }
                SendMode::Cycle => {
                    let n = m.messages.len() as i32;
                    if n > 0 {
                        let next = if alt_used {
                            let v = m.cycle_index - 1;
                            if v < 0 {
                                n - 1
                            } else {
                                v
                            }
                        } else {
                            let v = m.cycle_index + 1;
                            if v >= n {
                                0
                            } else {
                                v
                            }
                        };
                        let msg = &m.messages[next as usize];
                        for part in msg.text.split(",,") {
                            if !part.trim().is_empty() {
                                out.push((msg.delay, part.to_string()));
                            }
                        }
                        cycle_next = Some(next);
                    }
                }
                SendMode::Random => {
                    let n = m.messages.len();
                    if n > 0 {
                        rng_step = Some(
                            self.rng
                                .wrapping_mul(6364136223846793005)
                                .wrapping_add(1442695040888963407),
                        );
                        let idx = ((self.rng >> 33) as usize) % n;
                        let msg = &m.messages[idx];
                        if !msg.text.trim().is_empty() {
                            out.push((msg.delay, msg.text.clone()));
                        }
                    }
                }
                SendMode::Type | SendMode::Edit => {
                    // Degraded (README): no chat screen control, so the first
                    // message goes out directly instead of being typed into
                    // the box.
                    if let Some(msg) = m.messages.first() {
                        if !msg.text.trim().is_empty() {
                            out.push((0, msg.text.clone()));
                        }
                    }
                }
            }
        }
        if let Some(step) = rng_step {
            self.rng = step;
        }
        if let Some(next) = cycle_next {
            self.profiles[mp].macros[i].cycle_index = next;
        }
        let limit = self.length_limit.max(0) as usize;
        for (d, t) in out {
            if d > 0 {
                self.profiles[mp].macros[i].scheduled.push((d, t));
            } else {
                send_now(game, limit, &t);
            }
        }
        if self.profiles[mp].macros[i].activation != ActivationType::Hold {
            self.profiles[mp].macros[i].active = false;
        }
    }
}

fn send_now(game: &mut Game, limit: usize, text: &str) {
    if text.chars().count() > limit {
        // Original shows a client message here; degraded (README).
        return;
    }
    game.send_chat(text);
}

pub fn on_tick(game: &mut Game) {
    let cell = STATE.get_or_init(|| Mutex::new(None));
    let Ok(mut guard) = cell.lock() else {
        return;
    };
    if guard.is_none() {
        *guard = Some(State::load(game));
    }
    let Some(st) = guard.as_mut() else { return };
    // ratelimiter countdown
    for t in st.limiter.iter_mut() {
        *t -= 1;
    }
    st.limiter.retain(|t| *t > 0);
    // due scheduled messages
    let mut due: Vec<String> = Vec::new();
    for p in &mut st.profiles {
        for m in &mut p.macros {
            let mut keep: Vec<(i32, String)> = Vec::new();
            for (d, t) in m.scheduled.drain(..) {
                let d = d - 1;
                if d <= 0 {
                    due.push(t);
                } else {
                    keep.push((d, t));
                }
            }
            m.scheduled = keep;
        }
    }
    let limit = st.length_limit.max(0) as usize;
    for t in &due {
        send_now(game, limit, t);
    }
    // macro ticking: HOLD release-off watchdogs and REPEAT resends
    let mp = st.pi();
    let mut resends: Vec<usize> = Vec::new();
    for i in 0..st.profiles[mp].macros.len() {
        let held = {
            let kb = st.profiles[mp].macros[i].keybind.name.clone();
            kb.is_empty() || kb == "unknown" || st.down(&kb)
        };
        let m = &mut st.profiles[mp].macros[i];
        if !m.active {
            continue;
        }
        if m.activation == ActivationType::Hold && !held {
            m.active = false;
            continue;
        }
        if m.mode == SendMode::Repeat
            && (m.space_ticks == 0 || (m.active_ticks > 0 && m.active_ticks % m.space_ticks == 0))
            && (m.max_repeats == 0 || m.repetitions < m.max_repeats)
        {
            m.repetitions += 1;
            resends.push(i);
        }
        m.active_ticks += 1;
    }
    for i in resends {
        st.do_action(game, mp, i, false);
    }
}

pub fn on_key(game: &mut Game, key_name: &str, pressed: bool) {
    if game.screen != Screen::Playing {
        // The original only handles keys with no screen open; same gate here,
        // since Horizon's native_key fires on any screen.
        return;
    }
    let cell = STATE.get_or_init(|| Mutex::new(None));
    let Ok(mut guard) = cell.lock() else {
        return;
    };
    if guard.is_none() {
        *guard = Some(State::load(game));
    }
    let Some(st) = guard.as_mut() else { return };
    st.down.insert(key_name.to_string(), pressed);
    if !pressed {
        let mp = st.pi();
        let mut holds: Vec<usize> = Vec::new();
        let mut acts: Vec<(usize, bool)> = Vec::new();
        for i in 0..st.profiles[mp].macros.len() {
            let m = &st.profiles[mp].macros[i];
            if !m.active {
                continue;
            }
            let on_main = !m.keybind.is_unknown() && m.keybind.name == key_name;
            let on_alt = !m.alt.is_unknown() && m.alt.name == key_name;
            if !(on_main || on_alt) {
                continue;
            }
            match m.activation {
                ActivationType::Hold => holds.push(i),
                ActivationType::Release if m.waiting_release => acts.push((i, on_alt)),
                _ => {}
            }
        }
        for i in holds {
            st.profiles[mp].macros[i].active = false;
        }
        for (i, alt_used) in acts {
            let ready = {
                let m = &mut st.profiles[mp].macros[i];
                m.waiting_release = false;
                true
            };
            let _ = ready;
            st.do_action(game, mp, i, alt_used);
        }
        return;
    }
    let mp = st.pi();
    let mut cands: Vec<(usize, bool)> = Vec::new();
    for i in 0..st.profiles[mp].macros.len() {
        let m = &st.profiles[mp].macros[i];
        if m.keybind.matches(key_name, &st.down) {
            cands.push((i, false));
        } else if m.alt.matches(key_name, &st.down) {
            cands.push((i, true));
        }
    }
    // Conflict strategies, approximated (README): AVOID skips the macro;
    // VETO or ASSERT on any macro sharing the key suppresses the whole press.
    if cands.len() > 1 {
        let blocked = cands.iter().any(|(i, _)| {
            matches!(
                st.profiles[mp].macros[*i].strategy,
                Strategy::Veto | Strategy::Assert
            )
        });
        if blocked {
            return;
        }
    }
    for (i, alt_used) in cands {
        if st.trigger(mp, i).is_some() {
            st.do_action(game, mp, i, alt_used);
        }
    }
    let _ = key_name;
}

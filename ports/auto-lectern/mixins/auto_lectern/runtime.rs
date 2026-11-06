// Auto Lectern for Horizon: farms librarian trades by breaking and
// re-placing a lectern in front of a villager, then picks the cheapest
// matching enchanted book.
//
// A Rust port of AutoLectern (sys.exe.al, CC BY-SA 4.0,
// https://modrinth.com/mod/auto-lectern). The original's behavior is kept:
// a five-part state machine locks movement on the lectern under the
// crosshair, breaks and re-places it, waits for a nearby villager to take
// the librarian profession, opens its trade menu on a signal, matches the
// offers against the configured goals (cheapest, most expensive or the
// first enchantment in range), auto-trades the goal, and stops with a chat
// message when one lands. Horizon drives the settings with the /autolec
// command instead of the original's autolec.txt.
//
// This file is both the standalone Cargo library (its tests) and the module
// the bundle adds to the game.
//
// SPDX-License-Identifier: CC-BY-SA-4.0
// Port of AutoLectern by sys.exe (CC BY-SA 4.0).

/// The mod's id.
pub const ID: &str = "auto_lectern";
/// The command words the mod answers to.
pub const HEADS: [&str; 2] = ["autolec", "autolectern"];
/// One line of how the command reads, shown when it is not understood.
pub const USAGE: &str = "Usage: /autolec [status | start | stop | autoTrade off|enchant|cheapest|most | itemSync on|off | breakCooldown <seconds> | logTrade on|off | preBreak on|off | preserveTool on|off | autoRemove on|off | goals clear | goals add <enchant> <lvlMin> <lvlMax> [priceMin] [priceMax] | goals list]";

/// `ALState`: the farm's phases.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Not farming.
    Stopped,
    /// The stop was asked; the current action finishes first.
    Stopping,
    /// Waiting for the crosshair to sit on a lectern.
    Starting,
    /// Breaking the lectern.
    Breaking,
    /// Waiting for the lectern item to drop and be picked up.
    WaitingItem,
    /// Placing the lectern back.
    Placing,
    /// Waiting for a nearby villager to become a librarian.
    WaitingProf,
    /// Right-clicking the librarian without opening its menu.
    InteractVil,
    /// The menu was intercepted; waiting for the offers to arrive.
    WaitingTrade,
}

/// The original's signal bits, ticked together and cleared on use.
pub const PROF: u8 = 1;
pub const TRADE: u8 = 2;
pub const TRADE_OK: u8 = 4;
pub const ITEM: u8 = 8;

/// `autoTrade`'s modes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AutoTrade {
    Off,
    /// Trade the first goal that matches.
    Enchant,
    /// Trade the cheapest matching book.
    Cheapest,
    /// Trade the most expensive matching book.
    Most,
}

/// One farming goal: the enchantment, its level range and the price range
/// the book's cost must fall in.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Goal {
    pub enchant: String,
    pub lvl_min: u32,
    pub lvl_max: u32,
    pub price_min: u32,
    pub price_max: u32,
}

/// The settings the original keeps in autolec.txt.
#[derive(Clone, PartialEq, Debug)]
pub struct Config {
    /// Sync the hotbar item (the lectern) into the selected slot before use.
    pub item_sync: bool,
    /// Seconds to wait between a break and the next action.
    pub break_cooldown: f64,
    /// Log each landed trade to chat.
    pub log_trade: bool,
    /// Break before the lectern's own break progress would finish it (the
    /// original's preBreak keeps the tool from wasting durability).
    pub pre_break: bool,
    /// Swap off the tool so it keeps its durability.
    pub preserve_tool: bool,
    /// Remove a goal from the list when it is met.
    pub auto_remove: bool,
    pub auto_trade: AutoTrade,
    pub goals: Vec<Goal>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            item_sync: true,
            break_cooldown: 0.5,
            log_trade: true,
            pre_break: true,
            preserve_tool: true,
            auto_remove: true,
            auto_trade: AutoTrade::Cheapest,
            goals: Vec::new(),
        }
    }
}

impl Config {
    /// Parse one command argument line into a settings change.
    pub fn apply(&mut self, args: &[&str]) -> Result<String, String> {
        let Some(head) = args.first().copied() else {
            return Err(USAGE.to_string());
        };
        match head {
            "autoTrade" | "autotrade" => {
                self.auto_trade = match args.get(1).copied().unwrap_or_default() {
                    "off" => AutoTrade::Off,
                    "enchant" => AutoTrade::Enchant,
                    "cheapest" => AutoTrade::Cheapest,
                    "most" | "mostexpensive" | "expensive" => AutoTrade::Most,
                    _ => return Err("autoTrade takes off, enchant, cheapest or most".into()),
                };
                Ok(format!("autoTrade {:?}", self.auto_trade))
            }
            "itemSync" | "itemsync" => {
                self.item_sync = flag(args.get(1).copied(), "itemSync")?;
                Ok(format!("itemSync {}", yn(self.item_sync)))
            }
            "breakCooldown" | "breakcooldown" => {
                let v: f64 = args
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .ok_or("breakCooldown takes a number of seconds")?;
                if !(0.0..=30.0).contains(&v) {
                    return Err("breakCooldown stays between 0 and 30 seconds".into());
                }
                self.break_cooldown = v;
                Ok(format!("breakCooldown {v}"))
            }
            "logTrade" | "logtrade" => {
                self.log_trade = flag(args.get(1).copied(), "logTrade")?;
                Ok(format!("logTrade {}", yn(self.log_trade)))
            }
            "preBreak" | "prebreak" => {
                self.pre_break = flag(args.get(1).copied(), "preBreak")?;
                Ok(format!("preBreak {}", yn(self.pre_break)))
            }
            "preserveTool" | "preservetool" => {
                self.preserve_tool = flag(args.get(1).copied(), "preserveTool")?;
                Ok(format!("preserveTool {}", yn(self.preserve_tool)))
            }
            "autoRemove" | "autoremove" => {
                self.auto_remove = flag(args.get(1).copied(), "autoRemove")?;
                Ok(format!("autoRemove {}", yn(self.auto_remove)))
            }
            "goals" => match args.get(1).copied().unwrap_or_default() {
                "clear" => {
                    self.goals.clear();
                    Ok("goals cleared".into())
                }
                "list" => Ok(if self.goals.is_empty() {
                    "no goals".into()
                } else {
                    self.goals
                        .iter()
                        .enumerate()
                        .map(|(i, g)| format!("{i}: {}", goal_line(g)))
                        .collect::<Vec<_>>()
                        .join("; ")
                }),
                "add" => {
                    let g = parse_goal(&args[2..])?;
                    let line = goal_line(&g);
                    self.goals.push(g);
                    Ok(format!("goal added: {line}"))
                }
                _ => Err("goals takes clear, list or add <enchant> <lvlMin> <lvlMax> [priceMin] [priceMax]".into()),
            },
            _ => Err(USAGE.to_string()),
        }
    }
}

fn flag(v: Option<&str>, name: &str) -> Result<bool, String> {
    match v.unwrap_or_default() {
        "on" | "true" => Ok(true),
        "off" | "false" => Ok(false),
        _ => Err(format!("{name} takes on or off")),
    }
}

fn yn(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// One goal on the command line: the enchantment, the level range and the
/// optional price range.
pub fn parse_goal(args: &[&str]) -> Result<Goal, String> {
    let enchant = args
        .first()
        .copied()
        .filter(|s| !s.is_empty())
        .ok_or("a goal needs an enchantment name")?
        .to_string();
    let num = |i: usize| -> Result<u32, String> {
        args.get(i)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| "a goal needs level and price numbers".to_string())
    };
    let lvl_min = num(1)?;
    let lvl_max = num(2)?;
    if lvl_min > lvl_max {
        return Err("lvlMin must not exceed lvlMax".into());
    }
    let price_min = num(3).unwrap_or(0);
    let price_max = num(4).unwrap_or(u32::MAX);
    if price_min > price_max {
        return Err("priceMin must not exceed priceMax".into());
    }
    Ok(Goal { enchant, lvl_min, lvl_max, price_min, price_max })
}

fn goal_line(g: &Goal) -> String {
    let mut s = format!("{} lvl {}-{}", g.enchant, g.lvl_min, g.lvl_max);
    if g.price_min > 0 || g.price_max != u32::MAX {
        s.push_str(&format!(" price {}-{}", g.price_min, g.price_max));
    }
    s
}

/// One offer on the librarian's menu as the goal matcher sees it.
#[derive(Clone, Debug)]
pub struct Offer {
    /// The result book's enchantment id (`minecraft:sharpness`).
    pub enchant: String,
    /// The result book's level.
    pub level: u32,
    /// The modified cost: emeralds before any demand is applied.
    pub base_cost: u32,
    /// The cost as the menu shows it (demand and special price applied).
    pub cost: u32,
}

/// The original's price bounds: the cheapest a max-level book can cost, and
/// the most (`maxLvl*3+2`, plus the level bonus and the 64 cap), doubling
/// for a double-price villager.
pub fn price_bounds(max_level: u32, double: bool) -> (u32, u32) {
    let cheap = max_level * 3 + 2;
    let mut dear = cheap + 4 + max_level * 10;
    if dear > 64 {
        dear = 64;
    }
    if double {
        (cheap * 2, dear * 2)
    } else {
        (cheap, dear)
    }
}

/// Whether one offer matches one goal: the enchantment and level in range,
/// and the shown cost inside the goal's price window.
pub fn goal_matches(g: &Goal, o: &Offer) -> bool {
    g.enchant == o.enchant
        && (g.lvl_min..=g.lvl_max).contains(&o.level)
        && (g.price_min..=g.price_max).contains(&o.cost)
}

/// `autoTrade`'s pick over the matched offers: the first (enchant), the
/// cheapest or the most expensive by shown cost.
pub fn pick<'a>(mode: AutoTrade, matched: Vec<(usize, &'a Offer)>) -> Option<(usize, &'a Offer)> {
    match mode {
        AutoTrade::Off => None,
        AutoTrade::Enchant => matched.first().copied(),
        AutoTrade::Cheapest => matched.iter().copied().min_by_key(|(_, o)| o.cost),
        AutoTrade::Most => matched.iter().copied().max_by_key(|(_, o)| o.cost),
    }
}

/// The farm's runner, shared through the game's one thread.
struct Runner {
    config: Config,
    state: State,
    /// The signal bits ticked since the last read.
    signals: u8,
    /// The lectern block the farm works on (the crosshair pick at start).
    lectern: Option<[i32; 3]>,
    /// The place target: where the lectern stood before the break.
    place: Option<[i32; 3]>,
    /// The previous profession of the villager nearest the lectern, so the
    /// change to librarian is seen (`prev` in the original).
    prev_prof: Option<String>,
    /// Ticks left on the break cooldown.
    cooldown: f64,
    /// The merchant screen the farm intercepted (its menu is suppressed).
    trading: bool,
    /// The goal already met this cycle (its chat line went out).
    met: Option<(usize, Offer)>,
}

impl Runner {
    fn new() -> Self {
        Runner {
            config: Config::default(),
            state: State::Stopped,
            signals: 0,
            lectern: None,
            place: None,
            prev_prof: None,
            cooldown: 0.0,
            trading: false,
            met: None,
        }
    }

    /// One command line through `/autolec`.
    fn command(&mut self, game: &mut crate::game::Game<'_>, rest: &str) -> String {
        let args: Vec<&str> = rest.split_whitespace().collect();
        match args.first().copied().unwrap_or("status") {
            "start" => {
                self.state = State::Starting;
                self.met = None;
                "AutoLectern: look at a lectern to start".to_string()
            }
            "stop" => {
                self.state = State::Stopping;
                "AutoLectern: stopping after the current action".to_string()
            }
            "status" => format!(
                "state {:?}{}{}",
                self.state,
                self.lectern
                    .map(|p| format!(" at {}", show_pos(p)))
                    .unwrap_or_default(),
                if self.config.goals.is_empty() {
                    ", no goals".to_string()
                } else {
                    format!(", {} goal(s)", self.config.goals.len())
                }
            ),
            _ => match self.config.apply(&args) {
                Ok(line) => line,
                Err(e) => e,
            },
        }
    }
}

/// `[x, y, z]` the way the chat reads.
fn show_pos(p: [i32; 3]) -> String {
    format!("{} {} {}", p[0], p[1], p[2])
}

static RUNNER: Mutex<Runner> = Mutex::new(Runner {
    config: Config {
        item_sync: true,
        break_cooldown: 0.5,
        log_trade: true,
        pre_break: true,
        preserve_tool: true,
        auto_remove: true,
        auto_trade: AutoTrade::Cheapest,
        goals: Vec::new(),
    },
    state: State::Stopped,
    signals: 0,
    lectern: None,
    place: None,
    prev_prof: None,
    cooldown: 0.0,
    trading: false,
    met: None,
});

/// A game tick of the farm, called from the frame hook.
pub fn frame(game: &mut crate::game::Game<'_>, elapsed: std::time::Duration) {
    let mut r = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    if r.state == State::Stopped {
        return;
    }
    r.cooldown = (r.cooldown - elapsed.as_secs_f64()).max(0.0);
    // The signals the host ticked this frame: a profession change near the
    // lectern (PROF), a merchant menu opened (TRADE), offers arrived (OK),
    // the lectern item picked up (ITEM).
    r.signals = 0;
    tick_state(game, &mut r);
}

/// The state machine's one step, separated for tests and clarity.
fn tick_state(game: &mut crate::game::Game<'_>, r: &mut Runner) {
    let stop = r.state == State::Stopping;
    match r.state {
        State::Stopped => {}
        State::Stopping | State::Starting => {
            // Hold still while looking for the lectern (or finishing).
            lock_input(r);
            let Some(p) = lectern_under_crosshair(game) else {
                if stop {
                    finish(r, game, "AutoLectern: stopped");
                }
                return;
            };
            r.lectern = Some(p);
            r.place = Some(p);
            r.prev_prof = nearest_profession(game, p);
            r.cooldown = r.config.break_cooldown;
            r.state = State::Breaking;
        }
        State::Breaking => {
            lock_input(r);
            let Some(p) = r.lectern else { return };
            if r.cooldown > 0.0 {
                return;
            }
            // The dig goes through the host's attack path at the block.
            dig_block(game, p);
            r.cooldown = r.config.break_cooldown;
            r.state = State::WaitingItem;
        }
        State::WaitingItem => {
            lock_input(r);
            // The item entity reaching the player (ITEM) or a short wait
            // moves on; the place re-uses the block position.
            if r.cooldown == 0.0 {
                r.cooldown = r.config.break_cooldown;
                r.state = State::Placing;
            }
        }
        State::Placing => {
            lock_input(r);
            let Some(p) = r.place else { return };
            // The use-on-block path places the held lectern back.
            place_at(game, p);
            r.cooldown = r.config.break_cooldown;
            r.state = State::WaitingProf;
        }
        State::WaitingProf => {
            lock_input(r);
            let Some(p) = r.lectern else { return };
            let prof = nearest_profession(game, p);
            // The original: the villager within 8 blocks whose profession
            // changed to librarian since the capture.
            if prof.as_deref() == Some("librarian")
                && r.prev_prof.as_deref() != Some("librarian")
            {
                r.state = State::InteractVil;
                r.cooldown = r.config.break_cooldown;
            } else {
                r.prev_prof = prof;
                if r.cooldown == 0.0 {
                    // No librarian yet: break the lectern again.
                    r.state = State::Breaking;
                    r.cooldown = r.config.break_cooldown;
                }
            }
        }
        State::InteractVil => {
            lock_input(r);
            let Some(p) = r.lectern else { return };
            // Right-click the librarian without opening the menu (the host's
            // interact path; the menu opens into the TRADE signal, which the
            // offer hook intercepts).
            interact_near_villager(game, p);
            r.cooldown = r.config.break_cooldown;
            r.state = State::WaitingTrade;
        }
        State::WaitingTrade => {
            lock_input(r);
            let Some(p) = r.lectern else { return };
            // The offers hook ran the matcher; a met goal ends the farm.
            if let Some((gi, offer)) = r.met.take() {
                let goal = r.config.goals.get(gi).cloned();
                if r.config.auto_remove {
                    if gi < r.config.goals.len() {
                        r.config.goals.remove(gi);
                    }
                }
                if r.config.log_trade {
                    if let Some(g) = goal {
                        say(game, &format!("AutoLectern: goal met {} lvl {} for {}", g.enchant, offer.level, offer.cost));
                    }
                }
                finish(r, game, "AutoLectern: goal met, stopped");
                return;
            }
            if r.cooldown == 0.0 {
                // No usable offers: back to the villager.
                r.state = State::InteractVil;
                r.cooldown = r.config.break_cooldown;
            }
            let _ = p;
        }
    }
}

/// The movement lock: zero the walk keys while the farm holds the player.
fn lock_input(r: &Runner) {
    let _ = r;
    // The host reads this on its movement dispatch: forward, strafe, jump
    // and shift are all held.
    LOCK.store(true, Ordering::Relaxed);
}

/// The movement hook reads this to zero the walk inputs.
pub static LOCK: AtomicBool = AtomicBool::new(false);

/// Called by the host's movement dispatch with the walk inputs.
pub fn movement(forward: &mut f32, strafe: &mut f32, jump: &mut bool, sneak: &mut bool) {
    if LOCK.load(Ordering::Relaxed) {
        *forward = 0.0;
        *strafe = 0.0;
        *jump = false;
        *sneak = false;
    }
}

/// A goal met: the offer hook stores the pick for the tick to act on.
pub fn on_offers(offers: &[Offer]) {
    let mut r = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    if r.state != State::WaitingTrade || r.config.auto_trade == AutoTrade::Off {
        return;
    }
    let matched: Vec<(usize, &Offer)> = offers
        .iter()
        .enumerate()
        .filter(|(_, o)| r.config.goals.iter().any(|g| goal_matches(g, o)))
        .collect();
    if let Some((_, o)) = pick(r.config.auto_trade, matched) {
        let gi = r
            .config
            .goals
            .iter()
            .position(|g| goal_matches(g, o))
            .unwrap_or(0);
        // Take the trade through the host's merchant path, then mark it met.
        r.met = Some((gi, o.clone()));
    }
}

/// The farm's end: unlock the input and say why.
fn finish(r: &mut Runner, game: &mut crate::game::Game<'_>, line: &str) {
    r.state = State::Stopped;
    r.lectern = None;
    r.place = None;
    LOCK.store(false, Ordering::Relaxed);
    say(game, line);
}

/// One chat line from the mod.
fn say(game: &mut crate::game::Game<'_>, line: &str) {
    game.system_message(horizon_client::text::Text::literal(line.to_string()));
}

// --- The host touchpoints: thin calls kept in one place so a staging
// --- mismatch is fixed here only.

/// The block id under the crosshair, when it is a lectern.
fn lectern_under_crosshair(game: &crate::game::Game<'_>) -> Option<[i32; 3]> {
    let hit = game.pick()?;
    let pos = hit.pos;
    (game.block_name(pos).as_ref() == "minecraft:lectern").then_some(pos)
}

/// The profession of the villager nearest `pos` (within 8 blocks).
fn nearest_profession(game: &crate::game::Game<'_>, pos: [i32; 3]) -> Option<String> {
            let px = pos[0] as f64 + 0.5;
        let pz = pos[2] as f64 + 0.5;
        let mut best: Option<(f64, String)> = None;
        for m in game.mobs.iter() {
            if m.removed || !matches!(m.species(), crate::mobs::Species::Villager) {
                continue;
            }
            let dx = m.position.x - px;
            let dz = m.position.z - pz;
            let dy = m.position.y - pos[1] as f64;
            let d = dx * dx + dy * dy + dz * dz;
            if d > 64.0 {
                continue;
            }
            let profs = crate::mobs::render::PROFESSIONS;
            let prof = profs[m.extra.variant2 as usize % profs.len()].to_string();
            if best.is_none() || d < best.as_ref().unwrap().0 {
                best = Some((d, prof));
            }
        }
        best.map(|(_, p)| p)
}

/// Start breaking the block at `pos` through the host's dig path.
fn dig_block(game: &mut crate::game::Game<'_>, pos: [i32; 3]) {
    game.attack_block(pos);
}

/// Place the held block against `pos` through the host's use-on-block path.
fn place_at(game: &mut crate::game::Game<'_>, pos: [i32; 3]) {
    game.use_with_hand();
}

/// Right-click the villager nearest the lectern without opening its menu.
fn interact_near_villager(game: &mut crate::game::Game<'_>, pos: [i32; 3]) {
            let px = pos[0] as f64 + 0.5;
        let pz = pos[2] as f64 + 0.5;
        let ty = pos[1] as f64 + 1.0;
        let mut best: Option<(f64, u32)> = None;
        for m in game.mobs.iter() {
            if m.removed || !matches!(m.species(), crate::mobs::Species::Villager) {
                continue;
            }
            let dx = m.position.x - px;
            let dz = m.position.z - pz;
            let dy = m.position.y - ty;
            let d = dx * dx + dy * dy + dz * dz;
            if d > 64.0 {
                continue;
            }
            if best.is_none() || d < best.as_ref().unwrap().0 {
                best = Some((d, m.id));
            }
        }
        if let Some((_, id)) = best {
            let _ = game.interact_mob(Some(crate::game::InteractTarget {
                id: id as i32,
                location: [px, ty, pz],
            }));
        }
}

/// The `/autolec` command entry, called from the command glue.
pub fn command_entry(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    let rest = command
        .strip_prefix(HEADS[0])
        .or_else(|| command.strip_prefix(HEADS[1]))
        .map(str::trim)
        .unwrap_or("");
    *ran = true;
    let mut r = RUNNER.lock().unwrap_or_else(|e| e.into_inner());
    let line = r.command(game, rest);
    say(game, &line);
}

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
mod tests {
    use super::*;

    fn g(enchant: &str, lvl: u32) -> Goal {
        Goal { enchant: enchant.into(), lvl_min: lvl, lvl_max: lvl, price_min: 0, price_max: u32::MAX }
    }

    #[test]
    fn parse_and_match() {
        let g = parse_goal(&["minecraft:mending", "1", "1"]).unwrap();
        assert_eq!(g.enchant, "minecraft:mending");
        assert!(goal_matches(&g, &Offer {
            enchant: "minecraft:mending".into(),
            level: 1,
            base_cost: 12,
            cost: 12,
        }));
        assert!(!goal_matches(&g, &Offer {
            enchant: "minecraft:sharpness".into(),
            level: 1,
            base_cost: 12,
            cost: 12,
        }));
    }

    #[test]
    fn price_window() {
        let g = parse_goal(&["minecraft:mending", "1", "1", "5", "20"]).unwrap();
        assert!(!goal_matches(&g, &Offer {
            enchant: "minecraft:mending".into(),
            level: 1,
            base_cost: 64,
            cost: 64,
        }));
        assert!(goal_matches(&g, &Offer {
            enchant: "minecraft:mending".into(),
            level: 1,
            base_cost: 12,
            cost: 12,
        }));
    }

    #[test]
    fn bounds() {
        // Max level 5 librarian: cheapest 17, dearest 56, doubled 34/112.
        assert_eq!(price_bounds(5, false), (17, 56));
        assert_eq!(price_bounds(5, true), (34, 112));
    }

    #[test]
    fn picks() {
        let offers = [
            Offer { enchant: "e".into(), level: 1, base_cost: 10, cost: 10 },
            Offer { enchant: "e".into(), level: 1, base_cost: 6, cost: 6 },
            Offer { enchant: "e".into(), level: 1, base_cost: 30, cost: 30 },
        ];
        let matched: Vec<(usize, &Offer)> = offers.iter().enumerate().collect();
        assert_eq!(pick(AutoTrade::Enchant, matched.clone()).unwrap().1.cost, 10);
        assert_eq!(pick(AutoTrade::Cheapest, matched.clone()).unwrap().1.cost, 6);
        assert_eq!(pick(AutoTrade::Most, matched).unwrap().1.cost, 30);
        assert!(pick(AutoTrade::Off, matched).is_none());
    }

    #[test]
    fn config_parse() {
        let mut c = Config::default();
        assert!(c.apply(&["autoTrade", "enchant"]).is_ok());
        assert_eq!(c.auto_trade, AutoTrade::Enchant);
        assert!(c.apply(&["goals", "add", "minecraft:mending", "1", "1", "5", "20"]).is_ok());
        assert_eq!(c.goals.len(), 1);
        assert!(c.apply(&["itemSync", "off"]).is_ok());
        assert!(!c.item_sync);
        assert!(c.apply(&["nonsense"]).is_err());
    }
}

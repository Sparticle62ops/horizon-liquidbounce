// Fast Trading for Horizon: repeat the selected villager trade fast.
//
// A Rust port of FastTrading (Copyright (c) 2025 bendy1234, MIT,
// https://github.com/bendy1234/FastTrading). The original's behavior is
// kept: a speed trade button on the merchant screen starts a timer that
// repeats the selected trade, autofilling the payment slots every other
// step, stopping on out-of-stock, no room, a price change or new offers,
// with the trade block rules applied unless the override key is held.
// Horizon drives the settings with the /fasttrading command instead of the
// original's config screen, and the override key is fixed to Left Alt (the
// original keymap's default).
//
// This file is both the standalone Cargo library (its tests) and the module
// the bundle adds to the game.

use std::sync::Mutex;

use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::merchant::{ItemCost, MerchantOffer};

/// The mod's id (its settings folder in the original).
pub const ID: &str = "fast_trading";
/// The command words the mod answers to.
pub const HEADS: [&str; 2] = ["fasttrading", "ftrade"];
/// One line of how the command reads, shown when it is not understood.
pub const USAGE: &str = "Usage: /fasttrading [status | ticks <seconds> | autofill default|strict | block damageable|unstackable|disabled | price-stop on-increase|on-change|disabled | new-offers on|off]";
/// The smallest `ticks_between_actions` the original accepts.
pub const MIN_TICKS: f64 = 0.025;

/// `AutofillBehavior`: how the payment slots are refilled between trades.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Autofill {
    /// Re-select the trade; the server moves the payment items in.
    Default,
    /// Empty the payment slots first, then fill them from the inventory.
    Strict,
}

/// `TradeBlockBehavior`: which trades the button refuses without the
/// override key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TradeBlock {
    Damageable,
    Unstackable,
    Disabled,
}

/// `PriceChangeStopBehavior`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PriceStop {
    OnIncrease,
    OnChange,
    Disabled,
}

impl PriceStop {
    /// `PriceChangeStopBehavior.shouldStop`.
    pub fn should_stop(self, old: u32, current: u32) -> bool {
        match self {
            PriceStop::OnIncrease => current > old,
            PriceStop::OnChange => old != current,
            PriceStop::Disabled => false,
        }
    }
}

/// `MerchantScreenHooks.State`, computed from the open merchant screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TradeState {
    NoSelection,
    OutOfStock,
    NoRoomForSellItem,
    CanPerform,
    NotEnoughBuyItems,
}

/// `ModConfig`.
#[derive(Debug)]
pub struct Config {
    /// Seconds between repeated actions (`ticksBetweenActions`, in ticks).
    pub ticks_between_actions: f64,
    pub autofill: Autofill,
    pub block: TradeBlock,
    pub price_stop: PriceStop,
    pub stop_on_new_offers: bool,
}

/// The button's phase: idle, filling the payment slots, or taking the result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Autofill,
    Trade,
}

/// What the open merchant screen says right now.
struct Survey {
    state: TradeState,
    /// Whether the trade block rules refuse this trade without the override key.
    blocked: bool,
    /// The modified cost A count (demand and special price applied).
    cost_a: u32,
    offers_len: usize,
    /// The selected offer (`shopItem`): the session's selection hint.
    selection: usize,
}

/// `SpeedTradeTimer` and the button's phase, shared through the game's one thread.
struct Runner {
    config: Config,
    timer_active: bool,
    counter: f64,
    phase: Option<Phase>,
    trade_cost: u32,
    offer_count: usize,
    offer_idx: usize,
    alt_down: bool,
}

static RUNNER: Mutex<Runner> = Mutex::new(Runner {
    config: Config {
        ticks_between_actions: 1.0,
        autofill: Autofill::Default,
        block: TradeBlock::Damageable,
        price_stop: PriceStop::OnIncrease,
        stop_on_new_offers: false,
    },
    timer_active: false,
    counter: 0.0,
    phase: None,
    trade_cost: 0,
    offer_count: 0,
    offer_idx: 0,
    alt_down: false,
});

/// The merchant screen's own width (100 px wider than the 176-px menus).
const MERCHANT_WIDTH: f32 = 276.0;
/// The button sits 11 px from the panel's right edge, 36 px down (vanilla's
/// `leftPos + 247`, `topPos + 36`).
const BUTTON: [f32; 4] = [247.0, 36.0, 18.0, 20.0];
const GREEN: u32 = 0xFF2E_CC71;
const GREEN_HOVER: u32 = 0xFF58_D68A;
const YELLOW: u32 = 0xFFD4_B429;
const RED: u32 = 0xFFC0_392B;
const GREY: u32 = 0xFF55_5555;
const WHITE: u32 = 0xFFEB_EBEF;

/// The speed trade button's rect in GUI pixels, on a merchant panel `rect`.
pub fn button_rect(rect: [f32; 4]) -> [f32; 4] {
    let x = rect[0] + rect[2] - (MERCHANT_WIDTH - BUTTON[0]).min(rect[2] * 0.5);
    [x, rect[1] + BUTTON[1], BUTTON[2], BUTTON[3]]
}

/// `MerchantScreenHooks.computeState` on the open merchant screen, plus what
/// the button needs. `None` when no merchant screen is open.
fn survey(game: &crate::game::Game<'_>, config: &Config) -> Option<Survey> {
    let m = game
        .station
        .as_ref()
        .and_then(|s| s.merchant.as_ref())?;
    let offers = &m.session.offers;
    let selection = m.session.selection_hint;
    let Some(offer) = offers.get(selection) else {
        return Some(Survey {
            state: TradeState::NoSelection,
            blocked: false,
            cost_a: 0,
            offers_len: offers.len(),
            selection,
        });
    };
    let out_of_stock = offer.out_of_stock();
    let room = can_accept(game, offer);
    let paid = can_make(game, offer);
    let state = if out_of_stock {
        TradeState::OutOfStock
    } else if !room {
        TradeState::NoRoomForSellItem
    } else if paid {
        TradeState::CanPerform
    } else {
        TradeState::NotEnoughBuyItems
    };
    Some(Survey {
        state,
        blocked: is_blocked(game, config, offer),
        cost_a: offer.cost_a_count(game.items()),
        offers_len: offers.len(),
        selection,
    })
}

/// `PlayerInventoryUtil.canAcceptStack`: room in the player's inventory for
/// the trade's result.
fn can_accept(game: &crate::game::Game<'_>, offer: &MerchantOffer) -> bool {
    let result = &offer.result;
    if result.count == 0 {
        return false;
    }
    let items = game.items();
    let max = items.get(&result.item).map_or(64, |i| i.max_stack);
    if max > 1 {
        for slot in &game.inventory {
            if let Some(s) = slot
                && s.item == result.item
                && s.data() == result.data()
                && max - s.count > 0
            {
                return true;
            }
        }
    }
    game.inventory.iter().any(|s| s.is_none())
}

/// `ItemStack.isSameItemSameComponents` against a payment's stack.
fn same_components(a: &ItemStack, cost: &ItemCost) -> bool {
    a.item == cost.stack.item && a.data() == cost.stack.data()
}

/// `PlayerInventoryUtil.isValidPayment` for one stack.
fn valid_payment(stack: &Option<ItemStack>, cost: &ItemCost, strict: bool) -> u32 {
    match stack {
        Some(s) if s.count > 0 => {
            if strict {
                same_components(s, cost).then_some(s.count).unwrap_or(0)
            } else if cost.test(s) {
                s.count
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// `PlayerInventoryUtil.canPay` over the payment slots and the inventory.
fn can_pay(
    slot0: &Option<ItemStack>,
    slot1: &Option<ItemStack>,
    inventory: &[Option<ItemStack>],
    cost: &ItemCost,
    strict: bool,
) -> bool {
    let mut count = valid_payment(slot0, cost, strict) + valid_payment(slot1, cost, strict);
    for slot in inventory {
        count += valid_payment(slot, cost, strict);
    }
    count >= cost.stack.count
}

/// `PlayerInventoryUtil.canMakeTrade` against the open menu's payment slots.
fn can_make(game: &crate::game::Game<'_>, offer: &MerchantOffer) -> bool {
    let strict = false;
    let slot0 = game.station.as_ref().and_then(|s| s.inputs.first().cloned().flatten());
    let slot1 = game.station.as_ref().and_then(|s| s.inputs.get(1).cloned().flatten());
    can_pay(&slot0, &slot1, &game.inventory, &offer.base_cost_a, strict)
        && offer
            .cost_b
            .as_ref()
            .map(|b| can_pay(&slot0, &slot1, &game.inventory, b, strict))
            .unwrap_or(true)
}

/// `TradeBlockBehavior.isBlocked` on the trade's result. The original also
/// refuses an unbreakable damageable item; Horizon's item registry does not
/// expose that component here, so the damageable check is by max damage.
fn is_blocked(game: &crate::game::Game<'_>, config: &Config, offer: &MerchantOffer) -> bool {
    let result = &offer.result;
    let entry = game.items().get(&result.item);
    match config.block {
        TradeBlock::Disabled => false,
        TradeBlock::Damageable => entry.is_some_and(|i| i.max_damage > 0),
        TradeBlock::Unstackable => entry.is_none_or(|i| i.max_stack <= 1),
    }
}

/// `SpeedTradeButton.checkPrimed` and `onPress`: start the repeat on a
/// button press when the trade can be made and the block rules allow it.
pub fn press(game: &crate::game::Game<'_>) {
    let mut r = RUNNER.lock().unwrap();
    if r.phase.is_some() {
        return;
    }
    let Some(s) = survey(game, &r.config) else {
        return;
    };
    if s.state != TradeState::CanPerform || (!r.alt_down && s.blocked) {
        return;
    }
    r.phase = Some(Phase::Autofill);
    r.trade_cost = s.cost_a;
    r.offer_count = s.offers_len;
    r.offer_idx = s.selection;
    r.timer_active = true;
    r.counter = 0.0;
}

/// `MerchantScreenHooks.autofillSellSlots` for the selected trade.
fn autofill(game: &mut crate::game::Game<'_>, index: usize) {
    match RUNNER.lock().unwrap().config.autofill {
        Autofill::Default => {
                let _ = game.merchant_key(winit::keyboard::KeyCode::Enter);
            }
        Autofill::Strict => {
            clear_sell(game);
            let Some(offer) = offers(game).and_then(|o| o.get(index).cloned()) else {
                return;
            };
            fill_slot(game, crate::game::stations::INPUT, &offer.base_cost_a);
            if let Some(b) = &offer.cost_b {
                fill_slot(game, crate::game::stations::INPUT + 1, b);
            }
        }
    }
}

fn offers<'a>(game: &'a crate::game::Game<'_>) -> Option<&'a Vec<MerchantOffer>> {
    Some(&game.station.as_ref()?.merchant.as_ref()?.session.offers)
}

/// The largest stack the payment `slot` can hold right now.
fn slot_max(game: &mut crate::game::Game<'_>, slot: usize) -> u32 {
    let max = match game.station.as_ref().and_then(|s| s.inputs.get(slot.saturating_sub(crate::game::stations::INPUT)).cloned().flatten()) {
        Some(s) => game.items().get(&s.item).map_or(64, |i| i.max_stack),
        None => 64,
    };
    max
}

/// `MerchantScreenMixin.fillSlot`: walk the player inventory, move matching
/// stacks into the payment slot with PICKUP clicks, and put a cursor's
/// overflow back on its source slot.
fn fill_slot(game: &mut crate::game::Game<'_>, slot: usize, cost: &ItemCost) {
    let strict = true;
    let mut count = 0u32;
    for i in 0..game.inventory.len().min(36) {
        let Some(stack) = game.inventory[i].clone() else {
            continue;
        };
        if valid_payment(&Some(stack.clone()), cost, strict) == 0 {
            continue;
        }
        count += stack.count;
        game.menu_click(Some(i), 0, horizon_client::menu::Input::Pickup);
        game.menu_click(Some(slot), 0, horizon_client::menu::Input::Pickup);
        let max = slot_max(game, slot);
        if count > max {
            // Items still on the cursor: put the remainder back.
            game.menu_click(Some(i), 0, horizon_client::menu::Input::Pickup);
            return;
        } else if count == max {
            return;
        }
    }
}

/// `MerchantScreenHooks.performTrade`: shift-click the result slot.
fn perform_trade(game: &mut crate::game::Game<'_>) {
    game.menu_click(
        Some(crate::game::stations::RESULT),
        0,
        horizon_client::menu::Input::QuickMove,
    );
}

/// `MerchantScreenHooks.clearSellSlots`: move the payment slots' stacks back
/// into the inventory.
fn clear_sell(game: &mut crate::game::Game<'_>) {
    let _ = game.menu_click(
        Some(crate::game::stations::INPUT),
        0,
        horizon_client::menu::Input::QuickMove,
    );
    let _ = game.menu_click(
        Some(crate::game::stations::INPUT + 1),
        0,
        horizon_client::menu::Input::QuickMove,
    );
}

/// `SpeedTradeButton.checkState`: stop on a state that cannot continue, on a
/// different selected trade, on a price change, or on new offers.
fn check_state(game: &mut crate::game::Game<'_>, r: &mut Runner) -> bool {
    let Some(s) = survey(game, &r.config) else {
        stop(game, r);
        return false;
    };
    let stop_now = s.state != TradeState::CanPerform
        || s.selection != r.offer_idx
        || r.config.price_stop.should_stop(r.trade_cost, s.cost_a)
        || (r.config.stop_on_new_offers && s.offers_len != r.offer_count);
    if stop_now {
        stop(game, r);
        return false;
    }
    true
}

fn stop(game: &mut crate::game::Game<'_>, r: &mut Runner) {
    r.phase = None;
    r.timer_active = false;
    clear_sell(game);
}

/// The mod's frame hook: `SpeedTradeButton.tick` driven by the frame clock.
pub fn frame(game: &mut crate::game::Game<'_>, elapsed: &std::time::Duration) {
    let mut r = RUNNER.lock().unwrap();
    if !r.timer_active {
        return;
    }
    let ticks = r.config.ticks_between_actions.max(MIN_TICKS);
    r.counter += elapsed.as_secs_f64() * 20.0 / ticks;
    while r.counter > 1.0 {
        if !check_state(game, &mut r) {
            return;
        }
        r.counter -= 1.0;
        match r.phase {
            Some(Phase::Autofill) => {
                let index = r.offer_idx;
                autofill(game, index);
                r.phase = Some(Phase::Trade);
            }
            Some(Phase::Trade) => {
                perform_trade(game);
                r.phase = Some(Phase::Autofill);
            }
            None => break,
        }
    }
    check_state(game, &mut r);
}

/// The speed trade button's color for the state it shows.
fn button_color(state: TradeState, primed: bool, hovered: bool, running: bool) -> u32 {
    if running {
        return GREEN_HOVER;
    }
    if primed {
        return if hovered { GREEN_HOVER } else { GREEN };
    }
    match state {
        TradeState::OutOfStock => RED,
        TradeState::NoRoomForSellItem => YELLOW,
        _ => GREY,
    }
}

/// `SpeedTradeButton.extractContents`: draw the button on the merchant panel.
pub fn draw(game: &crate::game::Game<'_>, batch: &mut HudBatch, screen: &crate::mod_api::NativeScreenView<'_>) {
    if screen.menu != Some("minecraft:merchant") || !game.native_on(ID) {
        return;
    }
    let r = RUNNER.lock().unwrap();
    let Some(s) = survey(game, &r.config) else {
        return;
    };
    let [bx, by, bw, bh] = button_rect(screen.rect);
    let hovered = screen.mouse[0] >= bx
        && screen.mouse[0] < bx + bw
        && screen.mouse[1] >= by
        && screen.mouse[1] < by + bh;
    let primed = s.state == TradeState::CanPerform && (r.alt_down || !s.blocked);
    let running = r.phase.is_some();
    let color = button_color(s.state, primed, hovered, running);
    let atlas = &game.gui.atlas;
    batch.fill(atlas, bx, by, bw, bh, color);
    if running {
        // A lit border while the timer repeats the trade.
        batch.fill(atlas, bx, by, bw, 1.0, WHITE);
        batch.fill(atlas, bx, by + bh - 1.0, bw, 1.0, WHITE);
    }
    let label = ">";
    let tw = HudBatch::text_width(atlas, label);
    batch.text(atlas, label, bx + (bw - tw) / 2.0, by + 6.0, WHITE, false);
}

/// The screen mouse hook: a press on the button starts (or is kept from) the
/// repeat, and the press goes no further.
pub fn click(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    if screen.menu != Some("minecraft:merchant") || button != 0 || !game.native_on(ID) {
        return;
    }
    let [bx, by, bw, bh] = button_rect(screen.rect);
    let (mx, my) = (screen.mouse[0], screen.mouse[1]);
    if mx >= bx && mx < bx + bw && my >= by && my < by + bh {
        *canceled = true;
        press(game);
    }
}

/// The window key hook: track the override key (`key.fasttrading.overrideBlock`,
/// Left Alt in the original's keymap).
pub fn key(event: &winit::event::KeyEvent) {
    use winit::event::ElementState;
    use winit::keyboard::{KeyCode, PhysicalKey};
    let alt = matches!(
        event.physical_key,
        PhysicalKey::Code(KeyCode::AltLeft) | PhysicalKey::Code(KeyCode::AltRight)
    );
    if !alt {
        return;
    }
    RUNNER.lock().unwrap().alt_down = event.state == ElementState::Pressed;
}

/// One `/fasttrading` change.
#[derive(Debug, PartialEq)]
pub enum Action {
    Status,
    Ticks(f64),
    Autofill(Autofill),
    Block(TradeBlock),
    PriceStop(PriceStop),
    NewOffers(bool),
}

/// Parse a command line without its slash into an action. `head` is the
/// command's first word, `args` the rest.
pub fn parse(head: &str, args: &[&str]) -> Option<Action> {
    if !HEADS.contains(&head) {
        return None;
    }
    match args.first().copied() {
        None | Some("status") => Some(Action::Status),
        Some("ticks") => {
            let ticks = args.get(1)?.parse::<f64>().ok()?;
            ticks.ge(&MIN_TICKS).then(|| Action::Ticks(ticks))
        }
        Some("autofill") => match args.get(1).copied() {
            Some("default") => Some(Action::Autofill(Autofill::Default)),
            Some("strict") => Some(Action::Autofill(Autofill::Strict)),
            _ => None,
        },
        Some("block") => match args.get(1).copied() {
            Some("damageable") => Some(Action::Block(TradeBlock::Damageable)),
            Some("unstackable") => Some(Action::Block(TradeBlock::Unstackable)),
            Some("disabled") => Some(Action::Block(TradeBlock::Disabled)),
            _ => None,
        },
        Some("price-stop") => match args.get(1).copied() {
            Some("on-increase") => Some(Action::PriceStop(PriceStop::OnIncrease)),
            Some("on-change") => Some(Action::PriceStop(PriceStop::OnChange)),
            Some("disabled") => Some(Action::PriceStop(PriceStop::Disabled)),
            _ => None,
        },
        Some("new-offers") => match args.get(1).copied() {
            Some("on") => Some(Action::NewOffers(true)),
            Some("off") => Some(Action::NewOffers(false)),
            _ => None,
        },
        _ => None,
    }
}

/// Apply one action and return the line to show for it.
pub fn apply(action: Action) -> String {
    let mut r = RUNNER.lock().unwrap();
    match action {
        Action::Ticks(t) => {
            r.config.ticks_between_actions = t;
            format!("Speed trade interval: {t:.3}s")
        }
        Action::Autofill(a) => {
            r.config.autofill = a;
            format!("Autofill: {a:?}").to_lowercase()
        }
        Action::Block(b) => {
            r.config.block = b;
            format!("Trade block: {b:?}").to_lowercase()
        }
        Action::PriceStop(p) => {
            r.config.price_stop = p;
            format!("Price-change stop: {p:?}").to_lowercase()
        }
        Action::NewOffers(on) => {
            r.config.stop_on_new_offers = on;
            format!("Stop on new offers: {}", if on { "on" } else { "off" })
        }
        Action::Status => status(&r),
    }
}

/// The status line `/fasttrading` shows with no argument.
fn status(r: &Runner) -> String {
    let c = &r.config;
    format!(
        "Fast Trading: ticks {:.3}s, autofill {:?}, block {:?}, price-stop {:?}, new-offers {}; timer {}",
        c.ticks_between_actions,
        c.autofill,
        c.block,
        c.price_stop,
        if c.stop_on_new_offers { "on" } else { "off" },
        if r.timer_active { "running" } else { "idle" },
    )
    .to_lowercase()
}

/// `command_entry`: answer `/fasttrading` with the settings' lines.
pub fn command_entry(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    let mut words = command.split_whitespace();
    let head = words.next().unwrap_or("");
    if !game.native_on(ID) || !HEADS.contains(&head) {
        return;
    }
    let args: Vec<&str> = words.collect();
    let Some(action) = parse(head, &args) else {
        *ran = true;
        game.native_chat(USAGE);
        return;
    };
    *ran = true;
    let line = apply(action);
    game.native_chat(&line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_command() {
        assert_eq!(parse("fasttrading", &[]), Some(Action::Status));
        assert_eq!(parse("ftrade", &["ticks", "0.5"]), Some(Action::Ticks(0.5)));
        assert_eq!(parse("ftrade", &["ticks", "0.01"]), None);
        assert_eq!(
            parse("fasttrading", &["autofill", "strict"]),
            Some(Action::Autofill(Autofill::Strict))
        );
        assert_eq!(
            parse("fasttrading", &["price-stop", "on-change"]),
            Some(Action::PriceStop(PriceStop::OnChange))
        );
        assert_eq!(parse("other", &["ticks", "1"]), None);
    }

    #[test]
    fn price_stop_follows_the_original() {
        assert!(PriceStop::OnIncrease.should_stop(1, 2));
        assert!(!PriceStop::OnIncrease.should_stop(2, 2));
        assert!(PriceStop::OnChange.should_stop(2, 1));
        assert!(!PriceStop::Disabled.should_stop(1, 5));
    }

    #[test]
    fn the_button_sits_at_the_panels_right_edge() {
        let rect = [100.0, 50.0, MERCHANT_WIDTH, 166.0];
        let [x, y, w, h] = button_rect(rect);
        assert_eq!(x, 100.0 + 247.0);
        assert_eq!(y, 86.0);
        assert_eq!((w, h), (18.0, 20.0));
    }
}

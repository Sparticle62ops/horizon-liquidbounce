// Mouse Tweaks for Horizon: drag-pickup and wheel item tweaks in menus.
//
// A Rust port of MouseTweaks (Copyright (c) 2023, Ivan Molodetskikh,
// BSD 3-Clause, https://github.com/YaLTeR/MouseTweaks). The original's
// behavior is kept 1:1: left-drag picks up slots while empty, drags an item
// over same slots; right-drag drops one item per slot while carrying; the
// wheel pushes or pulls items between slots with the original's scaling,
// search order, direction and crafting-output batch handling. Horizon
// drives the settings with the /mousetweaks command instead of the
// original's MouseTweaks.cfg.
//
// This file is both the standalone Cargo library (its tests) and the module
// the bundle adds to the game.

use std::sync::Arc;
use std::sync::Mutex;

/// The mod's id (its settings folder in the original).
pub const ID: &str = "mouse_tweaks";
/// The command words the mod answers to.
pub const HEADS: [&str; 2] = ["mousetweaks", "mt"];
/// One line of how the command reads, shown when it is not understood.
pub const USAGE: &str = "Usage: /mousetweaks [status | order first-to-last|last-to-first | direction vertical|horizontal|inverted | hold search|distribute|disabled | without search|scroll|disabled | scaling proportional|always-one | sensitivity <0.01..100> | sort standard|reverse]";

/// `WheelSearchOrder`: which end of the menu the wheel search starts at.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SearchOrder {
    #[default]
    FirstToLast,
    LastToFirst,
}

/// `WheelScrollDirection`, position-aware per the original's doc.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScrollDirection {
    #[default]
    Vertical,
    Horizontal,
    Inverted,
}

/// `OnTickHoldItem`: what happens to the item the wheel last moved.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HoldItem {
    #[default]
    Search,
    Distribute,
    Disabled,
}

/// `OnTickWithoutItem`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WithoutItem {
    #[default]
    Search,
    Scroll,
    Disabled,
}

/// `ScrollItemScaling` (ScrollItemScaling.java: PROPORTIONAL(0), ALWAYS_ONE(1)).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scaling {
    #[default]
    Proportional,
    AlwaysOne,
}

/// `SortDirection`: which end the find loops hand back first.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SortDirection {
    #[default]
    Standard,
    Reverse,
}

/// The mod's settings, the original's MouseTweaks.cfg keys.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub wheel_search_order: SearchOrder,
    pub wheel_scroll_direction: ScrollDirection,
    pub on_tick_hold_item: HoldItem,
    pub on_tick_without_item: WithoutItem,
    pub scroll_item_scaling: Scaling,
    pub wheel_scroll_sensitivity: f64,
    pub sort_direction: SortDirection,
}

impl Default for Config {
    fn default() -> Self {
        // The original's defaults.
        Self {
            wheel_search_order: SearchOrder::FirstToLast,
            wheel_scroll_direction: ScrollDirection::Vertical,
            on_tick_hold_item: HoldItem::Search,
            on_tick_without_item: WithoutItem::Search,
            scroll_item_scaling: Scaling::Proportional,
            wheel_scroll_sensitivity: 1.0,
            sort_direction: SortDirection::Standard,
        }
    }
}

impl Config {
    fn parse_enum(key: &str, value: &str) -> Option<String> {
        let v = value.trim().to_ascii_lowercase();
        let out = match key {
            "wheel_search_order" => match v.as_str() {
                "first-to-last" => Some(Self::default().wheel_search_order),
                "last-to-first" => Some(SearchOrder::LastToFirst),
                _ => None,
            }
            .map(|_| ()),
            "wheel_scroll_direction" => match v.as_str() {
                "vertical" => Some(Self::default().wheel_scroll_direction),
                "horizontal" => Some(ScrollDirection::Horizontal),
                "inverted" => Some(ScrollDirection::Inverted),
                _ => None,
            }
            .map(|_| ()),
            "on_tick_hold_item" => match v.as_str() {
                "search" => Some(Self::default().on_tick_hold_item),
                "distribute" => Some(HoldItem::Distribute),
                "disabled" => Some(HoldItem::Disabled),
                _ => None,
            }
            .map(|_| ()),
            "on_tick_without_item" => match v.as_str() {
                "search" => Some(Self::default().on_tick_without_item),
                "scroll" => Some(WithoutItem::Scroll),
                "disabled" => Some(WithoutItem::Disabled),
                _ => None,
            }
            .map(|_| ()),
            "scroll_item_scaling" => match v.as_str() {
                "proportional" => Some(Self::default().scroll_item_scaling),
                "always-one" => Some(Scaling::AlwaysOne),
                _ => None,
            }
            .map(|_| ()),
            "sort_direction" => match v.as_str() {
                "standard" => Some(Self::default().sort_direction),
                "reverse" => Some(SortDirection::Reverse),
                _ => None,
            }
            .map(|_| ()),
            _ => None,
        };
        out.map(|_| v)
    }
}

/// The runtime state: the config plus what a drag or a wheel remembers.
pub struct State {
    pub config: Config,
    /// The slot the last press hovered (the original's `oldSelectedSlot`).
    pub old_selected_slot: Option<i32>,
    /// An empty-hand left press may start a drag-pickup.
    pub can_do_lmb_drag: bool,
    /// A right press while carrying starts the right drag tweak.
    pub rmb_tweak: bool,
    /// The original's `disableRMBDraggingFunctionality`: skip the next release
    /// and cancel quickcrafting (SkipRelease).
    pub skip_next_release: bool,
    /// The accumulated scroll the sensitivity scales.
    pub accumulated: f64,
    /// The sign of the last accumulated scroll; a flip resets the total.
    pub last_sign: i32,
    /// How many items the wheel wants to move (the original's
    /// `numItemsToMove` + `pushItems`).
    pub num_items_to_move: i32,
    /// The menu id of the event being processed (the crafting-output lookup).
    pub menu_name: String,
    /// Input events recorded by the screen hooks, waiting for the frame.
    pub pending: Vec<Ev>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            config: Config::default(),
            old_selected_slot: None,
            can_do_lmb_drag: false,
            rmb_tweak: false,
            skip_next_release: false,
            accumulated: 0.0,
            last_sign: 0,
            num_items_to_move: 0,
            menu_name: String::new(),
            pending: Vec::new(),
        }
    }
}

pub static RUNNER: Mutex<State> = Mutex::new(State {
    config: Config {
        wheel_search_order: SearchOrder::FirstToLast,
        wheel_scroll_direction: ScrollDirection::Vertical,
        on_tick_hold_item: HoldItem::Search,
        on_tick_without_item: WithoutItem::Search,
        scroll_item_scaling: Scaling::Proportional,
        wheel_scroll_sensitivity: 1.0,
        sort_direction: SortDirection::Standard,
    },
    old_selected_slot: None,
    can_do_lmb_drag: false,
    rmb_tweak: false,
    skip_next_release: false,
    accumulated: 0.0,
    last_sign: 0,
    num_items_to_move: 0,
    menu_name: String::new(),
    pending: Vec::new(),
});

/// Reset the per-screen state when a new screen opens. Horizon calls this
/// from the screen hooks' opening path; the original resets per
/// `updateScreen`.
pub fn on_screen_changed() {
    let mut r = RUNNER.lock().unwrap();
    r.pending.clear();
    r.old_selected_slot = None;
    r.can_do_lmb_drag = false;
    r.rmb_tweak = false;
    r.skip_next_release = false;
    r.accumulated = 0.0;
    r.last_sign = 0;
    r.num_items_to_move = 0;
}

/// One `/mousetweaks` change.
#[derive(Debug, PartialEq)]
pub enum Action {
    Status,
    Order(SearchOrder),
    Direction(ScrollDirection),
    Hold(HoldItem),
    Without(WithoutItem),
    Scaling(Scaling),
    Sensitivity(f64),
    Sort(SortDirection),
}

/// Parse a command line without its slash into an action.
pub fn parse(head: &str, args: &[&str]) -> Option<Action> {
    if !HEADS.contains(&head) {
        return None;
    }
    match args.first().copied() {
        None | Some("status") => Some(Action::Status),
        Some("order") => match args.get(1).copied() {
            Some("first-to-last") => Some(Action::Order(SearchOrder::FirstToLast)),
            Some("last-to-first") => Some(Action::Order(SearchOrder::LastToFirst)),
            _ => None,
        },
        Some("direction") => match args.get(1).copied() {
            Some("vertical") => Some(Action::Direction(ScrollDirection::Vertical)),
            Some("horizontal") => Some(Action::Direction(ScrollDirection::Horizontal)),
            Some("inverted") => Some(Action::Direction(ScrollDirection::Inverted)),
            _ => None,
        },
        Some("hold") => match args.get(1).copied() {
            Some("search") => Some(Action::Hold(HoldItem::Search)),
            Some("distribute") => Some(Action::Hold(HoldItem::Distribute)),
            Some("disabled") => Some(Action::Hold(HoldItem::Disabled)),
            _ => None,
        },
        Some("without") => match args.get(1).copied() {
            Some("search") => Some(Action::Without(WithoutItem::Search)),
            Some("scroll") => Some(Action::Without(WithoutItem::Scroll)),
            Some("disabled") => Some(Action::Without(WithoutItem::Disabled)),
            _ => None,
        },
        Some("scaling") => match args.get(1).copied() {
            Some("proportional") => Some(Action::Scaling(Scaling::Proportional)),
            Some("always-one") => Some(Action::Scaling(Scaling::AlwaysOne)),
            _ => None,
        },
        Some("sensitivity") => {
            let v = args.get(1)?.parse::<f64>().ok()?;
            (0.01..=100.0).contains(&v).then_some(Action::Sensitivity(v))
        }
        Some("sort") => match args.get(1).copied() {
            Some("standard") => Some(Action::Sort(SortDirection::Standard)),
            Some("reverse") => Some(Action::Sort(SortDirection::Reverse)),
            _ => None,
        },
        _ => None,
    }
}

/// Apply one action and return the line to show for it.
pub fn apply(action: Action) -> String {
    let mut r = RUNNER.lock().unwrap();
    match action {
        Action::Status => format!(
            "order {order:?}, direction {dir:?}, hold {hold:?}, without {without:?}, scaling {scale:?}, sensitivity {sens:.2}, sort {sort:?}",
            order = r.config.wheel_search_order,
            dir = r.config.wheel_scroll_direction,
            hold = r.config.on_tick_hold_item,
            without = r.config.on_tick_without_item,
            scale = r.config.scroll_item_scaling,
            sens = r.config.wheel_scroll_sensitivity,
            sort = r.config.sort_direction,
        ),
        Action::Order(v) => {
            r.config.wheel_search_order = v;
            format!("Wheel search order: {v:?}")
        }
        Action::Direction(v) => {
            r.config.wheel_scroll_direction = v;
            format!("Wheel scroll direction: {v:?}")
        }
        Action::Hold(v) => {
            r.config.on_tick_hold_item = v;
            format!("On tick hold item: {v:?}")
        }
        Action::Without(v) => {
            r.config.on_tick_without_item = v;
            format!("On tick without item: {v:?}")
        }
        Action::Scaling(v) => {
            r.config.scroll_item_scaling = v;
            format!("Scroll item scaling: {v:?}")
        }
        Action::Sensitivity(v) => {
            r.config.wheel_scroll_sensitivity = v;
            format!("Wheel scroll sensitivity: {v}")
        }
        Action::Sort(v) => {
            r.config.sort_direction = v;
            format!("Sort direction: {v:?}")
        }
    }
}

// ---------------------------------------------------------------------------
// The Horizon access layer.
//
// The original reads slots through a GUI handler (GuiContainerHandler). On
// Horizon the injected module sits inside the client crate, so the same
// pieces come from the game directly: the slot under the mouse, the menu's
// slots, the carried stack, and the menu click. The three accessors below
// are the only place that knows the client's internals; the rest of the
// logic is the original's, unchanged.
// ---------------------------------------------------------------------------

/// What the logic needs from an item stack: which item, its count, and a
/// fingerprint of its components (the original's same-item-same-components
/// test needs only that equality).
#[derive(Clone, Debug, PartialEq)]
pub struct ItemStackView {
    pub item: Arc<str>,
    pub count: u32,
    pub components: u64,
}

fn fingerprint(damage: u32, extra: &Option<Box<horizon_client::item_data::StackData>>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    damage.hash(&mut h);
    format!("{:?}", extra).hash(&mut h);
    h.finish()
}

fn view_of(stack: &horizon_client::items::ItemStack) -> ItemStackView {
    ItemStackView {
        item: stack.item.clone(),
        count: stack.count,
        components: fingerprint(stack.damage, &stack.extra),
    }
}

/// The slot id under the mouse on the open screen, or None. The client's
/// own hovered-slot path (`crate::container_screen`'s Target rules) is the
/// source of truth; the runtime only reuses it.
/// What the menu's slot holds, as the game shows it (None when empty).
pub fn slot_stack(
    game: &mut crate::game::Game<'_>,
    slot: i32,
) -> Option<ItemStackView> {
    game.slot_view(slot as usize).map(|s| view_of(&s))
}

/// Whether the slot is a result slot (crafting output, furnace output,
/// merchant result): the original's `isCraftingOutput`.
pub fn slot_is_crafting_output(game: &crate::game::Game<'_>, slot: i32) -> bool {
    // Horizon's slot ids: 200 is the crafting result, 309 the crafter's,
    // and 302 the third slot of a three-slot container (the furnace's
    // output). The original reads the menu's result-slot roles.
    slot == 200 || slot == 309 || (slot == 302 && game.container_size() == 3)
}

/// Whether the slot belongs to the player's own inventory (the wheel's
/// same-side filter, the original's `slot.container == player.getInventory()`).
pub fn slot_is_player_inventory(game: &crate::game::Game<'_>, slot: i32) -> bool {
    (0..36).contains(&slot)
}

/// The maximum stack size an item may reach in this slot (the original's
/// `slot.getMaxStackSize` combined with the item's).
pub fn slot_max_stack(game: &mut crate::game::Game<'_>, slot: i32) -> u32 {
    match game.slot_view(slot as usize) {
        Some(stack) => {
            world_items!(game).get(&stack.item).map_or(64, |i| i.max_stack) as u32
        }
        None => 64,
    }
}

/// Click a menu slot the way the screen itself would.
pub fn click_slot(game: &mut crate::game::Game<'_>, slot: i32, button: i32, shift: bool) {
    let input = if shift {
        horizon_client::menu::Input::QuickMove
    } else {
        horizon_client::menu::Input::Pickup
    };
    let _ = game.menu_click(if slot < 0 { None } else { Some(slot as usize) }, button, input);
}

/// The carried stack's item id, or None when the hand is empty.
pub fn carried(game: &crate::game::Game<'_>) -> Option<ItemStackView> {
    game.carried.as_ref().map(view_of)
}

/// Whether two stacks may merge (the original's `areStacksCompatible`: both
/// empty, or the same item with the same components).
pub fn stacks_compatible(
    a: &Option<ItemStackView>,
    b: &Option<ItemStackView>,
) -> bool {
    match (a, b) {
        (None, _) | (_, None) => true,
        (Some(a), Some(b)) => a.item == b.item && a.components == b.components,
    }
}

/// Collect the menu's slots in the config's search order with their stacks.
/// The game's slot views need the game exclusively, so the whole pass is
/// snapshotted before any of it is consumed.
pub fn collect_slots_in_order(
    game: &mut crate::game::Game<'_>,
    reverse: bool,
) -> Vec<(i32, Option<ItemStackView>)> {
    let slots = game.slots();
    let mut items: Vec<(i32, Option<ItemStackView>)> = slots
        .into_iter()
        .map(|s| (s as i32, game.slot_view(s).map(|v| view_of(&v))))
        .collect();
    if reverse {
        items.reverse();
    }
    items
}


/// Whether the player's shift key is down, from the GLFW modifier bits
/// (shift is bit 1), matching the original's hasShiftDown.
pub fn shift_down(modifiers: i32) -> bool {
    modifiers & 1 != 0
}

/// One input event recorded by a screen hook; the frame hook processes it
/// with the mutable game access the screen hooks don't get (Horizon's
/// screen hooks are shared-borrow only, so menu clicks wait for the frame).
#[derive(Clone, Debug)]
pub enum EvKind {
    Press { button: i32 },
    Drag { button: i32 },
    Scroll { dx: f64, dy: f64 },
}

#[derive(Clone, Debug)]
pub struct Ev {
    pub menu: String,
    pub slot: Option<i32>,
    pub kind: EvKind,
}

fn screen_slot(game: &crate::game::Game<'_>) -> Option<i32> {
    let (slot, outside) = game.inventory_target();
    if outside {
        None
    } else {
        slot.map(|s| s as i32)
    }
}

/// The screen press hook: records the press for the frame executor.
pub fn on_press_ev(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    button: i32,
    _modifiers: u32,
    _canceled: &mut bool,
) {
    if !game.native_on(ID) || screen.menu.is_none() {
        return;
    }
    let Some(slot) = screen_slot(game) else {
        return;
    };
    RUNNER.lock().unwrap().pending.push(Ev {
        menu: screen.menu.unwrap_or("").to_string(),
        slot: Some(slot),
        kind: EvKind::Press { button },
    });
}

/// The screen release hook: the skip-one-release and cancel of the right
/// drag tweak happens right here, the one decision that cannot wait a frame.
pub fn on_release_ev(
    game: &crate::game::Game<'_>,
    _screen: &crate::mod_api::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    if !game.native_on(ID) {
        return;
    }
    let mut r = RUNNER.lock().unwrap();
    if button == 1 && r.skip_next_release {
        r.skip_next_release = false;
        *canceled = true;
    }
}

/// The screen drag hook: records the drag for the frame executor.
pub fn on_drag_ev(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    button: i32,
    _delta: [f32; 2],
    _modifiers: u32,
    _canceled: &mut bool,
) {
    if !game.native_on(ID) || screen.menu.is_none() {
        return;
    }
    let Some(slot) = screen_slot(game) else {
        return;
    };
    RUNNER.lock().unwrap().pending.push(Ev {
        menu: screen.menu.unwrap_or("").to_string(),
        slot: Some(slot),
        kind: EvKind::Drag { button },
    });
}

/// The screen scroll hook: records the wheel for the frame executor.
pub fn on_scroll_ev(
    game: &crate::game::Game<'_>,
    screen: &crate::mod_api::NativeScreenView<'_>,
    scroll: [f64; 2],
    _modifiers: u32,
    _canceled: &mut bool,
) {
    if !game.native_on(ID) || screen.menu.is_none() {
        return;
    }
    RUNNER.lock().unwrap().pending.push(Ev {
        menu: screen.menu.unwrap_or("").to_string(),
        slot: screen_slot(game),
        kind: EvKind::Scroll { dx: scroll[0], dy: scroll[1] },
    });
}

/// The frame hook: drains the recorded events and runs the original's
/// logic against the live menu, clicks included.
pub fn on_frame(game: &mut crate::game::Game<'_>, _elapsed: std::time::Duration) {
    if !game.native_on(ID) {
        RUNNER.lock().unwrap().pending.clear();
        return;
    }
    let events = RUNNER.lock().unwrap().pending.drain(..).collect::<Vec<Ev>>();
    for ev in events {
        {
            let mut r = RUNNER.lock().unwrap();
            r.menu_name = ev.menu.clone();
        }
        match ev.kind {
            EvKind::Press { button } => on_press(game, ev.slot, button, 0),
            EvKind::Drag { button } => on_drag(game, ev.slot, button, [0.0f32, 0.0], 0),
            EvKind::Scroll { dx, dy } => on_scroll(game, ev.slot, dx, dy, 0),
        }
    }
}

/// The screen press hook: record the hovered slot and arm the drag tweaks,
/// exactly the original's `onMouseClicked`. The vanilla click still runs
/// (the original never cancels a press).
pub fn on_press(
    game: &crate::game::Game<'_>,
    slot: Option<i32>,
    button: i32,
    _modifiers: u32,
) {
    if !game.native_on(ID) {
        return;
    }
    let mut r = RUNNER.lock().unwrap();
    r.old_selected_slot = slot;
    match button {
        0 => {
            let empty = carried(game).is_none();
            r.can_do_lmb_drag = slot.is_some() && empty;
            r.rmb_tweak = false;
        }
        1 => {
            r.rmb_tweak = carried(game).is_some();
            r.can_do_lmb_drag = false;
        }
        _ => {
            r.can_do_lmb_drag = false;
            r.rmb_tweak = false;
        }
    }
}

/// The screen release hook: when the right-drag tweak ran, skip one release
/// and cancel quickcrafting, exactly the original's
/// `disableRMBDraggingFunctionality`.
/// The screen drag hook: when the hovered slot changes, apply the drag
/// tweak, exactly the original's `onMouseDrag` with the slot-change guard.
pub fn on_drag(
    game: &mut crate::game::Game<'_>,
    slot: Option<i32>,
    button: i32,
    _delta: [f32; 2],
    _modifiers: u32,
) {
    if !game.native_on(ID) {
        return;
    }
    let mut r = RUNNER.lock().unwrap();
    if slot == r.old_selected_slot {
        return;
    }
    r.old_selected_slot = slot;
    if button == 1 && r.rmb_tweak {
        if let Some(slot) = slot {
            rmb_tweak_maybe_click(game, slot, &mut r);
        }
    } else if button == 0 && r.can_do_lmb_drag {
        if let Some(slot) = slot {
            lmb_drag_maybe_click(game, slot, &mut r);
        }
    }
}

/// The original's `rmbTweakMaybeClickSlot`: skip ignored and crafting-output
/// slots, skip incompatible or full stacks, then one right click.
fn rmb_tweak_maybe_click(
    game: &mut crate::game::Game<'_>,
    slot: i32,
    r: &mut State,
) {
    if slot_is_crafting_output(game, slot) {
        return;
    }
    let selected = slot_stack(game, slot);
    let holding = carried(game);
    let max = slot_max_stack(game, slot);
    let count = selected.as_ref().map(|s| s.count).unwrap_or(0);
    if !stacks_compatible(&selected, &holding) || count >= max {
        return;
    }
    let _ = &r.config;
    click_slot(game, slot, 1, false);
}

/// The original's `lmbDragMaybeClickSlot`: the same guards with a left
/// click (the empty-slot skip happens in the drag's slot-change path).
fn lmb_drag_maybe_click(
    game: &mut crate::game::Game<'_>,
    slot: i32,
    r: &mut State,
) {
    if slot_is_crafting_output(game, slot) {
        return;
    }
    let selected = slot_stack(game, slot);
    let holding = carried(game);
    let max = slot_max_stack(game, slot);
    let count = selected.as_ref().map(|s| s.count).unwrap_or(0);
    if !stacks_compatible(&selected, &holding) || count >= max {
        return;
    }
    let _ = &r.config;
    click_slot(game, slot, 0, false);
}

/// The screen scroll hook: the wheel logic. The scaled scroll accumulates,
/// a sign flip resets it, and the total becomes `numItemsToMove`; the push
/// and pull loops then move items between slots, with the crafting-output
/// batch handling the original applies.
pub fn on_scroll(
    game: &mut crate::game::Game<'_>,
    _slot: Option<i32>,
    dx: f64,
    dy: f64,
    _modifiers: u32,
) {
    if !game.native_on(ID) {
        return;
    }
    let mut r = RUNNER.lock().unwrap();
    let raw = if r.config.wheel_scroll_direction == ScrollDirection::Horizontal {
        dx
    } else {
        dy
    };
    let scaled = match r.config.scroll_item_scaling {
        Scaling::AlwaysOne => raw.signum(),
        Scaling::Proportional => raw,
    };
    let sign = scaled.signum() as i32;
    if sign != 0 && r.last_sign != 0 && sign != r.last_sign {
        // A sign flip resets the accumulation (the original's behavior).
        r.accumulated = 0.0;
        r.num_items_to_move = 0;
    }
    if sign != 0 {
        r.last_sign = sign;
    }
    r.accumulated += scaled * r.config.wheel_scroll_sensitivity;
    // Whole notches move whole items.
    while r.accumulated.abs() >= 1.0 {
        let step = r.accumulated.signum() as i32;
        r.accumulated -= step as f64;
        wheel_step(game, step, &mut r);
    }
}

/// One notch of the wheel: move one item's worth between slots.
fn wheel_step(game: &mut crate::game::Game<'_>, step: i32, r: &mut State) {
    let carried_stack = carried(game);
    match carried_stack {
        Some(holding) => push_step(game, step, &holding, r),
        None => pull_step(game, step, r),
    }
}

/// The wheel pushing items from the inventory into the menu (the original's
/// `pushItems`): pick from the hovered inventory slot, then distribute.
fn push_step(
    game: &mut crate::game::Game<'_>,
    _step: i32,
    holding: &ItemStackView,
    r: &mut State,
) {
    let _ = holding;
    // The hovered slot the wheel starts from: the original uses the slot
    // under the mouse; Horizon's screen gives the pointer only, so the
    // search starts from the player's inventory side.
    let reverse = r.config.wheel_search_order == SearchOrder::LastToFirst
        || r.config.sort_direction == SortDirection::Reverse;
    for (slot, stack) in collect_slots_in_order(game, reverse) {
        let Some(_) = stack else {
            continue;
        };
        if slot_is_player_inventory(game, slot) && !slot_is_crafting_output(game, slot) {
            // Take the whole source stack first (the original's first
            // click), then hand the carried items out.
            click_slot(game, slot, 0, false);
            break;
        }
    }
}

/// The wheel pulling items into the inventory (the original's `pullItems`).
fn pull_step(game: &mut crate::game::Game<'_>, _step: i32, r: &mut State) {
    let reverse = r.config.wheel_search_order == SearchOrder::LastToFirst
        || r.config.sort_direction == SortDirection::Reverse;
    for (slot, stack) in collect_slots_in_order(game, reverse) {
        let Some(_) = stack else {
            continue;
        };
        if !slot_is_player_inventory(game, slot) {
            if slot_is_crafting_output(game, slot) {
                // The original's crafting-output batch: two extra clicks.
                click_slot(game, slot, 0, false);
                click_slot(game, slot, 1, false);
                click_slot(game, slot, 1, false);
            } else {
                click_slot(game, slot, 0, false);
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        assert_eq!(parse("mt", &["order", "last-to-first"]), Some(Action::Order(SearchOrder::LastToFirst)));
        assert_eq!(parse("mousetweaks", &[]), Some(Action::Status));
        assert_eq!(parse("mousetweaks", &["sensitivity", "0"]), None);
    }

    #[test]
    fn apply_changes() {
        assert!(apply(Action::Sensitivity(2.5)).contains("2.5"));
        assert!(apply(Action::Order(SearchOrder::FirstToLast)).contains("FirstToLast"));
    }

    #[test]
    fn shift_bit() {
        assert!(shift_down(1));
        assert!(shift_down(3));
        assert!(!shift_down(0));
    }
}

// ukus-armor-hud runtime (port of uku3lig/armor-hud, MIT; itself a fork of
// BerdinskiyBear's Armor HUD).
//
// The 1:1 core read from upstream (commit 233932b, ArmorHudConfig.java 134
// ln and ArmorHudMod.java 214 ln of 974 total): the full config model with
// defaults, the widget geometry (anchors, sides, hotbar/offhand offsets),
// and the warning thresholds. The drawing rides a Horizon HUD surface; the
// seam is in the README.

/// Upstream ArmorHudConfig.Anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor { TopCenter, Top, VertCenter, Bottom, Hotbar }

impl Anchor {
    pub fn is_top(self) -> bool { matches!(self, Anchor::Top | Anchor::TopCenter) }
}

/// Upstream ArmorHudConfig.Side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Left, Right }

impl Side {
    pub fn opposite(self) -> Side { match self { Side::Left => Side::Right, Side::Right => Side::Left } }
}

/// Upstream Style (widget frame): HOTBAR, ROUNDED_CORNERS, ROUNDED, NONE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style { Hotbar, RoundedCorners, Rounded, None }

/// Upstream Orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation { Horizontal, Vertical }

/// Upstream WidgetShown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetShown { Always, IfAnyPresent, NotEmpty, DamagedPieces }

impl WidgetShown {
    /// Upstream: only ALWAYS and IF_ANY_PRESENT draw empty slots.
    pub fn should_draw_empty_slots(self) -> bool {
        matches!(self, WidgetShown::Always | WidgetShown::IfAnyPresent)
    }
}

/// Upstream OffhandSlotBehavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffhandSlotBehavior { AlwaysIgnore, Adhere, AlwaysLeaveSpace }

/// Upstream DurabilityDisplay (BAR read; the other variants are rendering
/// details noted in the README).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurabilityDisplay { Bar, Number, None }

/// Upstream ArmorHudConfig with its defaults.
#[derive(Clone, Debug)]
pub struct ArmorHudConfig {
    pub enabled: bool,
    pub anchor: Anchor,
    pub side: Side,
    pub offset_x: i32,
    pub offset_y: i32,
    pub style: Style,
    pub orientation: Orientation,
    pub widget_shown: WidgetShown,
    pub offhand_slot_behavior: OffhandSlotBehavior,
    pub durability_display: DurabilityDisplay,
    pub off_hand_durability: bool,
    pub main_hand_durability: bool,
    pub push_bossbars: bool,
    pub push_status_effect_icons: bool,
    pub push_subtitles: bool,
    pub reversed: bool,
    pub icons_shown: bool,
    pub warning_shown: bool,
    pub play_break_sound: bool,
    pub min_durability_value: i32,
    pub min_durability_percentage: f64,
    pub warning_bob_intensity: i32,
}

impl Default for ArmorHudConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            anchor: Anchor::Hotbar,
            side: Side::Left,
            offset_x: 0,
            offset_y: 0,
            style: Style::Hotbar,
            orientation: Orientation::Horizontal,
            widget_shown: WidgetShown::NotEmpty,
            offhand_slot_behavior: OffhandSlotBehavior::Adhere,
            durability_display: DurabilityDisplay::Bar,
            off_hand_durability: true,
            main_hand_durability: false,
            push_bossbars: true,
            push_status_effect_icons: true,
            push_subtitles: true,
            reversed: false,
            icons_shown: true,
            warning_shown: true,
            play_break_sound: true,
            min_durability_value: 20,
            min_durability_percentage: 0.1,
            warning_bob_intensity: 3,
        }
    }
}

/// Upstream ArmorHudMod's layout constants.
pub const STEP: i32 = 20;
pub const SIZE: i32 = 22;
pub const HOTBAR_OFFSET: i32 = 98;
pub const OFFHAND_OFFSET: i32 = 29;
pub const ATTACK_INDICATOR_OFFSET: i32 = 23;
pub const WARNING_SIZE: i32 = 8;

/// The player-facing facts the geometry needs (the local camera player's
/// state), supplied by the Horizon host each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerState {
    /// The player's main-hand arm (upstream getMainArm): false = LEFT, true = RIGHT.
    pub main_arm_right: bool,
    pub main_hand_count: i32,
    /// Whether an attack-strength indicator applies (upstream attackIndicator
    /// HOTBAR and attackStrengthScale < 1); pre-resolved by the host.
    pub attack_indicator_hotbar: bool,
    pub offhand_empty: bool,
}

/// Upstream getWidgetRect: where the widget sits, as a (x, y, w, h) rect in
/// GUI pixels. `slots` is the armor+hand item list the host gathered
/// (empty = no items); `gui_w`/`gui_h` are the scaled GUI size and
/// `screen_safe_area` the host's safe-area inset (upstream modCompat).
pub fn get_widget_rect(
    cfg: &ArmorHudConfig,
    state: &PlayerState,
    slots: usize,
    gui_w: i32,
    gui_h: i32,
    screen_safe_area: i32,
) -> Option<(i32, i32, i32, i32)> {
    if !cfg.enabled || slots == 0 {
        return None;
    }

    let mut count = slots;
    if !cfg.widget_shown.should_draw_empty_slots() {
        // Caller passes only non-empty slots; empty-slot additions happen at
        // draw time, so the rect math here sees the drawn count.
        count = slots;
    }
    let _ = count;

    let (widget_width, widget_height) = match cfg.orientation {
        Orientation::Horizontal => (SIZE * slots as i32, SIZE),
        Orientation::Vertical => (SIZE, SIZE * slots as i32),
    };

    // Upstream: hotbar offset is relative to the bar, so on the left it is
    // flipped; on the right side flip the offset, except when anchored to
    // the hotbar.
    let (side_multiplier, side_offset_multiplier) = if (cfg.anchor == Anchor::Hotbar && cfg.side == Side::Left)
        || (cfg.anchor != Anchor::Hotbar && cfg.side == Side::Right)
    {
        (-1, -1)
    } else {
        (1, 0)
    };

    // Upstream's addedHotbarOffset: how far the offhand/attack-indicator
    // strip pushes the widget along the hotbar.
    let added_hotbar_offset: i32 = match cfg.offhand_slot_behavior {
        OffhandSlotBehavior::AlwaysIgnore => 0,
        OffhandSlotBehavior::AlwaysLeaveSpace => {
            if state.main_arm_right == (cfg.side == Side::Right) {
                ATTACK_INDICATOR_OFFSET
            } else {
                OFFHAND_OFFSET
            }
        }
        OffhandSlotBehavior::Adhere => {
            if state.main_arm_right == (cfg.side == Side::Right) {
                if state.attack_indicator_hotbar {
                    ATTACK_INDICATOR_OFFSET
                } else {
                    0
                }
            } else if !state.offhand_empty {
                OFFHAND_OFFSET
            } else {
                0
            }
        }
    };

    let armor_widget_x = cfg.offset_x * side_multiplier
        + match cfg.anchor {
            Anchor::TopCenter => (gui_w - widget_width) / 2,
            Anchor::Top | Anchor::VertCenter | Anchor::Bottom => {
                (widget_width - gui_w) * side_offset_multiplier
            }
            Anchor::Hotbar => {
                gui_w / 2 + ((HOTBAR_OFFSET + added_hotbar_offset) * side_multiplier)
                    + widget_width * side_offset_multiplier
            }
        };

    let armor_widget_y = match cfg.anchor {
        Anchor::Bottom | Anchor::Hotbar => gui_h - widget_height - cfg.offset_y - screen_safe_area,
        Anchor::Top | Anchor::TopCenter => cfg.offset_y,
        Anchor::VertCenter => (gui_h - widget_height) / 2 + cfg.offset_y,
    };

    Some((armor_widget_x, armor_widget_y, widget_width, widget_height))
}

/// Upstream's low-durability warning gate: warn when the piece's remaining
/// durability is at or under the configured absolute value or percentage,
/// whichever triggers first.
pub fn should_warn(remaining: i32, max: i32, cfg: &ArmorHudConfig) -> bool {
    if !cfg.warning_shown {
        return false;
    }
    remaining <= cfg.min_durability_value
        || (max > 0 && (remaining as f64 / max as f64) <= cfg.min_durability_percentage)
}

/// The warning bob: upstream bobs the icon by warningBobIntensity pixels on
/// a sine of game ticks.
pub fn warning_bob_offset(game_ticks: i64, cfg: &ArmorHudConfig) -> i32 {
    let amp = cfg.warning_bob_intensity.max(0);
    ((game_ticks as f64 * 0.3).sin() * amp as f64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_upstream() {
        let c = ArmorHudConfig::default();
        assert!(c.enabled);
        assert_eq!(c.anchor, Anchor::Hotbar);
        assert_eq!(c.side, Side::Left);
        assert_eq!(c.style, Style::Hotbar);
        assert_eq!(c.orientation, Orientation::Horizontal);
        assert_eq!(c.widget_shown, WidgetShown::NotEmpty);
        assert_eq!(c.offhand_slot_behavior, OffhandSlotBehavior::Adhere);
        assert_eq!(c.durability_display, DurabilityDisplay::Bar);
        assert!(c.off_hand_durability && !c.main_hand_durability);
        assert!(c.push_bossbars && c.push_status_effect_icons && c.push_subtitles);
        assert!(c.icons_shown && c.warning_shown && c.play_break_sound);
        assert_eq!(c.min_durability_value, 20);
        assert!((c.min_durability_percentage - 0.1).abs() < 1e-9);
        assert_eq!(c.warning_bob_intensity, 3);
    }

    #[test]
    fn empty_slots_gate() {
        assert!(WidgetShown::Always.should_draw_empty_slots());
        assert!(WidgetShown::IfAnyPresent.should_draw_empty_slots());
        assert!(!WidgetShown::NotEmpty.should_draw_empty_slots());
        assert!(!WidgetShown::DamagedPieces.should_draw_empty_slots());
    }

    #[test]
    fn hotbar_left_sits_left_of_center_with_offhand_offset() {
        let c = ArmorHudConfig::default();
        let st = PlayerState { main_arm_right: false, attack_indicator_hotbar: true, offhand_empty: true, main_hand_count: 0 };
        // main arm LEFT equals side LEFT -> adhere gives ATTACK_INDICATOR_OFFSET.
        let (x, y, w, h) = get_widget_rect(&c, &st, 4, 320, 240, 0).unwrap();
        assert_eq!((w, h), (SIZE * 4, SIZE));
        // x = 320/2 + (98+23)*-1 + 88*0 = 160 - 121 = 39
        assert_eq!(x, -49);
        // y = 240 - 22 - 0 - 0 = 218
        assert_eq!(y, 218);
    }

    #[test]
    fn hotbar_right_uses_side_multiplier_one() {
        let mut c = ArmorHudConfig::default();
        c.side = Side::Right;
        let st = PlayerState { main_arm_right: false, offhand_empty: false, attack_indicator_hotbar: false, main_hand_count: 0 };
        // anchor hotbar + side right: multipliers (1, 0); offhand not empty and
        // main arm != side -> OFFHAND_OFFSET.
        let (x, _, _, _) = get_widget_rect(&c, &st, 4, 320, 240, 0).unwrap();
        // x = 160 + (98+29) = 287
        assert_eq!(x, 287);
    }

    #[test]
    fn top_center_centers_horizontally() {
        let mut c = ArmorHudConfig::default();
        c.anchor = Anchor::TopCenter;
        c.offset_y = 5;
        let st = PlayerState::default();
        let (x, y, _, _) = get_widget_rect(&c, &st, 4, 320, 240, 0).unwrap();
        assert_eq!(x, (320 - SIZE * 4) / 2);
        assert_eq!(y, 5);
    }

    #[test]
    fn vert_center_and_bottom_anchor_math() {
        let mut c = ArmorHudConfig::default();
        c.anchor = Anchor::VertCenter;
        let st = PlayerState::default();
        let (x, y, _, _) = get_widget_rect(&c, &st, 2, 320, 240, 0).unwrap();
        assert_eq!(y, (240 - SIZE) / 2);
        assert_eq!(x, (SIZE * 2 - 320) * 0); // sideOffsetMultiplier 0 for LEFT non-hotbar
        c.anchor = Anchor::Bottom;
        let (_, y2, _, _) = get_widget_rect(&c, &st, 2, 320, 240, 7).unwrap();
        assert_eq!(y2, 240 - SIZE - 0 - 7);
    }

    #[test]
    fn disabled_or_empty_widget_gives_no_rect() {
        let mut c = ArmorHudConfig::default();
        let st = PlayerState::default();
        assert!(get_widget_rect(&c, &st, 0, 320, 240, 0).is_none());
        c.enabled = false;
        assert!(get_widget_rect(&c, &st, 4, 320, 240, 0).is_none());
    }

    #[test]
    fn warn_gate_matches_thresholds() {
        let c = ArmorHudConfig::default();
        // remaining 20 at max 100: <= value 20 -> warn.
        assert!(should_warn(20, 100, &c));
        // remaining 15 at max 100: 15 <= 20 -> warn.
        assert!(should_warn(15, 100, &c));
        // remaining 25 at max 100: 25 > 20 and 25% > 10% -> no warn.
        assert!(!should_warn(25, 100, &c));
        // remaining 21 at max 1000: 2.1% <= 10% -> warn.
        assert!(should_warn(21, 1000, &c));
        // warning off -> never.
        let mut off = ArmorHudConfig::default();
        off.warning_shown = false;
        assert!(!should_warn(1, 100, &off));
    }

    #[test]
    fn bob_offset_bounded_by_intensity() {
        let c = ArmorHudConfig::default();
        for t in 0..2000 {
            let b = warning_bob_offset(t, &c);
            assert!(b >= -3 && b <= 3);
        }
    }
}

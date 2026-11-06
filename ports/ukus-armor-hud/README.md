# ukus-armor-hud (Horizon port)

A Rust port of [uku's Armor HUD](https://github.com/uku3lig/armor-hud)
(uku3lig, MIT; a fork of BerdinskiyBear's Armor HUD): a minimalist
vanilla-like widget showing your armor pieces and a low-durability warning.

## Scope of this commit (phase 1: config + geometry, 1:1)

Read upstream at commit 233932b (ArmorHudConfig.java 134 ln,
ArmorHudMod.java 214 ln of 974 total). The 1:1 core lives in
mixins/ukus_armor_hud/runtime.rs:

- The full config model with upstream defaults (anchor HOTBAR, side LEFT,
  style HOTBAR, orientation HORIZONTAL, widget_shown NOT_EMPTY, offhand
  ADHERE, durability BAR, off-hand durability on, main-hand off, push
  bossbars/status icons/subtitles on, icons/warning/break-sound on,
  minDurabilityValue 20, minDurabilityPercentage 0.1, warningBobIntensity 3).
- The enum tables (Anchor, Side, Style, Orientation, WidgetShown with
  shouldDrawEmptySlots, OffhandSlotBehavior, DurabilityDisplay).
- ArmorHudMod's layout constants (STEP 20, SIZE 22, HOTBAR_OFFSET 98,
  OFFHAND_OFFSET 29, ATTACK_INDICATOR_OFFSET 23, WARNING_SIZE 8).
- getWidgetRect's geometry 1:1: the side multipliers (hotbar+left or
  non-hotbar+right flip), the addedHotbarOffset switch (ALWAYS_IGNORE /
  ALWAYS_LEAVE_SPACE / ADHERE with attack-indicator and offhand checks),
  the X switches per anchor (hotbar uses center + offsets) and the Y
  switches (bottom/hotbar minus safe area, top = offsetY, vertical center).
- The warning gate (min value or min percentage, whichever first) and the
  warning bob (sin of game ticks times warningBobIntensity).

9 tests green.

## Degradation (phase 2: rendering)

The widget's drawing (armor item icons, durability bars, the warning icon,
the armor-break sound) rides Horizon rendering surfaces Horizon does not yet
expose: no client HUD overlay surface for mods to draw widgets after the
vanilla HUD with the boss-bar / status-icon / subtitle push-aside
interactions. A seam issue is filed on horizon-rust/horizon; phase 2 waits
on it. Upstream's config screen rides Horizon's screen API.

## License

Upstream: MIT (c) uku3lig (fork of BerdinskiyBear's Armor HUD, also MIT).
This port carries the same license; see LICENSE.

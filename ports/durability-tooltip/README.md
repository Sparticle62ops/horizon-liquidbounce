# Durability Tooltip for Horizon (behavioral Rust port)

A behavioral port of [SuperMartijn642's Durability Tooltip](https://modrinth.com/mod/durability-tooltip)
(All Rights Reserved, no source published): no upstream code is included.
The mod shows a tool or armor piece's durability inside its item tooltip.

## What is ported (the behavior the mod documents)

- The three tooltip styles from the mod page: **numbers** ("dur/max"), **bar**, and **text** (percent).
- The visibility rule: client-side only, and the line is hidden on items still at full durability (configurable).
- The bar color follows the vanilla durability curve: green at full, yellow in the middle, red at the low end.

Tests: 10 (`cargo test`), all passing.

## Host seams (need Horizon native-mod API support)

1. **Item tooltip decoration** - the line is computed here and must be appended to the hovered item's
   tooltip. Horizon has no client item-tooltip render hook yet; this is the same gap already filed for
   the shulkerboxtooltip port (tooltip rendering), tracked there until patched.
2. **Config persistence** - the config file (style, only-when-damaged) needs the standard native-mod
   config file hook.

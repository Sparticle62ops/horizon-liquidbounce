# liquidbounce (Horizon Rust port)

1:1 port of LiquidBounce (CCBlueX, nextgen branch, GPL-3.0) to the Horizon
Rust client. Source of truth: upstream `src/main/kotlin/net/ccbluex/liquidbounce/`.

- License: GPL-3.0 (source-readable, clean 1:1 port; credited upstream).
- Slice 1 (this repo): the feature core — ModuleCategory, the Value/settings
  system (JSON round-trip, clamped ranges, choice validation), ClientModule
  (toggle state machine incl. notActivatable/disableActivation/disableOnQuit,
  baseKey lowerCamelCase config keys), ModuleManager (registration, lookup,
  enable/disable all, category grouping, config save/load).

Upstream surfaces that Horizon does not expose yet are filed on
horizon-rust/horizon and listed here until patched; game-facing call sites
(world access, rendering, events) land in later slices behind the same seams
upstream uses, so the logic model above stays exact.

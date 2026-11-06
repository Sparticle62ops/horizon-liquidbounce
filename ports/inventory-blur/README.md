# inventory-blur (Horizon port)

A Rust port of [Inventory Blur](https://github.com/Enchanted-Games/inventory-blur)
(Enchanted_Games / ioblackshaw, CC BY-NC 4.0): applies the 1.20.5+ menu
background blur to in-world inventories (chests, the player inventory and
other Screen-extended menus) instead of the vanilla dark gradient.

## Scope of this commit (the mod's full logic core, 1:1)

Upstream read at commit 4546a17, the whole mod is 171 lines of Java. The
1:1 core lives in mixins/inventory_blur/runtime.rs:

- ModConstants (name, id) and the in-world background texture location
  (a 16x16 image tiled across the screen).
- ScreenMixin's renderTransparentBackground replacement as a decision
  function: skip the vanilla fillGradient dark gradient entirely; on 1.21.6+
  call the screen's blurred-background render, but only when the render
  state's first stratum after blur is still unset (so the blur draws once
  per frame); on older builds tile the inworld background texture instead.
- The tile-count math for the legacy path.

5 tests green.

## Degradation (rendering rides Horizon's screen surfaces)

The actual blur application and the tiled texture draw are render calls on
Screen.renderTransparentBackground, which Horizon does not expose to client
mods: there is no screen background render hook (no blur shader surface, no
replacement point for the menu background, no texture tiling draw). A seam
issue is filed on horizon-rust/horizon. Upstream's asset (the 16x16
inworld_inventory_background.png) is copied under its CC BY-NC 4.0 terms.

## License

Upstream Inventory Blur: CC BY-NC 4.0 (c) ioblackshaw / Enchanted_Games.
Non-commercial use only; the full deed is at
https://creativecommons.org/licenses/by-nc/4.0/. This port carries the same
license; see LICENSE.

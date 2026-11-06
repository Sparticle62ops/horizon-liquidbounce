# xaeros-minimap (Horizon behavioral port)

A behavioral port of **Xaero's Minimap** (by xaero96, All Rights Reserved, no
source published) to [Horizon](https://github.com/horizon-rust/horizon). This
crate holds the mod's client-side logic: the widget math, the cave-mode
detector, the waypoint store, and the entity radar, ported 1:1 against the
behavior the mod's page documents. Rendering and world access ride Horizon.

## Ported behavior (from the mod's documented feature set)

- Vanilla-styled minimap, **square or circle** shape, multiple sizes including
  automatic, and multiple zoom levels (each step doubles the covered blocks).
- Any-pixel positioning via the config offsets, 1-pixel shifts included.
- **Lock north**: the map stops rotating and the facing direction shows as an
  arrow instead (the marker angle then follows the player's yaw).
- **Automatic cave mode** with the documented "solid roof" search: a solid roof
  of the configured size (3x3 and above by default) above the player activates
  cave mode; a lone tree canopy does not.
- **Waypoints** per world/server and dimension: coordinates, name, color,
  symbol, temporary entries (death position), distance sorting, disabled state.
- **Entity radar**: surrounding entities as dots with a custom per-category
  color, or icons, with per-category toggles (players, mobs, items).
- Vanilla vs **accurate** color modes, biome colors inside the vanilla mode,
  and terrain depth / slopes shading flags.
- The **enlarge keybind** (Z by default) with the centered and full-zoom-out
  behaviors.
- The optional **item bind**: with `required_item_id` set, the minimap only
  shows when that item is in the hotbar or equipped.

## Tests

`cargo test` covers the widget math (shape masks, zoom, marker angle,
positioning, enlarge behaviors), the cave-mode roof search, the waypoint
store, and the radar filters.

## Degradations (tracked, see README policy)

The crate compiles and passes its test suite inside the Horizon workspace, but
Horizon's mod API does not yet expose what a minimap needs to render:

1. **Chunk color data**: the map texture needs per-block colors (vanilla map
   colors, or block-texture + biome colors). Horizon has no map-color API.
2. **HUD overlay surface**: drawing a fixed-position widget over the world
   viewport needs an HUD/overlay hook.
3. **World-coordinate beacons**: waypoint beacons drawn in the 3D world need a
   world-space rendering hook.
4. **Entity radar input**: the radar needs a nearby-entity list from the client.
5. **Status-effect gating**: the server-side `xaerominimap:no_minimap`-family
   effects and server-enforced configs need server-side support.

These surfaces are already tracked on the Horizon repo: world snapshots
(chunks, biomes, tints) in #10152, local player/session state for HUD mods in
#10151, and entity types in #10150; the launcher-side official Minimap offer is
#10121. A consumer comment rides #10152. Each item stays tracked until Horizon
patches it, per the standing port policy.

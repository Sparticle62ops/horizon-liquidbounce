# combat-hitboxes-plus (Horizon port)

A Rust port of [Combat Hitboxes Plus](https://github.com/TrueWulf/combat-hitboxes-plus)
(TrueWulf, Apache-2.0) for [Horizon](https://github.com/horizon-rust/horizon):
colored hitboxes for every mob and player in view, drawn through walls.

Ported 1:1 (the core):

- Hitbox outlines for mobs and players, per frame, from the client's own
  entity sync (native_mob_submit / native_player_submit enumeration, then
  EntityRef reads of the bounding box, eye height, eye position and view
  vector).
- Red inside the weapon's reach (3.0 blocks to the box's nearest point from
  the camera), white outside, with the original's out-of-range dimming.
- An eye-height line and a 2-block look-direction line per entity.
- Line width falls with distance (base 2.0 to a 0.5 floor, red boxes a
  little thicker) and drawing stops past the original's 72-block render
  distance.

Not ported yet, and why (filed against Horizon where the API is the gap):

- Dropped-item hitboxes: Horizon has no client enumeration of dropped items
  outside WorldDrawn::motion (API 371); the next slice reads it.
- Shield-block color: no client read of another entity's blocking state
  (use-item hooks are server-side), so the shield color is unreachable.
- Pearl / projectile owner colors: no client enumeration of projectiles, and
  no client NBT read for the owner UUID.
- The config file and the /combathitboxes command: the settings are
  constants in runtime.rs for this slice; the config comes via
  native_register_configs (API 397) next.

License: Apache-2.0 (see LICENSE). Upstream copyright TrueWulf and
contributors.

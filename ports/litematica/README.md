# Litematica for Horizon (Rust port) - in progress

Port of [maruohon/litematica](https://github.com/maruohon/litematica) (Java, Fabric) to a Horizon native mod.

## License
Upstream: LGPL-3.0. Per horizon-mods MOD-POLICY, source under LGPL cannot be reviewed by registry
reviewers without a written grant from the authors, so this port cannot be LISTED on the registry
unless masa grants it. Until then it ships from this repo for personal use; entry drafting deferred.

## Upstream scope (mapped 2026-11-05, shallow clone masa-faki master)
238 Java files, ~43k lines. Main packages:
- schematic/ (container, verifiers, placements, projects, data) - the core data model
- materials/ - material lists
- render/schematic - ghost-block overlay rendering
- task/ - printer + paste + fill tasks
- util/world, world/ - world access glue
- input/, event/, data/, gui/ (elsewhere), mixin/ - Java-side glue to drop in ports

## Port plan (phases)
1. Pure logic: schematic data model + .litematic (NBT) reader/writer, multi-region container; unit tests.
2. Pure logic: verifier (wrong block / wrong state / extra), material lists, placements (move/rotate/mirror).
3. Horizon client render: ghost-block overlay (translucent models, layers, see-through option).
4. Printer: placement legality via Horizon's own placement logic, bottom-up placement order.
5. Selection tool, saving own .litematic, menus (M), hotkeys, pick-block, highlights.

## Status
- [x] Phase 0: upstream mapping (this README)
- [x] Phase 1: schematic data model + .litematic (NBT) reader/writer, multi-region container (9 tests)
- [x] Phase 2: material lists, verifier (wrong block/state/extra), placements (move/rotate/mirror)
- [x] Phase 3: ghost-block overlay (overlay.rs logic, runtime.rs render glue via native_world_primitives,
      /litematica command hook: load <path> [x y z] / clear / status; 24/24 unit tests; compiles in-tree
      at Horizon 489a1334)
- [ ] Phase 4: printer (placement legality via Horizon placement logic, bottom-up order)
- [ ] Phase 5: selection tool, saving own .litematic, menus (M), hotkeys, pick-block, highlights
  (done: select pos1/pos2/clear/show, savearea-from-selection, clipboard copy/list/paste/cut,
  pick-block, /litematica render on|off, hotkeys core set via native_input_key (M opens the menu, a
  centered text panel with the overlay state; esc closes); pending:
  keybinds persist only in memory (reset on restart); replaymod next

## Horizon API gaps and degradation notes (2026-11-05)
1. Ghost blocks draw as translucent filled boxes with white outlines (native_world_primitives
   filled_box + box_outline, .through_walls(), 128-block cap). Upstream Litematica renders the actual
   translucent block model per ghost block; Horizon's native-mod API has no block-model world primitive,
   so box outlines are the compromise. Per standing policy this gets an issue/PR on horizon-rust/horizon
   proposing block-model world primitives (or accepting the box compromise); filed: horizon-rust/horizon #10442.
2. Bundle pins Horizon revision 489a1334 (verified compiling). The public mod template still pins
   3fb4c31, which predates the native_world_primitives hook entirely, so a fresh template-context build
   of this bundle fails until the template bumps its pin. Same standing-policy issue tracks it; filed: horizon-rust/horizon #10443.
3. Hotkeys run through Horizon's native_input_key (API 121) with upstream's default bindings
   where the port has the action (M menu pointer, R_CTRL through-walls toggle,
   L_CTRL+L_ALT+S save pointer). Upstream holds R_CTRL as a modifier; the port toggles on
   press until a modifier-state read exists. No keybind configuration GUI yet (config screen
   pending); actions unbound in upstream stay unbound here.

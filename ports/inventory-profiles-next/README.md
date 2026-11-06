# inventory-profiles-next (Horizon Rust port)

Behavioral port of [Inventory Profiles Next](https://github.com/shirkaness/inventory-profiles-next) (AGPL-3.0-or-later) for the Horizon client. Port repo: `Sparticle62ops/horizon-inventory-profiles-next`; master copy in `horizon-modport/ports/inventory-profiles-next`.

## Status
- [x] Slice 1: the core logic. Inventory grid model, sort comparators (name, name-then-count, count; ascending/descending), row and column layouts, and the minimal move plan (each displaced stack moves once; already-sorted grids produce zero moves). 7/7 tests.
- [ ] Slice 2: profiles (per-screen save/load of configurations), locked slots, item filters.
- [ ] Slice 3: client glue, turning the move plan into slot clicks on Horizon's container screen.

## Degradations
- IPN is an inventory-screen mod: it needs a GUI/widget interaction surface (slot clicks, overlays) that Horizon's native mod API does not expose yet; same scope as horizon-rust/horizon #10025's GUI/widget gap. The port's slice 3 glue waits on that surface; issue to be filed once the saved GitHub token has Issues permission (currently 403 on writes to horizon-rust repos).

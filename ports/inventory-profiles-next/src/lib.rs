//! inventory-profiles-next runtime for Horizon (slice 1): the inventory grid
//! model, sorting comparators, layouts and the minimal move plan that the
//! client glue turns into slot clicks. Ported behaviorally from Inventory
//! Profiles Next (AGPL-3.0-or-later). The screen/interaction glue waits on
//! Horizon's GUI surface (upstream issue #10025's widget scope).

pub mod grid;
pub mod sort;

//! Entity radar: surrounding entities as dots with a custom color or as icons
//! (usually heads), with per-category toggles.

use crate::config::{Config, EntityDisplay};

/// An entity on the radar, in world coordinates relative to the player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entity {
    /// Category kind, kept abstract: the host registry classifies it.
    pub kind: EntityKind,
    pub dx: f64,
    pub dz: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind { Player, Mob, Item }

/// The dot color per category, the radar's "custom color" defaults.
pub fn dot_color(kind: EntityKind) -> (u8, u8, u8) {
    match kind {
        EntityKind::Player => (255, 255, 255),
        EntityKind::Mob => (255, 64, 64),
        EntityKind::Item => (255, 255, 0),
    }
}

/// Which entities the radar draws, honoring the display mode and the
/// per-category toggles. Entities outside the widget never draw.
pub fn visible(cfg: &Config, all: &[Entity], widget_size: u32) -> Vec<Entity> {
    if cfg.entity_radar == EntityDisplay::Off {
        return Vec::new();
    }
    let half = widget_size as f64 / 2.0;
    all.iter()
        .filter(|e| match e.kind {
            EntityKind::Player => cfg.radar_players,
            EntityKind::Mob => cfg.radar_mobs,
            EntityKind::Item => cfg.radar_items,
        })
        .filter(|e| e.dx.abs() <= half && e.dz.abs() <= half)
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config { Config::default() }

    #[test]
    fn off_radar_hides_everything() {
        let mut c = cfg();
        c.entity_radar = EntityDisplay::Off;
        let es = [Entity { kind: EntityKind::Player, dx: 1.0, dz: 1.0 }];
        assert!(visible(&c, &es, 128).is_empty());
    }

    #[test]
    fn category_toggles_filter() {
        let mut c = cfg();
        c.radar_items = false;
        let es = [
            Entity { kind: EntityKind::Player, dx: 1.0, dz: 1.0 },
            Entity { kind: EntityKind::Item, dx: 2.0, dz: 2.0 },
        ];
        let v = visible(&c, &es, 128);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, EntityKind::Player);
    }

    #[test]
    fn entities_outside_widget_do_not_draw() {
        let es = [Entity { kind: EntityKind::Mob, dx: 500.0, dz: 0.0 }];
        assert!(visible(&cfg(), &es, 128).is_empty());
    }

    #[test]
    fn dot_colors_differ_by_category() {
        assert_ne!(dot_color(EntityKind::Player), dot_color(EntityKind::Mob));
    }
}

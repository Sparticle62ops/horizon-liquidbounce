//! Port of `features/module/ModuleCategory.kt` and `ModuleCategories.kt`.

/// One module category. Upstream holds a display name, a color int and a
/// stable tag string; order matters for the click GUI and the config tree.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct ModuleCategory {
    pub display_name: &'static str,
    pub color: u32,
    pub tag: &'static str,
}

pub const COMBAT: ModuleCategory = ModuleCategory { display_name: "Combat", color: 0xFFFF5555, tag: "combat" };
pub const MOVEMENT: ModuleCategory = ModuleCategory { display_name: "Movement", color: 0xFF55FF55, tag: "movement" };
pub const RENDER: ModuleCategory = ModuleCategory { display_name: "Render", color: 0xFF5555FF, tag: "render" };
pub const PLAYER: ModuleCategory = ModuleCategory { display_name: "Player", color: 0xFFFFFF55, tag: "player" };
pub const WORLD: ModuleCategory = ModuleCategory { display_name: "World", color: 0xFF55FFFF, tag: "world" };
pub const MISC: ModuleCategory = ModuleCategory { display_name: "Misc", color: 0xFFFF55FF, tag: "misc" };
pub const CLIENT: ModuleCategory = ModuleCategory { display_name: "Client", color: 0xFFAAAAAA, tag: "client" };

/// Upstream `ModuleCategories.All`, in registration order.
pub fn all() -> [ModuleCategory; 7] {
    [COMBAT, MOVEMENT, RENDER, PLAYER, WORLD, MISC, CLIENT]
}

/// Upstream `ModuleCategories.getById` (case-insensitive, falls back to MISC).
pub fn get_by_id(id: &str) -> ModuleCategory {
    all()
        .into_iter()
        .find(|c| c.tag.eq_ignore_ascii_case(id))
        .unwrap_or(MISC)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_has_seven_in_order() {
        let a = all();
        assert_eq!(a.len(), 7);
        assert_eq!(a[0].display_name, "Combat");
        assert_eq!(a[6].display_name, "Client");
    }

    #[test]
    fn get_by_id_is_case_insensitive() {
        assert_eq!(get_by_id("COMBAT"), COMBAT);
        assert_eq!(get_by_id("movement"), MOVEMENT);
    }

    #[test]
    fn unknown_id_falls_back_to_misc() {
        assert_eq!(get_by_id("nonsense"), MISC);
    }
}

//! Automatic cave mode: the "solid roof" search the mod documents. Cave mode
//! activates when a solid roof of blocks sits above the player within the
//! configured roof size (a 3x3-and-larger search must NOT trigger under
//! scattered log blocks, per the mod page).

/// Minimal world read the algorithm needs: solid or not, above the player.
pub trait WorldColumns {
    /// Returns whether the block at (x, z) and height y (0 = player level + 1)
    /// is solid. Out-of-world reads are not solid.
    fn is_solid_above(&self, x: i32, z: i32, y: u16) -> bool;
    /// How many blocks above the player to search for the roof.
    fn search_depth(&self) -> u16 { 32 }
}

/// Whether cave mode activates for a player at (px, pz) with the configured
/// roof size `n` (odd; 1, 3, 5...). The search requires every column of the
/// n-by-n footprint centered on the player to have a solid roof block within
/// the search depth above.
pub fn cave_mode_active(world: &impl WorldColumns, px: i32, pz: i32, roof_size: u8, _slope_shading: bool) -> bool {
    let n = roof_size.max(1) as i32;
    let half = n / 2;
    for dx in -half..=half {
        for dz in -half..=half {
            let mut solid = false;
            for y in 1..=world.search_depth() {
                if world.is_solid_above(px + dx, pz + dz, y) {
                    solid = true;
                    break;
                }
            }
            if !solid {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct Grid {
        solid: HashSet<(i32, i32, u16)>,
    }
    impl WorldColumns for Grid {
        fn is_solid_above(&self, x: i32, z: i32, y: u16) -> bool {
            self.solid.contains(&(x, z, y))
        }
    }

    fn full_roof() -> Grid {
        let mut solid = HashSet::new();
        for x in -2..=2 {
            for z in -2..=2 {
                solid.insert((x, z, 5));
            }
        }
        Grid { solid }
    }

    #[test]
    fn full_roof_activates_cave_mode() {
        assert!(cave_mode_active(&full_roof(), 0, 0, 3, false));
    }

    #[test]
    fn open_sky_never_activates() {
        assert!(!cave_mode_active(&Grid { solid: HashSet::new() }, 0, 0, 3, false));
    }

    #[test]
    fn tree_canopy_does_not_activate_at_roof_size_3() {
        // a lone log-and-canopy footprint: only the center column is covered
        let mut solid = HashSet::new();
        solid.insert((0, 0, 6));
        let g = Grid { solid };
        assert!(!cave_mode_active(&g, 0, 0, 3, false));
    }

    #[test]
    fn single_column_roof_activates_at_roof_size_1() {
        let mut solid = HashSet::new();
        solid.insert((0, 0, 6));
        let g = Grid { solid };
        assert!(cave_mode_active(&g, 0, 0, 1, false));
    }

    #[test]
    fn roof_beyond_search_depth_does_not_count() {
        let mut solid = HashSet::new();
        for x in -2..=2 {
            for z in -2..=2 {
                solid.insert((x, z, 40));
            }
        }
        let g = Grid { solid };
        assert!(!cave_mode_active(&g, 0, 0, 3, false));
    }
}

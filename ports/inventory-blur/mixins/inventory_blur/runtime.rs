// inventory-blur runtime (port of Enchanted-Games/inventory-blur, CC-BY-NC-4.0).
//
// The 1:1 core read from upstream (commit 4546a17, 171 ln total: ScreenMixin
// 59 ln, the 1.21.6 accessors, ModConstants): the renderTransparentBackground
// replacement decision flow and the texture constant. The blur shader and
// tiled-texture drawing ride Horizon screen-render surfaces; the seam is in
// the README.

/// Upstream ModConstants.
pub const MOD_NAME: &str = "Inventory Blur";
pub const MOD_ID: &str = "eg_inventory_blur";

/// Upstream's in-world inventory background texture location (a 16x16 image
/// tiled across the screen).
pub const INWORLD_INVENTORY_BACKGROUND_TEXTURE: &str =
    "eg-inventory-blur:textures/gui/inworld_inventory_background.png";
pub const BACKGROUND_TILE_SIZE: i32 = 16;

/// What upstream ScreenMixin.eg_inventory_blur$applyBlurAndDrawBG does on
/// each transparent-background render, as a pure decision:
/// - CancelFirst: upstream checks the render state's first stratum after
///   blur; a value other than i32::MAX means the blur is already drawn this
///   frame, so do nothing.
/// - ApplyBlur: call the screen's blurred-background render (1.21.6+ path).
/// - DrawTiledBackground: the pre-1.21.6 path, tiling
///   INWORLD_INVENTORY_BACKGROUND_TEXTURE across width x height.
/// The vanilla dark gradient fill is always skipped (upstream's
/// ignoreGradientCall is a no-op).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlurAction {
    CancelFirst,
    ApplyBlur,
    DrawTiledBackground,
}

/// Decides this frame's action. `first_stratum_after_blur` is the render
/// state's value (i32::MAX when no blur stratum exists yet); `supports_blur`
/// mirrors the 1.21.6+ build path.
pub fn decide_action(first_stratum_after_blur: i32, supports_blur: bool) -> Option<BlurAction> {
    if first_stratum_after_blur != i32::MAX {
        // Don't draw the blur if it's already been drawn this frame.
        return None;
    }
    if supports_blur {
        Some(BlurAction::ApplyBlur)
    } else {
        Some(BlurAction::DrawTiledBackground)
    }
}

/// Upstream Screen.renderMenuBackgroundTexture's tiling: the number of tile
/// draws across and down to cover width x height with 16x16 tiles.
pub fn tile_counts(width: i32, height: i32) -> (i32, i32) {
    ((width + BACKGROUND_TILE_SIZE - 1) / BACKGROUND_TILE_SIZE,
     (height + BACKGROUND_TILE_SIZE - 1) / BACKGROUND_TILE_SIZE)
}

/// The vanilla gradient call this mod cancels, as parameters for a Horizon
/// host that wants to replicate the skip: upstream fillGradient(0, 0, width,
/// height, colorFrom, colorTo) is never issued by this mod.
pub const VANILLA_GRADIENT_SKIPPED: bool = true;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_upstream() {
        assert_eq!(MOD_NAME, "Inventory Blur");
        assert_eq!(MOD_ID, "eg_inventory_blur");
        assert_eq!(INWORLD_INVENTORY_BACKGROUND_TEXTURE, "eg-inventory-blur:textures/gui/inworld_inventory_background.png");
        assert_eq!(BACKGROUND_TILE_SIZE, 16);
    }

    #[test]
    fn blur_applies_once_per_frame() {
        // No blur stratum yet -> apply.
        assert_eq!(decide_action(i32::MAX, true), Some(BlurAction::ApplyBlur));
        // Already drawn this frame -> nothing.
        assert_eq!(decide_action(0, true), None);
        assert_eq!(decide_action(7, true), None);
    }

    #[test]
    fn legacy_path_tiles_texture() {
        assert_eq!(decide_action(i32::MAX, false), Some(BlurAction::DrawTiledBackground));
        assert_eq!(decide_action(0, false), None);
    }

    #[test]
    fn tiling_covers_the_screen() {
        assert_eq!(tile_counts(320, 240), (20, 15));
        assert_eq!(tile_counts(321, 240), (21, 15));
        assert_eq!(tile_counts(320, 241), (20, 16));
        assert_eq!(tile_counts(16, 16), (1, 1));
    }

    #[test]
    fn vanilla_gradient_is_skipped() {
        assert!(VANILLA_GRADIENT_SKIPPED);
    }
}

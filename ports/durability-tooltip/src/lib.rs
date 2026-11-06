//! durability-tooltip runtime (behavioral port of SuperMartijn642's Durability
//! Tooltip, All Rights Reserved; no upstream code is included).
//!
//! Upstream shows a tool or armor piece's durability inside its item tooltip in
//! three styles (the mod's documented feature set): numbers ("dur/max"), a
//! bar, and text. Client-side only; works on vanilla servers.
//!
//! The logic core is ported 1:1 against the behavior the mod's page documents:
//! the three styles, the vanilla durability-bar color curve, and the
//! damaged-items-only visibility rule. The rendering rides Horizon's item
//! tooltip; the seam is in the README.

/// Upstream tooltip style: the config file's `style` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TooltipStyle { Numbers, Bar, Text }

/// One item's durability as the tooltip logic needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Durability {
    /// Remaining durability points.
    pub remaining: i32,
    /// Maximum durability points (0 when the item has none).
    pub max: i32,
}

impl Durability {
    /// 0.0 (fully damaged) to 1.0 (fully repaired); None when the item has no durability.
    pub fn ratio(self) -> Option<f64> {
        if self.max <= 0 {
            None
        } else {
            Some(self.remaining as f64 / self.max as f64)
        }
    }
}

/// The mod's config file model with its defaults.
#[derive(Clone, Debug)]
pub struct Config {
    /// Whether the tooltip addition is on at all.
    pub enabled: bool,
    /// Which of the three tooltip styles is active.
    pub style: TooltipStyle,
    /// Only show the line on items that are actually damaged (upstream hides it on full-durability gear).
    pub only_when_damaged: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { enabled: true, style: TooltipStyle::Numbers, only_when_damaged: true }
    }
}

/// Vanilla's durability-bar color: full green, yellowing toward the middle,
/// red at the low end. The hue interpolation matches how the game's own
/// durability bar fades (red hue rises as the bar empties).
pub fn bar_color(ratio: f64) -> (u8, u8, u8) {
    let ratio = ratio.clamp(0.0, 1.0);
    // hue sweeps green (1/3) -> red (0) as durability empties
    let hue = ratio * 1.0 / 3.0;
    hsv_to_rgb(hue, 1.0, 1.0)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let i = (h * 6.0).floor() as i64;
    let f = h * 6.0 - i as f64;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

/// The tooltip line this mod adds, in the active style. Returns None when the
/// line should not be shown (mod disabled, item without durability, or
/// undamaged item under the only-when-damaged rule).
pub fn tooltip_line(cfg: &Config, d: Durability) -> Option<Line> {
    if !cfg.enabled {
        return None;
    }
    let ratio = d.ratio()?;
    if cfg.only_when_damaged && (ratio - 1.0).abs() < f64::EPSILON {
        return None;
    }
    Some(match cfg.style {
        TooltipStyle::Numbers => Line::Numbers { remaining: d.remaining, max: d.max },
        TooltipStyle::Bar => Line::Bar { fill: ratio.clamp(0.0, 1.0), color: bar_color(ratio) },
        TooltipStyle::Text => Line::Text { percent: (ratio * 100.0).round() as i32 },
    })
}

/// One rendered tooltip line, as the host seam draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    /// "dur/max".
    Numbers { remaining: i32, max: i32 },
    /// A bar `fill` full, drawn in `color`.
    Bar { fill: f64, color: (u8, u8, u8) },
    /// "NN%".
    Text { percent: i32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(style: TooltipStyle) -> Config {
        Config { enabled: true, style, only_when_damaged: true }
    }

    #[test]
    fn full_durability_hidden_by_default() {
        let d = Durability { remaining: 59, max: 59 };
        assert_eq!(tooltip_line(&cfg(TooltipStyle::Numbers), d), None);
    }

    #[test]
    fn full_durability_shown_when_not_restricted() {
        let mut c = cfg(TooltipStyle::Numbers);
        c.only_when_damaged = false;
        let d = Durability { remaining: 59, max: 59 };
        assert_eq!(tooltip_line(&c, d), Some(Line::Numbers { remaining: 59, max: 59 }));
    }

    #[test]
    fn no_durability_items_get_no_line() {
        let d = Durability { remaining: 0, max: 0 };
        assert_eq!(tooltip_line(&cfg(TooltipStyle::Bar), d), None);
    }

    #[test]
    fn disabled_config_gets_no_line() {
        let c = Config { enabled: false, ..Config::default() };
        let d = Durability { remaining: 10, max: 59 };
        assert_eq!(tooltip_line(&c, d), None);
    }

    #[test]
    fn numbers_style_reports_both_ends() {
        let d = Durability { remaining: 156, max: 271 };
        assert_eq!(
            tooltip_line(&cfg(TooltipStyle::Numbers), d),
            Some(Line::Numbers { remaining: 156, max: 271 })
        );
    }

    #[test]
    fn text_style_rounds_to_whole_percent() {
        let d = Durability { remaining: 1, max: 3 };
        assert_eq!(tooltip_line(&cfg(TooltipStyle::Text), d), Some(Line::Text { percent: 33 }));
    }

    #[test]
    fn bar_fills_to_ratio() {
        let d = Durability { remaining: 271, max: 542 };
        match tooltip_line(&cfg(TooltipStyle::Bar), d) {
            Some(Line::Bar { fill, .. }) => assert!((fill - 0.5).abs() < 1e-9),
            other => panic!("expected bar, got {other:?}"),
        }
    }

    #[test]
    fn bar_color_is_green_when_full_and_red_when_empty() {
        let (r, g, _) = bar_color(1.0);
        assert!(g > r, "full bar should be green, got {r} {g}");
        let (r2, g2, _) = bar_color(0.0);
        assert!(r2 > g2, "empty bar should be red, got {r2} {g2}");
    }

    #[test]
    fn mid_bar_is_yellowish() {
        let (r, g, _) = bar_color(0.5);
        assert!(r > 120 && g > 120, "mid bar should be yellow-ish, got {r} {g}");
    }

    #[test]
    fn ratio_clamps() {
        let d = Durability { remaining: 900, max: 100 };
        match tooltip_line(&cfg(TooltipStyle::Bar), d) {
            Some(Line::Bar { fill, .. }) => assert_eq!(fill, 1.0),
            other => panic!("expected bar, got {other:?}"),
        }
    }
}

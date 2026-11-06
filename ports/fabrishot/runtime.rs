// fabrishot runtime (Horizon port of github.com/ramidzkh/fabrishot, MIT,
// (c) 2021 Ramid Khan).
//
// Upstream swaps the game window to a custom resolution for a configurable
// number of frames, hides the HUD on request, then captures the framebuffer
// and writes it out in the configured image format. Horizon has no screenshot
// capture hook yet (README seams; horizon-rust/horizon #9606), so this module
// carries the capture driver's logic 1:1: the config model, the output
// filename logic, and the frame state machine that decides when the capture
// fires. The framebuffer read and the image encode are the only pieces left
// for the day the surface lands.

use std::collections::BTreeMap;

// One image format the capture can be written in, upstream FileFormat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FabrishotFileFormat {
    Png,
    Jpg,
    Tga,
    Bmp,
}

impl FabrishotFileFormat {
    // Upstream FileFormat.extension(): a dot plus the lowercase name.
    pub fn extension(self) -> &'static str {
        match self {
            FabrishotFileFormat::Png => ".png",
            FabrishotFileFormat::Jpg => ".jpg",
            FabrishotFileFormat::Tga => ".tga",
            FabrishotFileFormat::Bmp => ".bmp",
        }
    }

    // The Java enum constant name, as stored in fabrishot.properties.
    pub fn name(self) -> &'static str {
        match self {
            FabrishotFileFormat::Png => "PNG",
            FabrishotFileFormat::Jpg => "JPG",
            FabrishotFileFormat::Tga => "TGA",
            FabrishotFileFormat::Bmp => "BMP",
        }
    }
}

// Upstream Config with its original defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FabrishotConfig {
    pub custom_file_name: String,
    pub override_screenshot_key: bool,
    pub hide_hud: bool,
    pub save_file: bool,
    pub capture_width: u32,
    pub capture_height: u32,
    pub capture_delay: u32,
    pub capture_file_format: FabrishotFileFormat,
}

impl Default for FabrishotConfig {
    fn default() -> Self {
        FabrishotConfig {
            custom_file_name: "huge_%time%".to_string(),
            override_screenshot_key: false,
            hide_hud: false,
            save_file: true,
            capture_width: 3840,
            capture_height: 2160,
            capture_delay: 3,
            capture_file_format: FabrishotFileFormat::Png,
        }
    }
}

impl FabrishotConfig {
    // Upstream Config's static block: fabrishot.properties is read as Java
    // Properties, and any read or parse failure throws the whole load away
    // and saves the defaults. So one bad line means all defaults.
    pub fn load(text: &str) -> Self {
        let mut props: BTreeMap<String, String> = BTreeMap::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
                continue;
            }

            // Java Properties uses the first unescaped = or : as the
            // separator; fabrishot's keys and values carry neither, so a
            // plain split is faithful here.
            if let Some((key, value)) = line.split_once('=') {
                props.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        let parsed = Self::from_properties(&props);
        match parsed {
            Some(config) => config,
            None => Self::default(),
        }
    }

    fn from_properties(props: &BTreeMap<String, String>) -> Option<Self> {
        let get_bool = |key: &str, default: bool| -> Option<bool> {
            match props.get(key) {
                Some(value) => value.parse::<bool>().ok(),
                None => Some(default),
            }
        };
        let get_u32 = |key: &str| -> Option<u32> {
            props.get(key).and_then(|value| value.parse::<u32>().ok())
        };
        let get_format = |key: &str| -> Option<FabrishotFileFormat> {
            match props.get(key) {
                Some(value) => match value.as_str() {
                    "PNG" => Some(FabrishotFileFormat::Png),
                    "JPG" => Some(FabrishotFileFormat::Jpg),
                    "TGA" => Some(FabrishotFileFormat::Tga),
                    "BMP" => Some(FabrishotFileFormat::Bmp),
                    _ => None,
                },
                None => Some(FabrishotFileFormat::Png),
            }
        };

        Some(FabrishotConfig {
            custom_file_name: props
                .get("custom_file_name")
                .cloned()
                .unwrap_or_else(|| "huge_%time%".to_string()),
            override_screenshot_key: get_bool("override_screenshot_key", false)?,
            hide_hud: get_bool("hide_hud", false)?,
            save_file: get_bool("save_file", true)?,
            capture_width: get_u32("width")?,
            capture_height: get_u32("height")?,
            capture_delay: get_u32("delay")?,
            capture_file_format: get_format("file_format")?,
        })
    }

    // Upstream Config.save(): every field back as key=value lines.
    pub fn save(&self) -> String {
        let mut out = String::new();
        out.push_str("custom_file_name=");
        out.push_str(&self.custom_file_name);
        out.push('\n');
        out.push_str("override_screenshot_key=");
        out.push_str(if self.override_screenshot_key { "true" } else { "false" });
        out.push('\n');
        out.push_str("hide_hud=");
        out.push_str(if self.hide_hud { "true" } else { "false" });
        out.push('\n');
        out.push_str("save_file=");
        out.push_str(if self.save_file { "true" } else { "false" });
        out.push('\n');
        out.push_str("width=");
        out.push_str(&self.capture_width.to_string());
        out.push('\n');
        out.push_str("height=");
        out.push_str(&self.capture_height.to_string());
        out.push('\n');
        out.push_str("delay=");
        out.push_str(&self.capture_delay.to_string());
        out.push('\n');
        out.push_str("file_format=");
        out.push_str(self.capture_file_format.name());
        out.push('\n');
        out
    }
}

// Upstream Fabrishot.getScreenshotFile: the screenshots directory, the %time%
// and %world% placeholders (no_world when nothing is loaded), and the
// no-clobber suffix loop.
//
// The suffix sequence is upstream's own increment order: the first file takes
// no suffix, the second takes _3, the third _4, and so on. That gap is real
// upstream behavior (the Java i++ reads i before the increment and builds the
// suffix after it, so the _2 slot is never used) and this port preserves it
// exactly.
pub fn screenshot_file_name(
    prefix_template: &str,
    file_format: FabrishotFileFormat,
    time_formatted: &str,
    world: Option<&str>,
    mut exists: impl FnMut(&str) -> bool,
) -> String {
    let prefix = prefix_template
        .replace("%time%", time_formatted)
        .replace("%world%", world.unwrap_or("no_world"));

    let mut i = 1u32;

    loop {
        // (i++ == 1 ? "" : "_" + i) in Java: the compare reads i before the
        // increment, the suffix reads it after.
        let compared = i;
        i += 1;
        let suffix = if compared == 1 {
            String::new()
        } else {
            format!("_{i}")
        };

        let name = format!("{prefix}{suffix}{}", file_format.extension());
        if !exists(&name) {
            return name;
        }
    }
}

// What one render frame of an active capture wants, upstream
// CaptureTask.onRenderTick.
pub enum CaptureAction {
    // More frames to wait through; keep driving the task next frame.
    Wait,
    // The capture frame. Upstream reads the framebuffer here, encodes it on
    // its IO pool, restores the HUD if the task hid it, and ends the task.
    // On Horizon the framebuffer read is the seam (README); the caller
    // captures when it can, applies restore_resolution afterwards, and drops
    // the task.
    Fire,
}

// The per-capture state machine, upstream CaptureTask.
pub struct FabrishotCaptureTask {
    width: u32,
    height: u32,
    prev_width: u32,
    prev_height: u32,
    capture_delay: u32,
    hide_hud: bool,
    hid_hud: bool,
    frame: u32,
}

impl FabrishotCaptureTask {
    // Upstream Fabrishot.startCapture: a task exists only while a capture is
    // running, and starting one while another runs is a no-op.
    pub fn start(
        prev_width: u32,
        prev_height: u32,
        config: &FabrishotConfig,
    ) -> Self {
        FabrishotCaptureTask {
            width: config.capture_width,
            height: config.capture_height,
            prev_width,
            prev_height,
            capture_delay: config.capture_delay,
            hide_hud: config.hide_hud,
            hid_hud: false,
            frame: 0,
        }
    }

    // Upstream CaptureTask.setResolution: swap the window to the capture
    // size. Upstream notes that since 26.2 framebuffer resizing happens
    // during frame extraction, so only the GUI scale needs a refresh.
    pub fn capture_resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    // Upstream CaptureTask.restoreResolution.
    pub fn restore_resolution(&self) -> (u32, u32) {
        (self.prev_width, self.prev_height)
    }

    // Whether this task hid the HUD on frame 0, for the caller to restore on
    // Fire (upstream hides only when the HUD was visible and hide_hud is on,
    // and toggles it back on the capture frame).
    pub fn hid_hud(&self) -> bool {
        self.hid_hud
    }

    // One render frame of the capture, upstream CaptureTask.onRenderTick.
    // hud_is_hidden is the HUD's hidden state this frame. When this comes
    // back Fire, the capture fires and the task is done.
    pub fn on_render_tick(&mut self, hud_is_hidden: bool) -> CaptureAction {
        if self.frame == 0 {
            self.hid_hud = !hud_is_hidden && self.hide_hud;
            self.frame += 1;
            return CaptureAction::Wait;
        }

        if self.frame < self.capture_delay {
            self.frame += 1;
            return CaptureAction::Wait;
        }

        CaptureAction::Fire
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_upstream() {
        let config = FabrishotConfig::default();
        assert_eq!(config.custom_file_name, "huge_%time%");
        assert!(!config.override_screenshot_key);
        assert!(!config.hide_hud);
        assert!(config.save_file);
        assert_eq!(config.capture_width, 3840);
        assert_eq!(config.capture_height, 2160);
        assert_eq!(config.capture_delay, 3);
        assert_eq!(config.capture_file_format, FabrishotFileFormat::Png);
    }

    #[test]
    fn round_trips_properties() {
        let config = FabrishotConfig::default();
        let text = config.save();
        assert_eq!(FabrishotConfig::load(&text), config);
    }

    #[test]
    fn bad_line_falls_back_to_defaults() {
        let config = FabrishotConfig::load("width=128\nheight=not_a_number\n");
        assert_eq!(config, FabrishotConfig::default());
    }

    #[test]
    fn suffix_loop_is_upstreams() {
        let taken = ["huge_2026-01-02_03.04.05.png", "huge_2026-01-02_03.04.05_3.png"];
        let name = screenshot_file_name(
            "huge_%time%",
            FabrishotFileFormat::Png,
            "2026-01-02_03.04.05",
            None,
            |candidate| taken.contains(&candidate),
        );
        assert_eq!(name, "huge_2026-01-02_03.04.05_4.png");
    }

    #[test]
    fn world_placeholder_falls_back() {
        let name = screenshot_file_name(
            "huge_%world%",
            FabrishotFileFormat::Bmp,
            "t",
            None,
            |_| false,
        );
        assert_eq!(name, "huge_no_world.bmp");
    }

    #[test]
    fn capture_frames_match_upstream() {
        let config = FabrishotConfig::default();
        let mut task = FabrishotCaptureTask::start(1920, 1080, &config);
        assert_eq!(task.capture_resolution(), (3840, 2160));
        assert_eq!(task.restore_resolution(), (1920, 1080));
        // Frame 0 records the HUD and hides it only when asked.
        assert!(matches!(task.on_render_tick(false), CaptureAction::Wait));
        assert!(!task.hid_hud());
        let mut task = FabrishotCaptureTask::start(1920, 1080, &FabrishotConfig {
            hide_hud: true,
            ..FabrishotConfig::default()
        });
        assert!(matches!(task.on_render_tick(false), CaptureAction::Wait));
        assert!(task.hid_hud());
        // Delay 3: two more wait frames, then the capture fires.
        assert!(matches!(task.on_render_tick(true), CaptureAction::Wait));
        assert!(matches!(task.on_render_tick(true), CaptureAction::Wait));
        assert!(matches!(task.on_render_tick(true), CaptureAction::Fire));
    }
}

# fabrishot (Horizon port)

A Rust port of [fabrishot](https://github.com/ramidzkh/fabrishot) (MIT,
(c) 2021 Ramid Khan, itself a Fabric port of ata4's mineshot), which takes
screenshots at a custom resolution far above the window size: it swaps the
game window to the configured width and height for a few frames, hides the
HUD on request, then captures the framebuffer and writes the image out.

## Ported (logic core)

- All eight config values with the original defaults (custom_file_name
  `huge_%time%`, override_screenshot_key off, hide_hud off, save_file on,
  3840x2160, delay 3, PNG), loaded and saved as
  `fabrishot.properties`-style key=value lines with upstream's all-or-nothing
  fallback to defaults on a bad line.
- The output filename: the screenshots directory, the `%time%` and `%world%`
  placeholders with the `no_world` fallback, and the no-clobber suffix loop,
  preserving upstream's exact suffix sequence (first file plain, then _3,
  _4, ... because the Java increment order never fills the _2 slot).
- The capture state machine: the resolution swap with bookkeeping for the
  restore, the delay frame count, and the HUD hide/restore around the
  capture frame.
- The FileFormat set (PNG, JPG, TGA, BMP) with upstream's extensions.

## Horizon seams

The heart of fabrishot is the framebuffer read, and Horizon has no screenshot
hook yet: the client's own screenshot path supports a custom capture size
(`ScreenshotRequest` carries `size`), but no native mod can request a capture.
There is also no image encode/write surface a mod could drive. Everything
around it is here and 1:1: the config, the filename logic, and the capture
driver's frame state machine, so the port fires the moment a capture surface
lands.

- Filed on horizon-rust/horizon per standing policy
  ([#9606](https://github.com/horizon-rust/horizon/issues/9606)); tracked
  until patched, then the port upgrades to full 1:1.

## License

Upstream fabrishot: MIT (c) 2021 Ramid Khan. This port carries the same
license; see LICENSE.

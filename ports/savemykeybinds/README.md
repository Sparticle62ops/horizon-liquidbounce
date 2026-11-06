# Save My Keybinds for Horizon

Save and load presets of your keybinds. A Rust port of
[SaveMyKeybinds](https://github.com/MisterCheezeCake/SaveMyKeybinds)
(by MisterCheezeCake, LGPL-3.0-or-later) for
[Horizon](https://github.com/horizon-rust/horizon), a native mod built on
horizon-mixins.

## What it does

- `/savemykeybinds save <name>` saves every keybind to a preset.
- `/savemykeybinds load <name>` applies a preset. A strict preset (its JSON
  says `"strict": true`) unbinds every key it does not name.
- `/savemykeybinds delete <name>` deletes a preset.
- `/savemykeybinds list` lists the presets. `/smkb` is a shorter alias.

Presets live in a `savemykeybinds` folder in the config dir as
`<name>.preset.json` and keep the original's JSON shape, so presets made with
the original mod load here unchanged.

The original adds its buttons to the options screen; Horizon native mods have
no way to add buttons to that screen yet, so the commands stand in for them.

## License

LGPL-3.0-or-later, see LICENSE. The original mod is credited above.

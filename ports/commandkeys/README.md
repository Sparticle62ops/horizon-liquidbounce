# commandkeys (Horizon port)

Rust port of CommandKeys by TerminalMC (github.com/TerminalMC/CommandKeys,
Apache-2.0) for the Horizon client; this port keeps Apache-2.0.

Bind keys to macros: lists of chat messages/commands with per-message delays
(`delayTicks`), spacing-based repeats (`sendMode: REPEAT` with `spaceTicks`
and `maxRepeats`), message cycling (`CYCLE`, the alt keybind cycles
backwards, messages split on ",,"), random pick (`RANDOM`), dual-key
activation (`keybind.limitKeyName` must be held), HOLD/RELEASE/VANILLA
activation, a global ratelimit (`ratelimitCount` triggers per
`ratelimitTicks`) and the chat length limit.

Config lives at the mod data dir's `commandkeys.json`, schema-compatible with
the original's `config/commandkeys.json`.

Degradations vs the original (Horizon hooks missing):

- No GUI: config is file-only, the K config key and the vanilla KeyMapping
  macro keys (`mcActivatorKey`) do nothing.
- `TYPE`/`EDIT` modes send the first message directly instead of opening the
  chat screen; a message that is exactly `%edit%` is skipped.
- The ratelimit and length-limit client messages are not shown; blocked
  sends are silently dropped.
- Conflict strategies are approximated: AVOID skips the macro, and VETO or
  ASSERT on any macro sharing the pressed key suppresses all of them.
- The multiplayer default profile is used; `ratelimitSp` singleplayer
  detection is not available, so the ratelimit applies everywhere.
- Key names follow Horizon's vanilla-style names (`key.keyboard.z`,
  `key.mouse.left`); state tracking starts at mod load, so keys already held
  before then count as up.

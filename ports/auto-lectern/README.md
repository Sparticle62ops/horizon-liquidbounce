# auto-lectern (Horizon port)

A Rust port of [Auto Lectern](https://github.com/capthehacker99/AutoLectern)
(capthehacker99, CC-BY-NC-SA-4.0), which cycles librarian trades by breaking
and replacing lecterns until an offered book matches your goal.

## Scope of this commit (phase 1: state logic, 1:1)

The 1:1 state logic lives in mixins/auto_lectern/runtime.rs:

- AlGoal, upstream's ALGoal record: enchant id plus level and price windows,
  -1 as the extreme-bound sentinel.
- The match functions verbatim, including upstream's quirk that a -1 level
  minimum with a set level maximum accepts only level 1 or below.
- The vanilla villager price helpers: cheapest = maxLevel*3+2, most expensive
  = maxLevel*3+2+(4+maxLevel*10), both doubled for double-trade-price
  enchants and capped at 64 emeralds.
- The signal bitmask (prof, trade, trade-ok, item) and the ALState cycle
  states (stopped, stopping, starting, breaking, waiting-item, placing,
  waiting-prof, interact-vil, waiting-trade).
- The cycle driver: goals, config flags (itemSync, breakCooldown, logTrade,
  preBreaking, preserveTool, autoRemove, attempts), signals, trade-uuid
  counter, first-matching-goal lookup.

7 tests green.

## Next phase: interaction wiring

The tick-loop half (break the lectern, wait for the item, place it, wait for
the villager profession, open the trade, read the offered book, stop or
continue) drives Horizon client interaction surfaces. Which surfaces exist
and which need an upstream issue are documented as that phase lands.

## License

Upstream Auto Lectern: CC-BY-NC-SA-4.0 (c) capthehacker99. This port carries
the same license; see LICENSE.

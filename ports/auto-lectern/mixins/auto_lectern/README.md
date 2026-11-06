# Auto Lectern for Horizon

Farms librarian trades: locks you on a lectern under your crosshair, breaks
and re-places it until the nearby villager takes the librarian profession,
opens its trade menu, and trades the enchanted book that matches your goals
(cheapest, most expensive, or the first in range), then stops with a chat
message.

A Rust port of [AutoLectern](https://modrinth.com/mod/auto-lectern) by
sys.exe (CC BY-SA 4.0). Same behavior, re-written for Horizon's native mod
API; the settings live in the `/autolec` command instead of `autolec.txt`.

## Commands

- `/autolec start` and `/autolec stop` - the farm.
- `/autolec status` - the current state.
- `/autolec goals add <enchant> <lvlMin> <lvlMax> [priceMin] [priceMax]`
- `/autolec goals list | clear`
- `/autolec autoTrade off|enchant|cheapest|most`
- `/autolec itemSync|logTrade|preBreak|preserveTool|autoRemove on|off`
- `/autolec breakCooldown <seconds>`

## License

This port, like the original, is CC BY-SA 4.0. AutoLectern is (c) sys.exe.

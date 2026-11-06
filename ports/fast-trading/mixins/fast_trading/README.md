# Fast Trading for Horizon

A Rust port of [FastTrading](https://github.com/bendy1234/FastTrading)
(bendy1234, MIT) for the [Horizon](https://github.com/horizon-rust) Minecraft
client: a speed trade button on the merchant screen repeats the selected
trade as fast as the settings allow.

The original's behavior is kept:

- The button sits at the merchant panel's right edge (`leftPos + 247`,
  `topPos + 36`, 18x20), reads the trade state the original computes
  (`NO_SELECTION`, `OUT_OF_STOCK`, `NO_ROOM_FOR_SELL_ITEM`, `CAN_PERFORM`,
  `NOT_ENOUGH_BUY_ITEMS`), and only starts when the trade can be made and
  the trade block rules allow it.
- `SpeedTradeTimer` counts ticks; at `ticksBetweenActions` (default 1s,
  minimum 0.025s) the repeat alternates between autofilling the payment
  slots and taking the result (a shift-click on the result slot).
- The repeat stops on out-of-stock, no room, a price change
  (`on-increase` / `on-change` / `disabled`), a different selected trade,
  new offers (`stopOnNewOffers`), or the screen closing, and empties the
  payment slots when it stops.
- `autofillBehavior` `DEFAULT` re-selects the trade so the server moves the
  payment items in; `STRICT` empties the payment slots first, then fills
  them from the inventory with pickup clicks, keeping the cursor's overflow
  on the source slot, matching items and components exactly.
- `tradeBlockBehavior` `DAMAGEABLE` (the default) / `UNSTACKABLE` / `DISABLED`
  refuses a trade whose result is damageable or unstackable, unless the
  override key is held.

Horizon differences, standing in for the original's Fabric-only pieces:

- The config screen is the `/fasttrading` command (`/ftrade` for short):
  `status`, `ticks <seconds>`, `autofill default|strict`,
  `block damageable|unstackable|disabled`,
  `price-stop on-increase|on-change|disabled`, `new-offers on|off`. The
  settings hold until the client restarts, like the original's defaults
  before they are saved; the button itself draws as a colored panel with the
  state in its color (green primed, yellow no room, red out of stock, grey
  otherwise) instead of the original's texture, which is its own asset.
- The override key is fixed to Left Alt, the original keymap's default.

MIT, see `LICENSE`.

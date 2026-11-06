# auto-reauth for Horizon

Automatically refreshes a Microsoft-account session when it expires, so a long
play session does not end at the token's expiry. A Rust port of
[Auto Reauth](https://github.com/Basicprogrammer10/auto-reauth) by Connor
(MIT), built on Horizon's native mod API.

## Usage

- `/autoreauth login` opens the Microsoft sign-in page in your browser and
  stores the tokens when you finish it (the same OAuth client as the original,
  so logins carry over). Finish the sign-in, then run `/autoreauth status`.
- `/autoreauth status` checks the stored session against the Minecraft session
  service and reports `Online`, `Invalid`, `Offline` or `Unknown`.
- `/autoreauth reauth` refreshes the stored tokens through the
  Microsoft -> Xbox -> XSTS -> Minecraft chain.
- `/autoreauth auto on|off` toggles the automatic refresh (on by default):
  when a status check finds the game's session token stale and a stored
  account exists, the mod refreshes and re-signs the chat keys on its own.
- `/reauth` is a short alias for `/autoreauth status`.

Tokens are stored in `<config dir>/auto-reauth/config.json`, the same folder
the game keeps its other config in. Like the original warns: that file holds
live authentication tokens, so treat it like a password.

## What is faithful

The whole Microsoft authentication chain, the session status check, the
stored-account flow and the auto toggle work as in the original, and the
stored tokens are compatible with its OAuth client.

## What had to change

- Chat commands stand in for the config screen, the status overlay and the
  toasts: Horizon's native mod API cannot draw on those screens.
- The config is JSON, not an NBT file, and holds one account: Horizon itself
  holds a single session.
- Login runs in its own thread so the browser wait never freezes the game;
  the original shows a waiting screen instead.
- Horizon reads its session from the launcher once at start and keeps it for
  the whole run, so a refreshed token cannot replace the session the game
  joins servers with mid-run. What the mod refreshes is the stored tokens and
  the chat-signing keys. Until Horizon grows a way to swap the running
  session, a join refused as "invalid session" still needs a restart from the
  launcher; the refreshed stored tokens are what the next steps can use.

## License

MIT, like the original. See LICENSE.

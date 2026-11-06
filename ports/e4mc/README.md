# e4mc for Horizon

Open your Horizon world to friends without port forwarding: the mod asks the
e4mc relay for a short domain, friends join it, and every connection rides a
QUIC stream through the relay into your world. A Rust port of
[e4mc](https://github.com/vgskye/e4mc-minecraft-architectury) (MIT, by vgskye
and contributors); the relay protocol follows
[e4mc-quiclime](https://github.com/vgskye/e4mc-quiclime) (Apache-2.0).

## Status and degradations

- The relay session is ported: broker lookup (or a hard-coded relay), the QUIC
  control channel (ALPN `quiclime`, JSON kind-tagged messages: probe,
  capabilities, domain assignment), and the stream-to-TCP bridge that carries a
  joining player to the hosted world's local listener.
- `/e4mc stop`, `/e4mc restart`, `/e4mc doctor`, `/e4mc conninfo` and the
  dedicated-command restore (ban/pardon/whitelist) come with the hook wiring.
- Dialtone (Iroh peer-to-peer) transport is not ported yet: joining rides the
  relay only.
- The original's Minecraft-only mixins (server address resolution, whitelist
  patches) are replaced by Horizon's own connection path.

## Config

`<config dir>/e4mc/config.json` (same defaults as the original's TOML): broker
on by default (`https://broker.e4mc.link/getBestRelay`), fallback relay
`test.e4mc.link:25575`, and `local_target` for where joining connections land.

## Licence

MIT, like the original. e4mc is by vgskye and contributors; this port keeps
their notices and credits them for the design.

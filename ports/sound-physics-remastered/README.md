# sound-physics-remastered (Horizon Rust port)

Behavioral port of [Sound Physics Remastered](https://github.com/henkelmax/Sound-Physics-Remastered) (GPL-3.0-only) for the Horizon client. Port repo: `Sparticle62ops/horizon-sound-physics-remastered`; master copy in `horizon-modport/ports/sound-physics-remastered`.

## Status
- [x] Slice 1: the pure sound physics. Per-block sound materials (absorption + reflectivity from the block's sound type), direct-path occlusion raymarching (half-block sampling, one hit per voxel), distance falloff, and raytraced reverb estimation (deterministic LCG rays, bounce budget, mean-free-path decay). 10/10 tests.
- [ ] Slice 2: client glue, feeding the physics from Horizon's world snapshot and applying the computed occlusion/reverb to sound playback.

## Degradations
- Client-side world access for the raymarcher rides world snapshots (horizon-rust/horizon #10152, open).
- SPR hooks Minecraft's client sound engine to resample each played sound's volume/reverb per position; Horizon's native mod API has no sound playback hook yet. The client glue (slice 2) waits on that surface; filed: horizon-rust/horizon #10444 (sound playback hook).

# Staging plan for the SVC client mod (horizon-mixins 0.6.0)

Verified from horizon-mixins/README.md + docs/crates-data-and-named-hooks.md,
~/horizon/native-hooks.json (hook catalog) and ~/horizon/Cargo.lock (2026-11-04).

## Architecture
- 0.6.0 bundles get [[crate]] entries: the mod's own crates staged as workspace
  members of the modded game. Deps must be PLAIN registry requirements that
  already exist in Horizon's Cargo.lock (its lock is the allow list). No path,
  no git deps, no deps between bundle crates, no dep on the game crate.
- Lock check done: ring 0.17.14, serde 1.0.229, serde_json, cpal 0.16.0,
  cc, libc, pkg-config, cpufeatures are all available. opus / audiopus_sys /
  libopus-sys are NOT in the lock, so the audio module's opus = "0.3" dep is
  refused at staging (E_CRATE_DEPENDENCY).
- All hooks used here exist in Horizon's catalog at version 1: world_lifecycle,
  net_from_server, player_submit, input_key, command, frame (native-hooks.json
  at the Horizon repo root; plan resolves hook+hook_version).

## Steps
1. Merge protocol/session/audio/glue into ONE bundle crate svc_client at
   crates/svc_client inside the bundle (bundle crates cannot depend on each
   other). Keep the standalone workspace for tests until the merge passes,
   then drop it.
2. Replace the opus crate dep with vendored libopus C inside the crate:
   vendor libopus source (Xiph, BSD, NOT henkelmax code), build.rs compiles it
   with cc (cc/libc/pkg-config are in the lock), and a small hand-written FFI
   module exposes the same Encoder/Decoder API the audio module uses.
3. Module payload rules: no inner doc comments (//!), no external mod files,
   no include!. Helpers compile in the game crate and may import native types.
4. mixin.toml now carries [[crate]] svc_client + named hooks (hook_version 1).
5. Then: template tools/horizon.py stage -> check -> test against the pinned
   ~/horizon export, fix helper binding mismatches in-client, wire cpal mic +
   playback (cpal 0.16 is lockable), package and submit.

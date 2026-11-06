# visuality (Horizon port)

Rust port of [Visuality](https://github.com/PinkGoosik/visuality) by PinkGoosik,
MIT, read upstream at commit fd8e592. Upstream is 27 Java files / 1,277 lines.

## Scope ported (logic core, 1:1)

- config/VisualityConfig: all defaults verbatim (hit particle, shiny armor and
  shiny block entry lists, water-circles density 10 / radius 16, slime color
  8978297) plus WaterCirclesConfigOption.
- registries: HitParticleRegistry entry parsing ("entity/particle"), shiny
  armor/block lists with # tag entries and isShiny matching.
- CirclesOnWaterEvent.onTick: shown() gate order (config, paused, particle
  status, player, underwater, overworld, raining, biome), density roll
  nextInt(density) + density/2, radius-offset scan, heightmap + water-below /
  air-above / fluid-amount-8 spawn condition, WATER_CIRCLE at +0.05.
- LivingEntityMixin: hurtClient hit particles (attacker attribute-modifier
  ATTACK_DAMAGE count = (int)(amount*0.5), default nextInt(3), 10-tick delay,
  chicken cold/warm variant feathers), spawnHitParticles height formula and
  capped loop, tick sparkles (slot heights 0.25/0.65/1.1/1.65-while-sneaking
  else 1.85, chance nextInt(20 - 2*count) == 0, self-player first-person gate,
  baby zombie skip).
- EntityMixin sprint soul puffs (1 in 5 over wither-summon base blocks),
  BlockMixin randomDisplayTick (soul above wither bases nextFloat > 0.995,
  shiny-block sparkles nextFloat > 0.8 with the axis fixed-offset math
  0.5 + 0.5625*step), AmethystClusterMixin sparkles (height > 5, nextFloat >
  0.5), CreeperEntityMixin charge (powered, nextInt(20) == 0).
- 13 particle ids (sparkle, bone, wither_bone, feather, cold_feather,
  warm_feather, small/medium/big_slime_blob, charge, water_circle, emerald,
  soul) and the rainbow color constants.

## Degradations (need Horizon support, issue filed on horizon-rust/horizon)

- No client particle spawning/rendering surface: particle textures, movement,
  collision and scaling (the seven particle classes) and every addParticle call
  cannot render. The spawn logic above yields (particle, x, y, z) events that
  glue would forward once the surface exists.
- Missing world/entity hooks: block animateTick, entity tick with entity data
  (powered creeper, slime size), client hurt hook, camera type (first-person),
  chicken variant, item attribute-modifier data read, client heightmap / fluid
  state / biome precipitation queries.

Note: upstream's SlimeEntityMixin is fully commented out in the source; the
slime config options exist but the spawn path is disabled upstream too.

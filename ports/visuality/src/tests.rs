use crate::circles::*;
use crate::config::*;
use crate::ids::ResourceId;
use crate::particles;
use crate::registries::*;
use crate::rng::JavaRandom;
use crate::rules::*;
use crate::sparkles::*;

fn rid(s: &str) -> ResourceId {
    ResourceId::parse(s).unwrap()
}

#[test]
fn config_defaults_match_upstream() {
    let c = VisualityConfig::default();
    assert!(c.slime_enabled && c.charge_enabled && c.sparkle_enabled && c.soul_enabled);
    assert_eq!(c.slime_color, 8978297);
    assert!(c.hit_particles_enabled && c.shiny_armor_enabled && c.shiny_blocks_enabled);
    assert_eq!(c.water_circles, WaterCirclesConfigOption::default());
    assert_eq!(c.hit_particle_entries.len(), 6);
    assert_eq!(c.hit_particle_entries[0], "minecraft:skeleton/visuality:bone");
    assert_eq!(c.shiny_armor_entries.len(), 8);
    assert_eq!(c.shiny_block_entries, ["#minecraft:gold_ores", "#minecraft:diamond_ores", "#minecraft:emerald_ores"]);
}

#[test]
fn hit_registry_parses_and_gates_lookups() {
    let src = vec![
        "minecraft:skeleton/visuality:bone".to_string(),
        "minecraft:chicken/visuality:feather".to_string(),
        "minecraft:missing_entity/visuality:bone".to_string(),
        "no-slash-entry".to_string(),
    ];
    let known_entity = |e: &ResourceId| e.namespace == "minecraft" && e.path != "missing_entity";
    let known_particle = |p: &ResourceId| p.namespace == "visuality";
    let out = reload_hit_particles(&src, &known_entity, &known_particle);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].entity, rid("minecraft:skeleton"));
    assert_eq!(out[0].particle, rid("visuality:bone"));
    assert_eq!(out[1].entity, rid("minecraft:chicken"));
}

#[test]
fn shiny_registries_split_items_and_tags() {
    let src = vec![
        "minecraft:golden_helmet".to_string(),
        "#minecraft:gold_ores".to_string(),
        "not an id!".to_string(),
    ];
    let armor = ShinyArmorRegistry::reload(&src);
    assert_eq!(armor.0.items, [rid("minecraft:golden_helmet")]);
    assert_eq!(armor.0.tags, [rid("minecraft:gold_ores")]);
    assert!(armor.is_shiny(&rid("minecraft:golden_helmet"), &|_| false));
    assert!(armor.is_shiny(&rid("minecraft:deepslate_gold_ore"), &|t| *t == rid("minecraft:gold_ores")));
    assert!(!armor.is_shiny(&rid("minecraft:iron_helmet"), &|_| false));
}

struct TestWorld {
    raining: bool,
    paused: bool,
    minimal: bool,
    player: Option<PlayerSnapshot>,
    overworld: bool,
    biome: Option<Biome>,
    spawns: Vec<(String, f64, f64, f64)>,
}

impl Default for TestWorld {
    fn default() -> Self {
        TestWorld {
            raining: true,
            paused: false,
            minimal: false,
            player: Some(PlayerSnapshot { x: 100, y: 64, z: 200, under_water: false }),
            overworld: true,
            biome: Some(Biome { precipitation: Precipitation::Rain, warm_enough_to_rain: true }),
            spawns: Vec::new(),
        }
    }
}

impl CirclesWorld for TestWorld {
    fn is_paused(&self) -> bool { self.paused }
    fn particles_minimal(&self) -> bool { self.minimal }
    fn player(&self) -> Option<PlayerSnapshot> { self.player }
    fn is_raining(&self) -> bool { self.raining }
    fn is_overworld(&self) -> bool { self.overworld }
    fn sea_level(&self) -> i32 { 63 }
    fn biome_at(&self, _pos: BlockPos) -> Option<Biome> { self.biome }
    fn heightmap_top_motion_blocking(&self, pos: BlockPos) -> BlockPos { pos }
    fn block_state(&self, pos: BlockPos) -> BlockRef {
        let p = self.player.unwrap();
        if pos.y < p.y { BlockRef::Water } else { BlockRef::Air }
    }
    fn fluid_amount(&self, _pos: BlockPos) -> i32 { 8 }
    fn add_particle(&mut self, particle: &str, x: f64, y: f64, z: f64) {
        self.spawns.push((particle.to_string(), x, y, z));
    }
}

fn tick_world(world: &mut TestWorld, config: &VisualityConfig) {
    let mut rng = JavaRandom::new(7);
    on_tick(world, config, &mut rng);
}

#[test]
fn water_circles_gates_block_every_spawn() {
    let config = VisualityConfig::default();
    let mut disabled_config = config.clone();
    disabled_config.water_circles.enabled = false;
    for mut_world in [
        TestWorld { raining: false, ..Default::default() },
        TestWorld { paused: true, ..Default::default() },
        TestWorld { minimal: true, ..Default::default() },
        TestWorld { player: Some(PlayerSnapshot { x: 1, y: 64, z: 1, under_water: true }), ..Default::default() },
        TestWorld { overworld: false, ..Default::default() },
    ] {
        let mut w = mut_world;
        tick_world(&mut w, &config);
        assert!(w.spawns.is_empty());
    }
    let mut w = TestWorld::default();
    tick_world(&mut w, &disabled_config);
    assert!(w.spawns.is_empty());
}

#[test]
fn water_circles_spawns_inside_the_density_roll() {
    let config = VisualityConfig::default();
    let mut w = TestWorld::default();
    tick_world(&mut w, &config);
    // randomDensity = nextInt(10) + 5, loop is inclusive -> 6..=15 spawns.
    assert!(w.spawns.len() >= 6 && w.spawns.len() <= 15, "got {}", w.spawns.len());
    for (particle, x, y, z) in &w.spawns {
        assert_eq!(particle, particles::WATER_CIRCLE);
        // x/z offset is nextInt(16) - 8 around the player, plus one random double.
        assert!(*x >= 92.0 && *x < 108.0);
        assert!((*y - 64.05).abs() < 1e-9);
        assert!(*z >= 192.0 && *z < 208.0);
    }
}

#[test]
fn water_circles_zero_density_is_a_noop() {
    let mut config = VisualityConfig::default();
    config.water_circles.density = 0;
    let mut w = TestWorld::default();
    tick_world(&mut w, &config);
    assert!(w.spawns.is_empty());
}

fn living() -> LivingSnapshot {
    LivingSnapshot {
        entity_type: Some(rid("minecraft:skeleton")),
        is_alive: true,
        client_side: true,
        x: 10.0,
        y: 20.0,
        z: 30.0,
        bb_height: 1.95,
        ..Default::default()
    }
}

#[test]
fn hit_particles_use_weapon_damage_and_latch_delay() {
    let config = VisualityConfig::default();
    let entries = reload_hit_particles(
        &config.hit_particle_entries,
        &|_| true,
        &|_| true,
    );
    let mut state = living();
    let attacker = Attacker { is_living: true, main_hand_attack_damage: Some(4.0) };
    let mut delay = 0;
    let mut rng = JavaRandom::new(1);
    let mut spawns = Vec::new();
    hurt_client(&state, Some(&attacker), &entries, &config, &mut rng, &mut delay, |p, x, y, z| {
        spawns.push((p.to_string(), x, y, z))
    });
    // count = (int)(4.0 * 0.5) = 2, loop is inclusive -> 3 spawns.
    assert_eq!(delay, 10);
    assert_eq!(spawns.len(), 3);
    for (particle, x, y, z) in &spawns {
        assert_eq!(particle, particles::BONE);
        assert_eq!(*x, 10.0);
        assert_eq!(*z, 30.0);
        assert!(*y >= 20.2 && *y < 20.2 + 2.45, "y={y}");
    }
    // Still latched: a second hurt spawns nothing.
    let mut spawns2 = Vec::new();
    hurt_client(&state, Some(&attacker), &entries, &config, &mut rng, &mut delay, |p, x, y, z| {
        spawns2.push((p.to_string(), x, y, z))
    });
    assert!(spawns2.is_empty());
}

#[test]
fn hit_particles_skip_non_living_attackers_and_disabled_config() {
    let config = VisualityConfig::default();
    let entries = reload_hit_particles(&config.hit_particle_entries, &|_| true, &|_| true);
    let state = living();
    let ghost = Attacker { is_living: false, main_hand_attack_damage: None };
    let mut delay = 0;
    let mut rng = JavaRandom::new(2);
    let mut count = 0;
    hurt_client(&state, Some(&ghost), &entries, &config, &mut rng, &mut delay, |_, _, _, _| count += 1);
    assert_eq!(count, 0);
    let mut off = config.clone();
    off.hit_particles_enabled = false;
    hurt_client(&state, Some(&Attacker { is_living: true, main_hand_attack_damage: None }), &entries, &off, &mut rng, &mut delay, |_, _, _, _| count += 1);
    assert_eq!(count, 0);
}

#[test]
fn chicken_variants_swap_the_feather() {
    let config = VisualityConfig::default();
    let entries = reload_hit_particles(&config.hit_particle_entries, &|_| true, &|_| true);
    let mut state = living();
    state.entity_type = Some(rid("minecraft:chicken"));
    state.is_chicken = true;
    state.chicken_variant_cold = true;
    let attacker = Attacker { is_living: true, main_hand_attack_damage: None };
    let mut delay = 0;
    let mut rng = JavaRandom::new(3);
    let mut seen = Vec::new();
    hurt_client(&state, Some(&attacker), &entries, &config, &mut rng, &mut delay, |p, _, _, _| seen.push(p.to_string()));
    assert!(seen.iter().all(|p| p == particles::COLD_FEATHER));
    state.chicken_variant_cold = false;
    state.chicken_variant_warm = true;
    let mut seen2 = Vec::new();
    let mut delay2 = 0;
    hurt_client(&state, Some(&attacker), &entries, &config, &mut rng, &mut delay2, |p, _, _, _| seen2.push(p.to_string()));
    assert!(seen2.iter().all(|p| p == particles::WARM_FEATHER));
}

fn shiny_state() -> LivingSnapshot {
    LivingSnapshot {
        entity_type: Some(rid("minecraft:zombie")),
        is_alive: true,
        client_side: true,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        armor: [
            Some(rid("minecraft:golden_boots")),
            Some(rid("minecraft:golden_leggings")),
            Some(rid("minecraft:golden_chestplate")),
            Some(rid("minecraft:diamond_helmet")),
        ],
        ..Default::default()
    }
}

#[test]
fn sparkles_need_shiny_armor_and_bounded_positions() {
    let config = VisualityConfig::default();
    let armor = ShinyArmorRegistry::reload(&config.shiny_armor_entries);
    let state = shiny_state();
    let mut rng = JavaRandom::new(9);
    let mut spawns = Vec::new();
    for _ in 0..500 {
        let mut s = Vec::new();
        spawn_sparkles(&state, &armor, &|_| false, &mut rng, |p, x, y, z| s.push((p.to_string(), x, y, z)));
        // At most one sparkle per tick.
        assert!(s.len() <= 1);
        for (particle, x, y, z) in &s {
            assert_eq!(particle, particles::SPARKLE);
            assert!(*x > -0.5 && *x < 0.5);
            assert!(*y > 0.05 && *y < 2.05);
            assert!(*z > -0.5 && *z < 0.5);
        }
        spawns.extend(s);
    }
    assert!(!spawns.is_empty(), "500 ticks should land at least one sparkle");
}

#[test]
fn sparkles_skip_baby_zombies_and_non_shiny_armor() {
    let config = VisualityConfig::default();
    let armor = ShinyArmorRegistry::reload(&config.shiny_armor_entries);
    let mut state = shiny_state();
    state.is_baby_zombie = true;
    let mut rng = JavaRandom::new(4);
    let mut count = 0;
    spawn_sparkles(&state, &armor, &|_| false, &mut rng, |_, _, _, _| count += 1);
    assert_eq!(count, 0);
    let mut plain = living();
    plain.armor = [None, None, None, None];
    let mut count2 = 0;
    spawn_sparkles(&plain, &armor, &|_| false, &mut rng, |_, _, _, _| count2 += 1);
    assert_eq!(count2, 0);
}

#[test]
fn entity_tick_respects_camera_and_config() {
    let config = VisualityConfig::default();
    let armor = ShinyArmorRegistry::reload(&config.shiny_armor_entries);
    let state = shiny_state();
    let mut delay = 0;
    let mut count = 0;
    // Self player in first person: no sparkles, delay still counts down.
    let mut d = 5;
    let mut rng = JavaRandom::new(5);
    let mut first_person = shiny_state();
    first_person.is_self_player = true;
    first_person.camera_first_person = true;
    tick(&first_person, true, &config, &armor, &|_| false, &mut rng, &mut d, &mut |_, _, _, _| count += 1);
    assert_eq!(count, 0);
    assert_eq!(d, 4);
    // Disabled config: no sparkles even third person.
    let mut off = config.clone();
    off.shiny_armor_enabled = false;
    tick(&state, true, &off, &armor, &|_| false, &mut rng, &mut delay, &mut |_, _, _, _| count += 1);
    assert_eq!(count, 0);
    // Third person other entity: spawns.
    let mut hits = 0;
    for _ in 0..500 {
        tick(&state, true, &config, &armor, &|_| false, &mut rng, &mut delay, &mut |_, _, _, _| hits += 1);
    }
    assert!(hits > 0);
}

#[test]
fn block_rules_cover_soul_and_shiny_sparkles() {
    let config = VisualityConfig::default();
    let pos = BlockPos { x: 5, y: 70, z: -3 };
    // Soul: needs air above.
    let mut rng = JavaRandom::new(11);
    let mut soul_hits = 0;
    let mut non_soul = 0;
    for _ in 0..2000 {
        block_random_display_tick(&config, &mut rng, true, false, pos, false, |_| true, |p, _, _, _| {
            if p == particles::SOUL { soul_hits += 1 } else { non_soul += 1 }
        });
    }
    assert_eq!(soul_hits, 0);
    let mut soul_with_air = 0;
    for _ in 0..4000 {
        block_random_display_tick(&config, &mut rng, true, true, pos, false, |_| true, |p, x, y, z| {
            if p == particles::SOUL {
                soul_with_air += 1;
                assert!((y - 71.1).abs() < 1e-9);
                assert!(x >= 5.0 && x < 6.0);
                assert!(z >= -3.0 && z < -2.0);
            }
        });
    }
    assert!(soul_with_air > 0 && non_soul == 0);
    // Shiny blocks: at least one fixed axis per sparkle, randoms inside the cell.
    let mut sparkles: Vec<(f64, f64, f64)> = Vec::new();
    for _ in 0..4000 {
        block_random_display_tick(&config, &mut rng, false, false, pos, true, |_| false, |p, x, y, z| {
            if p == particles::SPARKLE {
                sparkles.push((x - pos.x as f64, y - pos.y as f64, z - pos.z as f64));
            }
        });
    }
    assert!(!sparkles.is_empty());
    for &(x, y, z) in &sparkles {
        let fixed = |v: f64| (v - -0.0625).abs() < 1e-9 || (v - 1.0625).abs() < 1e-9;
        assert!(fixed(x) || fixed(y) || fixed(z), "no fixed axis on ({x},{y},{z})");
        for v in [x, y, z] {
            if !fixed(v) {
                assert!(v >= 0.0 && v < 1.0);
            }
        }
    }
}

#[test]
fn amethyst_needs_big_clusters_and_creeper_needs_power() {
    let config = VisualityConfig::default();
    let pos = BlockPos { x: 0, y: 40, z: 0 };
    let mut rng = JavaRandom::new(13);
    let mut count = 0;
    for _ in 0..500 {
        amethyst_cluster_animate_tick(&config, &mut rng, true, 4.9, pos, |_, _, _, _| count += 1);
    }
    assert_eq!(count, 0);
    let mut big = 0;
    for _ in 0..500 {
        amethyst_cluster_animate_tick(&config, &mut rng, true, 5.1, pos, |p, x, y, z| {
            if p == particles::SPARKLE {
                big += 1;
                assert!(x >= 0.0 && x < 1.0 && y >= 40.0 && y < 41.0 && z >= 0.0 && z < 1.0);
            }
        });
    }
    assert!(big > 0);
    let mut rng = JavaRandom::new(17);
    let mut powered_off = 0;
    for _ in 0..500 {
        creeper_tick(&config, &mut rng, true, true, false, 0.0, 0.0, 0.0, |_, _, _, _| powered_off += 1);
    }
    assert_eq!(powered_off, 0);
    let mut charge = 0;
    for _ in 0..2000 {
        creeper_tick(&config, &mut rng, true, true, true, 0.0, 0.0, 0.0, |p, x, y, z| {
            if p == particles::CHARGE {
                charge += 1;
                assert!(x > -1.0 && x < 1.0);
                assert!(y >= 1.0 && y < 2.0);
                assert!(z > -1.0 && z < 1.0);
            }
        });
    }
    assert!(charge > 0);
    let mut off = config.clone();
    off.charge_enabled = false;
    let mut none = 0;
    for _ in 0..500 {
        creeper_tick(&off, &mut rng, true, true, true, 0.0, 0.0, 0.0, |_, _, _, _| none += 1);
    }
    assert_eq!(none, 0);
}

#[test]
fn sprint_soul_gates_and_positions() {
    let config = VisualityConfig::default();
    let mut rng = JavaRandom::new(19);
    let mut hits = 0;
    for _ in 0..2000 {
        sprint_soul(&config, &mut rng, true, 1.0, 2.0, 3.0, |p, x, y, z| {
            if p == particles::SOUL {
                hits += 1;
                assert!((x - 1.0).abs() < 1e-9);
                assert!((y - 2.1).abs() < 1e-9);
                assert!((z - 3.0).abs() < 1e-9);
            }
        });
    }
    assert!(hits > 0);
    let mut off = config.clone();
    off.soul_enabled = false;
    let mut none = 0;
    for _ in 0..500 {
        sprint_soul(&off, &mut rng, true, 1.0, 2.0, 3.0, |_, _, _, _| none += 1);
    }
    assert_eq!(none, 0);
}

#[test]
fn rainbow_constants_and_pick() {
    assert_eq!(crate::colors::RAINBOW, [16735855, 16751185, 16318324, 7077755, 8245247, 16750831]);
    let mut rng = JavaRandom::new(23);
    for _ in 0..50 {
        assert!(crate::colors::RAINBOW.contains(&crate::colors::get_random_color_of_rainbow(&mut rng)));
    }
}

#[test]
fn particle_ids_are_complete() {
    assert_eq!(particles::ALL.len(), 13);
    for id in particles::ALL {
        assert!(id.starts_with("visuality:"));
    }
}

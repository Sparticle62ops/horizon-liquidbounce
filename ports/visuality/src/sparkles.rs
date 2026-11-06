//! LivingEntityMixin: hit particles on hurt and shiny-armor sparkles on tick.

use crate::config::VisualityConfig;
use crate::ids::ResourceId;
use crate::particles;
use crate::registries::{HitEntry, ShinyArmorRegistry};
use crate::rng::JavaRandom;

pub const SLOT_FEET: usize = 0;
pub const SLOT_LEGS: usize = 1;
pub const SLOT_CHEST: usize = 2;
pub const SLOT_HEAD: usize = 3;

#[derive(Clone, Debug, Default)]
pub struct LivingSnapshot {
    pub entity_type: Option<ResourceId>,
    pub is_chicken: bool,
    pub chicken_variant_cold: bool,
    pub chicken_variant_warm: bool,
    pub is_baby_zombie: bool,
    pub is_alive: bool,
    pub client_side: bool,
    pub is_self_player: bool,
    pub camera_first_person: bool,
    pub sneaking: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub bb_height: f32,
    pub armor: [Option<ResourceId>; 4],
}

#[derive(Clone, Debug)]
pub struct Attacker {
    pub is_living: bool,
    /// Some(amount) only when the main-hand stack has the attribute-modifiers
    /// component and it carries an ATTACK_DAMAGE modifier.
    pub main_hand_attack_damage: Option<f64>,
}

/// LivingEntityMixin.hurtClient, gate order as upstream. ticksDelay latches 10
/// ticks on the first matching entry.
pub fn hurt_client(
    state: &LivingSnapshot,
    attacker: Option<&Attacker>,
    hit_entries: &[HitEntry],
    config: &VisualityConfig,
    rng: &mut JavaRandom,
    ticks_delay: &mut i32,
    mut spawn: impl FnMut(&str, f64, f64, f64),
) {
    let Some(attacker) = attacker.filter(|a| a.is_living) else {
        return;
    };
    if !state.client_side || *ticks_delay != 0 || !state.is_alive || !config.hit_particles_enabled {
        return;
    }
    for entry in hit_entries {
        if state.entity_type.as_ref() == Some(&entry.entity) {
            *ticks_delay = 10;
            let mut count: i32 = rng.next_int_bounded(3);
            if let Some(amount) = attacker.main_hand_attack_damage {
                // Java (int)((float)amount * 0.5F): float math then truncate.
                count = ((amount as f32) * 0.5) as i32;
            }
            let mut particle = entry.particle.clone();
            if state.is_chicken && particle == ResourceId::parse(particles::FEATHER).unwrap() {
                if state.chicken_variant_cold {
                    particle = ResourceId::parse(particles::COLD_FEATHER).unwrap();
                } else if state.chicken_variant_warm {
                    particle = ResourceId::parse(particles::WARM_FEATHER).unwrap();
                }
            }
            spawn_hit_particles(state, &particle, count, rng, &mut spawn);
        }
    }
}

/// LivingEntityMixin.spawnHitParticles.
pub fn spawn_hit_particles(
    state: &LivingSnapshot,
    particle: &ResourceId,
    count: i32,
    rng: &mut JavaRandom,
    spawn: &mut dyn FnMut(&str, f64, f64, f64),
) {
    let mut height = state.bb_height;
    if height * 100.0 < 100.0 {
        height = 1.0;
    } else {
        height += 0.5;
    }
    for _i in 0..=count.min(10) {
        let bound = (height as i32) * 10;
        if bound <= 0 {
            break; // nextInt(0) would throw upstream; height >= 1.0 keeps this dead
        }
        let random_height = rng.next_int_bounded(bound as u32) as f64 / 10.0;
        spawn(&particle.to_string(), state.x, state.y + 0.2 + random_height, state.z);
    }
}

/// LivingEntityMixin.tick: delay countdown plus the sparkle pass.
pub fn tick(
    state: &LivingSnapshot,
    client_player_present: bool,
    config: &VisualityConfig,
    shiny_armor: &ShinyArmorRegistry,
    tag_matches: &dyn Fn(&ResourceId) -> bool,
    rng: &mut JavaRandom,
    ticks_delay: &mut i32,
    spawn: &mut dyn FnMut(&str, f64, f64, f64),
) {
    if state.client_side && *ticks_delay != 0 {
        *ticks_delay -= 1;
    }
    if state.client_side && state.is_alive && client_player_present && config.shiny_armor_enabled {
        if state.is_self_player {
            if !state.camera_first_person {
                spawn_sparkles(state, shiny_armor, tag_matches, rng, spawn);
            }
        } else {
            spawn_sparkles(state, shiny_armor, tag_matches, rng, spawn);
        }
    }
}

/// LivingEntityMixin.spawnSparkles.
pub fn spawn_sparkles(
    state: &LivingSnapshot,
    shiny_armor: &ShinyArmorRegistry,
    tag_matches: &dyn Fn(&ResourceId) -> bool,
    rng: &mut JavaRandom,
    mut spawn: impl FnMut(&str, f64, f64, f64),
) {
    if state.is_baby_zombie {
        return;
    }
    let shiny = |slot: usize| {
        state.armor[slot]
            .as_ref()
            .map(|id| shiny_armor.is_shiny(id, tag_matches))
            .unwrap_or(false)
    };
    if !(shiny(SLOT_FEET) || shiny(SLOT_LEGS) || shiny(SLOT_CHEST) || shiny(SLOT_HEAD)) {
        return;
    }
    let mut heights: Vec<f32> = Vec::new();
    if shiny(SLOT_FEET) {
        heights.push(0.25);
    }
    if shiny(SLOT_LEGS) {
        heights.push(0.65);
    }
    if shiny(SLOT_CHEST) {
        heights.push(1.1);
    }
    let head = shiny(SLOT_HEAD);
    if head {
        if state.sneaking {
            heights.push(1.65);
        } else {
            heights.push(1.85);
        }
    }
    if heights.is_empty() {
        return;
    }
    let height = heights[rng.next_int_bounded(heights.len() as u32) as usize];
    if rng.next_int_bounded((20 - (heights.len() as i32) * 2) as u32) == 0 {
        let rand_x = rng.next_f32() as f64 - 0.5;
        let rand_y = ((rng.next_f32() * 2.0 - 1.0) / 5.0) as f64;
        let rand_z = rng.next_f32() as f64 - 0.5;
        spawn(
            particles::SPARKLE,
            state.x + rand_x,
            state.y + rand_y + height as f64,
            state.z + rand_z,
        );
    }
}

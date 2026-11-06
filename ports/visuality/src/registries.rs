//! HitParticleRegistry / ShinyArmorRegistry / ShinyBlockRegistry logic.
//! Registry lookups stay outside the core: callers pass resolvers, tests stub them.

use crate::ids::ResourceId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitEntry {
    pub entity: ResourceId,
    pub particle: ResourceId,
}

/// HitParticleRegistry.reload(): entry shape "entity_id/particle_id", both sides
/// must resolve. Upstream would throw on an entry without a "/"; that is skipped here.
pub fn reload_hit_particles(
    entries_src: &[String],
    entity_exists: &dyn Fn(&ResourceId) -> bool,
    particle_exists: &dyn Fn(&ResourceId) -> bool,
) -> Vec<HitEntry> {
    let mut out = Vec::new();
    for entry in entries_src {
        if let Some((entity_part, particle_part)) = entry.split_once('/') {
            if let (Some(entity), Some(particle)) =
                (ResourceId::parse(entity_part), ResourceId::parse(particle_part))
            {
                if entity_exists(&entity) && particle_exists(&particle) {
                    out.push(HitEntry { entity, particle });
                }
            }
        }
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShinyLists {
    pub items: Vec<ResourceId>,
    pub tags: Vec<ResourceId>,
}

fn reload_shiny(entries_src: &[String]) -> ShinyLists {
    let mut lists = ShinyLists::default();
    for entry in entries_src {
        if let Some(tag) = entry.strip_prefix('#') {
            if let Some(id) = ResourceId::parse(tag) {
                lists.tags.push(id);
            }
        } else if let Some(id) = ResourceId::parse(entry) {
            lists.items.push(id);
        }
    }
    lists
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShinyArmorRegistry(pub ShinyLists);

impl ShinyArmorRegistry {
    pub fn reload(entries_src: &[String]) -> Self {
        ShinyArmorRegistry(reload_shiny(entries_src))
    }
    pub fn is_shiny(&self, item: &ResourceId, tag_matches: &dyn Fn(&ResourceId) -> bool) -> bool {
        self.0.items.contains(item) || self.0.tags.iter().any(|t| tag_matches(t))
    }
}

/// ShinyBlockRegistry.java is the same parse + match over blocks/tags.
pub type ShinyBlockRegistry = ShinyArmorRegistry;

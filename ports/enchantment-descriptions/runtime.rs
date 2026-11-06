// Enchantment Descriptions: a short description right under each enchantment
// on the hovered item's tooltip.
//
// Port of Darkhax-Minecraft/Enchantment-Descriptions (LGPL-2.1-only). Every
// enchantment on the stack, the held ones and an enchanted book's stored
// ones, finds the tooltip line carrying its name and the description goes
// right after it, dark gray. Descriptions ship as a compiled-in vanilla
// table taken from the original's en_us lang file; upstream reads them from
// lang keys, which a resource pack could change. The original's config
// screen and display toggles (enchanting-table-only, books-only,
// require-keybind, per-entry indent) are degradations (see README), with the
// compiled-in defaults matching upstream's: shown wherever the tooltip
// shows, no keybind, no indent.

use crate::mod_api::ItemStack;

const DESCRIPTIONS: &[(&str, &str)] = &[
("minecraft:aqua_affinity", "Increases mining speed while underwater."),
    ("minecraft:bane_of_arthropods", "Increases damage against arthropods such as Spiders and Silverfish."),
    ("minecraft:binding_curse", "Prevents the cursed item from being removed from an armor slot."),
    ("minecraft:blast_protection", "Reduces damage from explosions and explosion knockback."),
    ("minecraft:channeling", "Allows the trident to summon lightning bolts during thunderstorms."),
    ("minecraft:depth_strider", "Increases movement speed while underwater."),
    ("minecraft:efficiency", "Increases mining speed of the tool."),
    ("minecraft:feather_falling", "Reduces fall damage and ender pearl teleportation damage."),
    ("minecraft:fire_aspect", "Causes additional fire damage when used to attack a mob."),
    ("minecraft:fire_protection", "Reduces the effects of fire damage. Also reduces the burn time when set on fire."),
    ("minecraft:flame", "Arrows fired from the bow will deal additional fire damage."),
    ("minecraft:fortune", "Some blocks like coal and diamond ore may drop additional items."),
    ("minecraft:frost_walker", "Water under the user will freeze into frosted ice."),
    ("minecraft:impaling", "Increases damage to aquatic mobs."),
    ("minecraft:infinity", "Allows the bow to fire normal arrows for free. You must have at least one arrow for this to work."),
    ("minecraft:knockback", "Increases the knockback strength of the weapon."),
    ("minecraft:looting", "Mobs will drop more loot when killed."),
    ("minecraft:loyalty", "Allows the trident to automatically return after being thrown."),
    ("minecraft:luck_of_the_sea", "Increases the chance of getting good loot while fishing."),
    ("minecraft:lure", "Decreases the amount of time it takes for a fish to bite the hook."),
    ("minecraft:mending", "Repairs the durability of armor and tools with XP."),
    ("minecraft:multishot", "Fires additional arrows in similar directions."),
    ("minecraft:piercing", "Allows projectiles to pierce through mobs."),
    ("minecraft:power", "Increases the damage of arrows fired from the bow."),
    ("minecraft:projectile_protection", "Reduces damage from projectiles such as arrows and fire balls."),
    ("minecraft:protection", "Reduces damage from most sources."),
    ("minecraft:punch", "Increases the knockback strength of arrows fired by the bow."),
    ("minecraft:quick_charge", "Increases the reload speed of crossbows."),
    ("minecraft:respiration", "Allows the user to breathe longer underwater."),
    ("minecraft:riptide", "Using the trident while in rain or water will launch the user forward."),
    ("minecraft:sharpness", "Increases the damage of the item."),
    ("minecraft:silk_touch", "Allows fragile blocks such as glass to be collected."),
    ("minecraft:smite", "Increases damage against undead mobs such as Zombies and Skeletons."),
    ("minecraft:soul_speed", "Increases movement speed on soul blocks."),
    ("minecraft:sweeping", "Increases the damage of sweeping attacks."),
    ("minecraft:swift_sneak", "Increases movement speed while sneaking."),
    ("minecraft:thorns", "Causes damage to enemies when they attack you."),
    ("minecraft:unbreaking", "Causes the tool to lose durability at a slower rate."),
    ("minecraft:vanishing_curse", "Destroys the cursed item if you die with it in your inventory."),
];

/// The hovered stack's enchantments: the held ones and an enchanted book's
/// stored ones, in the stack's own order.
fn enchantment_ids(stack: &ItemStack) -> Vec<&str> {
    let mut ids: Vec<&str> = Vec::new();
    ids.extend(stack.enchantments().iter().map(|(e, _)| &**e));
    if let Some(data) = stack.data() {
        ids.extend(data.stored_enchantments.iter().map(|(e, _)| &**e));
    }
    ids
}

/// The enchantment's vanilla lang key: minecraft:sharpness turns into
/// enchantment.minecraft.sharpness.
fn lang_key(id: &str) -> Option<String> {
    let (ns, path) = id.split_once(':')?;
    Some(format!("enchantment.{ns}.{path}"))
}

/// Whether a tooltip line's text is this enchantment's name, with an
/// optional level suffix such as " V".
fn is_name_line(plain: &str, name: &str) -> bool {
    plain == name
        || (plain.len() > name.len()
            && plain.starts_with(name)
            && plain.as_bytes()[name.len()] == b' ')
}

impl crate::game::Game<'_> {
    pub fn enchantment_descriptions_apply(
        &self,
        stack: &ItemStack,
        lines: &mut Vec<(crate::text::Text, u32)>,
    ) {
        let ids = enchantment_ids(stack);
        if ids.is_empty() {
            return;
        }
        let mut inserts: Vec<(usize, &'static str)> = Vec::new();
        for id in ids {
            let Some((_, desc)) = DESCRIPTIONS.iter().find(|(key, _)| *key == id) else {
                continue;
            };
            let Some(key) = lang_key(id) else {
                continue;
            };
            // The name lookup goes through the same lang the name line was
            // built with, so the match stays vanilla's match; a name the
            // lang does not know gets no insertion, never a wrong one.
            let Some(name) = self.stations().enchantments.translate(&key) else {
                continue;
            };
            for (index, (line, _)) in lines.iter().enumerate() {
                if is_name_line(&line.plain(), name) {
                    inserts.push((index + 1, desc));
                    break;
                }
            }
        }
        // Insert back to front so the earlier positions stay put.
        inserts.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        for (pos, desc) in inserts {
            lines.insert(
                pos,
                (
                    crate::text::Text::literal(desc),
                    // Upstream's dark gray, ChatFormatting.DARK_GRAY.
                    crate::hud::rgba(85, 85, 85, 255),
                ),
            );
        }
    }
}

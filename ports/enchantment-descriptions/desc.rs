fn enchantment_descriptions_on_tooltip(
    game: &crate::game::Game<'_>,
    stack: &crate::mod_api::ItemStack,
    lines: &mut Vec<(crate::text::Text, u32)>,
) {
    game.enchantment_descriptions_apply(stack, lines);
}

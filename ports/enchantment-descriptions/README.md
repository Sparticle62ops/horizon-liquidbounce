# Enchantment Descriptions (Horizon port)

A short description right under each enchantment on the hovered item's
tooltip, dark gray, including an enchanted book's stored enchantments. Port
of [Enchantment-Descriptions](https://modrinth.com/mod/enchantment-descriptions)
by Darkhax (LGPL-2.1-only, 39.7M downloads); the vanilla descriptions come
from the original's own en_us lang file.

Degradations from the original:

- The config screen and its toggles (enchanting-table-only, books-only,
  require-keybind, per-entry indent) are not ported; the compiled-in
  defaults match upstream's defaults (shown wherever the tooltip shows, no
  keybind, no indent).
- Upstream reads descriptions from lang keys so resource packs and other
  mods can add or change them; the port ships the vanilla set compiled in,
  so modded enchantments get no description.
- Lines are matched by rendered name text (Horizon's tooltip lines carry
  plain text, not translation keys), so an item custom-named to an
  enchantment's name could gain a stray description line.

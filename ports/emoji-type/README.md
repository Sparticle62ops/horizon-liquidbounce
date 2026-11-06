# emoji_type

Typing a `:code:` shortcode in Horizon chat replaces it with its emoji when the
message is sent. A Rust port of [EmojiType](https://modrinth.com/mod/emojitype)
by Norbiros, licensed GPL-3.0 like the original.

The three emoji packs bundled with the original mod (minecraft, text art faces,
letters) ship inside this mod and are copied into the mod's data folder
(`.horizon-mod-data/emoji_type/emoji_packs/`) on first load. Every `.yaml` or
`.yml` pack in that folder is loaded alongside them, so custom packs can be
dropped in the same way the original loads them from its config.

Phase 1 covers the send-time replacement and pack loading. The original also
replaces codes in the edit box as they are typed and shows code suggestions;
that in-field stage, plus the pack config UI, lands as a later phase on the
screen hooks.

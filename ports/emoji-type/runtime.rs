// EmojiType for Horizon: typing a `:code:` shortcode in chat replaces it with
// its emoji when the message is sent. A Rust port of EmojiType by Norbiros
// (github.com/Norbiros/EmojiType, GPL-3.0). The original replaces codes in the
// edit box as they are typed; this phase replaces them on send, which the
// 1:1 in-field replacement stage builds on. The three emoji packs bundled with
// the original are written into the mod's data folder on first load, and every
// *.yaml/*.yml pack in that folder is loaded alongside them.
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::game::Game;

#[derive(Clone)]
struct EmojiCode {
    code: String,
    emoji: String,
}

const PACK_MINECRAFT: &str =
    r#"name: Minecraft Emojis
description: Various popular Minecraft emojis and symbols
type: bundled
emojis:
  # Tools
  bucket: "🪣"
  shears: "✂"
  flag: "⚑"
  bell: "🔔"
  hunger: "🍖"

  # Face
  happy: "☺"
  happy_filled: "☻"
  sad: "☹"
  skull: "☠"

  # Tool
  pickaxe: "⛏"
  axe: "🪓"
  fishing: "🎣"
  umbrella: "☂"

  # Combat
  trident: "🔱"
  sword: "🗡"
  shield: "🛡"
  swords: "⚔"
  bow: "🏹"

  # Potion
  potion: "🧪"
  splash: "⚗"

  # Mark
  copyright: "©"
  registered: "®"
  protected: "℗"
  trademark: "™"

  # Globe
  globe: "🌍"
  globe2: "🌎"
  globe3: "🌏"

  # Water
  anchor: "⚓"
  wave: "🌊"

  # Sex
  male: "♂"
  female: "♀"
  intersex: "⚥"

  # Direction
  left: "⏴"
  right: "⏵"
  up: "⏶"
  down: "⏷"
  hand_left: "☜"
  hand_right: "☞"
  swap: "⇄"

  # Culture
  ying_yang: "☯"
  peace: "☮"

  # Weather
  sun: "☀"
  cloud: "☁"
  comet: "☄"
  moon: "☽"
  snowman: "⛄"
  snowman_snow: "☃"
  storm: "⛈"
  snowflake: "❄"
  lightning: "⚡"

  # Control
  eject: "⏏"
  fast_forward: "⏩"
  fast_backward: "⏪"
  to_end: "⏭"
  to_start: "⏮"
  play_pause: "⏯"
  pause: "⏸"
  power_on: "⏻"
  power_off: "⏼"

  # Flag
  black_flag: "⚐"

  # Boolean
  box: "☐"
  box_yes: "☑"
  box_no: "☒"
  yes: "✔"
  no: "✘"

  # Shape
  square_solid: "⏹"
  square: "□"
  circle_solid: "⏺"
  circle: "○"
  cross: "❌"
  heart: "❤"
  heart_exclamation: "❣"
  triangle_solid: "▲"
  triangle: "△"
  diamond_solid: "◆"
  diamond: "◇"
  looped_square: "⌘"
  target: "◎"
  star1: "★"
  star2: "☆"
  star3: "⭐"

  # Misc
  fire: "🔥"
  unknown: "�"
  house: "⌂"
  clock: "⌚"
  warning: "⚠"
  envelope: "✉"
  pencil: "✎"
  degree: "°"
  hourglass: "⌛"
  hourglass_full: "⏳"

  # Cards
  suit_spade_solid: "♠"
  suit_spade: "♤"
  suit_heart_solid: "♥"
  suit_heart: "♡"
  suit_club_solid: "♣"
  suit_club: "♧"
  suit_diamond_solid: "♦"
  suit_diamond2: "♢"

  # Music
  note_crotchet: "♩"
  note_quaver: "♪"
  note_quavers: "♫"
  note_semiquavers: "♬"
  note_flat: "♭"
  note_neutral: "♮"

  # Dice
  dice1: "⚀"
  dice2: "⚁"
  dice3: "⚂"
  dice4: "⚃"
  dice5: "⚄"
  dice6: "⚅"

  # Shading
  shade0: "▀"
  shade1: "▄"
  shade2: "█"
  shade3: "▌"
  shade4: "▐"
  shade5: "░"
  shade6: "▒"
  shade7: "▓""#;

const PACK_TEXT_ART_FACES: &str =
    r#"name: Text Art Faces
description: Collection of expressive ASCII-style text emoticons
type: bundled
emojis:
  face_surprised: "(°_°)"
  face_skeptical: "(ㆆ_ㆆ)"
  face_wide_eyed: "(O_O)"
  face_disapproving: "(ಠ_ಠ)"
  face_cute_smile: "(｡◕‿‿◕｡)"
  face_sly_smile: "(-‿-)"
  face_happy: "(◠‿◠)"
  face_flower_smile: "(✿◠‿◠)"
  face_confused: "(°o•)"
  bear_neutral: "ʕ·ᴥ·ʔ"
  bear_hug: "ʕっ·ᴥ·ʔっ"
  bear_love: "ʕ♥ᴥ♥ʔ"
  bird_face: "(°v°)"
  cat_face: "(°ᴥ°)"
  dog_face: "(◕ᴥ◕ʋ)"
  face_cheer1: "※\\(^o^)/※"
  face_cheer2: "*(^_^)*"
  face_cool1: "(⌐■_■)"
  face_cool2: "(•_•) ( •_•)>⌐■-■ (⌐■_■)"
  face_creep: "ԅ(≖‿≖ԅ)"
  face_cry: "(╥﹏╥)"
  face_dance1: "ᕕ(⌐■_■)ᕗ"
  face_dance2: "ᕕ( ᐛ ) ᕗ"
  face_table1: "(╯°□°)╯︵ ┻━┻"
  face_table2: "┳━┳ノ(°_°ノ)"
  face_table_minecraft: "(╯°□°)╯︵ ʇɟɐɹɔǝuᴉɯ"
  face_table_mojang: "(╯°□°)╯︵ ƃuɐɾoɯ"
  face_shrug: "¯\\_(ツ)_/¯"
  face_shrug_confused: "¯\\(°_o)/¯"
  face_gimme: "༼ つ◕_◕༽つ"
  face_lol: "L(°O°L)"
  face_zoidberg: "(V)(°,,,°)(V)"
  face_fly: "─=≡Σ((つ•ω•)つ""#;

const PACK_LETTERS: &str =
    r#"name: Letter Emojis
description: Enclosed alphabet letters as emoji codes
type: bundled
emojis:
  za: "ⓐ"
  zb: "ⓑ"
  zc: "ⓒ"
  zd: "ⓓ"
  ze: "ⓔ"
  zf: "ⓕ"
  zg: "ⓖ"
  zh: "ⓗ"
  zi: "ⓘ"
  zj: "ⓙ"
  zk: "ⓚ"
  zl: "ⓛ"
  zm: "ⓜ"
  zn: "ⓝ"
  zo: "ⓞ"
  zp: "ⓟ"
  zq: "ⓠ"
  zr: "ⓡ"
  zs: "ⓢ"
  zt: "ⓣ"
  zu: "ⓤ"
  zv: "ⓥ"
  zw: "ⓦ"
  zx: "ⓧ"
  zy: "ⓨ"
  zz: "ⓩ"
  za_: "Ⓐ"
  zb_: "Ⓑ"
  zc_: "Ⓒ"
  zd_: "Ⓓ"
  ze_: "Ⓔ"
  zf_: "Ⓕ"
  zg_: "Ⓖ"
  zh_: "Ⓗ"
  zi_: "Ⓘ"
  zj_: "Ⓙ"
  zk_: "Ⓚ"
  zl_: "Ⓛ"
  zm_: "Ⓜ"
  zn_: "Ⓝ"
  zo_: "Ⓞ"
  zp_: "Ⓟ"
  zq_: "Ⓠ"
  zr_: "Ⓡ"
  zs_: "Ⓢ"
  zt_: "Ⓣ"
  zu_: "Ⓤ"
  zv_: "Ⓥ"
  zw_: "Ⓦ"
  zx_: "Ⓧ"
  zy_: "Ⓨ"
  zz_: "Ⓩ""#;


fn default_packs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("minecraft.yaml", PACK_MINECRAFT),
        ("text_art_faces.yaml", PACK_TEXT_ART_FACES),
        ("letters.yaml", PACK_LETTERS),
    ]
}

/// Strips one layer of matching single or double quotes from a YAML value.
fn strip_quotes(value: &mut String) {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0];
        let last = bytes[value.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            *value = value[1..value.len() - 1].to_string();
        }
    }
}

/// Parses one pack's `emojis:` mapping: `code: emoji` pairs, comments and
/// blank lines ignored. Reads leniently like the original's YAML load.
fn parse_pack(text: &str) -> Vec<EmojiCode> {
    let mut codes = Vec::new();
    let mut in_emojis = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !in_emojis {
            if line == "emojis:" {
                in_emojis = true;
            }
            continue;
        }
        if line.starts_with("- ") {
            continue;
        }
        let Some((raw_code, raw_emoji)) = line.split_once(':') else {
            continue;
        };
        let mut code = raw_code.trim().to_string();
        let mut emoji = raw_emoji.trim().to_string();
        strip_quotes(&mut code);
        strip_quotes(&mut emoji);
        if code.is_empty() || emoji.is_empty() || code.contains(' ') {
            continue;
        }
        codes.push(EmojiCode { code, emoji });
    }
    codes
}

fn cache() -> &'static RwLock<Option<Vec<EmojiCode>>> {
    static CACHE: OnceLock<RwLock<Option<Vec<EmojiCode>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(None))
}

fn ensure_loaded(game: &Game) -> Vec<EmojiCode> {
    if let Ok(loaded) = cache().read() {
        if let Some(codes) = loaded.as_ref() {
            return codes.clone();
        }
    }
    let codes = load_packs(game);
    if let Ok(mut slot) = cache().write() {
        *slot = Some(codes.clone());
    }
    codes
}

fn load_packs(game: &Game) -> Vec<EmojiCode> {
    let dir = game.native_data_dir("emoji_type").join("emoji_packs");
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
        for (name, body) in default_packs() {
            let path = dir.join(name);
            if !path.exists() {
                let _ = std::fs::write(&path, body);
            }
        }
    }
    let mut codes = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return codes;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .map_or(false, |ext| ext == "yaml" || ext == "yml")
        })
        .collect();
    files.sort();
    for path in files {
        if let Ok(text) = std::fs::read_to_string(&path) {
            codes.extend(parse_pack(&text));
        }
    }
    codes
}

/// Replaces every `:code:` in the message with its emoji. The original
/// replaces each code as it is typed, so the sent message is the same either
/// way once the in-field stage lands.
pub(crate) fn transform(game: &Game, message: &str) -> String {
    let codes = ensure_loaded(game);
    if codes.is_empty() {
        return message.to_string();
    }
    let mut out = message.to_string();
    for code in &codes {
        let pattern = format!(":{}:", code.code);
        if out.contains(&pattern) {
            out = out.replace(&pattern, &code.emoji);
        }
    }
    out
}

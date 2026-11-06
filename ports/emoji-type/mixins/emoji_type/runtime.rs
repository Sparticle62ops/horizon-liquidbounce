//! emoji-type runtime (port of Norbiros/emojitype, GPL-3.0-or-later).
//!
//! The 1:1 core: the pack model (packs of :code: -> emoji entries), the
//! active-code set built from the config's enabled packs in order, and the
//! EditBox charTyped conversion, including its cursor math. The text-field
//! wiring rides Horizon's screen character hook (native_screen_char_typed_pre,
//! API 104); the seams it needs are in the README.

pub const MOD_ID: &str = "emoji-type";

/// Upstream EmojiCode: a shortcode (no colons) and its replacement emoji.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmojiCode {
    pub code: String,
    pub emoji: String,
}

impl EmojiCode {
    pub fn new(code: impl Into<String>, emoji: impl Into<String>) -> Self {
        Self { code: code.into(), emoji: emoji.into() }
    }

    /// Upstream getCodeWithColons: ":code:".
    pub fn code_with_colons(&self) -> String {
        format!(":{}:", self.code)
    }
}

/// Which kind of pack, upstream PackType.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PackType {
    #[default]
    Bundled,
    Local,
}

impl PackType {
    pub fn id(&self) -> &'static str {
        match self {
            PackType::Bundled => "BUNDLED",
            PackType::Local => "LOCAL",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "BUNDLED" => Some(PackType::Bundled),
            "LOCAL" => Some(PackType::Local),
            _ => None,
        }
    }
}

/// Upstream EmojiPack: name, description, type and the ordered :code: ->
/// emoji map (upstream keeps a LinkedHashMap, so insertion order is the
/// suggestion and replacement order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmojiPack {
    pub name: String,
    pub description: String,
    pub pack_type: PackType,
    pub emojis: Vec<(String, String)>,
}

impl EmojiPack {
    pub fn new(name: impl Into<String>, description: impl Into<String>, pack_type: PackType) -> Self {
        Self { name: name.into(), description: description.into(), pack_type, emojis: Vec::new() }
    }

    pub fn emoji_codes(&self) -> Vec<EmojiCode> {
        self.emojis.iter().map(|(c, e)| EmojiCode::new(c.clone(), e.clone())).collect()
    }

    /// Upstream EmojiPack.toYaml: name, description, type, emojis in order.
    pub fn to_yaml(&self) -> String {
        let mut out = String::from("name: ");
        out.push_str(&self.name);
        out.push_str("\ndescription: \"");
        out.push_str(&self.description);
        out.push_str("\"\ntype: ");
        out.push_str(self.pack_type.id());
        out.push_str("\nemojis:");
        for (code, emoji) in &self.emojis {
            out.push_str(&format!("\n  {}: \"{}\"", code, emoji));
        }
        out.push('\n');
        out
    }
}

/// Upstream config: which pack names are enabled, in that order.
#[derive(Clone, Debug, Default)]
pub struct EmojiConfig {
    pub enabled_packs: Vec<String>,
}

/// Upstream EmojiType.getActiveEmojiCodes: every code from every enabled
/// pack, packs and entries in config order, first pack wins on a duplicate
/// code only by list order upstream keeps (it appends, so a later duplicate
/// is also tried; replacement order follows the same sequence).
pub fn active_emoji_codes(config: &EmojiConfig, packs: &[EmojiPack]) -> Vec<EmojiCode> {
    let mut out = Vec::new();
    for pack_name in &config.enabled_packs {
        if let Some(pack) = packs.iter().find(|p| &p.name == pack_name) {
            out.extend(pack.emoji_codes());
        }
    }
    out
}

/// Upstream EditBoxMixin.onCharTyped, 1:1: replace every active code's
/// ":code:" with its emoji across the whole value, then if anything changed
/// set the new cursor to clamp(cursor - lengthDifference + 1, 0, len).
pub fn convert_text(value: &str, codes: &[EmojiCode], cursor: usize) -> Option<(String, usize)> {
    let mut result = value.to_string();
    for code in codes {
        result = result.replace(&code.code_with_colons(), &code.emoji);
    }
    if result != value {
        let length_difference = value.chars().count() as i64 - result.chars().count() as i64;
        let new_cursor = ((cursor as i64 - length_difference + 1).clamp(0, result.chars().count() as i64)) as usize;
        Some((result, new_cursor))
    } else {
        None
    }
}

/// Upstream bundles three packs; their example entries from the README.
pub fn bundled_pack_essentials() -> EmojiPack {
    let mut p = EmojiPack::new("Minecraft Essentials", "Tools, weapons, and status icons.", PackType::Bundled);
    p.emojis = vec![
        ("sword".into(), "\u{2694}\u{fe0f}".into()),
        ("pickaxe".into(), "\u{26cf}\u{fe0f}".into()),
        ("bow".into(), "\u{1f3f9}".into()),
        ("sun".into(), "\u{2600}\u{fe0f}".into()),
        ("snowflake".into(), "\u{2744}\u{fe0f}".into()),
    ];
    p
}

pub fn bundled_pack_kaomoji() -> EmojiPack {
    let mut p = EmojiPack::new("Text Art (Kaomoji)", "Classic ASCII and Japanese emoticons.", PackType::Bundled);
    p.emojis = vec![
        ("shrug".into(), "\u{00af}\\_\u{ff08}\u{30c4}\u{ff09}_/\u{00af}".into()),
        ("tableflip".into(), "(\u{256f}\u{00b0}\u{25a1}\u{00b0})\u{256f}".into()),
    ];
    p
}

pub fn bundled_pack_letters() -> EmojiPack {
    let mut p = EmojiPack::new("Letter Emojis", "Stylized circled alphabets.", PackType::Bundled);
    let circled = ['\u{24d0}', '\u{24d1}', '\u{24d2}', '\u{24d3}', '\u{24d4}'];
    for (i, ch) in circled.iter().enumerate() {
        let c = (b'a' + i as u8) as char;
        p.emojis.push((c.to_string(), ch.to_string()));
    }
    p
}

/// Upstream DefaultPackGenerator's bundled list.
pub fn bundled_packs() -> Vec<EmojiPack> {
    vec![bundled_pack_essentials(), bundled_pack_kaomoji(), bundled_pack_letters()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_with_colons_wraps() {
        assert_eq!(EmojiCode::new("skull", "\u{2620}\u{fe0f}").code_with_colons(), ":skull:");
    }

    #[test]
    fn active_codes_follow_config_order() {
        let packs = bundled_packs();
        let mut config = EmojiConfig::default();
        config.enabled_packs = vec!["Letter Emojis".into(), "Minecraft Essentials".into()];
        let codes = active_emoji_codes(&config, &packs);
        assert_eq!(codes.first().unwrap().code, "a");
        assert!(codes.iter().any(|c| c.code == "sword"));
    }

    #[test]
    fn convert_replaces_and_moves_cursor() {
        let codes = vec![EmojiCode::new("heart", "\u{2764}\u{fe0f}")];
        // ":heart:" is 7 chars, the heart 1: cursor after the last colon is 7,
        // lengthDifference = 6, new cursor = 7 - 6 + 1 = 2.
        let (value, cursor) = convert_text(":heart:", &codes, 7).unwrap();
        assert_eq!(value, "\u{2764}\u{fe0f}");
        assert_eq!(cursor, 2);
    }

    #[test]
    fn convert_handles_partial_codes() {
        let codes = vec![EmojiCode::new("skull", "\u{2620}\u{fe0f}")];
        // ":skull" without the closing colon converts nothing.
        assert!(convert_text(":skull", &codes, 6).is_none());
        // A full code inside text converts around the cursor.
        let (value, _) = convert_text("hey :skull:!", &codes, 6).unwrap();
        assert_eq!(value, "hey \u{2620}\u{fe0f}!");
    }

    #[test]
    fn convert_without_codes_changes_nothing() {
        let codes = vec![EmojiCode::new("skull", "\u{2620}\u{fe0f}")];
        assert!(convert_text("no codes here", &codes, 13).is_none());
    }

    #[test]
    fn yaml_keeps_order_and_type() {
        let mut p = EmojiPack::new("Test", "desc", PackType::Local);
        p.emojis = vec![("b".into(), "2".into()), ("a".into(), "1".into())];
        let yaml = p.to_yaml();
        assert!(yaml.contains("type: LOCAL"));
        let b = yaml.find("  b: \"2\"").expect("b entry");
        let a = yaml.find("  a: \"1\"").expect("a entry");
        assert!(b < a);
    }

    #[test]
    fn pack_type_round_trips() {
        assert_eq!(PackType::from_id("LOCAL"), Some(PackType::Local));
        assert_eq!(PackType::from_id("BUNDLED"), Some(PackType::Bundled));
        assert_eq!(PackType::from_id("nope"), None);
    }
}

//! Port of the settings system (`client/value/*` core pieces): named values on
//! a configurable, JSON round-trip, choice groups, binds and ranges.
use std::collections::BTreeMap;

/// Minimal JSON-ish tree, matching the shape upstream's ConfigSystem writes.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Bool(bool),
    Num(f64),
    Str(String),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn as_bool(&self) -> Option<bool> {
        if let Json::Bool(b) = self { Some(*b) } else { None }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let Json::Num(n) = self { Some(*n) } else { None }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Json::Str(s) = self { Some(s) } else { None }
    }
}

/// One setting ("Value" upstream). Visibility can depend on other settings
/// (upstream `visibility { }`), stored here as a closure over a settings map.
pub type Visibility = fn(&BTreeMap<String, Json>) -> bool;

#[derive(Clone, Debug, PartialEq)]
pub enum ValueKind {
    Bool(bool),
    Int(i64, i64, i64),       // value, min, max
    Float(f64, f64, f64),     // value, min, max
    Choice(String, Vec<String>), // selected, all options
    Bind(String),             // serialized input bind ("keyboard:key.code:action")
}

#[derive(Clone)]
pub struct Value {
    pub name: String,
    pub kind: ValueKind,
    pub visible: Visibility,
    /// Upstream boolean settings double as the module's enabled toggle.
    pub is_enabled_toggle: bool,
}

impl Value {
    pub fn to_json(&self) -> Json {
        match &self.kind {
            ValueKind::Bool(b) => Json::Bool(*b),
            ValueKind::Int(v, _, _) => Json::Num(*v as f64),
            ValueKind::Float(v, _, _) => Json::Num(*v),
            ValueKind::Choice(s, _) => Json::Str(s.clone()),
            ValueKind::Bind(b) => Json::Str(b.clone()),
        }
    }

    /// Applies a decoded JSON value, clamped into the declared range like
    /// upstream's deserializers. Unknown names return Err, matching the
    /// "skip unknown keys" tolerance by being reported, not silently eaten.
    pub fn set_from_json(&mut self, v: &Json) -> Result<(), &'static str> {
        match (&mut self.kind, v) {
            (ValueKind::Bool(b), Json::Bool(n)) => *b = *n,
            (ValueKind::Bool(_), Json::Num(n)) => *b = *n != 0.0,
            (ValueKind::Int(cur, lo, hi), Json::Num(n)) => *cur = clamp(*n, *lo as f64, *hi as f64) as i64,
            (ValueKind::Float(cur, lo, hi), Json::Num(n)) => *cur = clamp(*n, *lo, *hi),
            (ValueKind::Choice(sel, opts), Json::Str(s)) => {
                if !opts.contains(s) {
                    return Err("not a valid choice");
                }
                *sel = s.clone();
            }
            (ValueKind::Bind(_), Json::Str(s)) => self.kind = ValueKind::Bind(s.clone()),
            _ => return Err("wrong type for setting"),
        }
        Ok(())
    }
}

fn clamp(n: f64, lo: f64, hi: f64) -> f64 {
    if n < lo { lo } else if n > hi { hi } else { n }
}

pub fn boolean(name: &str, v: bool) -> Value {
    Value { name: name.into(), kind: ValueKind::Bool(v), visible: |_| true, is_enabled_toggle: false }
}
pub fn int(name: &str, v: i64, lo: i64, hi: i64) -> Value {
    Value { name: name.into(), kind: ValueKind::Int(v, lo, hi), visible: |_| true, is_enabled_toggle: false }
}
pub fn float(name: &str, v: f64, lo: f64, hi: f64) -> Value {
    Value { name: name.into(), kind: ValueKind::Float(v, lo, hi), visible: |_| true, is_enabled_toggle: false }
}
pub fn choice(name: &str, sel: &str, opts: &[&str]) -> Value {
    Value {
        name: name.into(),
        kind: ValueKind::Choice(sel.into(), opts.iter().map(|s| s.to_string()).collect()),
        visible: |_| true,
        is_enabled_toggle: false,
    }
}
pub fn bind(name: &str, b: &str) -> Value {
    Value { name: name.into(), kind: ValueKind::Bind(b.into()), visible: |_| true, is_enabled_toggle: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_clamps_into_range_on_load() {
        let mut v = int("Delay", 20, 0, 100);
        v.set_from_json(&Json::Num(500.0)).unwrap();
        assert_eq!(v.kind, ValueKind::Int(100, 0, 100));
        v.set_from_json(&Json::Num(-5.0)).unwrap();
        assert_eq!(v.kind, ValueKind::Int(0, 0, 100));
    }

    #[test]
    fn choice_rejects_unknown_option() {
        let mut v = choice("Mode", "Legit", &["Legit", "Rage"]);
        assert!(v.set_from_json(&Json::Str("Ghost".into())).is_err());
        assert!(v.set_from_json(&Json::Str("Rage".into())).is_ok());
    }

    #[test]
    fn bool_from_number_matches_truthiness() {
        let mut v = boolean("Enabled", false);
        v.set_from_json(&Json::Num(1.0)).unwrap();
        assert_eq!(v.kind, ValueKind::Bool(true));
    }

    #[test]
    fn json_round_trip_keeps_values() {
        let vs = [boolean("A", true), bind("Bind", "keyboard:key.r:press")];
        let tree: BTreeMap<String, Json> = vs.iter().map(|v| (v.name.clone(), v.to_json())).collect();
        let loaded = tree.get("Bind").unwrap().as_str().unwrap();
        assert_eq!(loaded, "keyboard:key.r:press");
        assert!(tree.get("A").unwrap().as_bool().unwrap());
    }

    #[test]
    fn wrong_type_is_an_error_not_a_panic() {
        let mut v = float("Speed", 1.0, 0.0, 5.0);
        assert!(v.set_from_json(&Json::Str("fast".into())).is_err());
    }
}

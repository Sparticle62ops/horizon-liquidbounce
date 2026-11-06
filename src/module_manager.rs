//! Port of `features/module/ModuleManager.kt`: registration, lookup, enable/
//! disable by name, category grouping, and config save/load of every module's
//! settings under its baseKey.
use crate::category::{self, ModuleCategory};
use crate::module::{to_lower_camel_case, Module};
use crate::value::{Json, ValueKind};
use std::collections::BTreeMap;

pub struct ModuleManager {
    pub modules: Vec<Module>,
}

impl ModuleManager {
    pub fn new() -> ModuleManager {
        ModuleManager { modules: Vec::new() }
    }

    pub fn register(&mut self, m: Module) -> Result<(), &'static str> {
        if self.modules.iter().any(|x| x.name == m.name) {
            return Err("module already registered");
        }
        Ok(self.modules.push(m))
    }

    pub fn by_name(&self, name: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.name == name)
    }

    pub fn by_name_mut(&mut self, name: &str) -> Option<&mut Module> {
        self.modules.iter_mut().find(|m| m.name == name)
    }

    /// Upstream `enableAll` / `disableAll` skip notActivatable modules.
    pub fn enable_all(&mut self) {
        for m in self.modules.iter_mut() {
            if !m.not_activatable && !m.disable_activation {
                m.running = true;
            }
        }
    }

    pub fn disable_all(&mut self) {
        for m in self.modules.iter_mut() {
            m.running = false;
        }
    }

    pub fn in_category(&self, c: ModuleCategory) -> Vec<&Module> {
        self.modules.iter().filter(|m| m.category == c).collect()
    }

    /// Config save: each module's settings under `ConfigSystem prefix +
    /// module.<lowerCamelCase name>`, exactly upstream's tree shape.
    pub fn save_config(&self) -> BTreeMap<String, Json> {
        let mut out = BTreeMap::new();
        for m in &self.modules {
            let mut fields: BTreeMap<String, Json> = BTreeMap::new();
            fields.insert("Enabled".into(), Json::Bool(m.running));
            fields.insert("Hidden".into(), Json::Bool(m.hidden));
            fields.insert("Bind".into(), Json::Str(m.bind.clone()));
            for v in &m.settings {
                if v.is_enabled_toggle {
                    continue; // the Enabled toggle is written above
                }
                fields.insert(v.name.clone(), v.to_json());
            }
            out.insert(m.base_key(), Json::Obj(fields));
        }
        out
    }

    /// Config load: unknown module keys are tolerated (different version),
    /// unknown setting keys inside a module are skipped, values clamp.
    pub fn load_config(&mut self, tree: &BTreeMap<String, Json>) -> usize {
        let mut applied = 0;
        for m in self.modules.iter_mut() {
            let Some(Json::Obj(fields)) = tree.get(&m.base_key()) else {
                continue;
            };
            if let Some(Json::Bool(b)) = fields.get("Enabled") {
                m.on_toggled(*b);
                applied += 1;
            }
            if let Some(Json::Bool(h)) = fields.get("Hidden") {
                m.hidden = *h;
                applied += 1;
            }
            if let Some(Json::Str(b)) = fields.get("Bind") {
                m.bind = b.clone();
                applied += 1;
            }
            for v in m.settings.iter_mut() {
                if v.is_enabled_toggle {
                    continue;
                }
                if let Some(j) = fields.get(&v.name) {
                    if v.set_from_json(j).is_ok() {
                        applied += 1;
                    }
                }
            }
        }
        applied
    }

    /// Upstream "modules in category X that are running".
    pub fn running_in_category(&self, c: ModuleCategory) -> Vec<&Module> {
        self.modules.iter().filter(|m| m.running && m.category == c).collect()
    }
}

impl Default for ModuleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::{COMBAT, MOVEMENT};
    use crate::module::Module;
    use crate::value::{choice, float, int};

    fn sample() -> ModuleManager {
        let mut mm = ModuleManager::new();
        mm.register(Module::new(
            "Speed",
            MOVEMENT,
            vec![float("Horizontal Speed", 1.0, 0.1, 5.0)],
        ))
        .unwrap();
        let mut kill = Module::new("Kill Aura", COMBAT, vec![int("Range", 3, 1, 6)]);
        kill.disable_on_quit = true;
        mm.register(kill).unwrap();
        mm
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut mm = sample();
        assert!(mm.register(Module::new("Speed", MOVEMENT, vec![])).is_err());
    }

    #[test]
    fn enable_all_skips_not_activatable() {
        let mut mm = sample();
        mm.by_name_mut("Kill Aura").unwrap().not_activatable = true;
        mm.enable_all();
        assert!(mm.by_name("Speed").unwrap().running);
        assert!(!mm.by_name("Kill Aura").unwrap().running);
        mm.disable_all();
        assert!(!mm.by_name("Speed").unwrap().running);
    }

    #[test]
    fn category_grouping_and_running_filter() {
        let mut mm = sample();
        assert_eq!(mm.in_category(COMBAT).len(), 1);
        assert_eq!(mm.in_category(category::RENDER).len(), 0);
        mm.by_name_mut("Speed").unwrap().running = true;
        assert_eq!(mm.running_in_category(MOVEMENT).len(), 1);
        assert_eq!(mm.running_in_category(COMBAT).len(), 0);
    }

    #[test]
    fn config_round_trip_preserves_everything() {
        let mut mm = sample();
        let m = mm.by_name_mut("Speed").unwrap();
        m.running = true;
        m.setting_mut("Horizontal Speed").unwrap().set_from_json(&Json::Num(2.5)).unwrap();
        let tree = mm.save_config();
        let key = "liquidbounce.module.speed".to_string();
        assert!(tree.contains_key(&key));
        let Json::Obj(f) = &tree[&key] else { panic!() };
        assert_eq!(f["Horizontal Speed"], Json::Num(2.5));

        // Fresh manager, same modules: load restores state.
        let mut fresh = sample();
        let n = fresh.load_config(&tree);
        assert!(n >= 3, "expected Enabled+Hidden+Bind+settings, got {n}");
        assert!(fresh.by_name("Speed").unwrap().running);
        assert_eq!(
            fresh.by_name("Speed").unwrap().setting("Horizontal Speed").unwrap().kind,
            ValueKind::Float(2.5, 0.1, 5.0)
        );
    }

    #[test]
    fn unknown_config_keys_are_tolerated() {
        let mut mm = sample();
        let mut tree = mm.save_config();
        tree.insert("liquidbounce.module.ghostModule".into(), Json::Obj(BTreeMap::new()));
        let before = mm.by_name("Speed").unwrap().running;
        let n = mm.load_config(&tree);
        assert_eq!(mm.by_name("Speed").unwrap().running, before);
        assert!(n >= 2);
        let _ = choice("unused", "a", &["a"]); // keep import honest
    }

    #[test]
    fn lower_camel_case_helper_matches_upstream_shape() {
        assert_eq!(to_lower_camel_case("Auto Clicker"), "autoClicker");
        assert_eq!(to_lower_camel_case("sprint"), "sprint");
    }
}

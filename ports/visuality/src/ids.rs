//! Resource identifiers (net.minecraft.resources.Identifier analog).

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId {
    pub namespace: String,
    pub path: String,
}

fn valid(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.' || c == '-' || c == '/')
}

impl ResourceId {
    /// Bare ids get the minecraft namespace, matching Identifier.parse defaults.
    pub fn parse(s: &str) -> Option<ResourceId> {
        match s.split_once(':') {
            Some((ns, path)) if valid(ns) && valid(path) => {
                Some(ResourceId { namespace: ns.to_string(), path: path.to_string() })
            }
            None if valid(s) => {
                Some(ResourceId { namespace: "minecraft".to_string(), path: s.to_string() })
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

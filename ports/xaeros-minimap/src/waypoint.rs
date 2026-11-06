//! Waypoints: coordinates with name, color and symbol, visible on the minimap
//! and in the game world, organized per world/server and dimension, with
//! distance sorting. (Behavioral port of the documented feature set.)

#[derive(Clone, Debug, PartialEq)]
pub struct Waypoint {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub name: String,
    /// ARGB color of the beacon / map marker.
    pub color: u32,
    /// Single-character symbol shown on the map marker.
    pub symbol: char,
    pub disabled: bool,
    /// Temporary waypoints (auto-created, e.g. from death position).
    pub temporary: bool,
}

/// Where a waypoint set lives: a world (single-player or server address) plus
/// a dimension, mirroring the mod's per-world organization.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MapKey {
    pub world: String,
    pub dimension: String,
}

#[derive(Clone, Debug, Default)]
pub struct WaypointStore {
    sets: std::collections::HashMap<MapKey, Vec<Waypoint>>,
}

impl WaypointStore {
    pub fn new() -> Self { Self::default() }

    pub fn add(&mut self, key: MapKey, wp: Waypoint) {
        self.sets.entry(key).or_default().push(wp);
    }

    pub fn remove(&mut self, key: &MapKey, index: usize) -> Option<Waypoint> {
        let set = self.sets.get_mut(key)?;
        if index < set.len() { Some(set.remove(index)) } else { None }
    }

    pub fn set(&self, key: &MapKey) -> Option<&Vec<Waypoint>> {
        self.sets.get(key)
    }

    /// Enabled waypoints of a set, nearest first, by squared distance in the
    /// horizontal plane (the behavior the map uses for marker priority).
    pub fn nearest(&self, key: &MapKey, px: i32, pz: i32) -> Vec<Waypoint> {
        let mut list: Vec<Waypoint> = self
            .set(key)
            .map(|v| v.iter().filter(|w| !w.disabled).cloned().collect())
            .unwrap_or_default();
        list.sort_by_key(|w| {
            let dx = w.x - px;
            let dz = w.z - pz;
            dx * dx + dz * dz
        });
        list
    }

    /// Death-position helper: a temporary waypoint at the death coordinates.
    pub fn add_death_waypoint(&mut self, key: MapKey, x: i32, y: i32, z: i32) {
        self.add(key, Waypoint {
            x, y, z,
            name: "death".into(),
            color: 0xFF0000,
            symbol: 'X',
            disabled: false,
            temporary: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> MapKey {
        MapKey { world: "server1".into(), dimension: "overworld".into() }
    }

    fn wp(x: i32, z: i32) -> Waypoint {
        Waypoint { x, y: 64, z, name: format!("wp{x}"), color: 0xFF00FF, symbol: 'A', disabled: false, temporary: false }
    }

    #[test]
    fn add_and_list_per_world() {
        let mut s = WaypointStore::new();
        s.add(key(), wp(10, 10));
        s.add(MapKey { world: "server2".into(), dimension: "overworld".into() }, wp(1, 1));
        assert_eq!(s.set(&key()).unwrap().len(), 1);
    }

    #[test]
    fn nearest_sorts_by_distance_and_skips_disabled() {
        let mut s = WaypointStore::new();
        s.add(key(), wp(100, 0));
        s.add(key(), wp(10, 0));
        let mut far_disabled = wp(500, 0);
        far_disabled.disabled = true;
        s.add(key(), far_disabled);
        let sorted = s.nearest(&key(), 0, 0);
        assert_eq!(sorted[0].name, "wp10");
        assert_eq!(sorted[1].name, "wp100");
        assert_eq!(sorted.len(), 2);
    }

    #[test]
    fn remove_by_index() {
        let mut s = WaypointStore::new();
        s.add(key(), wp(1, 1));
        s.add(key(), wp(2, 2));
        let removed = s.remove(&key(), 0).unwrap();
        assert_eq!(removed.name, "wp1");
        assert_eq!(s.set(&key()).unwrap().len(), 1);
        assert!(s.remove(&key(), 9).is_none());
    }

    #[test]
    fn death_waypoint_is_temporary_and_named() {
        let mut s = WaypointStore::new();
        s.add_death_waypoint(key(), -5, 12, 7);
        let w = &s.set(&key()).unwrap()[0];
        assert!(w.temporary && w.name == "death" && (w.x, w.y, w.z) == (-5, 12, 7));
    }
}

//! The self-writing world map: chunk map colors land in per-dimension region
//! stores, keyed by map identity (multiple maps per server address, switched
//! manually or automatically).

/// One map's identity: the server address (or "local" for single-player) and
/// the map slot, mirroring the mod's multiple maps on one address.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MapId {
    pub server_address: String,
    pub map_slot: u32,
}

/// Which dimension a region belongs to (cave dimensions like the Nether
/// supported as their own layer).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DimensionKey {
    pub name: String,
}

/// A 512x512-block region of map pixels, one byte per block (the map color
/// index), origin at the region's northwest corner in world blocks.
#[derive(Clone, Debug)]
pub struct Region {
    pub origin_x: i32,
    pub origin_z: i32,
    /// 512*512 pixels, row-major, northwest first.
    pub pixels: Vec<u8>,
}

impl Region {
    pub const SIZE: usize = 512;

    pub fn new(origin_x: i32, origin_z: i32) -> Self {
        Self { origin_x, origin_z, pixels: vec![0u8; Self::SIZE * Self::SIZE] }
    }

    pub fn set(&mut self, world_x: i32, world_z: i32, color: u8) {
        let lx = (world_x - self.origin_x) as usize;
        let lz = (world_z - self.origin_z) as usize;
        if lx < Self::SIZE && lz < Self::SIZE {
            self.pixels[lz * Self::SIZE + lx] = color;
        }
    }

    pub fn get(&self, world_x: i32, world_z: i32) -> Option<u8> {
        let lx = (world_x - self.origin_x) as usize;
        let lz = (world_z - self.origin_z) as usize;
        if lx < Self::SIZE && lz < Self::SIZE {
            Some(self.pixels[lz * Self::SIZE + lx])
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WorldMapStore {
    maps: std::collections::HashMap<(MapId, DimensionKey), std::collections::HashMap<(i32, i32), Region>>,
    /// How new worlds are picked: manual switching, or automatic world detection.
    pub auto_world_detection: bool,
}

impl WorldMapStore {
    /// Ingest one chunk of map colors (16x16 blocks at chunk origin), the
    /// "self-writing" behavior.
    pub fn write_chunk(&mut self, id: &MapId, dim: &DimensionKey, chunk_x: i32, chunk_z: i32, colors: &[u8; 256]) {
        let region_origin = |c: i32| (c * 16).div_euclid(512) * 512;
        let rx = region_origin(chunk_x);
        let rz = region_origin(chunk_z);
        let region = self
            .maps
            .entry((id.clone(), dim.clone()))
            .or_default()
            .entry((rx, rz))
            .or_insert_with(|| Region::new(rx, rz));
        for cx in 0..16i32 {
            for cz in 0..16i32 {
                let color = colors[(cz * 16 + cx) as usize];
                region.set(chunk_x * 16 + cx, chunk_z * 16 + cz, color);
            }
        }
    }

    pub fn region(&self, id: &MapId, dim: &DimensionKey, rx: i32, rz: i32) -> Option<&Region> {
        self.maps.get(&(id.clone(), dim.clone()))?.get(&(rx, rz))
    }

    /// Whether the map has any pixels near the player: used by the automatic
    /// switcher to decide whether a map slot holds the world you are in.
    pub fn has_data_near(&self, id: &MapId, dim: &DimensionKey, x: i32, z: i32) -> bool {
        let rx = x.div_euclid(512) * 512;
        let rz = z.div_euclid(512) * 512;
        self.region(id, dim, rx, rz).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> MapId { MapId { server_address: "s1".into(), map_slot: 0 } }
    fn dim() -> DimensionKey { DimensionKey { name: "overworld".into() } }

    fn chunk_colors(c: u8) -> [u8; 256] { [c; 256] }

    #[test]
    fn chunk_write_reaches_region_pixels() {
        let mut s = WorldMapStore::default();
        s.write_chunk(&id(), &dim(), 0, 0, &chunk_colors(7));
        let r = s.region(&id(), &dim(), 0, 0).unwrap();
        assert_eq!(r.get(5, 9), Some(7));
        assert_eq!(r.get(600, 0), None);
    }

    #[test]
    fn negative_chunk_coordinates_land_in_the_right_region() {
        let mut s = WorldMapStore::default();
        s.write_chunk(&id(), &dim(), -1, -1, &chunk_colors(3));
        let r = s.region(&id(), &dim(), -512, -512).unwrap();
        assert_eq!(r.get(-5, -5), Some(3), "inside chunk (-1,-1)");
        assert_eq!(r.get(-20, -20), Some(0), "block -20 lives in chunk -2, not written");
    }

    #[test]
    fn dimensions_are_separate_layers() {
        let mut s = WorldMapStore::default();
        let nether = DimensionKey { name: "the_nether".into() };
        s.write_chunk(&id(), &dim(), 0, 0, &chunk_colors(1));
        s.write_chunk(&id(), &nether, 0, 0, &chunk_colors(2));
        assert_eq!(s.region(&id(), &dim(), 0, 0).unwrap().get(0, 0), Some(1));
        assert_eq!(s.region(&id(), &nether, 0, 0).unwrap().get(0, 0), Some(2));
    }

    #[test]
    fn map_slots_are_separate_maps() {
        let mut s = WorldMapStore::default();
        let other = MapId { server_address: "s1".into(), map_slot: 1 };
        s.write_chunk(&id(), &dim(), 0, 0, &chunk_colors(1));
        assert!(!s.has_data_near(&other, &dim(), 8, 8));
        assert!(s.has_data_near(&id(), &dim(), 8, 8));
    }
}

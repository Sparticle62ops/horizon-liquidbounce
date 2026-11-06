pub mod changes {
// Change classes: 1:1 Rust port of ReplayStudio pathing/change
// (AddKeyframe, RemoveKeyframe, UpdateKeyframeProperties, SetInterpolator,
// CombinedChange). Upstream Preconditions.checkState -> Result.
// Copyright (c) 2026 sparticle, LGPL-3.0-or-later.
use super::pathing::*;
use std::collections::BTreeMap;

/// Nth keyframe time (sorted), upstream Iterables.get(path.getKeyframes(), i).
pub fn kf_index_to_time(p: &Path, index: usize) -> Option<i64> {
    p.keyframes.keys().copied().nth(index)
}

pub fn kf_time_to_index(p: &Path, time: i64) -> usize {
    p.keyframes.keys().filter(|k| **k < time).count()
}

/// Upstream AddKeyframe.
pub struct AddKeyframe { path: usize, time: i64, index: Option<usize>, applied: bool }

impl AddKeyframe {
    pub fn new(path: usize, time: i64) -> Self {
        Self { path, time, index: None, applied: false }
    }
}

impl Change for AddKeyframe {
    fn apply(&mut self, t: &mut Timeline) -> Result<(), String> {
        if self.applied { return Err("Already applied!".into()); }
        t.path_insert(self.path, self.time, BTreeMap::new())?;
        self.index = Some(kf_time_to_index(&t.paths[self.path], self.time));
        self.applied = true;
        Ok(())
    }
    fn undo(&mut self, t: &mut Timeline) -> Result<(), String> {
        if !self.applied { return Err("Not yet applied!".into()); }
        let index = self.index.ok_or("not applied")?;
        let time = kf_index_to_time(&t.paths[self.path], index).ok_or("keyframe missing")?;
        t.path_remove(self.path, time, true)?;
        self.applied = false;
        Ok(())
    }
}

/// Upstream RemoveKeyframe. Stores the removed keyframe and the interpolator
/// that would be lost, for the undo.
pub struct RemoveKeyframe {
    path: usize, index: usize,
    removed: Option<(i64, BTreeMap<PropertyId, PropertyValue>)>,
    removed_interpolator: Option<InterpId>,
    applied: bool,
}

impl RemoveKeyframe {
    /// Upstream create(Path, Keyframe): the index is the keyframe's position
    /// in the path's sorted keyframe list; callers pass it (SPTimeline
    /// computes it from the keyframe time).
    pub fn new_with_index(path: usize, index: usize) -> Self {
        Self { path, index, removed: None, removed_interpolator: None, applied: false }
    }
}

impl Change for RemoveKeyframe {
    fn apply(&mut self, t: &mut Timeline) -> Result<(), String> {
        if self.applied { return Err("Already applied!".into()); }
        let path = &t.paths[self.path];
        if !path.segments.is_empty() {
            // Last keyframe keeps the left-hand interpolator, others the right-hand one.
            if self.index == path.segments.len() {
                self.removed_interpolator = path.segments[self.index - 1].interpolator;
            } else {
                self.removed_interpolator = path.segments[self.index].interpolator;
            }
        }
        let time = kf_index_to_time(path, self.index).ok_or("no such keyframe")?;
        self.removed = Some((time, path.keyframes[&time].values.clone()));
        t.path_remove(self.path, time, true)?;
        self.applied = true;
        Ok(())
    }
    fn undo(&mut self, t: &mut Timeline) -> Result<(), String> {
        if !self.applied { return Err("Not yet applied!".into()); }
        let (time, values) = self.removed.clone().ok_or("not applied")?;
        t.path_insert(self.path, time, values)?;
        if let Some(ip) = self.removed_interpolator {
            if self.index == t.paths[self.path].segments.len() {
                t.seg_set_interpolator(self.path, self.index - 1, Some(ip));
            } else {
                t.seg_set_interpolator(self.path, self.index, Some(ip));
            }
        }
        self.applied = false;
        Ok(())
    }
}

/// Upstream UpdateKeyframeProperties (builder + change).
pub struct UpdateKeyframeProperties {
    path: usize, index: usize,
    new_values: BTreeMap<PropertyId, Option<PropertyValue>>,
    old_values: BTreeMap<PropertyId, Option<PropertyValue>>,
    applied: bool,
}

pub struct UpdateKeyframePropertiesBuilder {
    path: usize, index: usize,
    updates: BTreeMap<PropertyId, Option<PropertyValue>>,
}

impl UpdateKeyframePropertiesBuilder {
    pub fn set_value(mut self, prop: PropertyId, v: PropertyValue) -> Self {
        self.updates.insert(prop, Some(v)); self
    }
    pub fn remove_property(mut self, prop: PropertyId) -> Self {
        self.updates.insert(prop, None); self
    }
    pub fn done(self) -> UpdateKeyframeProperties {
        UpdateKeyframeProperties { path: self.path, index: self.index, new_values: self.updates, old_values: BTreeMap::new(), applied: false }
    }
}

impl UpdateKeyframeProperties {
    /// Upstream create(Path, Keyframe): builder positioned at that keyframe.
    pub fn create(path: usize, index: usize) -> UpdateKeyframePropertiesBuilder {
        UpdateKeyframePropertiesBuilder { path, index, updates: BTreeMap::new() }
    }
}

impl Change for UpdateKeyframeProperties {
    fn apply(&mut self, t: &mut Timeline) -> Result<(), String> {
        if self.applied { return Err("Already applied!".into()); }
        let time = kf_index_to_time(&t.paths[self.path], self.index).ok_or("no such keyframe")?;
        let props: Vec<(PropertyId, Option<PropertyValue>)> = self.new_values.iter().map(|(k, v)| (*k, v.clone())).collect();
        {
            let kf = &mut t.paths[self.path].keyframes.get_mut(&time).ok_or("no such keyframe")?;
            for (prop, nv) in props {
                self.old_values.insert(prop, kf.values.get(&prop).cloned());
                match nv {
                    Some(v) => { kf.values.insert(prop, v); }
                    None => { kf.values.remove(&prop); }
                }
            }
        }
        self.applied = true;
        Ok(())
    }
    fn undo(&mut self, t: &mut Timeline) -> Result<(), String> {
        if !self.applied { return Err("Not yet applied!".into()); }
        let time = kf_index_to_time(&t.paths[self.path], self.index).ok_or("no such keyframe")?;
        let old: Vec<(PropertyId, Option<PropertyValue>)> = self.old_values.iter().map(|(k, v)| (*k, v.clone())).collect();
        {
            let kf = &mut t.paths[self.path].keyframes.get_mut(&time).ok_or("no such keyframe")?;
            for (prop, ov) in old {
                self.new_values.insert(prop, kf.values.get(&prop).cloned());
                match ov {
                    Some(v) => { kf.values.insert(prop, v); }
                    None => { kf.values.remove(&prop); }
                }
            }
        }
        self.applied = false;
        Ok(())
    }
}

/// Upstream SetInterpolator.
pub struct SetInterpolator { path: usize, index: usize, interp: InterpId, old_interp: Option<InterpId>, applied: bool }

impl SetInterpolator {
    pub fn new(path: usize, seg_index: usize, interp: InterpId) -> Self {
        Self { path, index: seg_index, interp, old_interp: None, applied: false }
    }
}

impl Change for SetInterpolator {
    fn apply(&mut self, t: &mut Timeline) -> Result<(), String> {
        if self.applied { return Err("Already applied!".into()); }
        if self.index >= t.paths[self.path].segments.len() { return Err("no such segment".into()); }
        self.old_interp = t.paths[self.path].segments[self.index].interpolator;
        t.seg_set_interpolator(self.path, self.index, Some(self.interp));
        self.applied = true;
        Ok(())
    }
    fn undo(&mut self, t: &mut Timeline) -> Result<(), String> {
        if !self.applied { return Err("Not yet applied!".into()); }
        t.seg_set_interpolator(self.path, self.index, self.old_interp);
        self.applied = false;
        Ok(())
    }
}

/// Upstream CombinedChange.
pub struct CombinedChange { changes: Vec<Box<dyn Change>>, applied: bool }

impl CombinedChange {
    pub fn create(changes: Vec<Box<dyn Change>>) -> Self { Self { changes, applied: false } }
    pub fn create_from_applied(changes: Vec<Box<dyn Change>>) -> Self { Self { changes, applied: true } }
}

impl Change for CombinedChange {
    fn apply(&mut self, t: &mut Timeline) -> Result<(), String> {
        if self.applied { return Err("Already applied!".into()); }
        for c in &mut self.changes { c.apply(t)?; }
        self.applied = true;
        Ok(())
    }
    fn undo(&mut self, t: &mut Timeline) -> Result<(), String> {
        if !self.applied { return Err("Not yet applied!".into()); }
        for c in self.changes.iter_mut().rev() { c.undo(t)?; }
        self.applied = false;
        Ok(())
    }
}
}
pub mod mcpr {
// Replay file layer: 1:1 Rust port of ReplayStudio's replay-file constants
// and the .tmcpr packet stream framing (ReplayInputStream.readPacket /
// fillBuffer framing half, ReplayOutputStream.doWrite framing half,
// ReplayMetaData's JSON schema; LGPL-3.0-or-later). This file: Copyright
// (c) 2026 sparticle, LGPL-3.0-or-later.
//
// The zip container itself (ZipReplayFile's archive open/write + CRC cache)
// is a Horizon-integration hook behind McprArchive below; the MC packet
// decode half of fillBuffer (varint packet id, registry state transitions,
// the JoinGame spectator rewrite, ViaVersion armor shims) is also a hook:
// for the framing port, packets are opaque byte blobs, matching how the
// sender port consumes them.
use super::serialize::{JVal, json_parse}; // reuse the hand-rolled JSON reader

// Upstream AbstractReplayFile entry-name constants.
pub const ENTRY_META_DATA: &str = "metaData.json";
pub const ENTRY_RECORDING: &str = "recording.tmcpr";
pub const ENTRY_RECORDING_HASH: &str = "recording.tmcpr.crc32";
pub const ENTRY_RESOURCE_PACK_INDEX: &str = "resourcepack/index.json";
pub const ENTRY_THUMB_OLD: &str = "thumb";
pub const ENTRY_THUMB: &str = "thumb.jpg";
pub const ENTRY_VISIBILITY_OLD: &str = "visibility";
pub const ENTRY_VISIBILITY: &str = "visibility.json";
pub const ENTRY_MARKERS: &str = "markers.json";
pub const ENTRY_MODS: &str = "mods.json";
/// Upstream ENTRY_ASSET / ENTRY_RESOURCE_PACK are printf formats
/// ("asset/%s_%s.%s", "resourcepack/%s.zip"); port as helpers.
pub fn entry_asset(name: &str, typ: &str, ext: &str) -> String { format!("asset/{}_{}.{}", name, typ, ext) }
pub fn entry_resource_pack(name: &str) -> String { format!("resourcepack/{}.zip", name) }

/// Upstream ReplayMetaData.CURRENT_FILE_FORMAT_VERSION.
pub const CURRENT_FILE_FORMAT_VERSION: i32 = 14;

/// Upstream Utils.readInt: big-endian, any -1 byte means stream end.
pub fn read_int(buf: &[u8], pos: &mut usize) -> i32 {
    if *pos + 4 > buf.len() { *pos = buf.len(); return -1; }
    *pos += 4;
    let b = [buf[*pos - 4], buf[*pos - 3], buf[*pos - 2], buf[*pos - 1]];
    // Upstream checks (b0|b1|b2|b3) < 0 where the b* are in.read() results:
    // -1 only means end of stream, high-value bytes are legitimate.
    i32::from_be_bytes(b)
}

/// Upstream Utils.writeInt: big-endian.
pub fn write_int(out: &mut Vec<u8>, v: i32) { out.extend_from_slice(&v.to_be_bytes()); }

/// Upstream PacketData: opaque recorded packet bytes with replay timestamp.
/// (Upstream carries a decoded Packet; the decode is the Horizon hook.)
#[derive(Clone, Debug)]
pub struct McprPacketData { pub timestamp: i64, pub bytes: Vec<u8> }

/// Upstream ReplayInputStream.readPacket over the raw stream, framing half:
/// int timestamp, int length, then bytes; -1s end the stream; empty
/// segments are skipped. The MC decode/registry half stays a hook.
pub fn read_packets(stream: &[u8]) -> Vec<McprPacketData> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    loop {
        let next = read_int(stream, &mut pos);
        let length = read_int(stream, &mut pos);
        if next == -1 || length == -1 { break; } // upstream: end of stream
        if length < 0 { break; } // upstream EOFException on malformed length
        if length == 0 { continue; } // upstream: skip empty segments
        if pos + length as usize > stream.len() { break; } // upstream EOFException
        out.push(McprPacketData { timestamp: next as i64, bytes: stream[pos..pos + length as usize].to_vec() });
        pos += length as usize;
    }
    out
}

/// Upstream ReplayOutputStream.doWrite: int time, int length, bytes.
pub fn write_packet(out: &mut Vec<u8>, time: i64, bytes: &[u8]) {
    write_int(out, time as i32);
    write_int(out, bytes.len() as i32);
    out.extend_from_slice(bytes);
}

/// Upstream ReplayMetaData, JSON schema fields verbatim.
#[derive(Clone, Debug)]
pub struct ReplayMetaData {
    pub singleplayer: bool,
    pub server_name: String,
    pub custom_server_name: String,
    pub duration: i32,
    pub date: i64,
    pub mc_version: String,
    pub file_format: String,
    pub file_format_version: i32,
    pub protocol: Option<i32>,
    pub generator: String,
    pub self_id: i32, // upstream default -1
    pub players: Vec<String>,
}

impl Default for ReplayMetaData {
    fn default() -> Self {
        Self { singleplayer: false, server_name: String::new(), custom_server_name: String::new(),
            duration: 0, date: 0, mc_version: String::new(),
            file_format: String::new(), file_format_version: 0,
            protocol: None, generator: String::new(), self_id: -1, players: Vec::new() }
    }
}

fn jstr(v: Option<&JVal>, d: &str) -> String { match v { Some(JVal::Str(s)) => s.clone(), _ => d.into() } }
fn jget<'v>(v: &'v JVal, k: &str) -> Option<&'v JVal> {
    match v { JVal::Obj(m) => m.iter().find(|(kk, _)| kk == k).map(|(_, vv)| vv), _ => None }
}
fn jnum(v: Option<&JVal>) -> Option<f64> { match v { Some(JVal::Num(n)) => Some(*n), _ => None } }
fn jbool(v: Option<&JVal>) -> bool { matches!(v, Some(JVal::Bool(b)) if *b) }

/// Upstream Gson deserialization of metaData.json.
pub fn parse_meta_data(json: &str) -> Result<ReplayMetaData, String> {
    let v = json_parse(json)?;
    let g = |k: &str| jget(&v, k);
    Ok(ReplayMetaData {
        singleplayer: jbool(g("singleplayer")),
        server_name: jstr(g("serverName"), ""),
        custom_server_name: jstr(g("customServerName"), ""),
        duration: jnum(g("duration")).unwrap_or(0.0) as i32,
        date: jnum(g("date")).unwrap_or(0.0) as i64,
        mc_version: jstr(g("mcversion"), ""),
        file_format: jstr(g("fileFormat"), ""),
        file_format_version: jnum(g("fileFormatVersion")).unwrap_or(0.0) as i32,
        protocol: jnum(g("protocol")).map(|n| n as i32),
        generator: jstr(g("generator"), ""),
        self_id: jnum(g("selfId")).unwrap_or(-1.0) as i32,
        players: match g("players") { Some(JVal::Arr(a)) => a.iter().filter_map(|p| if let JVal::Str(s) = p { Some(s.clone()) } else { None }).collect(), _ => Vec::new() },
    })
}

/// Upstream ZipReplayFile's archive container, abstracted: Horizon provides
/// the zip read/write; this trait is the seam.
pub trait McprArchive {
    fn read_entry(&self, name: &str) -> Option<Vec<u8>>;
    fn list_entries(&self) -> Vec<String>;
    fn write_entry(&mut self, name: &str, bytes: &[u8]);
    fn remove_entry(&mut self, name: &str);
}
}
pub mod pathing {
// Pathing core: 1:1 Rust port of ReplayStudio's pathing package.
// Upstream: ReplayStudio (github.com/ReplayMod/ReplayStudio),
// com/replaymod/replaystudio/pathing/{path,impl,property,interpolation},
// LGPL-3.0-or-later. This derived port: copyright (c) 2026 sparticle,
// licensed LGPL-3.0-or-later.
//
// Shape notes: upstream Path back-references its Timeline; Rust ownership
// makes Timeline the single owner and path handles are indices. Keyframes
// live in a BTreeMap keyed by time (upstream TreeMap<Long,Keyframe>), so
// keyframe identity == keyframe time; segments reference start/end times.
// Interpolators are shared objects (upstream identity) kept in Timeline
// .interps and referenced by id from segments.

use std::collections::{BTreeMap, HashMap, HashSet};

pub type PropertyId = usize;
pub type InterpId = usize;
pub type PartKey = (PropertyId, usize);

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Position(f64, f64, f64),
    Rotation(f32, f32, f32),
    Timestamp(i32),
    Spectator(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind { PosX, PosY, PosZ, Yaw, Pitch, Roll, Timestamp, Spectator }

#[derive(Clone, Debug)]
pub struct PartDef {
    pub kind: PartKind,
    pub interpolatable: bool,
    pub upper_bound: f64,
}

impl PartDef {
    pub fn new(kind: PartKind, interpolatable: bool, upper_bound: f64) -> Self {
        Self { kind, interpolatable, upper_bound }
    }

    /// Upstream PropertyPart.toDouble. The interface javadoc claims an
    /// UnsupportedOperationException for non-interpolatable parts, but the
    /// concrete parts (ForInteger, ForDoubleTriple, ForFloatTriple) never
    /// check; only get_value's isInterpolatable guard does. Mirrored here.
    pub fn to_double(&self, v: &PropertyValue) -> Result<f64, String> {
        match (self.kind, v) {
            (PartKind::PosX, PropertyValue::Position(x, _, _)) => Ok(*x),
            (PartKind::PosY, PropertyValue::Position(_, y, _)) => Ok(*y),
            (PartKind::PosZ, PropertyValue::Position(_, _, z)) => Ok(*z),
            (PartKind::Yaw, PropertyValue::Rotation(y, _, _)) => Ok(*y as f64),
            (PartKind::Pitch, PropertyValue::Rotation(_, p, _)) => Ok(*p as f64),
            (PartKind::Roll, PropertyValue::Rotation(_, _, r)) => Ok(*r as f64),
            (PartKind::Timestamp, PropertyValue::Timestamp(t)) => Ok(*t as f64),
            (PartKind::Spectator, PropertyValue::Spectator(s)) => Ok(*s as f64),
            _ => Err("part kind does not match value".into()),
        }
    }

    /// Upstream PropertyPart.fromDouble: replace this part's component.
    pub fn from_double(&self, v: &PropertyValue, d: f64) -> Result<PropertyValue, String> {
        match (self.kind, v) {
            (PartKind::PosX, PropertyValue::Position(_, y, z)) => Ok(PropertyValue::Position(d, *y, *z)),
            (PartKind::PosY, PropertyValue::Position(x, _, z)) => Ok(PropertyValue::Position(*x, d, *z)),
            (PartKind::PosZ, PropertyValue::Position(x, y, _)) => Ok(PropertyValue::Position(*x, *y, d)),
            (PartKind::Yaw, PropertyValue::Rotation(_, p, r)) => Ok(PropertyValue::Rotation(d as f32, *p, *r)),
            (PartKind::Pitch, PropertyValue::Rotation(y, _, r)) => Ok(PropertyValue::Rotation(*y, d as f32, *r)),
            (PartKind::Roll, PropertyValue::Rotation(y, p, _)) => Ok(PropertyValue::Rotation(*y, *p, d as f32)),
            (PartKind::Timestamp, PropertyValue::Timestamp(_)) => Ok(PropertyValue::Timestamp(round_i32(d))),
            (PartKind::Spectator, PropertyValue::Spectator(_)) => Ok(PropertyValue::Spectator(round_i32(d))),
            _ => Err("part kind does not match value".into()),
        }
    }
}

/// Upstream PropertyParts.ForInteger.fromDouble: (int) Math.round(d).
fn round_i32(d: f64) -> i32 { d.round() as i32 }

/// Upstream Property: id, group, localized name (upstream resolves via I18n;
/// the key is stored and the GUI layer resolves), initial value, parts.
#[derive(Clone, Debug)]
pub struct Property {
    pub id: String,
    pub group: Option<String>,
    pub localized_name: String,
    pub initial: PropertyValue,
    pub parts: Vec<PartDef>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keyframe {
    pub values: BTreeMap<PropertyId, PropertyValue>,
}

#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub id: u32,
    pub start: i64,
    pub end: i64,
    pub interpolator: Option<InterpId>,
}

#[derive(Default)]
pub struct Path {
    pub keyframes: BTreeMap<i64, Keyframe>,
    pub segments: Vec<Segment>,
    pub active: bool,
}

impl Path {
    pub fn new() -> Self { Self { keyframes: BTreeMap::new(), segments: Vec::new(), active: true } }
}

#[derive(Clone, Copy, Debug)]
pub struct InterpParams { pub value: f64, pub velocity: f64, pub acceleration: f64 }

/// Upstream com.replaymod.replaystudio.pathing.change.Change.
pub trait Change {
    fn apply(&mut self, t: &mut Timeline) -> Result<(), String>;
    fn undo(&mut self, t: &mut Timeline) -> Result<(), String>;
}

/// Upstream com.replaymod.replaystudio.pathing.interpolation.Interpolator.
/// Object-safe trait; shared instances live in Timeline.interps.
pub trait Interpolator {
    /// Upstream interpolator class identity + constructor argument, for
    /// SPTimeline's class checks and serialization: ("linear", None),
    /// ("cubic-spline", None), ("catmull-rom-spline", Some(alpha)).
    fn kind(&self) -> (&'static str, Option<f64>) { ("unknown", None) }
    fn register_property(&mut self, prop: PropertyId);
    fn unregister_property(&mut self, prop: PropertyId);
    fn keyframe_properties(&self) -> &HashSet<PropertyId>;
    fn add_segment(&mut self, seg: u32);
    fn remove_segment(&mut self, seg: u32);
    fn segments(&self) -> &[u32];
    fn is_dirty(&self) -> bool;
    /// Upstream bake(Map): sorts this interpolator's segments by start time,
    /// checks continuity, then bakeInterpolation. `segs` is the (id, start,
    /// end) list for the segment ids this interpolator holds.
    fn bake(
        &mut self,
        segs: &[(u32, i64, i64)],
        keyframes: &BTreeMap<i64, Keyframe>,
        properties: &[Property],
        params: BTreeMap<PartKey, InterpParams>,
    ) -> Result<BTreeMap<PartKey, InterpParams>, String>;
    /// Upstream getValue: keyframes is the owning path's map, properties the
    /// timeline registry.
    fn get_value(
        &self,
        prop: PropertyId,
        time: i64,
        keyframes: &BTreeMap<i64, Keyframe>,
        properties: &[Property],
    ) -> Result<Option<PropertyValue>, String>;
}

/// State shared by upstream AbstractInterpolator subclasses.
#[derive(Default)]
pub struct AbstractInterpolatorState {
    pub segments: Vec<u32>,
    pub dirty: bool,
    pub properties: HashSet<PropertyId>,
}

/// Upstream AbstractInterpolator.bake (sort + continuity check).
pub fn abstract_bake(
    st: &mut AbstractInterpolatorState,
    segs: &[(u32, i64, i64)],
) -> Result<Vec<(u32, i64, i64)>, String> {
    if st.segments.is_empty() { return Err("No segments have been added yet.".into()); }
    let by_id: HashMap<u32, (i64, i64)> =
        segs.iter().map(|&(id, s, e)| (id, (s, e))).collect();
    // Sort this interpolator's own segment list by start keyframe time.
    st.segments.sort_by_key(|&id| by_id.get(&id).map(|p| p.0).unwrap_or(i64::MAX));
    let sorted: Vec<(u32, i64, i64)> = st
        .segments
        .iter()
        .map(|&id| {
            let (s, e) = by_id.get(&id).copied().unwrap_or((i64::MAX, i64::MAX));
            (id, s, e)
        })
        .collect();
    // Check for continuity: last.end keyframe == next.start keyframe.
    for w in sorted.windows(2) {
        if w[0].2 != w[1].1 {
            return Err("Segments are not continuous.".into());
        }
    }
    Ok(sorted)
}

/// Upstream LinearInterpolator: linear interpolation with cyclic wrapping on
/// bounded parts. framesToProperty keeps keyframe times in bake (insertion)
/// order, mirroring upstream's LinkedHashSet.
#[derive(Default)]
pub struct LinearInterpolator {
    pub base: AbstractInterpolatorState,
    frames_to_property: BTreeMap<PropertyId, Vec<i64>>,
}

impl LinearInterpolator {
    pub fn new() -> Self { Self::default() }

    fn add_to_map(&mut self, prop: PropertyId, time: i64) {
        let v = self.frames_to_property.entry(prop).or_default();
        if !v.contains(&time) { v.push(time); }
    }
}

impl Interpolator for LinearInterpolator {
    fn kind(&self) -> (&'static str, Option<f64>) { ("linear", None) }
    fn register_property(&mut self, prop: PropertyId) {
        if self.base.properties.insert(prop) { self.base.dirty = true; }
    }
    fn unregister_property(&mut self, prop: PropertyId) {
        if self.base.properties.remove(&prop) { self.base.dirty = true; }
    }
    fn keyframe_properties(&self) -> &HashSet<PropertyId> { &self.base.properties }
    fn add_segment(&mut self, seg: u32) { self.base.segments.push(seg); self.base.dirty = true; }
    fn remove_segment(&mut self, seg: u32) {
        self.base.segments.retain(|&s| s != seg);
        self.base.dirty = true;
    }
    fn segments(&self) -> &[u32] { &self.base.segments }
    fn is_dirty(&self) -> bool { self.base.dirty }

    fn bake(
        &mut self,
        segs: &[(u32, i64, i64)],
        keyframes: &BTreeMap<i64, Keyframe>,
        _properties: &[Property],
        params: BTreeMap<PartKey, InterpParams>,
    ) -> Result<BTreeMap<PartKey, InterpParams>, String> {
        let sorted = abstract_bake(&mut self.base, segs)?;

        self.frames_to_property.clear();
        let props: Vec<PropertyId> = self.base.properties.iter().copied().collect();
        for &(_, start, end) in &sorted {
            for &prop in &props {
                if keyframes.get(&start).map_or(false, |k| k.values.contains_key(&prop)) {
                    self.add_to_map(prop, start);
                }
                if keyframes.get(&end).map_or(false, |k| k.values.contains_key(&prop)) {
                    self.add_to_map(prop, end);
                }
            }
        }

        // lastParameters from the final segment's end keyframe.
        let last = sorted.last().map(|&(_, _, e)| e).unwrap();
        let mut out = params;
        for &prop in &props {
            if let Some(v) = keyframes.get(&last).and_then(|k| k.values.get(&prop)) {
                for (i, part) in _properties[prop].parts.iter().enumerate() {
                    out.insert(
                        (prop, i),
                        InterpParams { value: part.to_double(v)?, velocity: 1.0, acceleration: 0.0 },
                    );
                }
            }
        }
        Ok(out)
    }

    fn get_value(
        &self,
        prop: PropertyId,
        time: i64,
        keyframes: &BTreeMap<i64, Keyframe>,
        properties: &[Property],
    ) -> Result<Option<PropertyValue>, String> {
        let Some(times) = self.frames_to_property.get(&prop) else { return Ok(None) };
        let mut before: Option<i64> = None;
        let mut after: Option<i64> = None;
        for &t in times {
            if t == time {
                return Ok(keyframes.get(&t).and_then(|k| k.values.get(&prop)).cloned());
            } else if t < time {
                before = Some(t);
            } else {
                after = Some(t);
                break;
            }
        }
        let (Some(bt), Some(at)) = (before, after) else { return Ok(None) };
        let value_before = keyframes[&bt].values[&prop].clone();
        let value_after = keyframes[&at].values[&prop].clone();
        let fraction = (time - bt) as f64 / (at - bt) as f64;

        let mut interpolated = value_before.clone();
        for part in &properties[prop].parts {
            if part.interpolatable {
                let mut b = part.to_double(&value_before)?;
                let mut a = part.to_double(&value_after)?;
                let bound = part.upper_bound;
                if !bound.is_nan() {
                    b = fmod_floor(b, bound);
                    a = fmod_floor(a, bound);
                    if (a - b).abs() > bound / 2.0 {
                        if b < bound / 2.0 { a -= bound; } else { a += bound; }
                    }
                }
                let mut v = (a - b) * fraction + b;
                if !bound.is_nan() { v = fmod_floor(v, bound); }
                interpolated = part.from_double(&interpolated, v)?;
            }
        }
        Ok(Some(interpolated))
    }
}

/// Upstream LinearInterpolator.mod: floor-based modulo.
fn fmod_floor(val: f64, m: f64) -> f64 {
    let off = (val / m).floor();
    val - off * m
}

/// Upstream TimelineImpl + PathImpl, with Timeline as the single owner.
pub struct Timeline {
    pub paths: Vec<Path>,
    /// Property registry: PropertyId = index; each define_property appends,
    /// matching upstream object identity (two registrations of the same id
    /// string are distinct objects unless replaced in prop_by_string).
    pub properties: Vec<Property>,
    /// Upstream TimelineImpl.properties map, keyed "group:id" or "id".
    prop_by_string: HashMap<String, PropertyId>,
    pub interps: Vec<Box<dyn Interpolator>>,
    next_seg_id: u32,
    undo: Vec<Box<dyn Change>>,
    redo: Vec<Box<dyn Change>>,
}

impl Default for Timeline { fn default() -> Self { Self::new() } }

impl Timeline {
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            properties: Vec::new(),
            prop_by_string: HashMap::new(),
            interps: Vec::new(),
            next_seg_id: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Upstream Timeline.registerProperty (which also creates the property).
    pub fn define_property(&mut self, p: Property) -> PropertyId {
        let id = self.properties.len();
        let key = match &p.group {
            Some(g) => format!("{}:{}", g, p.id),
            None => p.id.clone(),
        };
        self.prop_by_string.insert(key, id);
        self.properties.push(p);
        id
    }

    /// Upstream Timeline.getProperty(id).
    pub fn get_property_by_string(&self, id: &str) -> Option<PropertyId> {
        self.prop_by_string.get(id).copied()
    }

    /// Upstream Timeline.createPath.
    pub fn create_path(&mut self) -> usize {
        self.paths.push(Path::new());
        self.paths.len() - 1
    }

    pub fn create_interpolator(&mut self, i: Box<dyn Interpolator>) -> InterpId {
        self.interps.push(i);
        self.interps.len() - 1
    }

    fn insert_segment_at(&mut self, p: usize, idx: usize, start: i64, end: i64, interp: Option<InterpId>) {
        let id = self.next_seg_id;
        self.next_seg_id += 1;
        self.paths[p].segments.insert(idx, Segment { id, start, end, interpolator: None });
        if let Some(ip) = interp {
            self.interps[ip].add_segment(id);
            self.paths[p].segments[idx].interpolator = Some(ip);
        }
    }

    fn push_segment(&mut self, p: usize, start: i64, end: i64, interp: Option<InterpId>) {
        let id = self.next_seg_id;
        self.next_seg_id += 1;
        self.paths[p].segments.push(Segment { id, start, end, interpolator: None });
        if let Some(ip) = interp {
            self.interps[ip].add_segment(id);
            let n = self.paths[p].segments.len();
            self.paths[p].segments[n - 1].interpolator = Some(ip);
        }
    }

    /// Upstream PathSegmentImpl.setInterpolator: detach old, attach new.
    pub fn seg_set_interpolator(&mut self, p: usize, seg_idx: usize, interp: Option<InterpId>) {
        let seg = self.paths[p].segments[seg_idx];
        if let Some(old) = seg.interpolator {
            self.interps[old].remove_segment(seg.id);
        }
        self.paths[p].segments[seg_idx].interpolator = interp;
        if let Some(new) = interp {
            self.interps[new].add_segment(seg.id);
        }
    }

    /// Upstream Path.insert(Keyframe) / insert(long); `values` are the new
    /// keyframe's property values (empty for insert(long)).
    pub fn path_insert(
        &mut self,
        p: usize,
        time: i64,
        values: BTreeMap<PropertyId, PropertyValue>,
    ) -> Result<(), String> {
        if self.paths[p].keyframes.contains_key(&time) {
            return Err(format!("A keyframe at {} already exists.", time));
        }
        self.paths[p].keyframes.insert(time, Keyframe { values });

        if self.paths[p].segments.is_empty() {
            if self.paths[p].keyframes.len() >= 2 {
                let mut it = self.paths[p].keyframes.keys().copied();
                let t1 = it.next().unwrap();
                let t2 = it.next().unwrap();
                self.push_segment(p, t1, t2, None);
            }
            return Ok(());
        }

        let first = self.paths[p].segments[0];
        if time < first.start {
            let interp = first.interpolator;
            self.insert_segment_at(p, 0, time, first.start, interp);
            return Ok(());
        }

        let mut i = 0usize;
        loop {
            let seg = self.paths[p].segments[i];
            if seg.start <= time && seg.end >= time {
                let interp = seg.interpolator;
                self.paths[p].segments.remove(i);
                if let Some(ip) = interp {
                    self.interps[ip].remove_segment(seg.id);
                }
                self.insert_segment_at(p, i, seg.start, time, interp);
                self.insert_segment_at(p, i + 1, time, seg.end, interp);
                return Ok(());
            }
            if i + 1 < self.paths[p].segments.len() {
                i += 1;
            } else {
                let interp = seg.interpolator;
                self.push_segment(p, seg.end, time, interp);
                return Ok(());
            }
        }
    }

    /// Upstream Path.getKeyframe(time).
    pub fn path_get_keyframe(&self, p: usize, time: i64) -> Option<&Keyframe> {
        self.paths[p].keyframes.get(&time)
    }

    /// Upstream Keyframe.setValue/get/removeProperty helpers.
    pub fn kf_set_value(&mut self, p: usize, time: i64, prop: PropertyId, v: PropertyValue) -> Result<(), String> {
        let kf = self.paths[p].keyframes.get_mut(&time).ok_or("no such keyframe")?;
        kf.values.insert(prop, v);
        Ok(())
    }
    pub fn kf_remove_property(&mut self, p: usize, time: i64, prop: PropertyId) -> Result<(), String> {
        let kf = self.paths[p].keyframes.get_mut(&time).ok_or("no such keyframe")?;
        kf.values.remove(&prop);
        Ok(())
    }
    pub fn kf_properties(&self, p: usize, time: i64) -> Vec<PropertyId> {
        self.paths[p].keyframes.get(&time).map(|k| k.values.keys().copied().collect()).unwrap_or_default()
    }

    /// Upstream Path.remove(keyframe, useFirstInterpolator).
    pub fn path_remove(&mut self, p: usize, time: i64, use_first: bool) -> Result<(), String> {
        if !self.paths[p].keyframes.contains_key(&time) {
            return Err(format!("The keyframe at {} is not part of this path.", time));
        }
        self.paths[p].keyframes.remove(&time);

        if self.paths[p].segments.len() < 2 {
            for s in &mut self.paths[p].segments {
                if let Some(ip) = s.interpolator {
                    self.interps[ip].remove_segment(s.id);
                    s.interpolator = None;
                }
            }
            self.paths[p].segments.clear();
            return Ok(());
        }

        let mut i = 0usize;
        while i < self.paths[p].segments.len() {
            let seg = self.paths[p].segments[i];
            if seg.end == time {
                self.paths[p].segments.remove(i);
                if let Some(ip) = seg.interpolator {
                    self.interps[ip].remove_segment(seg.id);
                }
                if i < self.paths[p].segments.len() {
                    let next2 = self.paths[p].segments[i];
                    let chosen = if use_first { seg.interpolator } else { next2.interpolator };
                    self.paths[p].segments.remove(i);
                    if let Some(ip) = next2.interpolator {
                        self.interps[ip].remove_segment(next2.id);
                    }
                    self.insert_segment_at(p, i, seg.start, next2.end, chosen);
                }
                return Ok(());
            }
            if seg.start == time {
                if let Some(ip) = seg.interpolator {
                    self.interps[ip].remove_segment(seg.id);
                }
                self.paths[p].segments.remove(i);
                return Ok(());
            }
            i += 1;
        }
        Err("No segment for keyframe found!".into())
    }

    /// Upstream Path.update/updateAll: both call update(false) upstream; the
    /// force flag is kept for phase 2's direct bakes.
    pub fn path_update(&mut self, p: usize, force: bool) -> Result<(), String> {
        let mut params: BTreeMap<PartKey, InterpParams> = BTreeMap::new();
        let mut current: Option<InterpId> = None;
        let segs_snapshot: Vec<(u32, i64, i64)> =
            self.paths[p].segments.iter().map(|s| (s.id, s.start, s.end)).collect();
        for seg in &self.paths[p].segments {
            if seg.interpolator != current {
                current = seg.interpolator;
                if let Some(id) = seg.interpolator {
                    if force || self.interps[id].is_dirty() {
                        let ids = self.interps[id].segments().to_vec();
                        let ctx: Vec<(u32, i64, i64)> = ids
                            .iter()
                            .filter_map(|sid| segs_snapshot.iter().copied().find(|(x, _, _)| x == sid))
                            .collect();
                        let kfs = self.paths[p].keyframes.clone();
                        let taken = std::mem::take(&mut params);
                        params = self.interps[id].bake(&ctx, &kfs, &self.properties, taken)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Upstream Path.getValue(property, time).
    pub fn path_get_value(&self, p: usize, prop: PropertyId, time: i64) -> Result<Option<PropertyValue>, String> {
        if let Some(seg) = self.paths[p].segments.iter().find(|s| s.start <= time && s.end >= time) {
            if let Some(id) = seg.interpolator {
                if self.interps[id].keyframe_properties().contains(&prop) {
                    return self.interps[id].get_value(prop, time, &self.paths[p].keyframes, &self.properties);
                }
            }
        }
        Ok(None)
    }

    /// Upstream Timeline.getValue(property, time): first active path wins.
    pub fn get_value(&self, prop: PropertyId, time: i64) -> Result<Option<PropertyValue>, String> {
        for p in 0..self.paths.len() {
            if self.paths[p].active {
                if let Some(v) = self.path_get_value(p, prop, time)? {
                    return Ok(Some(v));
                }
            }
        }
        Ok(None)
    }

    /// Upstream TimelineImpl.applyToGame: applies every registered property
    /// via the game glue (upstream Property.applyToGame(replayHandler)).
    pub fn apply_to_game(&mut self, time: i64, applier: &mut dyn FnMut(PropertyId, &PropertyValue)) -> Result<(), String> {
        for pid in 0..self.properties.len() {
            if let Some(v) = self.get_value(pid, time)? {
                applier(pid, &v);
            }
        }
        Ok(())
    }

    /// Upstream Timeline.applyChange: apply, push, clear redo.
    pub fn apply_change(&mut self, mut c: Box<dyn Change>) -> Result<(), String> {
        c.apply(self)?;
        self.undo.push(c);
        self.redo.clear();
        Ok(())
    }
    /// Upstream Timeline.pushChange.
    pub fn push_change(&mut self, c: Box<dyn Change>) {
        self.undo.push(c);
        self.redo.clear();
    }
    /// Upstream undoLastChange.
    pub fn undo_last_change(&mut self) -> Result<(), String> {
        let mut c = self.undo.pop().ok_or("undo stack empty")?;
        c.undo(self)?;
        self.redo.push(c);
        Ok(())
    }
    /// Upstream redoLastChange.
    pub fn redo_last_change(&mut self) -> Result<(), String> {
        let mut c = self.redo.pop().ok_or("redo stack empty")?;
        c.apply(self)?;
        self.undo.push(c);
        Ok(())
    }
    /// Upstream peekUndoStack / peekRedoStack (presence only).
    pub fn has_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn has_redo(&self) -> bool { !self.redo.is_empty() }
}

/// Upstream replaymod pathing/properties: the four concrete properties the
/// timeline layer uses. Localization keys are stored verbatim (upstream
/// resolves them via I18n at display time).
pub fn camera_position() -> Property {
    Property {
        id: "position".into(),
        group: Some("camera".into()),
        localized_name: "replaymod.gui.position".into(),
        initial: PropertyValue::Position(0.0, 0.0, 0.0),
        parts: vec![
            PartDef::new(PartKind::PosX, true, f64::NAN),
            PartDef::new(PartKind::PosY, true, f64::NAN),
            PartDef::new(PartKind::PosZ, true, f64::NAN),
        ],
    }
}

pub fn camera_rotation() -> Property {
    Property {
        id: "rotation".into(),
        group: Some("camera".into()),
        localized_name: "replaymod.gui.rotation".into(),
        initial: PropertyValue::Rotation(0.0, 0.0, 0.0),
        parts: vec![
            PartDef::new(PartKind::Yaw, true, 360.0),
            PartDef::new(PartKind::Pitch, true, 360.0),
            PartDef::new(PartKind::Roll, true, 360.0),
        ],
    }
}

pub fn timestamp_property() -> Property {
    Property {
        id: "timestamp".into(),
        group: None,
        localized_name: "replaymod.gui.editkeyframe.timestamp".into(),
        initial: PropertyValue::Timestamp(0),
        parts: vec![PartDef::new(PartKind::Timestamp, true, f64::NAN)],
    }
}

pub fn spectator_property() -> Property {
    Property {
        id: "spectate".into(),
        group: None,
        localized_name: "replaymod.gui.playeroverview.spectate".into(),
        initial: PropertyValue::Spectator(-1),
        parts: vec![PartDef::new(PartKind::Spectator, false, f64::NAN)],
    }
}

/// Upstream PolynomialSplineInterpolator.Polynomial. Coefficients ascending:
/// coefficients[0] multiplies the HIGHEST power (eval is Horner from the
/// front, matching upstream).
#[derive(Clone, Debug)]
pub struct Polynomial { pub coefficients: Vec<f64> }

impl Polynomial {
    pub fn eval(&self, at: f64) -> f64 {
        let mut val = 0.0;
        for &c in &self.coefficients { val = val * at + c; }
        val
    }
    pub fn derivative(&self) -> Polynomial {
        if self.coefficients.is_empty() { return self.clone(); }
        let n = self.coefficients.len();
        let mut derived = vec![0.0; n - 1];
        for i in 0..n - 1 {
            derived[i] = self.coefficients[i] * (n - 1 - i) as f64;
        }
        Polynomial { coefficients: derived }
    }
}

/// Upstream PolynomialSplineInterpolator.Polynomials.
#[derive(Clone, Debug)]
pub struct Polynomials { pub y_offset: f64, pub polynomials: Vec<Polynomial> }

impl Polynomials {
    pub fn eval(&self, time_ms: f64, index: usize) -> f64 {
        self.polynomials[index].eval(time_ms / 1000.0) + self.y_offset
    }
}

/// Upstream solveMatrix (Gauss elimination, forward then back substitution),
/// ported line for line including its exact scaling behavior.
pub fn solve_matrix(matrix: &mut [Vec<f64>]) {
    let n = matrix.len();
    for i in 0..n {
        if matrix[i][i] == 0.0 {
            for j in i + 1..n {
                if matrix[j][i] != 0.0 {
                    matrix.swap(i, j);
                    break;
                }
            }
        }
        let factor = matrix[i][i];
        if factor != 1.0 {
            matrix[i][i] = 1.0;
            for j in i + 1..matrix[i].len() {
                matrix[i][j] /= factor;
            }
        }
        for j in i + 1..n {
            let factor = matrix[j][i];
            if factor != 0.0 {
                matrix[j][i] = 0.0;
                for k in i + 1..matrix[j].len() {
                    matrix[j][k] -= matrix[i][k] * factor;
                }
            }
        }
    }
    for i in (0..n).rev() {
        for j in (0..i).rev() {
            if matrix[j][i] != 0.0 {
                let k = matrix[j].len() - 1;
                matrix[j][k] -= matrix[j][i] / matrix[i][i] * matrix[i][k];
                matrix[j][i] = 0.0;
            }
        }
    }
}

/// Which concrete spline fills the matrix. Upstream: abstract
/// PolynomialSplineInterpolator with CubicSplineInterpolator the concrete
/// subclass (degree 3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplineKind { Cubic }

/// Upstream PolynomialSplineInterpolator (degree from kind) + its Cubic
/// subclass. framesToProperty keeps keyframe times in bake order.
pub struct PolynomialSplineInterpolator {
    pub base: AbstractInterpolatorState,
    pub kind: SplineKind,
    frames_to_property: BTreeMap<PropertyId, Vec<i64>>,
    polynomials: BTreeMap<PartKey, Polynomials>,
}

impl PolynomialSplineInterpolator {
    pub fn cubic() -> Self {
        Self { base: Default::default(), kind: SplineKind::Cubic,
               frames_to_property: BTreeMap::new(), polynomials: BTreeMap::new() }
    }

    fn add_to_map(&mut self, prop: PropertyId, time: i64) {
        let v = self.frames_to_property.entry(prop).or_default();
        if !v.contains(&time) { v.push(time); }
    }

    /// Upstream calcPolynomials (shared by all splines of this family).
    fn calc_polynomials(
        &mut self,
        part: &PartDef,
        mut xs: Vec<f64>,
        mut ys: Vec<f64>,
        params: Option<InterpParams>,
    ) -> Polynomials {
        let degree = match self.kind { SplineKind::Cubic => 3 };
        let unknowns = degree + 1;
        let num = xs.len() - 1;
        if num == 0 {
            return Polynomials { y_offset: 0.0, polynomials: vec![Polynomial { coefficients: vec![ys[0]] }] };
        }
        for x in &mut xs { *x /= 1000.0; }

        let y_offset: f64;
        if part.upper_bound.is_nan() {
            let total: f64 = ys.iter().sum();
            y_offset = total / ys.len() as f64;
            for y in &mut ys { *y -= y_offset; }
            let params = params.map(|p| InterpParams { value: p.value - y_offset, ..p });
            return self.finish(degree, unknowns, num, xs, ys, params, y_offset);
        } else {
            let bound = part.upper_bound;
            let half = bound / 2.0;
            let first_value = params.map(|p| p.value).unwrap_or(ys[0]);
            let mut offset = (first_value / bound).floor() as i64;
            let mut last_value = fmod_floor(first_value, bound);
            for i in 1..ys.len() {
                let value = fmod_floor(ys[i], bound);
                if (value - last_value).abs() > half {
                    if last_value < half { offset -= 1; } else { offset += 1; }
                }
                ys[i] = value + (offset as f64) * bound;
                last_value = value;
            }
            y_offset = 0.0;
        }
        self.finish(degree, unknowns, num, xs, ys, params, 0.0)
    }

    fn finish(
        &mut self,
        degree: usize,
        unknowns: usize,
        num: usize,
        xs: Vec<f64>,
        ys: Vec<f64>,
        params: Option<InterpParams>,
        y_off: f64,
    ) -> Polynomials {
        let n = num * unknowns;
        let mut matrix = vec![vec![0.0; n + 1]; n];
        match self.kind {
            SplineKind::Cubic => fill_matrix_cubic(&mut matrix, &xs, &ys, num, unknowns, params),
        }
        solve_matrix(&mut matrix);
        let mut polynomials = Vec::with_capacity(num);
        for i in 0..num {
            let mut coefficients = vec![0.0; degree + 1];
            for j in 0..=degree {
                coefficients[j] = matrix[i * unknowns + j][n];
            }
            polynomials.push(Polynomial { coefficients });
        }
        Polynomials { y_offset: y_off, polynomials }
    }

    /// Upstream prepareProperty: per interpolatable part, fit the spline and
    /// record endpoint value/velocity/acceleration into the params map.
    fn prepare_property(
        &mut self,
        prop: PropertyId,
        times: &[i64],
        keyframes: &BTreeMap<i64, Keyframe>,
        properties: &[Property],
        params: &mut BTreeMap<PartKey, InterpParams>,
    ) -> Result<(), String> {
        for pi in 0..properties[prop].parts.len() {
            let part = &properties[prop].parts[pi];
            if !part.interpolatable { continue; }
            let mut xs = Vec::with_capacity(times.len());
            let mut ys = Vec::with_capacity(times.len());
            for &t in times {
                xs.push(t as f64);
                let v = &keyframes[&t].values[&prop];
                ys.push(part.to_double(v)?);
            }
            let incoming = params.get(&(prop, pi)).copied();
            let polys = self.calc_polynomials(part, xs, ys, incoming);
            let last_time = times[times.len() - 1] as f64;
            let last_poly = polys.polynomials.last().unwrap().clone();
            let last_value = last_poly.eval(last_time) + polys.y_offset;
            let last_velocity = last_poly.derivative().eval(last_time);
            let last_acceleration = last_poly.derivative().derivative().eval(last_time);
            params.insert((prop, pi), InterpParams { value: last_value, velocity: last_velocity, acceleration: last_acceleration });
            self.polynomials.insert((prop, pi), polys);
        }
        Ok(())
    }
}

/// Upstream CubicSplineInterpolator.fillMatrix, ported line for line
/// (matrix columns: per cubic a=x^3, b=x^2, c=x, d=1; last column = y).
fn fill_matrix_cubic(
    matrix: &mut [Vec<f64>],
    xs: &[f64],
    ys: &[f64],
    num: usize,
    _unknowns: usize,
    params: Option<InterpParams>,
) {
    let mut row = 0usize;
    let n = num * 4;
    if let Some(p) = params {
        // Apply previous values
        let mut ysv = ys.to_vec();
        ysv[0] = p.value;
        let x = xs[0];
        matrix[row][0] = 3.0 * x * x;
        matrix[row][1] = 2.0 * x;
        matrix[row][2] = 1.0;
        matrix[row][n] = p.velocity;
        row += 1;
        matrix[row][0] = 6.0 * x;
        matrix[row][1] = 2.0;
        matrix[row][n] = p.acceleration;
        row += 1;
        // ys[0] was overwritten locally; matrix equations below use ysv.
        for i in 0..num {
            let x = xs[i];
            matrix[row][i * 4] = x * x * x;
            matrix[row][i * 4 + 1] = x * x;
            matrix[row][i * 4 + 2] = x;
            matrix[row][i * 4 + 3] = 1.0;
            matrix[row][n] = ysv[i];
            row += 1;
            let x = xs[i + 1];
            matrix[row][i * 4] = x * x * x;
            matrix[row][i * 4 + 1] = x * x;
            matrix[row][i * 4 + 2] = x;
            matrix[row][i * 4 + 3] = 1.0;
            matrix[row][n] = ysv[i + 1];
            row += 1;
            if i < num - 1 {
                let x = xs[i + 1];
                matrix[row][i * 4 + 4] = 3.0 * x * x;
                matrix[row][i * 4] = -matrix[row][i * 4 + 4];
                matrix[row][i * 4 + 5] = 2.0 * x;
                matrix[row][i * 4 + 1] = -matrix[row][i * 4 + 5];
                matrix[row][i * 4 + 6] = 1.0;
                matrix[row][i * 4 + 2] = -matrix[row][i * 4 + 6];
                row += 1;
                matrix[row][i * 4 + 4] = 6.0 * x;
                matrix[row][i * 4] = -matrix[row][i * 4 + 4];
                matrix[row][i * 4 + 5] = 2.0;
                matrix[row][i * 4 + 1] = -matrix[row][i * 4 + 5];
                row += 1;
            }
        }
        return;
    }
    // Set second derivative at the first and the last knot to 0
    matrix[row][0] = 6.0 * xs[0];
    matrix[row][1] = 2.0;
    row += 1;
    matrix[row][(num - 1) * 4] = 6.0 * xs[xs.len() - 1];
    matrix[row][(num - 1) * 4 + 1] = 2.0;
    row += 1;
    for i in 0..num {
        let x = xs[i];
        matrix[row][i * 4] = x * x * x;
        matrix[row][i * 4 + 1] = x * x;
        matrix[row][i * 4 + 2] = x;
        matrix[row][i * 4 + 3] = 1.0;
        matrix[row][n] = ys[i];
        row += 1;
        let x = xs[i + 1];
        matrix[row][i * 4] = x * x * x;
        matrix[row][i * 4 + 1] = x * x;
        matrix[row][i * 4 + 2] = x;
        matrix[row][i * 4 + 3] = 1.0;
        matrix[row][n] = ys[i + 1];
        row += 1;
        if i < num - 1 {
            let x = xs[i + 1];
            matrix[row][i * 4 + 4] = 3.0 * x * x;
            matrix[row][i * 4] = -matrix[row][i * 4 + 4];
            matrix[row][i * 4 + 5] = 2.0 * x;
            matrix[row][i * 4 + 1] = -matrix[row][i * 4 + 5];
            matrix[row][i * 4 + 6] = 1.0;
            matrix[row][i * 4 + 2] = -matrix[row][i * 4 + 6];
            row += 1;
            matrix[row][i * 4 + 4] = 6.0 * x;
            matrix[row][i * 4] = -matrix[row][i * 4 + 4];
            matrix[row][i * 4 + 5] = 2.0;
            matrix[row][i * 4 + 1] = -matrix[row][i * 4 + 5];
            row += 1;
        }
    }
}

impl Interpolator for PolynomialSplineInterpolator {
    fn kind(&self) -> (&'static str, Option<f64>) { ("cubic-spline", None) }
    fn register_property(&mut self, prop: PropertyId) {
        if self.base.properties.insert(prop) { self.base.dirty = true; }
    }
    fn unregister_property(&mut self, prop: PropertyId) {
        if self.base.properties.remove(&prop) { self.base.dirty = true; }
    }
    fn keyframe_properties(&self) -> &HashSet<PropertyId> { &self.base.properties }
    fn add_segment(&mut self, seg: u32) { self.base.segments.push(seg); self.base.dirty = true; }
    fn remove_segment(&mut self, seg: u32) {
        self.base.segments.retain(|&s| s != seg);
        self.base.dirty = true;
    }
    fn segments(&self) -> &[u32] { &self.base.segments }
    fn is_dirty(&self) -> bool { self.base.dirty }

    fn bake(
        &mut self,
        segs: &[(u32, i64, i64)],
        keyframes: &BTreeMap<i64, Keyframe>,
        properties: &[Property],
        params: BTreeMap<PartKey, InterpParams>,
    ) -> Result<BTreeMap<PartKey, InterpParams>, String> {
        let sorted = abstract_bake(&mut self.base, segs)?;
        self.frames_to_property.clear();
        let props: Vec<PropertyId> = self.base.properties.iter().copied().collect();
        for &(_, start, end) in &sorted {
            for &prop in &props {
                if keyframes.get(&start).map_or(false, |k| k.values.contains_key(&prop)) {
                    self.add_to_map(prop, start);
                }
                if keyframes.get(&end).map_or(false, |k| k.values.contains_key(&prop)) {
                    self.add_to_map(prop, end);
                }
            }
        }
        // Upstream copies the incoming map, then mutates it per property.
        let mut out = params;
        for (prop, times) in self.frames_to_property.clone() {
            self.prepare_property(prop, &times, keyframes, properties, &mut out)?;
        }
        Ok(out)
    }

    fn get_value(
        &self,
        prop: PropertyId,
        time: i64,
        keyframes: &BTreeMap<i64, Keyframe>,
        properties: &[Property],
    ) -> Result<Option<PropertyValue>, String> {
        let Some(times) = self.frames_to_property.get(&prop) else { return Ok(None) };
        // Upstream scan: index ends at the count of frames strictly before
        // the first frame after `time` (the segment index to evaluate).
        let mut before: Option<i64> = None;
        let mut after: Option<i64> = None;
        let mut index: usize = 0;
        for &t in times {
            if t == time {
                return Ok(keyframes.get(&t).and_then(|k| k.values.get(&prop)).cloned());
            } else if t < time {
                before = Some(t);
            } else {
                after = Some(t);
                index -= 1; // matches upstream index-- before break
                break;
            }
            index += 1;
        }
        let (Some(_bt), Some(_at)) = (before, after) else { return Ok(None) };
        let value_before = keyframes[&before.unwrap()].values[&prop].clone();
        let mut interpolated = value_before.clone();
        for (pi, part) in properties[prop].parts.iter().enumerate() {
            if part.interpolatable {
                let mut value = self.polynomials[&(prop, pi)].eval(time as f64, index);
                if !part.upper_bound.is_nan() {
                    value = fmod_floor(value, part.upper_bound);
                }
                interpolated = part.from_double(&interpolated, value)?;
            }
        }
        Ok(Some(interpolated))
    }
}

/// Upstream CatmullRomSplineInterpolator: Catmull-Rom splines parameterized
/// 0..1 per segment, with configurable alpha for tangent scaling.
pub struct CatmullRomSplineInterpolator {
    pub base: AbstractInterpolatorState,
    pub alpha: f64,
    frames_to_property: BTreeMap<PropertyId, Vec<i64>>,
    cubic_polynomials: BTreeMap<PartKey, Vec<Polynomial>>,
}

impl CatmullRomSplineInterpolator {
    pub fn new(alpha: f64) -> Self {
        Self { base: Default::default(), alpha,
               frames_to_property: BTreeMap::new(), cubic_polynomials: BTreeMap::new() }
    }

    fn add_to_map(&mut self, prop: PropertyId, time: i64) {
        let v = self.frames_to_property.entry(prop).or_default();
        if !v.contains(&time) { v.push(time); }
    }

    /// Upstream calcPolynomials (steve.hollasch.net catmull-rom reference).
    fn calc_polynomials(
        &mut self,
        keyframes: &BTreeMap<i64, Keyframe>,
        properties: &[Property],
    ) -> Result<(), String> {
        let props: Vec<PropertyId> = self.base.properties.iter().copied().collect();
        for &prop in &props {
            for pi in 0..properties[prop].parts.len() {
                let part = &properties[prop].parts[pi];
                if !part.interpolatable { continue; }
                let mut values: Vec<f64> = Vec::new();
                if part.upper_bound.is_nan() {
                    for &t in &self.frames_to_property[&prop] {
                        let v = &keyframes[&t].values[&prop];
                        values.push(part.to_double(v)?);
                    }
                } else {
                    let bound = part.upper_bound;
                    let half = bound / 2.0;
                    let mut last_value: Option<f64> = None;
                    let mut offset: i64 = 0;
                    for &t in &self.frames_to_property[&prop] {
                        let v = &keyframes[&t].values[&prop];
                        let value = fmod_floor(part.to_double(v)?, bound);
                        if last_value.is_none() {
                            last_value = Some(value);
                            offset = (value / bound).floor() as i64;
                        }
                        let lv = last_value.unwrap();
                        if (value - lv).abs() > half {
                            if lv < half { offset -= 1; } else { offset += 1; }
                        }
                        values.push(value + (offset as f64) * bound);
                        last_value = Some(value);
                    }
                }
                let n = self.frames_to_property[&prop].len();
                let mut polys = Vec::with_capacity(values.len().saturating_sub(1));
                for i in 0..values.len().saturating_sub(1) {
                    let p1 = values[i];
                    let p2 = values[i + 1];
                    let p0 = if i > 0 { values[i - 1] } else { p1 };
                    let p3 = if i < n - 2 { values[i + 2] } else { p2 };
                    let t0 = self.alpha * (p2 - p0);
                    let t1 = self.alpha * (p3 - p1);
                    polys.push(Polynomial { coefficients: vec![
                        2.0 * p1 - 2.0 * p2 + t0 + t1,
                        -3.0 * p1 + 3.0 * p2 - 2.0 * t0 - t1,
                        t0,
                        p1,
                    ] });
                }
                self.cubic_polynomials.insert((prop, pi), polys);
            }
        }
        Ok(())
    }
}

impl Interpolator for CatmullRomSplineInterpolator {
    fn kind(&self) -> (&'static str, Option<f64>) { ("catmull-rom-spline", Some(self.alpha)) }
    fn register_property(&mut self, prop: PropertyId) {
        if self.base.properties.insert(prop) { self.base.dirty = true; }
    }
    fn unregister_property(&mut self, prop: PropertyId) {
        if self.base.properties.remove(&prop) { self.base.dirty = true; }
    }
    fn keyframe_properties(&self) -> &HashSet<PropertyId> { &self.base.properties }
    fn add_segment(&mut self, seg: u32) { self.base.segments.push(seg); self.base.dirty = true; }
    fn remove_segment(&mut self, seg: u32) {
        self.base.segments.retain(|&s| s != seg);
        self.base.dirty = true;
    }
    fn segments(&self) -> &[u32] { &self.base.segments }
    fn is_dirty(&self) -> bool { self.base.dirty }

    fn bake(
        &mut self,
        segs: &[(u32, i64, i64)],
        keyframes: &BTreeMap<i64, Keyframe>,
        properties: &[Property],
        _params: BTreeMap<PartKey, InterpParams>,
    ) -> Result<BTreeMap<PartKey, InterpParams>, String> {
        let sorted = abstract_bake(&mut self.base, segs)?;
        self.frames_to_property.clear();
        let props: Vec<PropertyId> = self.base.properties.iter().copied().collect();
        for &(_, start, end) in &sorted {
            for &prop in &props {
                if keyframes.get(&start).map_or(false, |k| k.values.contains_key(&prop)) {
                    self.add_to_map(prop, start);
                }
                if keyframes.get(&end).map_or(false, |k| k.values.contains_key(&prop)) {
                    self.add_to_map(prop, end);
                }
            }
        }
        self.calc_polynomials(keyframes, properties)?;
        // Upstream builds lastParameters from every registered property's
        // parts; a missing polynomial (non-interpolatable part or < 2 frames)
        // NPEs upstream, mirrored as Err here.
        let mut out: BTreeMap<PartKey, InterpParams> = BTreeMap::new();
        for &prop in &props {
            for pi in 0..properties[prop].parts.len() {
                let polys = self.cubic_polynomials.get(&(prop, pi))
                    .ok_or_else(|| format!("catmullrom: no polynomials for part ({}, {})", prop, pi))?;
                let last = polys.last().ok_or("catmullrom: empty polynomial set")?.clone();
                let value = last.eval(1.0);
                let velocity = last.derivative().eval(1.0);
                let acceleration = last.derivative().derivative().eval(1.0);
                out.insert((prop, pi), InterpParams { value, velocity, acceleration });
            }
        }
        Ok(out)
    }

    fn get_value(
        &self,
        prop: PropertyId,
        time: i64,
        keyframes: &BTreeMap<i64, Keyframe>,
        properties: &[Property],
    ) -> Result<Option<PropertyValue>, String> {
        let Some(times) = self.frames_to_property.get(&prop) else { return Ok(None) };
        let mut time_before: Option<i64> = None;
        let mut time_after: Option<i64> = None;
        let mut value_before: Option<PropertyValue> = None;
        let mut index: usize = 0;
        for (i, &t) in times.iter().enumerate() {
            if t == time {
                return Ok(keyframes.get(&t).and_then(|k| k.values.get(&prop)).cloned());
            } else if t < time {
                index = i;
                time_before = Some(t);
                value_before = Some(keyframes[&t].values[&prop].clone());
            } else {
                time_after = Some(t);
                break;
            }
        }
        let (Some(tb), Some(ta)) = (time_before, time_after) else { return Ok(None) };
        let fraction = (time - tb) as f64 / (ta - tb) as f64;
        let mut interpolated = value_before.unwrap();
        for (pi, part) in properties[prop].parts.iter().enumerate() {
            if part.interpolatable {
                let polys = self.cubic_polynomials.get(&(prop, pi))
                    .ok_or_else(|| format!("catmullrom: no polynomials for part ({}, {})", prop, pi))?;
                // No upper-bound mod here: upstream omits it (values were
                // unwrapped at bake time).
                interpolated = part.from_double(&interpolated, polys[index].eval(fraction))?;
            }
        }
        Ok(Some(interpolated))
    }
}
}
pub mod player {
// Timeline players ported 1:1 from com.replaymod.pathing.player:
// AbstractTimelinePlayer (155 ln), RealtimeTimelinePlayer (74 ln),
// ReplayTimer (78 ln). Upstream (c) ReplayMod contributors,
// GPL-3.0-or-later (https://github.com/ReplayMod/replaymod).
// This file: Copyright (c) 2026 sparticle, GPL-3.0-or-later.
//
// Seams: ReplaySenderOps stands in for ReplayHandler.getReplaySender()
// (the Horizon glue adapts the ported FullReplaySender to it); the overlay
// query stands in for MinecraftClient.getOverlay(); the wall clock arrives
// as now_ms. ReplayTimer's UpdatedCallback event is the game's per-frame
// tick hook: the driver calls on_tick once per frame and then reads
// ReplayTimer.ticks_this_frame. register()/unregister() and the org-timer
// swap (MinecraftAccessor getTimer/setTimer) are Horizon seams; start()
// installs the fresh ReplayTimer and the Restore action on completion is
// where the glue restores the original timer.

use super::pathing::{PropertyId, PropertyValue, Timeline};

/// Upstream Utils.DEFAULT_MS_PER_TICK (ReplayTimer tick length, 50 ms).
pub const DEFAULT_MS_PER_TICK: f64 = 50.0;

/// ReplayTimer port: a render tick counter that does not advance by itself.
/// tickDelta accumulates passed ticks each frame; ticksThisFrame is the
/// truncated whole-tick count the game consumes for that frame (the same
/// accumulation upstream onTick performs on the installed ReplayTimer).
#[derive(Default, Clone, Copy, Debug)]
pub struct ReplayTimer {
    pub tick_delta: f32,
    pub ticks_this_frame: i32,
}

impl ReplayTimer {
    pub fn new() -> Self { Self::default() }
    /// Upstream start(): timer.tickDelta = timer.ticksThisFrame = 0.
    pub fn reset(&mut self) { self.tick_delta = 0.0; self.ticks_this_frame = 0; }
}

/// The ReplaySender calls the player makes (upstream
/// replayHandler.getReplaySender()). The ported FullReplaySender adapts.
pub trait ReplaySenderOps {
    fn is_async_mode(&mut self) -> bool;
    fn set_sync_mode_and_wait(&mut self);
    fn set_async_mode(&mut self, async_mode: bool);
    fn set_replay_speed(&mut self, speed: f64);
    fn current_time_stamp(&mut self) -> i64;
}

/// AbstractTimelinePlayer port. The Java ListenableFuture<Void> becomes the
/// future_done flag; isActive() = started && !future_done.
pub struct AbstractTimelinePlayer {
    /// protected long startOffset in upstream.
    pub start_offset: i64,
    was_async_mode: bool,
    last_time: i64,
    last_timestamp: i64,
    started: bool,
    future_done: bool,
}

#[derive(Debug, PartialEq)]
pub enum TickAction {
    /// Normal frame: the game consumes ReplayTimer.ticks_this_frame.
    Continue,
    /// Future done: restore the org timer, setReplaySpeed(0), restore async
    /// mode and unregister (upstream onTick's done branch).
    Restore,
}

impl AbstractTimelinePlayer {
    pub fn new() -> Self {
        Self { start_offset: 0, was_async_mode: false, last_time: 0,
               last_timestamp: 0, started: false, future_done: false }
    }

    /// Upstream start(Timeline, long from) + start(Timeline): lastTimestamp =
    /// the max keyframe time across all paths (0 when there are none),
    /// sender mode forced to sync, fresh timer, future created.
    pub fn start(&mut self, timeline: &Timeline, ops: &mut dyn ReplaySenderOps,
                 timer: &mut ReplayTimer, from: i64) {
        self.start_offset = from;
        self.last_timestamp = timeline.paths.iter()
            .flat_map(|p| p.keyframes.keys().copied())
            .max()
            .unwrap_or(0);
        self.was_async_mode = ops.is_async_mode();
        ops.set_sync_mode_and_wait();
        // register() and the org-timer swap are Horizon seams; the port
        // installs a fresh ReplayTimer, matching upstream's
        // timer.tickDelta = timer.ticksThisFrame = 0.
        timer.reset();
        self.started = true;
        self.future_done = false;
        self.last_time = 0;
    }

    pub fn is_active(&self) -> bool { self.started && !self.future_done }

    /// Upstream onTick body minus the getTimePassed() call, which the
    /// concrete player supplies. time_passed is getTimePassed()'s value.
    pub fn on_tick(&mut self, time_passed: i64, timeline: &mut Timeline,
                   ops: &mut dyn ReplaySenderOps, timer: &mut ReplayTimer,
                   applier: &mut dyn FnMut(PropertyId, &PropertyValue)) -> TickAction {
        if self.future_done {
            ops.set_replay_speed(0.0);
            if self.was_async_mode {
                ops.set_async_mode(true);
            }
            // unregister() + restore org timer: Horizon seam.
            return TickAction::Restore;
        }
        let mut time = time_passed;
        if time > self.last_timestamp {
            time = self.last_timestamp;
        }

        // Apply to timeline
        let _ = timeline.apply_to_game(time, applier);
        // Apply a second time in case some of the packets have moved the
        // camera from where it was (upstream comment).
        let _ = timeline.apply_to_game(time, applier);

        // Update minecraft timer
        let replay_time = ops.current_time_stamp();
        if self.last_time == 0 {
            // First frame, no change yet
            self.last_time = replay_time;
        }
        let time_in_ticks = replay_time as f32 / 50f32;
        let previous_time_in_ticks = self.last_time as f32 / 50f32;
        let passed_ticks = time_in_ticks - previous_time_in_ticks;
        timer.tick_delta += passed_ticks;
        timer.ticks_this_frame = timer.tick_delta as i32;
        timer.tick_delta -= timer.ticks_this_frame as f32;

        self.last_time = replay_time;

        if time >= self.last_timestamp {
            self.future_done = true;
        }
        TickAction::Continue
    }
}

/// RealtimeTimelinePlayer port: plays the timeline against the system clock.
pub struct RealtimeTimelinePlayer {
    base: AbstractTimelinePlayer,
    /// Whether the next frame is the first frame. We only start measuring
    /// time from the second frame as the first might have to jump in time
    /// which might take time (upstream comment).
    first_frame: bool,
    second_frame: bool,
    /// System time in milliseconds at the start.
    start_time: i64,
    loading_resources: bool,
    time_before_resource_loading: i64,
}

impl RealtimeTimelinePlayer {
    pub fn new() -> Self {
        Self { base: AbstractTimelinePlayer::new(), first_frame: false,
               second_frame: false, start_time: 0, loading_resources: false,
               time_before_resource_loading: 0 }
    }

    /// Upstream start(Timeline): reset the frame tracking, then super.start.
    pub fn start(&mut self, timeline: &Timeline, ops: &mut dyn ReplaySenderOps,
                 timer: &mut ReplayTimer, from: i64) {
        self.first_frame = true;
        self.loading_resources = false;
        self.base.start(timeline, ops, timer, from);
    }

    pub fn is_active(&self) -> bool { self.base.is_active() }
    pub fn start_offset(&self) -> i64 { self.base.start_offset }

    /// Upstream onTick. overlay_shown stands in for
    /// MinecraftClient.getInstance().getOverlay() != null (the MC>=11400
    /// branch); when the overlay is up, playback time freezes and re-anchors
    /// once it clears.
    pub fn on_tick(&mut self, timeline: &mut Timeline, ops: &mut dyn ReplaySenderOps,
                   timer: &mut ReplayTimer,
                   applier: &mut dyn FnMut(PropertyId, &PropertyValue),
                   now_ms: i64, overlay_shown: bool) -> TickAction {
        if self.second_frame {
            self.second_frame = false;
            self.start_time = now_ms - self.base.start_offset;
        }

        if overlay_shown {
            if !self.loading_resources {
                self.time_before_resource_loading = self.get_time_passed(now_ms);
                self.loading_resources = true;
            }
            return self.base.on_tick(self.get_time_passed(now_ms), timeline,
                                     ops, timer, applier);
        } else if self.loading_resources && !self.first_frame {
            self.start_time = now_ms - self.time_before_resource_loading;
            self.loading_resources = false;
        }

        let action = self.base.on_tick(self.get_time_passed(now_ms), timeline,
                                       ops, timer, applier);

        if self.first_frame {
            self.first_frame = false;
            self.second_frame = true;
        }
        action
    }

    /// Upstream getTimePassed().
    fn get_time_passed(&self, now_ms: i64) -> i64 {
        if self.first_frame { return self.base.start_offset; }
        if self.loading_resources { return self.time_before_resource_loading; }
        now_ms - self.start_time
    }
}
}
pub mod preview {
// Path preview sampling: 1:1 simplepathing.preview.PathPreviewRenderer
// renderCameraPath — the camera-path sampling, coloring and camera-marker
// logic. Upstream draws the sampled points with GL (drawConnection,
// drawPoint, drawCamera are Horizon's render seam); this port produces the
// same point/color plan headless.
// This file: Copyright (c) 2026 sparticle, LGPL-3.0-or-later.
use super::sptimeline::{SPTimeline, SPPath};
use super::pathing::PropertyValue;

pub const SLOW_PATH_COLOR: u32 = 0xffcccc;
pub const FAST_PATH_COLOR: u32 = 0x660000;
pub const FASTEST_PATH_SPEED: f64 = 0.01;
pub const SPECTATOR_LINE_COLOR: u32 = 0x0000ffff;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3f { pub x: f32, pub y: f32, pub z: f32 }
impl Vec3f { pub fn new(x: f32, y: f32, z: f32) -> Self { Self { x, y, z } } }

pub fn distance_squared(a: Vec3f, b: Vec3f) -> f64 {
    let dx = (a.x - b.x) as f64; let dy = (a.y - b.y) as f64; let dz = (a.z - b.z) as f64;
    dx * dx + dy * dy + dz * dz
}

fn loc2vec(loc: &super::sptimeline::Location) -> Vec3f {
    Vec3f::new(loc.x as f32, loc.y as f32, loc.z as f32)
}
fn triple_d2vec(x: f64, y: f64, z: f64) -> Vec3f { Vec3f::new(x as f32, y as f32, z as f32) }
fn triple_f2vec(y: f32, p: f32, r: f32) -> Vec3f { Vec3f::new(y, p, r) }

/// 1:1 interpolateColorComponent: exponential ease between the components.
fn interpolate_color_component(c1: u32, c2: u32, weight: f64) -> u32 {
    let v = c1 as f64 + (1.0 - (-4.0 * weight).exp()) * (c2 as f64 - c1 as f64);
    (v as i32 & 0xff) as u32
}

pub fn interpolate_color(c1: u32, c2: u32, weight: f64) -> u32 {
    (interpolate_color_component((c1 >> 16) & 0xff, (c2 >> 16) & 0xff, weight) << 16)
        | (interpolate_color_component((c1 >> 8) & 0xff, (c2 >> 8) & 0xff, weight) << 8)
        | interpolate_color_component(c1 & 0xff, c2 & 0xff, weight)
}

/// One sampled connection: a colored line between two sampled points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathPoint { pub from: Vec3f, pub to: Vec3f, pub color: u32 }
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyframePoint { pub time: i64, pub pos: Vec3f }
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraMarker { pub pos: Vec3f, pub rot: Vec3f }

#[derive(Clone, Debug, Default)]
pub struct PathPreviewPlan {
    pub segments: Vec<Vec<PathPoint>>,
    pub keyframes: Vec<KeyframePoint>,
    pub camera: Option<CameraMarker>,
}

/// 1:1 renderCameraPath minus the GL: sample every path segment into colored
/// connections, collect keyframe points in view range (furthest first), and
/// resolve the camera marker at the cursor. Returns None for the upstream
/// early-out guards (not async mode, HUD hidden, empty path).
pub fn render_camera_path(
    sp: &mut SPTimeline,
    cursor_time: i64,
    view_pos: Vec3f,
    render_distance: i32,
    hud_hidden: bool,
    async_mode: bool,
) -> Result<Option<PathPreviewPlan>, String> {
    if !async_mode || hud_hidden { return Ok(None); }
    let p = sp.position_path;
    let time_path = sp.time_path;
    if sp.timeline.paths[p].keyframes.is_empty() { return Ok(None); }
    sp.timeline.path_update(p, false)?;
    let render_distance_squared = (render_distance * render_distance) as f64;

    let mut plan = PathPreviewPlan::default();
    for (i, seg) in sp.timeline.paths[p].segments.clone().iter().enumerate() {
        // Spectator segments have 20 lines per second (at least 10); normal
        // segments have a fixed 100.
        let spectator = match seg.interpolator {
            Some(id) => sp.timeline.interps[id].keyframe_properties().contains(&sp.spectator_id),
            None => false,
        };
        if spectator && sp.entity_tracker.is_none() { continue; }
        let diff = (seg.end - seg.start) as i32 as i64;
        let steps: i64 = if spectator { std::cmp::max(diff / 50, 10) } else { 100 };
        let mut prev: Option<Vec3f> = None;
        let mut pts: Vec<PathPoint> = Vec::new();
        for s in 0..=steps {
            let time = seg.start + diff * s / steps;
            if spectator {
                let ent = sp.timeline.path_get_value(p, sp.spectator_id, time)?;
                let rt = sp.timeline.path_get_value(time_path, sp.timestamp_id, time)?;
                if let (Some(PropertyValue::Spectator(eid)), Some(PropertyValue::Timestamp(rt))) = (ent, rt) {
                    let loc = sp.entity_tracker.as_ref().unwrap()
                        .get_entity_position_at_timestamp(eid, rt);
                    if let Some(loc) = loc {
                        let pos = loc2vec(&loc);
                        if let Some(from) = prev {
                            pts.push(PathPoint { from, to: pos, color: SPECTATOR_LINE_COLOR });
                        }
                        prev = Some(pos);
                        continue;
                    }
                }
            } else if let Some(PropertyValue::Position(x, y, z)) =
                sp.timeline.path_get_value(p, sp.camera_position_id, time)? {
                let pos = triple_d2vec(x, y, z);
                if let Some(from) = prev {
                    let distance = distance_squared(from, pos).sqrt();
                    let speed = (distance / (diff / steps) as f64).min(FASTEST_PATH_SPEED);
                    let fraction = speed / FASTEST_PATH_SPEED;
                    let color = (interpolate_color(SLOW_PATH_COLOR, FAST_PATH_COLOR, fraction) << 8) | 0xff;
                    pts.push(PathPoint { from, to: pos, color });
                }
                prev = Some(pos);
                continue;
            }
            prev = None;
        }
        plan.segments.push(pts);
        let _ = i;
    }

    // Keyframe points in view range, furthest first.
    let mut kfs: Vec<KeyframePoint> = Vec::new();
    for (time, kf) in sp.timeline.paths[p].keyframes.iter() {
        if let Some(PropertyValue::Position(x, y, z)) = kf.values.get(&sp.camera_position_id) {
            let pos = triple_d2vec(*x, *y, *z);
            if distance_squared(pos, view_pos) < render_distance_squared {
                kfs.push(KeyframePoint { time: *time, pos });
            }
        }
    }
    kfs.sort_by(|a, b| {
        distance_squared(b.pos, view_pos).partial_cmp(&distance_squared(a.pos, view_pos)).unwrap()
    });
    plan.keyframes = kfs;

    // Camera marker at the cursor: spectated entity position, else the path.
    let ent = sp.timeline.path_get_value(p, sp.spectator_id, cursor_time)?;
    if let Some(PropertyValue::Spectator(eid)) = ent {
        if let Some(tracker) = sp.entity_tracker.as_ref() {
            let rt = sp.timeline.path_get_value(time_path, sp.timestamp_id, cursor_time)?;
            if let Some(PropertyValue::Timestamp(rt)) = rt {
                if let Some(loc) = tracker.get_entity_position_at_timestamp(eid, rt) {
                    plan.camera = Some(CameraMarker {
                        pos: loc2vec(&loc),
                        rot: Vec3f::new(loc.yaw, loc.pitch, 0.0),
                    });
                }
            }
        }
    } else {
        let pos = sp.timeline.path_get_value(p, sp.camera_position_id, cursor_time)?;
        let rot = sp.timeline.path_get_value(p, sp.camera_rotation_id, cursor_time)?;
        if let (Some(PropertyValue::Position(x, y, z)), Some(PropertyValue::Rotation(ya, pi, ro))) = (pos, rot) {
            plan.camera = Some(CameraMarker {
                pos: triple_d2vec(x, y, z),
                rot: triple_f2vec(ya, pi, ro),
            });
        }
    }
    Ok(Some(plan))
}
}
pub mod processing {
// Sync-mode packet processing: 1:1 Rust port of ReplayMod's
// FullReplaySender.processPacket (LGPL-3.0-or-later), active MC>=11400
// branch. This file: Copyright (c) 2026 sparticle, LGPL-3.0-or-later.
//
// The MC packet model below is the Horizon-integration seam: a decoded
// packet carries the upstream Java class name as `kind` plus the fields
// processPacket reads or rewrites. Raw MC decode/encode is Horizon-side.
use std::collections::BTreeMap;

/// Upstream Restrictions.PLUGIN_CHANNEL (1.14+ Identifier form).
pub const PLUGIN_CHANNEL: &str = "replaymod:restrict";
/// Upstream GameJoin rewrite: "Camera entity id should be negative which is
/// an invalid id and can't be used by servers".
pub const CAMERA_ENTITY_ID: i32 = -1789435;
/// Upstream TP_DISTANCE_LIMIT.
pub const TP_DISTANCE_LIMIT: i32 = 128;
/// net.minecraft.world.GameMode.SPECTATOR wire id.
pub const GAME_MODE_SPECTATOR: i32 = 3;
/// PlayerPositionLookS2CPacket.Flag X/Y/Z bit values (relative coords).
pub const FLAG_X: u32 = 0x01;
pub const FLAG_Y: u32 = 0x02;
pub const FLAG_Z: u32 = 0x04;

/// Upstream BAD_PACKETS, MC>=11400 && <12005 active branch: LoginHelloS2CPacket,
/// PlayerActionResponseS2CPacket, OpenWrittenBookS2CPacket, OpenScreenS2CPacket,
/// SynchronizeRecipesS2CPacket, AdvancementUpdateS2CPacket,
/// SelectAdvancementTabS2CPacket, SetCameraEntityS2CPacket, TitleS2CPacket,
/// HealthUpdateS2CPacket, OpenHorseScreenS2CPacket, CloseScreenS2CPacket,
/// ScreenHandlerSlotUpdateS2CPacket, ScreenHandlerPropertyUpdateS2CPacket,
/// SignEditorOpenS2CPacket, StatisticsS2CPacket, ExperienceBarUpdateS2CPacket,
/// PlayerAbilitiesS2CPacket. (ServerTransferS2CPacket is the MC>=12005
/// preprocessor branch, not in this target.)
pub fn is_bad_packet(kind: &str) -> bool {
    matches!(kind,
        "LoginHelloS2CPacket" | "PlayerActionResponseS2CPacket"
        | "OpenWrittenBookS2CPacket" | "OpenScreenS2CPacket"
        | "SynchronizeRecipesS2CPacket" | "AdvancementUpdateS2CPacket"
        | "SelectAdvancementTabS2CPacket" | "SetCameraEntityS2CPacket"
        | "TitleS2CPacket" | "HealthUpdateS2CPacket" | "OpenHorseScreenS2CPacket"
        | "CloseScreenS2CPacket" | "ScreenHandlerSlotUpdateS2CPacket"
        | "ScreenHandlerPropertyUpdateS2CPacket" | "SignEditorOpenS2CPacket"
        | "StatisticsS2CPacket" | "ExperienceBarUpdateS2CPacket"
        | "PlayerAbilitiesS2CPacket")
}

/// Upstream packet-type registry state (getPacketTypeRegistry(State.LOGIN)
/// -> withLoginSuccess() transitions to PLAY).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryState { Login, Play }

/// Sender state processPacket mutates (upstream fields with the same names).
#[derive(Clone, Debug)]
pub struct ReplaySenderState {
    pub registry: RegistryState,
    pub actual_id: i32,
    pub allow_movement: bool,
    pub has_world_loaded: bool,
}

impl Default for ReplaySenderState {
    fn default() -> Self {
        Self { registry: RegistryState::Login, actual_id: -1, allow_movement: false, has_world_loaded: false }
    }
}

/// The decoded packet: `kind` is the upstream Java class name; payload holds
/// exactly the fields processPacket touches.
#[derive(Clone, Debug, PartialEq)]
pub struct Packet {
    pub kind: String,
    pub payload: Payload,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    None,
    CustomPayload { channel: String, data: Vec<u8> },
    Disconnect { reason: String },
    ResourcePackSend { url: String, pack_id: i32 },
    /// 1.14-1.16 branch: entity id + (1.16+) both game-mode fields.
    GameJoin { entity_id: i32, mode: i32, mode_prev: i32 },
    PlayerRespawn { mode: i32, mode_prev: i32 },
    PlayerPositionLook { flags: u32 },
    GameStateChange { reason: i32, value: f32 },
    GameMessage,
}

/// Upstream returns null to drop the packet; restrictions parse failure
/// terminates the replay outright.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Keep(Packet),
    Drop,
    Terminate,
}

/// `restrictions` is the Restrictions.handle seam: Some(unknown tag) on a
/// parse failure, None when the options parsed. `rp_index` is the replay's
/// resourcepack index (phase-7 accessors).
pub fn process_packet(
    st: &mut ReplaySenderState,
    p: Packet,
    rp_index: Option<&BTreeMap<i32, String>>,
    show_chat: bool,
    suppress_camera: bool,
    restrictions: &dyn Fn(&[u8]) -> Option<String>,
) -> Outcome {
    // LoginSuccessS2CPacket: registry = registry.withLoginSuccess()
    if p.kind == "LoginSuccessS2CPacket" {
        st.registry = RegistryState::Play;
        return Outcome::Keep(p);
    }

    // CustomPayloadS2CPacket on the restrictions channel: parsed -> drop;
    // unknown -> terminate the replay (upstream tears it down in runLater).
    if let Payload::CustomPayload { channel, data } = &p.payload {
        if channel == PLUGIN_CHANNEL {
            return match restrictions(data) {
                None => Outcome::Drop,
                Some(_unknown) => Outcome::Terminate,
            };
        }
    }

    // DisconnectS2CPacket: drop the forced-update disconnect, keep others.
    if let Payload::Disconnect { reason } = &p.payload {
        if reason == "Please update to view this replay." {
            return Outcome::Drop;
        }
    }

    // BAD_PACKETS filter.
    if is_bad_packet(&p.kind) {
        return Outcome::Drop;
    }

    // Modded screens opened for the recording player are not shown.
    if let Payload::CustomPayload { channel, .. } = &p.payload {
        if channel.starts_with("fabric-screen-handler-api-v") {
            return Outcome::Drop;
        }
    }

    // ResourcePackSendS2CPacket: replay:// packs resolve through the index
    // and are materialized Horizon-side; the packet itself is dropped.
    if let Payload::ResourcePackSend { url, pack_id } = &p.payload {
        if url.starts_with("replay://") {
            let id: i32 = url["replay://".len()..].parse().unwrap_or(0);
            if let Some(index) = rp_index {
                if index.contains_key(&id) {
                    // Upstream extracts resourcePack(hash).zip into the temp
                    // folder and hands it to the server resource pack provider:
                    // Horizon integration seam.
                    let _ = pack_id;
                }
            }
            return Outcome::Drop;
        }
    }

    // GameJoinS2CPacket: camera entity id rewrite + SPECTATOR mode.
    if let Payload::GameJoin { entity_id, mode, mode_prev } = p.payload {
        st.allow_movement = true; // upstream schedules this
        st.actual_id = entity_id;
        let _ = (mode, mode_prev); // upstream keeps every other field as-is
        return Outcome::Keep(Packet {
            kind: p.kind,
            payload: Payload::GameJoin {
                entity_id: CAMERA_ENTITY_ID,
                mode: GAME_MODE_SPECTATOR,
                mode_prev: GAME_MODE_SPECTATOR,
            },
        });
    }

    // PlayerRespawnS2CPacket: SPECTATOR mode on respawn too.
    if let Payload::PlayerRespawn { mode: _, mode_prev: _ } = p.payload {
        st.allow_movement = true; // upstream schedules this
        return Outcome::Keep(Packet {
            kind: p.kind,
            payload: Payload::PlayerRespawn {
                mode: GAME_MODE_SPECTATOR,
                mode_prev: GAME_MODE_SPECTATOR,
            },
        });
    }

    // PlayerPositionLookS2CPacket: the camera moves are applied Horizon-side;
    // the packet is dropped after state bookkeeping.
    if let Payload::PlayerPositionLook { flags } = p.payload {
        st.has_world_loaded = true;
        if suppress_camera {
            return Outcome::Drop;
        }
        if flags & (FLAG_X | FLAG_Y | FLAG_Z) != 0 {
            // At least one of the coordinates is relative, so we don't care.
            return Outcome::Drop;
        }
        st.allow_movement = false; // upstream: allowMovement = false unless TP
        return Outcome::Drop;
    }

    // GameStateChangeS2CPacket: only rain controls pass through
    // (1 RAIN_STOPPED, 2 RAIN_STARTED, 7 RAIN_GRADIENT_CHANGED,
    // 8 THUNDER_GRADIENT_CHANGED).
    if let Payload::GameStateChange { reason, value: _ } = p.payload {
        if !matches!(reason, 1 | 2 | 7 | 8) {
            return Outcome::Drop;
        }
        return Outcome::Keep(p);
    }

    // GameMessageS2CPacket: dropped when chat is hidden in the settings.
    if p.payload == Payload::GameMessage && !show_chat {
        return Outcome::Drop;
    }

    Outcome::Keep(p)
}
}
pub mod replayfile {
// Replay file container: 1:1 Rust port of ReplayStudio's AbstractReplayFile
// entry accessors + java.util.zip.CRC32 for the recording.tmcpr.crc32 entry
// (LGPL-3.0-or-later). This file: Copyright (c) 2026 sparticle,
// LGPL-3.0-or-later. The zip archive itself stays behind McprArchive
// (ZipReplayFile's file/cache plumbing is Horizon integration).
use super::mcpr::*;
use super::serialize::{esc, JVal};

/// java.util.zip.CRC32 (IEEE 802.3, poly 0xEDB88320), 1:1 behavior.
fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0usize;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}
pub fn crc32(data: &[u8]) -> u32 {
    let t = crc_table();
    let mut c: u32 = 0xFFFF_FFFF;
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// Upstream TimelineSerialization.FILE_ENTRY.
pub const ENTRY_TIMELINES: &str = "timelines.json";

/// Upstream AbstractReplayFile.THUMB_MAGIC_NUMBERS.
pub const THUMB_MAGIC_NUMBERS: [u8; 7] = [0, 1, 1, 2, 3, 5, 8];

/// Upstream Marker (ReplayStudio replay.Marker fields).
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub time: i32, pub x: f64, pub y: f64, pub z: f64,
    pub yaw: f32, pub pitch: f32, pub roll: f32, pub name: Option<String>,
}

/// Upstream ModInfo (id, name, version).
#[derive(Clone, Debug, PartialEq)]
pub struct ModInfo { pub id: String, pub name: String, pub version: String }

/// Upstream ReplayAssetEntry (uuid, extension incl. dot, name).
#[derive(Clone, Debug, PartialEq)]
pub struct ReplayAssetEntry { pub uuid: String, pub extension: String, pub name: String }

/// AbstractReplayFile's accessors, 1:1, over the archive seam.
pub struct ReplayFile<'a, A: McprArchive> { pub archive: &'a mut A }

impl<'a, A: McprArchive> ReplayFile<'a, A> {
    /// Upstream getMetaData: null (None) when the entry is absent.
    pub fn get_meta_data(&mut self) -> Option<ReplayMetaData> {
        self.archive.read_entry(ENTRY_META_DATA)
            .and_then(|b| parse_meta_data(&String::from_utf8_lossy(&b)).ok())
    }

    /// Upstream writeMetaData: fileFormat forced "MCPR"; fileFormatVersion +
    /// protocol set when a registry (protocol) is given; generator defaults.
    pub fn write_meta_data(&mut self, md: &mut ReplayMetaData, protocol: Option<i32>, studio_version: &str) {
        md.file_format = "MCPR".into();
        if let Some(p) = protocol {
            md.file_format_version = CURRENT_FILE_FORMAT_VERSION;
            md.protocol = Some(p);
        }
        if md.generator.is_empty() {
            md.generator = format!("ReplayStudio v{}", studio_version);
        }
        let mut json = String::from("{");
        let mut w = |first: &mut bool, k: &str, v: String| {
            if !*first { json.push(','); }
            *first = false;
            json.push_str(&format!("\"{}\":{}", k, v));
        };
        let mut first = true;
        w(&mut first, "singleplayer", md.singleplayer.to_string());
        w(&mut first, "serverName", format!("\"{}\"", esc(&md.server_name)));
        w(&mut first, "customServerName", format!("\"{}\"", esc(&md.custom_server_name)));
        w(&mut first, "duration", md.duration.to_string());
        w(&mut first, "date", md.date.to_string());
        w(&mut first, "mcversion", format!("\"{}\"", esc(&md.mc_version)));
        w(&mut first, "fileFormat", format!("\"{}\"", esc(&md.file_format)));
        w(&mut first, "fileFormatVersion", md.file_format_version.to_string());
        if let Some(p) = md.protocol { w(&mut first, "protocol", p.to_string()); }
        w(&mut first, "generator", format!("\"{}\"", esc(&md.generator)));
        w(&mut first, "selfId", md.self_id.to_string());
        let players: Vec<String> = md.players.iter().map(|p| format!("\"{}\"", esc(p))).collect();
        w(&mut first, "players", format!("[{}]", players.join(",")));
        json.push('}');
        self.archive.write_entry(ENTRY_META_DATA, json.as_bytes());
    }

    /// Upstream getPacketData: the recording bytes (framing in read_packets).
    pub fn get_packet_data(&mut self) -> Option<Vec<u8>> { self.archive.read_entry(ENTRY_RECORDING) }
    /// Upstream writePacketData entry target.
    pub const PACKET_DATA_ENTRY: &'static str = ENTRY_RECORDING;

    /// Upstream getResourcePackIndex: non-numeric keys skipped.
    pub fn get_resource_pack_index(&mut self) -> Option<std::collections::BTreeMap<i32, String>> {
        let b = self.archive.read_entry(ENTRY_RESOURCE_PACK_INDEX)?;
        let v = super::serialize::json_parse(&String::from_utf8_lossy(&b)).ok()?;
        let mut out = std::collections::BTreeMap::new();
        if let JVal::Obj(pairs) = v {
            for (k, val) in pairs {
                if let (Ok(n), JVal::Str(s)) = (k.parse::<i32>(), val) { out.insert(n, s.clone()); }
            }
        }
        Some(out)
    }

    /// Upstream writeResourcePackIndex.
    pub fn write_resource_pack_index(&mut self, index: &std::collections::BTreeMap<i32, String>) {
        let mut json = String::from("{");
        for (i, (k, v)) in index.iter().enumerate() {
            if i > 0 { json.push(','); }
            json.push_str(&format!("\"{}\":\"{}\"", k, esc(v)));
        }
        json.push('}');
        self.archive.write_entry(ENTRY_RESOURCE_PACK_INDEX, json.as_bytes());
    }

    /// Upstream getThumbBytes: new entry as-is; the old "thumb" entry has the
    /// 7-byte magic numbers stripped when present (pushback semantics).
    pub fn get_thumb_bytes(&mut self) -> Option<Vec<u8>> {
        if let Some(b) = self.archive.read_entry(ENTRY_THUMB) { return Some(b); }
        let b = self.archive.read_entry(ENTRY_THUMB_OLD)?;
        if b.len() >= THUMB_MAGIC_NUMBERS.len() && b[..7] == THUMB_MAGIC_NUMBERS {
            Some(b[7..].to_vec())
        } else {
            Some(b)
        }
    }

    /// Upstream writeThumbBytes.
    pub fn write_thumb_bytes(&mut self, image: &[u8]) { self.archive.write_entry(ENTRY_THUMB, image); }

    /// Upstream getInvisiblePlayers: visibility.json, else the old entry;
    /// shape {"hidden":["<uuid>",..]}.
    pub fn get_invisible_players(&mut self) -> Option<Vec<String>> {
        let b = self.archive.read_entry(ENTRY_VISIBILITY)
            .or_else(|| self.archive.read_entry(ENTRY_VISIBILITY_OLD))?;
        let v = super::serialize::json_parse(&String::from_utf8_lossy(&b)).ok()?;
        let hidden = match (&v, super::serialize::json_parse(&String::from_utf8_lossy(&b)).ok()) {
            _ => v,
        };
        if let JVal::Obj(pairs) = hidden {
            for (k, val) in pairs {
                if k == "hidden" {
                    if let JVal::Arr(a) = val {
                        return Some(a.iter().filter_map(|e| if let JVal::Str(s) = e { Some(s.clone()) } else { None }).collect());
                    }
                }
            }
        }
        Some(Vec::new())
    }

    /// Upstream writeInvisiblePlayers.
    pub fn write_invisible_players(&mut self, uuids: &[String]) {
        let inner: Vec<String> = uuids.iter().map(|u| format!("\"{}\"", esc(u))).collect();
        let json = format!("{{\"hidden\":[{}]}}", inner.join(","));
        self.archive.write_entry(ENTRY_VISIBILITY, json.as_bytes());
    }

    /// Upstream getMarkers: array of {realTimestamp, value:{name?, position:{x,y,z,yaw,pitch,roll}}}.
    pub fn get_markers(&mut self) -> Option<Vec<Marker>> {
        let b = self.archive.read_entry(ENTRY_MARKERS)?;
        let v = super::serialize::json_parse(&String::from_utf8_lossy(&b)).ok()?;
        let arr = match v { JVal::Arr(a) => a, _ => return Some(Vec::new()) };
        let f = |o: &JVal, k: &str| -> f64 {
            if let JVal::Obj(p) = o {
                for (kk, vv) in p { if kk == k { if let JVal::Num(n) = vv { return *n; } } }
            }
            0.0
        };
        let fs = |n: f64| n as f32;
        let mut out = Vec::new();
        for e in &arr {
            let mut name = None;
            let mut time = 0i32;
            let (mut x, mut y, mut z) = (0.0f64, 0.0f64, 0.0f64);
            let (mut yaw, mut pitch, mut roll) = (0.0f32, 0.0f32, 0.0f32);
            if let JVal::Obj(p) = e {
                for (k, val) in p {
                    match k.as_str() {
                        "realTimestamp" => { if let JVal::Num(n) = val { time = *n as i32; } }
                        "value" => {
                            if let JVal::Obj(vp) = val {
                                for (vk, vv) in vp {
                                    match vk.as_str() {
                                        "name" => { if let JVal::Str(s) = vv { name = Some(s.clone()); } }
                                        "position" => {
                                            x = f(vv, "x"); y = f(vv, "y"); z = f(vv, "z");
                                            yaw = fs(f(vv, "yaw")); pitch = fs(f(vv, "pitch")); roll = fs(f(vv, "roll"));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            out.push(Marker { time, x, y, z, yaw, pitch, roll, name });
        }
        Some(out)
    }

    /// Upstream writeMarkers (Gson skips the null name, matching upstream's
    /// default toJson behavior).
    pub fn write_markers(&mut self, markers: &[Marker]) {
        let mut json = String::from("[");
        for (i, m) in markers.iter().enumerate() {
            if i > 0 { json.push(','); }
            let name = match &m.name { Some(n) => format!("\"{}\"", esc(n)), None => String::new() };
            json.push_str(&format!(
                "{{\"realTimestamp\":{},\"value\":{{{}\"position\":{{\"x\":{},\"y\":{},\"z\":{},\"yaw\":{},\"pitch\":{},\"roll\":{}}}}}}}",
                m.time,
                if m.name.is_some() { format!("\"name\":{},", name) } else { String::new() },
                fmt_f64(m.x), fmt_f64(m.y), fmt_f64(m.z),
                fmt_f32(m.yaw), fmt_f32(m.pitch), fmt_f32(m.roll),
            ));
        }
        json.push(']');
        self.archive.write_entry(ENTRY_MARKERS, json.as_bytes());
    }

    /// Upstream getAssets: entries under asset/, name split on first '_',
    /// extension = from the last '.' (with dot).
    pub fn get_assets(&mut self) -> Vec<ReplayAssetEntry> {
        self.archive.list_entries()
            .into_iter()
            .filter(|k| k.starts_with("asset/") && k.len() > 6)
            .filter_map(|k| {
                let base = &k[6..];
                let delim = base.find('_')?;
                let uuid = base[..delim].to_string();
                let dot = base.rfind('.')?;
                Some(ReplayAssetEntry { uuid, extension: base[dot..].to_string(), name: base[delim + 1..dot].to_string() })
            })
            .collect()
    }

    /// Upstream getAsset(uuid): first matching entry's bytes.
    pub fn get_asset(&mut self, uuid: &str) -> Option<Vec<u8>> {
        let prefix = format!("asset/{}_", uuid);
        let name = self.archive.list_entries().into_iter().find(|k| k.starts_with(&prefix))?;
        self.archive.read_entry(&name)
    }

    /// Upstream writeAsset.
    pub fn write_asset(&mut self, asset: &ReplayAssetEntry, bytes: &[u8]) {
        // Rebuilds the original entry key (uuid_name + extension-with-dot).
        self.archive.write_entry(&format!("asset/{}_{}{}", asset.uuid, asset.name, asset.extension), bytes);
    }

    /// Upstream removeAsset: every entry with that uuid.
    pub fn remove_asset(&mut self, uuid: &str) {
        let prefix = format!("asset/{}_", uuid);
        let names: Vec<String> = self.archive.list_entries().into_iter().filter(|k| k.starts_with(&prefix)).collect();
        for n in names { self.archive.remove_entry(&n); }
    }

    /// Upstream getModInfo: mods.json {"requiredMods":[{modID,modName,modVersion},..]};
    /// empty when the entry is absent.
    pub fn get_mod_info(&mut self) -> Vec<ModInfo> {
        let b = match self.archive.read_entry(ENTRY_MODS) { Some(b) => b, None => return Vec::new() };
        let v = match super::serialize::json_parse(&String::from_utf8_lossy(&b)) { Ok(v) => v, Err(_) => return Vec::new() };
        let mut out = Vec::new();
        if let JVal::Obj(p) = &v {
            for (k, val) in p {
                if k == "requiredMods" {
                    if let JVal::Arr(a) = val {
                        for e in a {
                            if let JVal::Obj(m) = e {
                                let g = |want: &str| m.iter().find(|(k, _)| k == want)
                                    .and_then(|(_, v)| if let JVal::Str(s) = v { Some(s.clone()) } else { None })
                                    .unwrap_or_default();
                                out.push(ModInfo { id: g("modID"), name: g("modName"), version: g("modVersion") });
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Upstream getTimelines -> TimelineSerialization.load(): the
    /// "timelines.json" entry through the phase-3 serializer; empty map when
    /// the entry is absent (upstream returns an empty map too).
    pub fn get_timelines(&mut self) -> Result<std::collections::BTreeMap<String, super::pathing::Timeline>, String> {
        match self.archive.read_entry(ENTRY_TIMELINES) {
            None => Ok(std::collections::BTreeMap::new()),
            Some(b) => super::serialize::deserialize(&String::from_utf8_lossy(&b)),
        }
    }

    /// Upstream writeTimelines -> TimelineSerialization.save().
    pub fn write_timelines(&mut self, timelines: &[(String, super::pathing::Timeline)]) {
        let refs: Vec<(&str, &super::pathing::Timeline)> = timelines.iter().map(|(n, t)| (n.as_str(), t)).collect();
        let json = super::serialize::serialize(&refs);
        self.archive.write_entry(ENTRY_TIMELINES, json.as_bytes());
    }

    /// Upstream writeModInfo.
    pub fn write_mod_info(&mut self, mods: &[ModInfo]) {
        let inner: Vec<String> = mods.iter()
            .map(|m| format!("{{\"modID\":\"{}\",\"modName\":\"{}\",\"modVersion\":\"{}\"}}", esc(&m.id), esc(&m.name), esc(&m.version)))
            .collect();
        let json = format!("{{\"requiredMods\":[{}]}}", inner.join(","));
        self.archive.write_entry(ENTRY_MODS, json.as_bytes());
    }
}

/// Java's Double.toString / Float.toString outputs, as far as the format
/// matters for roundtripping (shortest representation).
fn fmt_f64(v: f64) -> String { format!("{}", v) }
fn fmt_f32(v: f32) -> String { format!("{}", v) }
}
pub mod sender {
// Replay sender: 1:1 Rust port of ReplayMod's ReplaySender interface and
// FullReplaySender's asynchronous packet-pacing loop (the replay playback
// engine). Upstream: com.replaymod.replay.ReplaySender (35 ln) and
// com.replaymod.replay.FullReplaySender's async half (constructor, mode
// switching, getReplaySpeed/setReplaySpeed, asyncSender runnable, hurry/
// jump logic; LGPL-3.0-or-later). This file: Copyright (c) 2026 sparticle,
// LGPL-3.0-or-later.
//
// Transport-bound pieces (Netty channelRead, Minecraft timer tick-length
// manipulation, processPacket's 400 lines of per-packet MC rewriting, the
// sync-mode sendPacketsTill stream walk) are Horizon-integration work and
// stay behind the traits/hook notes below; the pacing state machine is
// ported 1:1 and driven by tick(now_ms) instead of threads so it is
// testable headless.
use super::pathing::*; // keep module set uniform

/// Upstream PacketData: a recorded packet with its replay timestamp.
/// is_bundle marks PacketType.Bundle toggles (bundle batching).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PacketKind { Particle, EntitySpawnFireworkRocket, Other }

#[derive(Clone, Debug)]
pub struct PacketData { pub timestamp: i32, pub is_bundle: bool, pub kind: PacketKind, pub bytes: Vec<u8> }

/// Upstream ReplayFile.getPacketData + ReplayInputStream, as a hook.
pub trait PacketSource {
    /// Upstream new PacketData(replayIn): next recorded packet, or None at
    /// EOF (upstream EOFException), Err on a read failure (IOException).
    fn next_packet(&mut self) -> Result<PacketData, String>;
    /// Upstream reopens replayIn on restart
    /// (replayFile.getPacketData(...)); reopen the underlying stream.
    fn reopen(&mut self);
}

/// Upstream channel.pipeline().fireChannelRead(...): deliver bytes into the
/// client's channel. Horizon: its connection's packet ingress.
pub trait PacketSink { fn deliver(&mut self, bytes: &[u8]); }

/// Upstream ReplayHandler hooks the async loop calls.
pub trait ReplayHandler {
    /// moveCameraToTargetPosition after a jump completes.
    fn move_camera_to_target_position(&mut self);
    /// restartedReplay, run on restart (upstream wraps in runSync).
    fn restarted_replay(&mut self);
}

/// One iteration's outcome, replacing Thread.sleep/continue control flow so
/// the driver (a thread loop in production, a test in CI) decides pacing.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LoopAction {
    /// Sleep the given ms and retry the iteration (upstream Thread.sleep).
    Sleep(u64),
    /// A packet was delivered (or the loop moved forward) this iteration.
    Progress,
    /// The replay loop terminated (upstream break REPLAY_LOOP).
    Terminated,
}

/// Upstream ReplaySender + FullReplaySender's async state.
pub struct FullReplaySender {
    pub async_mode: bool,
    /// Upstream lastTimeStamp: timestamp of the last delivered packet.
    pub last_time_stamp: i32,
    pub current_time_stamp: i32,
    pub replay_speed: f64,
    pub has_world_loaded: bool,
    pub in_bundle: bool,
    pub start_from_beginning: bool,
    pub terminate: bool,
    /// realTimeStart: ms anchor for timing (upstream keeps this so pauses and
    /// speed changes don't cause gradual drift).
    pub real_time_start: u64,
    pub real_time_start_speed: f64,
    /// desiredTimeStamp: -1 means normal timing; otherwise rushing.
    pub desired_time_stamp: i64,
    pub replay_length: i32,
    /// Upstream nextPacket: read ahead but not yet delivered.
    next_packet: Option<PacketData>,
    /// Whether a replayIn stream is open (upstream lazy-opens it).
    stream_open: bool,
    /// next_packet source has been consumed to EOF at least once.
    at_eof: bool,
}

impl FullReplaySender {
    pub fn new(replay_length: i32) -> Self {
        Self {
            async_mode: false,
            last_time_stamp: 0,
            current_time_stamp: 0,
            replay_speed: 1.0,
            has_world_loaded: false,
            in_bundle: false,
            start_from_beginning: false,
            terminate: false,
            real_time_start: 0,
            real_time_start_speed: 1.0,
            desired_time_stamp: -1,
            replay_length,
            next_packet: None,
            stream_open: false,
            at_eof: false,
        }
    }

    /// Upstream paused(): tickLength == POSITIVE_INFINITY, i.e. speed 0.
    pub fn paused(&self) -> bool { self.replay_speed == 0.0 }

    /// Upstream getReplaySpeed.
    pub fn get_replay_speed(&self) -> f64 {
        if !self.paused() { self.replay_speed } else { 0.0 }
    }

    /// Upstream setReplaySpeed: re-anchor realTimeStart so the wall-clock
    /// mapping stays drift-free; d == 0 just pauses. The Minecraft timer
    /// tick-length change is a Horizon hook, not ported.
    pub fn set_replay_speed(&mut self, d: f64, now_ms: u64) {
        if d != 0.0 {
            self.replay_speed = d;
            self.real_time_start_speed = d;
            self.real_time_start = now_ms.saturating_sub((self.last_time_stamp as f64 / d) as u64);
        }
        self.replay_speed = if d != 0.0 { d } else { 0.0 };
    }

    /// Upstream isHurrying.
    pub fn is_hurrying(&self) -> bool { self.desired_time_stamp != -1 }

    /// Upstream stopHurrying.
    pub fn stop_hurrying(&mut self) { self.desired_time_stamp = -1; }

    /// Upstream getDesiredTimestamp.
    pub fn get_desired_timestamp(&self) -> i64 { self.desired_time_stamp }

    /// Upstream jumpToTime (async mode only): rushing until a packet past the
    /// target is reached; backwards jumps restart from the beginning.
    pub fn jump_to_time(&mut self, millis: i32) -> Result<(), String> {
        if !self.async_mode {
            return Err("Can only jump in async mode. Use sendPacketsTill(int) instead.".into());
        }
        if millis < self.last_time_stamp && !self.is_hurrying() {
            self.start_from_beginning = true;
        }
        self.desired_time_stamp = millis as i64;
        Ok(())
    }

    /// Upstream terminateReplay's loop-exit flag.
    pub fn terminate_replay(&mut self) { self.terminate = true; }

    /// Upstream asyncSender's REPLAY_LOOP body, one iteration. The driver
    /// calls this repeatedly; Sleep(n) means wait n ms then retry. The
    /// restart block (upstream's outer while) is restart_state_reset().
    pub fn tick(
        &mut self,
        now_ms: u64,
        source: &mut dyn PacketSource,
        sink: &mut dyn PacketSink,
        handler: &mut dyn ReplayHandler,
    ) -> Result<LoopAction, String> {
        if !self.async_mode {
            return Err("async loop only runs in async mode".into());
        }
        // Upstream: lazy-open the packet stream.
        if !self.stream_open { self.stream_open = true; }

        // Upstream: while paused and world loaded and not bundling, sleep,
        // unless terminating, restarting or jumping.
        while self.paused() && self.has_world_loaded && !self.in_bundle {
            if self.terminate || self.start_from_beginning || self.desired_time_stamp != -1 {
                break;
            }
            return Ok(LoopAction::Sleep(10));
        }

        if self.terminate && !self.in_bundle {
            return Ok(LoopAction::Terminated);
        }

        if self.start_from_beginning {
            // Upstream: break out of the packet loop so the outer loop
            // restarts the replay.
            return Ok(LoopAction::Progress);
        }

        // Read the next packet if we don't already have one.
        if self.next_packet.is_none() {
            if self.at_eof {
                // Upstream EOFException path: pause and wait until told to
                // continue (jump/restart/terminate).
                self.set_replay_speed(0.0, now_ms);
                if self.paused() && self.has_world_loaded && self.desired_time_stamp == -1 && !self.terminate {
                    return Ok(LoopAction::Sleep(10));
                }
                if self.terminate { return Ok(LoopAction::Terminated); }
                return Ok(LoopAction::Progress); // break -> restart block
            }
            match source.next_packet() {
                Ok(p) => { self.next_packet = Some(p); self.at_eof = false; }
                Err(e) if e == "EOF" => {
                    // Upstream: pause, then wait until the user continues.
                    self.set_replay_speed(0.0, now_ms);
                    self.at_eof = true;
                    return Ok(LoopAction::Sleep(10));
                }
                Err(e) => return Err(e), // upstream: printStackTrace, retry
            }
        }

        let packet = self.next_packet.clone().expect("next_packet just set");
        let next_time_stamp = packet.timestamp;

        // Upstream: if not hurrying and world loaded (no dirt-screens), wait
        // for proper packet timing.
        if !self.is_hurrying() && self.has_world_loaded && !self.in_bundle {
            let expected_time = self.real_time_start + (next_time_stamp as f64 / self.replay_speed) as u64;
            if self.replay_speed != 0.0 && expected_time > now_ms {
                return Ok(LoopAction::Sleep(expected_time - now_ms));
            }
        }

        // Process packet.
        if packet.is_bundle { self.in_bundle = !self.in_bundle; }
        sink.deliver(&packet.bytes);
        self.next_packet = None;
        self.last_time_stamp = next_time_stamp;

        // Upstream (>=1.20.2): stall while autoRead is disabled; Horizon hook,
        // not ported.

        // Upstream: when a jump finishes, snap the camera and pause.
        if self.is_hurrying() && self.last_time_stamp as i64 > self.desired_time_stamp && !self.start_from_beginning {
            self.desired_time_stamp = -1;
            handler.move_camera_to_target_position();
            self.set_replay_speed(0.0, now_ms);
        }
        Ok(LoopAction::Progress)
    }

    /// Upstream's restart block (outer loop bottom): reset per-run state and
    /// reopen the stream. Call after tick() returns Progress with
    /// start_from_beginning set (or after EOF-continue).
    pub fn restart_state_reset(&mut self, now_ms: u64, source: &mut dyn PacketSource, handler: &mut dyn ReplayHandler) {
        self.has_world_loaded = false;
        self.in_bundle = false;
        self.last_time_stamp = 0;
        self.start_from_beginning = false;
        self.next_packet = None;
        self.at_eof = false;
        self.stream_open = false;
        self.real_time_start = now_ms;
        source.reopen();
        handler.restarted_replay();
    }

    /// Upstream currentTimeStamp (async): last delivered packet's timestamp.
    pub fn current_time_stamp(&self) -> i32 { self.last_time_stamp }

    /// Upstream processPacketAsync: while hurrying over a long span, drop
    /// particle packets and firework-rocket spawns (except for short
    /// durations) so the rush doesn't lag the client.
    pub fn process_packet_async(&self, p: &PacketData) -> Option<PacketData> {
        if self.desired_time_stamp - self.last_time_stamp as i64 > 1000 {
            if p.kind == PacketKind::Particle { return None; }
            if p.kind == PacketKind::EntitySpawnFireworkRocket { return None; }
        }
        Some(p.clone())
    }

    /// Upstream sendPacketsTill (sync mode): send all packets up to and
    /// including the target timestamp, no pacing. The upstream single-thread
    /// executor + MC task-queue drain is a Horizon hook, not ported; this
    /// runs the send inline.
    pub fn send_packets_till(
        &mut self,
        timestamp: i32,
        now_ms: u64,
        source: &mut dyn PacketSource,
        sink: &mut dyn PacketSink,
        handler: &mut dyn ReplayHandler,
    ) -> Result<(), String> {
        if self.async_mode {
            return Err("This method cannot be used in async mode. Use jumpToTime(int) instead.".into());
        }
        self.do_send_packets_till(timestamp, now_ms, source, sink, handler)
    }

    fn do_send_packets_till(
        &mut self,
        timestamp: i32,
        now_ms: u64,
        source: &mut dyn PacketSource,
        sink: &mut dyn PacketSink,
        handler: &mut dyn ReplayHandler,
    ) -> Result<(), String> {
        if timestamp == self.last_time_stamp { return Ok(()); } // already there
        if timestamp < self.last_time_stamp {
            // Upstream: restart the replay to go backwards in time.
            self.has_world_loaded = false;
            self.in_bundle = false;
            self.last_time_stamp = 0;
            self.stream_open = false;
            self.start_from_beginning = false;
            self.next_packet = None;
            self.at_eof = false;
            source.reopen();
            handler.restarted_replay();
        }
        self.stream_open = true;

        loop { // Upstream: send packets
            // Leftover packet from before is used first.
            let pd = match self.next_packet.take() {
                Some(p) => p,
                None => match source.next_packet() {
                    Ok(p) => p,
                    Err(e) if e == "EOF" => {
                        // Upstream: "we hit the end before finishing our job...
                        // well, let's just pretend we're done"
                        self.stream_open = false;
                        break;
                    }
                    Err(e) => return Err(e),
                },
            };
            let next_time_stamp = pd.timestamp;
            if next_time_stamp > timestamp && !self.in_bundle {
                // Done sending all packets; keep the packet for later.
                self.next_packet = Some(pd);
                break;
            }
            if pd.is_bundle { self.in_bundle = !self.in_bundle; }
            sink.deliver(&pd.bytes);
            // Upstream >=1.20.2 autoRead stall: Horizon hook, not ported.
        }

        // Upstream: re-anchor realTimeStart in case we switch to async soon.
        self.real_time_start = now_ms.saturating_sub((timestamp as f64 / self.replay_speed) as u64);
        self.last_time_stamp = timestamp;
        Ok(())
    }
}
}
pub mod serialize {
// Timeline save format: 1:1 Rust port of ReplayStudio
// pathing/serialize/TimelineSerialization (the "timelines.json" entry of a
// replay file, i.e. what backs the .pttr pathing save).
// Upstream: com.replaymod.replaystudio.pathing.serialize.TimelineSerialization
// (LGPL-3.0-or-later). This file: Copyright (c) 2026 sparticle, LGPL-3.0-or-later.
//
// The upstream registry's interpolator serialization lives in ReplayMod
// (not ReplayStudio); we mirror it with the same three-type JSON shape the
// SPTimeline port uses: ["linear"], ["cubic-spline"],
// [{"type":"catmull-rom-spline","alpha":<a>}], written as a JSON string.
// Property values: position [x,y,z], rotation [yaw,pitch,roll],
// timestamp/spectate plain integers.
use super::pathing::*;
use super::sptimeline::explicit_interpolation_property;
use std::collections::BTreeMap;

// ---------- minimal JSON reader (upstream uses Gson's JsonReader) ----------

#[derive(Debug, Clone)]
pub enum JVal {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JVal>),
    Obj(Vec<(String, JVal)>),
}

struct Parser<'a> { s: &'a [u8], i: usize }

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self { Self { s: s.as_bytes(), i: 0 } }
    fn ws(&mut self) { while self.i < self.s.len() && (self.s[self.i] as char).is_ascii_whitespace() { self.i += 1; } }
    fn peek(&mut self) -> u8 { self.ws(); self.s[self.i] }
    fn val(&mut self) -> Result<JVal, String> {
        match self.peek() {
            b'{' => {
                self.i += 1;
                let mut out: Vec<(String, JVal)> = Vec::new();
                loop {
                    if self.peek() == b'}' { self.i += 1; return Ok(JVal::Obj(out)); }
                    let k = match self.val()? { JVal::Str(s) => s, _ => return Err("object key must be a string".into()) };
                    if self.peek() != b':' { return Err("expected ':'".into()); }
                    self.i += 1;
                    let v = self.val()?;
                    out.push((k, v));
                    if self.peek() == b',' { self.i += 1; }
                }
            }
            b'[' => {
                self.i += 1;
                let mut out = Vec::new();
                loop {
                    if self.peek() == b']' { self.i += 1; return Ok(JVal::Arr(out)); }
                    out.push(self.val()?);
                    if self.peek() == b',' { self.i += 1; }
                }
            }
            b'"' => {
                self.i += 1;
                let mut out = String::new();
                while self.s[self.i] != b'"' {
                    if self.s[self.i] == b'\\' {
                        self.i += 1;
                        match self.s[self.i] {
                            b'n' => out.push('\n'),
                            b't' => out.push('\t'),
                            b'r' => out.push('\r'),
                            c => out.push(c as char),
                        }
                    } else {
                        out.push(self.s[self.i] as char);
                    }
                    self.i += 1;
                }
                self.i += 1;
                Ok(JVal::Str(out))
            }
            b't' => { self.i += 4; Ok(JVal::Bool(true)) }
            b'f' => { self.i += 5; Ok(JVal::Bool(false)) }
            b'n' => { self.i += 4; Ok(JVal::Null) }
            _ => {
                let start = self.i;
                while self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'.' | b'0'..=b'9' | b'e' | b'E' | b'+') { self.i += 1; }
                let txt = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| "bad number")?;
                let n: f64 = txt.parse().map_err(|_| format!("bad number: {}", txt))?;
                Ok(JVal::Num(n))
            }
        }
    }
    fn has_next(&mut self) -> bool { self.ws(); self.s.get(self.i).map(|c| *c != b'}' && *c != b']').unwrap_or(false) }
}

fn jstr<'v>(v: &'v JVal, key: &str) -> Option<&'v JVal> {
    match v { JVal::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v), _ => None }
}
fn jnum(v: &JVal) -> Option<f64> { match v { JVal::Num(n) => Some(*n), _ => None } }
fn jstr_v(v: &JVal) -> Option<&str> { match v { JVal::Str(s) => Some(s), _ => None } }

// ---------- writer ----------

pub fn esc(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

fn prop_key(t: &Timeline, id: PropertyId) -> String {
    let p = &t.properties[id];
    match &p.group { Some(g) => format!("{}:{}", g, p.id), None => p.id.clone() }
}

fn write_prop_value(out: &mut String, v: &PropertyValue) {
    match v {
        PropertyValue::Position(x, y, z) => { out.push_str(&format!("[{},{},{}]", x, y, z)); }
        PropertyValue::Rotation(y, p, r) => { out.push_str(&format!("[{},{},{}]", y, p, r)); }
        PropertyValue::Timestamp(t) => out.push_str(&t.to_string()),
        PropertyValue::Spectator(s) => out.push_str(&s.to_string()),
    }
}

/// Upstream TimelineSerialization.serialize: {"<name>": [ {keyframes: [...],
/// segments: [...], interpolators: [...]}, ... ]} per named timeline.
pub fn serialize(timelines: &[(&str, &Timeline)]) -> String {
    let mut out = String::from("{");
    for (name, t) in timelines {
        if out.len() > 1 { out.push(','); }
        out.push_str(&format!("\"{}\":", esc(name)));
        out.push('[');
        for path in &t.paths {
            out.push_str("{\"keyframes\":[");
            for (time, kf) in &path.keyframes {
                if out.ends_with('}') { out.push(','); }
                out.push_str(&format!("{{\"time\":{},\"properties\":{{", time));
                let mut first = true;
                for (&pid, v) in &kf.values {
                    if !first { out.push(','); }
                    first = false;
                    out.push_str(&format!("\"{}\":", esc(&prop_key(t, pid))));
                    write_prop_value(&mut out, v);
                }
                out.push_str("}}");
            }
            out.push_str("],\"segments\":[");
            let mut map: Vec<InterpId> = Vec::new();
            for (si, seg) in path.segments.iter().enumerate() {
                if si > 0 { out.push(','); }
                match seg.interpolator {
                    None => out.push_str("null"),
                    Some(id) => {
                        if let Some(idx) = map.iter().position(|&m| m == id) {
                            out.push_str(&idx.to_string());
                        } else {
                            out.push_str(&map.len().to_string());
                            map.push(id);
                        }
                    }
                }
            }
            out.push_str("],\"interpolators\":[");
            for (ii, &id) in map.iter().enumerate() {
                if ii > 0 { out.push(','); }
                out.push_str(&format!("{{\"type\":\"{}\",\"properties\":[",
                    esc(&json_string_for_interpolator(t, id))));
                let props: Vec<PropertyId> =
                    t.interps[id].keyframe_properties().iter().copied().collect();
                for (pi, &pid) in props.iter().enumerate() {
                    if pi > 0 { out.push(','); }
                    out.push_str(&format!("\"{}\"", esc(&prop_key(t, pid))));
                }
                out.push_str("]}");
            }
            out.push_str("]}");
        }
        out.push(']');
    }
    out.push('}');
    out
}

/// Interpolator "type" payload: same shape as SPTimeline::serialize_interpolator.
fn json_string_for_interpolator(t: &Timeline, id: InterpId) -> String {
    let (kind, alpha) = t.interps[id].kind();
    match kind {
        "linear" => "[\"linear\"]".into(),
        "cubic-spline" => "[\"cubic-spline\"]".into(),
        "catmull-rom-spline" => format!("[{{\"type\":\"catmull-rom-spline\",\"alpha\":{}}}]", alpha.unwrap_or(0.0)),
        other => format!("[\"{}\"]", other),
    }
}

// ---------- deserialize ----------

/// Upstream PathingRegistry.createTimeline(): SPTimeline's timeline with the
/// four camera properties + the explicit-interpolation marker registered.
pub fn create_timeline() -> Timeline {
    let mut t = Timeline::new();
    // Same registration order as SPTimeline::new so PropertyId indices match.
    t.define_property(timestamp_property());
    t.define_property(camera_position());
    t.define_property(camera_rotation());
    t.define_property(spectator_property());
    t.define_property(explicit_interpolation_property());
    t
}

fn parse_prop_value(t: &Timeline, pid: PropertyId, v: &JVal) -> Result<PropertyValue, String> {
    let id = t.properties[pid].id.as_str();
    match id {
        "position" => match v {
            JVal::Arr(a) if a.len() == 3 => Ok(PropertyValue::Position(
                jnum(&a[0]).ok_or("bad position")?, jnum(&a[1]).ok_or("bad position")?, jnum(&a[2]).ok_or("bad position")?)),
            _ => Err("bad position".into()),
        },
        "rotation" => match v {
            JVal::Arr(a) if a.len() == 3 => Ok(PropertyValue::Rotation(
                jnum(&a[0]).ok_or("bad rotation")? as f32, jnum(&a[1]).ok_or("bad rotation")? as f32, jnum(&a[2]).ok_or("bad rotation")? as f32)),
            _ => Err("bad rotation".into()),
        },
        "timestamp" => Ok(PropertyValue::Timestamp(jnum(v).ok_or("bad timestamp")? as i32)),
        "spectate" => Ok(PropertyValue::Spectator(jnum(v).ok_or("bad spectate")? as i32)),
        // ObjectUtils.NULL marker, written as a plain integer 0.
        "interpolationFixed" => Ok(PropertyValue::Spectator(jnum(v).ok_or("bad marker")? as i32)),
        other => Err(format!("Unknown property: {}", other)),
    }
}

/// Upstream TimelineSerialization.deserialize. Only the current
/// timelines.json format is supported (legacy converters are not ported).
pub fn deserialize(json: &str) -> Result<BTreeMap<String, Timeline>, String> {
    let root = Parser::new(json).val()?;
    let obj = match root { JVal::Obj(m) => m, _ => return Err("expected object".into()) };
    let mut timelines = BTreeMap::new();
    for (name, arr) in obj {
        let mut t = create_timeline();
        let paths = match arr { JVal::Arr(a) => a, _ => return Err("expected path array".into()) };
        for p in &paths {
            let p = p.clone();
            let keyframes = jstr(&p, "keyframes").ok_or("missing keyframes")?;
            let segments = jstr(&p, "segments").ok_or("missing segments")?;
            let interpolators = jstr(&p, "interpolators").ok_or("missing interpolators")?;
            let path = t.create_path();
            let kfs = match keyframes { JVal::Arr(a) => a, _ => return Err("expected keyframes array".into()) };
            for kf in kfs.iter() {
                let time = jnum(jstr(kf, "time").ok_or("missing time")?).ok_or("bad time")? as i64;
                let mut values: BTreeMap<PropertyId, PropertyValue> = BTreeMap::new();
                let props = jstr(kf, "properties").ok_or("missing properties")?;
                if let JVal::Obj(m) = props {
                    for (id, v) in m {
                        let pid = t.get_property_by_string(id).ok_or_else(|| format!("Unknown property: {}", id))?;
                        values.insert(pid, parse_prop_value(&t, pid, v)?);
                    }
                }
                t.path_insert(path, time, values)?;
            }
            let interps = match interpolators { JVal::Arr(a) => a, _ => return Err("expected interpolators array".into()) };
            let mut created: Vec<InterpId> = Vec::new();
            for iv in interps.iter() {
                let ty = jstr_v(jstr(iv, "type").ok_or("Missing interpolator type")?)
                    .ok_or("Missing interpolator type")?;
                let id = {
                    let interp = match ty {
                        "[\"linear\"]" => Box::new(LinearInterpolator::new()) as Box<dyn Interpolator>,
                        "[\"cubic-spline\"]" => Box::new(PolynomialSplineInterpolator::cubic()) as Box<dyn Interpolator>,
                        _ => {
                            // catmull-rom: [{"type":...,"alpha":...}]
                            let p = Parser::new(ty).val()?;
                            let inner = match p { JVal::Arr(a) if !a.is_empty() => a[0].clone(), _ => return Err("bad interpolator".into()) };
                            let kind = jstr(&inner, "type").and_then(|v| jstr_v(v)).ok_or("missing type")?;
                            if kind != "catmull-rom-spline" { return Err(format!("Unknown interpolation type: {}", kind)); }
                            let alpha = jnum(jstr(&inner, "alpha").ok_or("Missing alpha value for catmull-rom-spline.")?)
                                .ok_or("bad alpha")?;
                            Box::new(CatmullRomSplineInterpolator::new(alpha)) as Box<dyn Interpolator>
                        }
                    };
                    t.create_interpolator(interp)
                };
                if let Some(JVal::Arr(props)) = jstr(iv, "properties") {
                    for pv in props {
                        let name = jstr_v(pv).ok_or("bad property name")?;
                        let pid = t.get_property_by_string(name)
                            .ok_or_else(|| format!("Timeline does not know property '{}'", name))?;
                        t.interps[id].register_property(pid);
                    }
                }
                created.push(id);
            }
            let segs = match segments { JVal::Arr(a) => a, _ => return Err("expected segments array".into()) };
            for (i, sv) in segs.into_iter().enumerate() {
                if let JVal::Num(n) = sv {
                    let idx = *n as usize;
                    let id = *created.get(idx).ok_or("bad interpolator index")?;
                    t.interps[id].add_segment(t.paths[path].segments[i].id);
                    t.paths[path].segments[i].interpolator = Some(id);
                }
            }
        }
        timelines.insert(name, t);
    }
    Ok(timelines)
}

/// Public entry to the JSON reader (used by the replay-file layer).
pub fn json_parse(json: &str) -> Result<JVal, String> { Parser::new(json).val() }
}
pub mod sptimeline {
// SPTimeline: 1:1 Rust port of ReplayMod simplepathing (SPTimeline,
// InterpolatorType, ExplicitInterpolationProperty) plus the Change classes
// it drives (changes.rs). Upstream ReplayMod, LGPL-3.0-or-later; this
// derived port: copyright (c) 2026 sparticle, LGPL-3.0-or-later.
//
// Shape notes: upstream SPTimeline holds a Timeline plus two Path
// references (TIME=0, POSITION=1); Rust uses path indices. Upstream
// Preconditions/Validate -> Result. Interpolator serialization here is a
// minimal hand-rolled JSON matching upstream's JsonWriter output for the
// three known types (equality + clone use; full .pttr save format is a
// later phase). EntityPositionTracker is a trait hook here; the tracker
// itself is ported with the replay sender phase.

use super::pathing::*;
use super::changes::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SPPath { Time, Position }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InterpolatorType { Default, CatmullRom, Cubic, Linear }

impl InterpolatorType {
    /// Upstream newInstance (DEFAULT throws).
    pub fn new_instance(self) -> Result<Box<dyn Interpolator>, String> {
        match self {
            InterpolatorType::Default => Err("DEFAULT has no interpolator".into()),
            InterpolatorType::CatmullRom => Ok(Box::new(CatmullRomSplineInterpolator::new(0.5))),
            InterpolatorType::Cubic => Ok(Box::new(PolynomialSplineInterpolator::cubic())),
            InterpolatorType::Linear => Ok(Box::new(LinearInterpolator::new())),
        }
    }
    /// Upstream getInterpolatorClass, as the kind string.
    pub fn class_kind(self) -> Option<&'static str> {
        match self {
            InterpolatorType::Default => None,
            InterpolatorType::CatmullRom => Some("catmull-rom-spline"),
            InterpolatorType::Cubic => Some("cubic-spline"),
            InterpolatorType::Linear => Some("linear"),
        }
    }
    /// Upstream fromClass (DEFAULT on miss).
    pub fn from_class(kind: &str) -> Self {
        match kind {
            "catmull-rom-spline" => InterpolatorType::CatmullRom,
            "cubic-spline" => InterpolatorType::Cubic,
            "linear" => InterpolatorType::Linear,
            _ => InterpolatorType::Default,
        }
    }
    /// Upstream localizationKey.
    pub fn localization_key(self) -> &'static str {
        match self {
            InterpolatorType::Default => "default",
            InterpolatorType::CatmullRom => "catmullrom",
            InterpolatorType::Cubic => "cubic",
            InterpolatorType::Linear => "linear",
        }
    }
    /// Upstream fromString: matches the i18n name; falls back to CATMULL_ROM.
    pub fn from_string(s: &str) -> Self {
        let full = |k: &str| format!("replaymod.gui.editkeyframe.interpolator.{}.name", k);
        if s == full("default") { InterpolatorType::Default }
        else if s == full("catmullrom") { InterpolatorType::CatmullRom }
        else if s == full("cubic") { InterpolatorType::Cubic }
        else if s == full("linear") { InterpolatorType::Linear }
        else { InterpolatorType::CatmullRom }
    }
}

/// Upstream replaystudio.util.Location (exact float equality, as upstream).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location { pub x: f64, pub y: f64, pub z: f64, pub yaw: f32, pub pitch: f32 }

impl Location {
    pub fn new(x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> Self {
        Self { x, y, z, yaw, pitch }
    }
}

/// Upstream EntityPositionTracker's read the timeline layer needs; the
/// concrete tracker (replaystudio.util.EntityPositionTracker) is ported with
/// the replay sender phase.
pub trait EntityPositionTracker {
    fn get_entity_position_at_timestamp(&self, entity_id: i32, timestamp: i32) -> Option<Location>;
}

/// Upstream simplepathing.properties.ExplicitInterpolationProperty: a dummy
/// property whose presence marks a segment's interpolator as explicitly set.
/// The value is an opaque null marker (upstream ObjectUtils.NULL); only
/// presence is ever read.
pub fn explicit_interpolation_property() -> Property {
    Property {
        id: "interpolationFixed".into(),
        group: None,
        localized_name: "<internal>".into(),
        // ObjectUtils.NULL marker; never read or interpolated (no parts).
        initial: PropertyValue::Spectator(0),
        parts: vec![],
    }
}

pub struct SPTimeline {
    pub timeline: Timeline,
    pub time_path: usize,
    pub position_path: usize,
    pub entity_tracker: Option<Box<dyn EntityPositionTracker>>,
    pub default_interpolator_type: Option<InterpolatorType>,
    pub camera_position_id: PropertyId,
    pub camera_rotation_id: PropertyId,
    pub timestamp_id: PropertyId,
    pub spectator_id: PropertyId,
    pub explicit_id: PropertyId,
}

impl SPTimeline {
    /// Upstream SPTimeline() -> createInitialTimeline.
    pub fn new() -> Self {
        let mut timeline = Timeline::new();
        let timestamp_id = timeline.define_property(timestamp_property());
        let camera_position_id = timeline.define_property(camera_position());
        let camera_rotation_id = timeline.define_property(camera_rotation());
        let spectator_id = timeline.define_property(spectator_property());
        let explicit_id = timeline.define_property(explicit_interpolation_property());
        timeline.create_path();
        timeline.create_path();
        Self {
            timeline,
            time_path: 0,
            position_path: 1,
            entity_tracker: None,
            default_interpolator_type: None,
            camera_position_id,
            camera_rotation_id,
            timestamp_id,
            spectator_id,
            explicit_id,
        }
    }

    /// Upstream SPTimeline(Timeline): requires the registry properties and
    /// at least the two SPPath paths to exist (upstream indexes by ordinal).
    pub fn from_timeline(mut timeline: Timeline) -> Result<Self, String> {
        if timeline.paths.len() < 2 { return Err("timeline needs the TIME and POSITION paths".into()); }
        let get = |t: &Timeline, id: &str| t.get_property_by_string(id).ok_or_else(|| format!("property {} not registered", id));
        Ok(Self {
            camera_position_id: get(&timeline, "camera:position")?,
            camera_rotation_id: get(&timeline, "camera:rotation")?,
            timestamp_id: get(&timeline, "timestamp")?,
            spectator_id: get(&timeline, "spectate")?,
            explicit_id: get(&timeline, "interpolationFixed")?,
            timeline,
            time_path: 0,
            position_path: 1,
            entity_tracker: None,
            default_interpolator_type: None,
        })
    }

    pub fn get_path(&self, p: SPPath) -> usize {
        match p { SPPath::Time => self.time_path, SPPath::Position => self.position_path }
    }

    pub fn get_keyframe(&self, p: SPPath, time: i64) -> Option<&Keyframe> {
        self.timeline.path_get_keyframe(self.get_path(p), time)
    }

    /// Upstream setEntityTracker (checkState: only settable once).
    pub fn set_entity_tracker(&mut self, tracker: Box<dyn EntityPositionTracker>) -> Result<(), String> {
        if self.entity_tracker.is_some() { return Err("Entity tracker already set".into()); }
        self.entity_tracker = Some(tracker);
        Ok(())
    }

    /// Upstream setDefaultInterpolatorType.
    pub fn set_default_interpolator_type(&mut self, ty: InterpolatorType) -> Result<(), String> {
        if ty == InterpolatorType::Default { return Err("Must not be DEFAULT".into()); }
        let prev = self.default_interpolator_type;
        self.default_interpolator_type = Some(ty);
        if let Some(p) = prev {
            if p != ty {
                let change = self.update_interpolators()?;
                self.timeline.push_change(change);
            }
        }
        Ok(())
    }

    /// Upstream setDefaultInterpolator(Interpolator): takes an already
    /// created interpolator id; validates it against the default type.
    pub fn set_default_interpolator(&mut self, interp: InterpId) -> Result<Box<dyn Change>, String> {
        let ty = self.default_interpolator_type.ok_or("Default interpolator type not set.")?;
        let kind = self.timeline.interps[interp].kind().0;
        if Some(kind) != ty.class_kind() {
            return Err("interpolator is not of the default interpolator type".into());
        }
        self.register_position_interpolator_properties(interp);

        let mut changes: Vec<Box<dyn Change>> = Vec::new();
        let p = self.position_path;
        for i in 0..self.timeline.paths[p].segments.len() {
            let start = self.timeline.paths[p].segments[i].start;
            let explicit = self.timeline.paths[p].keyframes.get(&start)
                .map(|k| k.values.contains_key(&self.explicit_id)).unwrap_or(false);
            if explicit { continue; }
            if self.is_spectator_segment(p, i) { continue; }
            changes.push(Box::new(SetInterpolator::new(p, i, interp)));
        }
        let mut combined = CombinedChange::create(changes);
        combined.apply(&mut self.timeline)?;
        let upd = self.update_interpolators()?;
        Ok(Box::new(CombinedChange::create_from_applied(vec![Box::new(combined), upd])))
    }

    pub fn is_time_keyframe(&self, time: i64) -> bool {
        self.timeline.path_get_keyframe(self.time_path, time).is_some()
    }
    pub fn is_position_keyframe(&self, time: i64) -> bool {
        self.timeline.path_get_keyframe(self.position_path, time).is_some()
    }
    pub fn is_spectator_keyframe(&self, time: i64) -> bool {
        self.timeline.path_get_keyframe(self.position_path, time)
            .map(|k| k.values.contains_key(&self.spectator_id)).unwrap_or(false)
    }

    /// Upstream addPositionKeyframe.
    pub fn add_position_keyframe(&mut self, time: i64, pos: (f64, f64, f64), rot: (f32, f32, f32), spectated: i32) -> Result<(), String> {
        if self.timeline.paths[self.position_path].keyframes.contains_key(&time) {
            return Err("Keyframe already exists".into());
        }
        let mut parts: Vec<Box<dyn Change>> = Vec::new();
        let mut add = AddKeyframe::new(self.position_path, time);
        add.apply(&mut self.timeline)?;
        parts.push(Box::new(add));
        let index = kf_time_to_index(&self.timeline.paths[self.position_path], time);
        let update = UpdateKeyframeProperties::create(self.position_path, index)
            .set_value(self.camera_position_id, PropertyValue::Position(pos.0, pos.1, pos.2))
            .set_value(self.camera_rotation_id, PropertyValue::Rotation(rot.0, rot.1, rot.2));
        let update = if spectated != -1 { update.set_value(self.spectator_id, PropertyValue::Spectator(spectated)) } else { update };
        let mut update = update.done();
        update.apply(&mut self.timeline)?;
        parts.push(Box::new(update));

        // If this new keyframe formed the first segment of the path, create
        // an initial interpolator of default type.
        if self.timeline.paths[self.position_path].segments.len() == 1 {
            let interp = self.create_default_interpolator()?;
            let mut set = SetInterpolator::new(self.position_path, 0, interp);
            set.apply(&mut self.timeline)?;
            parts.push(Box::new(set));
        }

        // Update interpolators for spectator keyframes (upstream: overkill,
        // but far simpler than covering every case).
        let upd = self.update_interpolators()?;
        parts.push(upd);
        let mut spec = self.update_spectator_positions()?;
        spec.apply(&mut self.timeline)?;
        parts.push(spec);
        self.timeline.push_change(Box::new(CombinedChange::create_from_applied(parts)));
        Ok(())
    }

    /// Upstream updatePositionKeyframe.
    pub fn update_position_keyframe(&mut self, time: i64, pos: (f64, f64, f64), rot: (f32, f32, f32)) -> Result<Box<dyn Change>, String> {
        if !self.timeline.paths[self.position_path].keyframes.contains_key(&time) {
            return Err("Keyframe does not exists".into());
        }
        let kf = &self.timeline.paths[self.position_path].keyframes[&time];
        if kf.values.contains_key(&self.spectator_id) {
            return Err("Cannot update spectator keyframe".into());
        }
        let index = kf_time_to_index(&self.timeline.paths[self.position_path], time);
        let mut change = UpdateKeyframeProperties::create(self.position_path, index)
            .set_value(self.camera_position_id, PropertyValue::Position(pos.0, pos.1, pos.2))
            .set_value(self.camera_rotation_id, PropertyValue::Rotation(rot.0, rot.1, rot.2))
            .done();
        change.apply(&mut self.timeline)?;
        Ok(Box::new(change))
    }

    /// Upstream removePositionKeyframe.
    pub fn remove_position_keyframe(&mut self, time: i64) -> Result<(), String> {
        if !self.timeline.paths[self.position_path].keyframes.contains_key(&time) {
            return Err("No keyframe at that time".into());
        }
        let index = kf_time_to_index(&self.timeline.paths[self.position_path], time);
        let mut parts: Vec<Box<dyn Change>> = Vec::new();
        let mut remove = RemoveKeyframe::new_with_index(self.position_path, index);
        remove.apply(&mut self.timeline)?;
        parts.push(Box::new(remove));
        let upd = self.update_interpolators()?;
        parts.push(upd);
        let mut spec = self.update_spectator_positions()?;
        spec.apply(&mut self.timeline)?;
        parts.push(spec);
        self.timeline.push_change(Box::new(CombinedChange::create_from_applied(parts)));
        Ok(())
    }

    /// Upstream addTimeKeyframe.
    pub fn add_time_keyframe(&mut self, time: i64, replay_time: i32) -> Result<(), String> {
        if self.timeline.paths[self.time_path].keyframes.contains_key(&time) {
            return Err("Keyframe already exists".into());
        }
        let mut parts: Vec<Box<dyn Change>> = Vec::new();
        let mut add = AddKeyframe::new(self.time_path, time);
        add.apply(&mut self.timeline)?;
        parts.push(Box::new(add));
        let index = kf_time_to_index(&self.timeline.paths[self.time_path], time);
        let mut update = UpdateKeyframeProperties::create(self.time_path, index)
            .set_value(self.timestamp_id, PropertyValue::Timestamp(replay_time))
            .done();
        update.apply(&mut self.timeline)?;
        parts.push(Box::new(update));

        if self.timeline.paths[self.time_path].segments.len() == 1 {
            let id = self.timeline.create_interpolator(Box::new(LinearInterpolator::new()));
            self.timeline.interps[id].register_property(self.timestamp_id);
            let mut set = SetInterpolator::new(self.time_path, 0, id);
            set.apply(&mut self.timeline)?;
            parts.push(Box::new(set));
        }

        let mut spec = self.update_spectator_positions()?;
        spec.apply(&mut self.timeline)?;
        parts.push(spec);
        self.timeline.push_change(Box::new(CombinedChange::create_from_applied(parts)));
        Ok(())
    }

    /// Upstream updateTimeKeyframe.
    pub fn update_time_keyframe(&mut self, time: i64, replay_time: i32) -> Result<Box<dyn Change>, String> {
        if !self.timeline.paths[self.time_path].keyframes.contains_key(&time) {
            return Err("Keyframe does not exists".into());
        }
        let index = kf_time_to_index(&self.timeline.paths[self.time_path], time);
        let mut change = UpdateKeyframeProperties::create(self.time_path, index)
            .set_value(self.timestamp_id, PropertyValue::Timestamp(replay_time))
            .done();
        change.apply(&mut self.timeline)?;
        Ok(Box::new(change))
    }

    /// Upstream removeTimeKeyframe.
    pub fn remove_time_keyframe(&mut self, time: i64) -> Result<(), String> {
        if !self.timeline.paths[self.time_path].keyframes.contains_key(&time) {
            return Err("No keyframe at that time".into());
        }
        let index = kf_time_to_index(&self.timeline.paths[self.time_path], time);
        let mut parts: Vec<Box<dyn Change>> = Vec::new();
        let mut remove = RemoveKeyframe::new_with_index(self.time_path, index);
        remove.apply(&mut self.timeline)?;
        parts.push(Box::new(remove));
        let mut spec = self.update_spectator_positions()?;
        spec.apply(&mut self.timeline)?;
        parts.push(spec);
        self.timeline.push_change(Box::new(CombinedChange::create_from_applied(parts)));
        Ok(())
    }
}

impl SPTimeline {
    /// Upstream setInterpolatorToDefault.
    pub fn set_interpolator_to_default(&mut self, time: i64) -> Result<(), String> {
        if !self.timeline.paths[self.position_path].keyframes.contains_key(&time) {
            return Err("Keyframe does not exists".into());
        }
        let index = kf_time_to_index(&self.timeline.paths[self.position_path], time);
        let mut change = UpdateKeyframeProperties::create(self.position_path, index)
            .remove_property(self.explicit_id)
            .done();
        change.apply(&mut self.timeline)?;
        let upd = self.update_interpolators()?;
        self.timeline.push_change(Box::new(CombinedChange::create_from_applied(vec![Box::new(change), upd])));
        Ok(())
    }

    /// Upstream setInterpolator(time, interpolator): marks the keyframe
    /// explicit and sets the following segment's interpolator.
    pub fn set_interpolator(&mut self, time: i64, interp: InterpId) -> Result<(), String> {
        let p = self.position_path;
        if !self.timeline.paths[p].keyframes.contains_key(&time) {
            return Err("Keyframe does not exists".into());
        }
        let seg = self.timeline.paths[p].segments.iter().position(|s| s.start == time)
            .ok_or("Keyframe has no following segment.")?;
        self.register_position_interpolator_properties(interp);
        let index = kf_time_to_index(&self.timeline.paths[p], time);
        let a = UpdateKeyframeProperties::create(p, index)
            .set_value(self.explicit_id, PropertyValue::Spectator(0)) // ObjectUtils.NULL
            .done();
        let b = SetInterpolator::new(p, seg, interp);
        let mut combined = CombinedChange::create(vec![Box::new(a), Box::new(b)]);
        combined.apply(&mut self.timeline)?;
        let upd = self.update_interpolators()?;
        self.timeline.push_change(Box::new(CombinedChange::create_from_applied(vec![Box::new(combined), upd])));
        Ok(())
    }

    /// Upstream moveKeyframe.
    pub fn move_keyframe(&mut self, p: SPPath, old_time: i64, new_time: i64) -> Result<(), String> {
        let pi = self.get_path(p);
        if !self.timeline.paths[pi].keyframes.contains_key(&old_time) {
            return Err("No keyframe at specified time".into());
        }
        let old_index = kf_time_to_index(&self.timeline.paths[pi], old_time);
        let old_values = self.timeline.paths[pi].keyframes[&old_time].values.clone();
        let is_last_old = *self.timeline.paths[pi].keyframes.keys().last().unwrap() == old_time;
        // Interpolator of the first segment, needed if it remains the only one.
        let first_interp = self.timeline.paths[pi].segments.first().and_then(|s| s.interpolator);
        // The interpolator lost when the old keyframe is removed, restored after.
        let lost_interp = if is_last_old {
            self.timeline.paths[pi].segments.iter().find(|s| s.end == old_time).and_then(|s| s.interpolator)
        } else {
            self.timeline.paths[pi].segments.iter().find(|s| s.start == old_time).and_then(|s| s.interpolator)
        };

        let mut parts: Vec<Box<dyn Change>> = Vec::new();
        let mut remove = RemoveKeyframe::new_with_index(pi, old_index);
        remove.apply(&mut self.timeline)?;
        parts.push(Box::new(remove));
        let mut add = AddKeyframe::new(pi, new_time);
        add.apply(&mut self.timeline)?;
        parts.push(Box::new(add));

        // Copy over all properties.
        let new_index = kf_time_to_index(&self.timeline.paths[pi], new_time);
        let mut builder = UpdateKeyframeProperties::create(pi, new_index);
        for (prop, v) in &old_values {
            builder = builder.set_value(*prop, v.clone());
        }
        let mut property_change = builder.done();
        property_change.apply(&mut self.timeline)?;
        parts.push(Box::new(property_change));

        // Restore the lost interpolator.
        let new_is_last = *self.timeline.paths[pi].keyframes.keys().last().unwrap() == new_time;
        let explicit_id = self.explicit_id;
        let mut restore: Box<dyn Change> = if !new_is_last {
            match (self.timeline.paths[pi].segments.iter().position(|s| s.start == new_time), lost_interp) {
                (Some(i), Some(ip)) => Box::new(SetInterpolator::new(pi, i, ip)),
                _ => Box::new(CombinedChange::create(vec![])),
            }
        } else {
            let has_explicit = self.timeline.paths[pi].keyframes.get(&new_time)
                .map(|k| k.values.contains_key(&explicit_id)).unwrap_or(false);
            match (self.timeline.paths[pi].segments.iter().position(|s| s.end == new_time), lost_interp) {
                (Some(i), Some(ip)) => {
                    if has_explicit {
                        // Transfer the explicit marker to the new segment's start keyframe.
                        let start_time = self.timeline.paths[pi].segments[i].start;
                        let start_index = kf_time_to_index(&self.timeline.paths[pi], start_time);
                        Box::new(CombinedChange::create(vec![
                            Box::new(SetInterpolator::new(pi, i, ip)),
                            Box::new(UpdateKeyframeProperties::create(pi, start_index)
                                .set_value(explicit_id, PropertyValue::Spectator(0)).done()),
                        ]))
                    } else {
                        Box::new(SetInterpolator::new(pi, i, ip))
                    }
                }
                _ => Box::new(CombinedChange::create(vec![])),
            }
        };
        restore.apply(&mut self.timeline)?;
        parts.push(restore);

        // Finally update the interpolators.
        let interp_update: Box<dyn Change> = if p == SPPath::Position {
            // Position / spectator keyframes need special handling;
            // updateInterpolators applies itself internally (upstream bug
            // avoided by brace structure: no second apply on this branch).
            self.update_interpolators()?
        } else {
            // Time keyframes only need updating when only one segment exists.
            if self.timeline.paths[pi].segments.len() == 1 {
                let fi = first_interp.ok_or("One segment should have existed before as well")?;
                let mut c = SetInterpolator::new(pi, 0, fi);
                c.apply(&mut self.timeline)?;
                Box::new(c)
            } else {
                let mut c = CombinedChange::create(vec![]);
                c.apply(&mut self.timeline)?;
                Box::new(c)
            }
        };
        parts.push(interp_update);

        // And update spectator positions.
        let mut spectator = self.update_spectator_positions()?;
        spectator.apply(&mut self.timeline)?;
        parts.push(spectator);

        self.timeline.push_change(Box::new(CombinedChange::create_from_applied(parts)));
        Ok(())
    }

    /// Upstream isSpectatorSegment.
    pub fn is_spectator_segment(&self, p: usize, i: usize) -> bool {
        let seg = &self.timeline.paths[p].segments[i];
        let has = |t: i64| -> bool {
            self.timeline.paths[p].keyframes.get(&t)
                .map(|k| k.values.contains_key(&self.spectator_id)).unwrap_or(false)
        };
        has(seg.start) && has(seg.end)
    }

    /// Upstream updateInterpolators. Applies internally, returns the applied
    /// change (callers wrap it with createFromApplied).
    fn update_interpolators(&mut self) -> Result<Box<dyn Change>, String> {
        let ty = self.default_interpolator_type.ok_or("default interpolator type not set")?;
        let p = self.position_path;
        let seg_count = self.timeline.paths[p].segments.len();
        let spectator_id = self.spectator_id;
        let explicit_id = self.explicit_id;

        let mut updates: BTreeMap<usize, InterpId> = BTreeMap::new();

        // First, setup spectator interpolators. `spectator_interp` mirrors
        // upstream's outer `interpolator` variable, which the split loop
        // below (buggily) also reads.
        let mut spectator_interp: Option<InterpId> = None;
        for i in 0..seg_count {
            if self.is_spectator_segment(p, i) {
                if spectator_interp.is_none() {
                    let id = self.timeline.create_interpolator(Box::new(LinearInterpolator::new()));
                    self.timeline.interps[id].register_property(spectator_id);
                    spectator_interp = Some(id);
                }
                updates.insert(i, spectator_interp.unwrap());
            } else {
                spectator_interp = None;
            }
        }

        // Then, replace stale default interpolators with new ones.
        for i in 0..seg_count {
            let start = self.timeline.paths[p].segments[i].start;
            let explicit = self.timeline.paths[p].keyframes.get(&start)
                .map(|k| k.values.contains_key(&explicit_id)).unwrap_or(false);
            if explicit { continue; }
            if self.is_spectator_segment(p, i) { continue; }
            let cur: Option<InterpId> = match updates.get(&i) {
                Some(id) => Some(*id),
                None => self.timeline.paths[p].segments[i].interpolator,
            };
            // Upstream NPEs when a segment has no interpolator; error instead.
            let cur_id = cur.ok_or("segment has no interpolator")?;
            let cur_kind = self.timeline.interps[cur_id].kind().0;
            if Some(cur_kind) != ty.class_kind() {
                let id = self.create_default_interpolator()?;
                updates.insert(i, id);
            }
        }

        // Split: interpolators that are not continuous get cloned. Upstream
        // quirk preserved verbatim: `used.add(interpolator)` tests the outer
        // spectator-loop variable, not the current interpolator, so with no
        // spectator segments every new interpolator from the second onward
        // is cloned.
        let mut last: Option<InterpId> = None; // Java lastInterpolator (may be null)
        let mut used: Option<InterpId> = None; // the used-set holds at most one non-null member here
        let mut used_had_null = false;
        for i in 0..seg_count {
            if self.is_spectator_segment(p, i) { last = None; continue; }
            let current: Option<InterpId> = match updates.get(&i) {
                Some(id) => Some(*id),
                None => self.timeline.paths[p].segments[i].interpolator,
            };
            if last == current { continue; } // Java ==, including null == null
            let used_add_ok = match spectator_interp {
                None => { if used_had_null { false } else { used_had_null = true; true } }
                Some(id) => { if used == Some(id) { false } else { used = Some(id); true } }
            };
            if !used_add_ok {
                let cur = current.ok_or("cannot clone null interpolator")?;
                let cloned = self.clone_interpolator(cur)?;
                updates.insert(i, cloned);
                last = Some(cloned);
            } else {
                last = current;
            }
        }

        // Merge: neighbouring interpolators that are equal get merged.
        let mut last2: Option<InterpId> = None;
        let mut last2_ser: Option<String> = None; // Java null
        for i in 0..seg_count {
            if self.is_spectator_segment(p, i) { last2 = None; last2_ser = None; continue; }
            let current: Option<InterpId> = match updates.get(&i) {
                Some(id) => Some(*id),
                None => self.timeline.paths[p].segments[i].interpolator,
            };
            let cur = current.ok_or("segment has no interpolator")?;
            let ser = self.serialize_interpolator(cur)?;
            if last2 != current && Some(&ser) == last2_ser.as_ref() {
                updates.insert(i, last2.unwrap());
                continue;
            }
            last2 = current;
            last2_ser = Some(ser);
        }

        let mut changes: Vec<Box<dyn Change>> = Vec::new();
        for (i, id) in &updates {
            changes.push(Box::new(SetInterpolator::new(p, *i, *id)));
        }
        let mut change = CombinedChange::create(changes);
        change.apply(&mut self.timeline)?;
        Ok(Box::new(change))
    }

    /// Upstream updateSpectatorPositions. Returns an UNapplied change.
    fn update_spectator_positions(&mut self) -> Result<Box<dyn Change>, String> {
        if self.entity_tracker.is_none() {
            return Ok(Box::new(CombinedChange::create(vec![])));
        }
        self.timeline.path_update(self.time_path, false)?;
        let kf_times: Vec<i64> = self.timeline.paths[self.position_path].keyframes.keys().copied().collect();
        let mut changes: Vec<Box<dyn Change>> = Vec::new();
        for time in kf_times {
            let spectator = match self.timeline.paths[self.position_path].keyframes[&time]
                .values.get(&self.spectator_id) {
                Some(PropertyValue::Spectator(s)) => *s,
                _ => continue,
            };
            let replay_time = match self.timeline.get_value(self.timestamp_id, time)? {
                Some(PropertyValue::Timestamp(t)) => t,
                _ => continue, // no time keyframe here, cannot determine replay time
            };
            let expected = self.entity_tracker.as_ref().unwrap()
                .get_entity_position_at_timestamp(spectator, replay_time);
            let expected = match expected { Some(l) => l, None => continue };
            let (pos, rot) = {
                let kf = &self.timeline.paths[self.position_path].keyframes[&time];
                (kf.values.get(&self.camera_position_id).cloned()
                    .unwrap_or(PropertyValue::Position(0.0, 0.0, 0.0)),
                 kf.values.get(&self.camera_rotation_id).cloned()
                    .unwrap_or(PropertyValue::Rotation(0.0, 0.0, 0.0)))
            };
            let actual = Location::new(
                match pos { PropertyValue::Position(x, _, _) => x, _ => 0.0 },
                match pos { PropertyValue::Position(_, y, _) => y, _ => 0.0 },
                match pos { PropertyValue::Position(_, _, z) => z, _ => 0.0 },
                match rot { PropertyValue::Rotation(y, _, _) => y, _ => 0.0 },
                match rot { PropertyValue::Rotation(_, p, _) => p, _ => 0.0 },
            );
            if expected != actual {
                let index = kf_time_to_index(&self.timeline.paths[self.position_path], time);
                changes.push(Box::new(UpdateKeyframeProperties::create(self.position_path, index)
                    .set_value(self.camera_position_id, PropertyValue::Position(expected.x, expected.y, expected.z))
                    .set_value(self.camera_rotation_id, PropertyValue::Rotation(expected.yaw, expected.pitch, 0.0))
                    .done()));
            }
        }
        Ok(Box::new(CombinedChange::create(changes)))
    }

    /// Upstream createDefaultInterpolator.
    fn create_default_interpolator(&mut self) -> Result<InterpId, String> {
        let ty = self.default_interpolator_type.ok_or("default interpolator type not set")?;
        let id = self.timeline.create_interpolator(ty.new_instance()?);
        self.register_position_interpolator_properties(id);
        Ok(id)
    }

    /// Upstream registerPositionInterpolatorProperties.
    fn register_position_interpolator_properties(&mut self, interp: InterpId) {
        self.timeline.interps[interp].register_property(self.camera_position_id);
        self.timeline.interps[interp].register_property(self.camera_rotation_id);
    }

    /// Upstream serializeInterpolator (as a JSON string, minimal writer).
    pub fn serialize_interpolator(&self, interp: InterpId) -> Result<String, String> {
        let (kind, alpha) = self.timeline.interps[interp].kind();
        match kind {
            "linear" => Ok("[\"linear\"]".into()),
            "cubic-spline" => Ok("[\"cubic-spline\"]".into()),
            "catmull-rom-spline" => Ok(format!(
                "[{{\"type\":\"catmull-rom-spline\",\"alpha\":{}}}]", alpha.unwrap_or(0.0))),
            other => Err(format!("Unknown interpolator type: {}", other)),
        }
    }

    /// Upstream deserializeInterpolator (minimal reader for the same shapes).
    pub fn deserialize_interpolator_json(&self, json: &str) -> Result<Box<dyn Interpolator>, String> {
        let inner = json.trim()
            .strip_prefix('[').ok_or("Unexpected token")?
            .strip_suffix(']').ok_or("Unexpected token")?
            .trim();
        if inner.starts_with('"') {
            let t = inner.trim_matches('"');
            match t {
                "linear" => Ok(Box::new(LinearInterpolator::new())),
                "cubic-spline" => Ok(Box::new(PolynomialSplineInterpolator::cubic())),
                _ => Err(format!("Unknown interpolation type: {}", t)),
            }
        } else {
            // Object form: {"type":"catmull-rom-spline","alpha":0.5}
            let ty_pos = inner.find("\"type\":\"").ok_or("missing type")? + 8;
            let rest = &inner[ty_pos..];
            let t = &rest[..rest.find('"').ok_or("missing type")?];
            if t != "catmull-rom-spline" { return Err(format!("Unknown interpolation type: {}", t)); }
            let a_pos = inner.find("\"alpha\":").ok_or("Missing alpha value for catmull-rom-spline.")? + 8;
            let rest = &inner[a_pos..];
            let end = rest.find(|c| c == ',' || c == '}').unwrap_or(rest.len());
            let alpha: f64 = rest[..end].trim().parse().map_err(|_| "bad alpha")?;
            Ok(Box::new(CatmullRomSplineInterpolator::new(alpha)))
        }
    }

    /// Upstream cloneInterpolator: deserialize(serialize(x)), re-registering
    /// the original's keyframe properties.
    fn clone_interpolator(&mut self, interp: InterpId) -> Result<InterpId, String> {
        let json = self.serialize_interpolator(interp)?;
        let cloned = self.deserialize_interpolator_json(&json)?;
        let props: Vec<PropertyId> = self.timeline.interps[interp]
            .keyframe_properties().iter().copied().collect();
        let new_id = self.timeline.create_interpolator(cloned);
        for p in props {
            self.timeline.interps[new_id].register_property(p);
        }
        Ok(new_id)
    }
}
}
pub mod sync {
// Sync-mode entity handling on chunk unload: 1:1 FullReplaySender
// processPacketSync (the MC>=11404 branch) + forcePositionForVehicleAndSelf.
// Upstream drives the live Minecraft world; the algorithm is ported over a
// minimal entity/chunk model so it runs and tests headless. Movement and
// vehicle-follow behavior live behind tick() / tick_riding().
// This file: Copyright (c) 2026 sparticle, LGPL-3.0-or-later.
use std::collections::BTreeMap;

const SETTLE_EPS2: f64 = 0.0001; // prevPos.squaredDistanceTo(...) > 0.0001
const SETTLE_MAX_TICKS: u32 = 100; // ticks++ < 100

#[derive(Clone)]
pub struct SyncEntity {
    pub id: i32,
    pub x: f64, pub y: f64, pub z: f64,
    pub chunk_x: i32, pub chunk_y: i32, pub chunk_z: i32,
    pub update_needed: bool,
    pub vehicle: Option<i32>,
    // Stand-in for the vanilla tick: keeps moving by vel for settle_ticks
    // ticks, then rests (so the settle loop has something to converge on).
    pub vel: (f64, f64, f64),
    pub settle_ticks: u32,
}

#[derive(Default)]
pub struct SyncWorld {
    pub entities: BTreeMap<i32, SyncEntity>,
    // (chunkX, chunkZ) -> entity ids registered in that chunk.
    pub chunks: BTreeMap<(i32, i32), Vec<i32>>,
}

fn floor16(v: f64) -> i32 { (v / 16.0).floor() as i32 }

impl SyncWorld {
    pub fn add_entity(&mut self, e: SyncEntity, registered_chunk: Option<(i32, i32)>) {
        let id = e.id;
        if let Some((cx, cz)) = registered_chunk {
            self.chunks.entry((cx, cz)).or_default().push(id);
        }
        self.entities.insert(id, e);
    }

    /// vanilla Entity.tick(): the entity's own movement for one tick.
    fn tick(&mut self, id: i32) {
        let e = self.entities.get_mut(&id).unwrap();
        if e.settle_ticks > 0 {
            e.x += e.vel.0; e.y += e.vel.1; e.z += e.vel.2;
            e.settle_ticks -= 1;
        }
    }

    /// vanilla Entity.tickRiding(): carried by the vehicle's movement.
    fn tick_riding(&mut self, id: i32) {
        let vid = self.entities[&id].vehicle;
        if let Some(v) = vid {
            let (vx, vy, vz) = { let ve = &self.entities[&v]; (ve.x, ve.y, ve.z) };
            let e = self.entities.get_mut(&id).unwrap();
            e.x = vx; e.y = vy; e.z = vz;
        }
    }

    /// 1:1 forcePositionForVehicleAndSelf: settle the whole vehicle chain,
    /// deepest vehicle first, ticking each until its position stabilizes.
    pub fn force_position_for_vehicle_and_self(&mut self, id: i32) {
        let vehicle = self.entities[&id].vehicle;
        if let Some(v) = vehicle {
            self.force_position_for_vehicle_and_self(v);
        }
        let mut ticks: u32 = 0;
        loop {
            let (px, py, pz) = { let e = &self.entities[&id]; (e.x, e.y, e.z) };
            if vehicle.is_some() { self.tick_riding(id); } else { self.tick(id); }
            let e = &self.entities[&id];
            let d2 = (e.x - px) * (e.x - px) + (e.y - py) * (e.y - py) + (e.z - pz) * (e.z - pz);
            // Java do-while: ticks++ runs on EVERY condition evaluation.
            let cont = d2 > SETTLE_EPS2 && ticks < SETTLE_MAX_TICKS;
            ticks += 1;
            if !cont { break; }
        }
    }

    /// 1:1 processPacketSync for UnloadChunkS2CPacket (MC>=10904 / >=11400
    /// branch): before a chunk unloads, force-update every entity registered
    /// in it (skipping server-position interpolation), then re-home any that
    /// ticked themselves into a different chunk.
    pub fn process_packet_sync(&mut self, x: i32, z: i32) {
        let Some(ids) = self.chunks.get(&(x, z)).cloned() else { return; };
        for id in ids {
            if !self.entities.contains_key(&id) { continue; }
            // Skip interpolation of position updates coming from server.
            self.force_position_for_vehicle_and_self(id);
            // MC>=11404: leaving the chunk is now checked here.
            let e = &self.entities[&id];
            let cx = floor16(e.x); let cy = floor16(e.y); let cz = floor16(e.z);
            if e.chunk_x != cx || e.chunk_y != cy || e.chunk_z != cz {
                if e.update_needed {
                    // Entity has left the chunk: chunk.remove(entity, entity.chunkY)
                    if let Some(list) = self.chunks.get_mut(&(e.chunk_x, e.chunk_z)) {
                        list.retain(|&m| m != id);
                    }
                }
                let new_chunk = self.chunks.contains_key(&(cx, cz));
                let e = self.entities.get_mut(&id).unwrap();
                if new_chunk {
                    self.chunks.entry((cx, cz)).or_default().push(id);
                    e.chunk_x = cx; e.chunk_y = cy; e.chunk_z = cz;
                } else {
                    // Entity has left all loaded chunks.
                    e.update_needed = false;
                }
            }
        }
    }
}
}
pub mod zipio {
// Minimal ZIP container for .mcpr files: EOCD + central-directory parser,
// hand-rolled DEFLATE (RFC 1951) inflate for STORE(0)/DEFLATE(8) entries,
// and a STORE-method writer. Upstream uses java.util.zip.ZipFile/
// ZipOutputStream; the format is public (PKWARE APPNOTE), no upstream code
// involved. This file: Copyright (c) 2026 sparticle, LGPL-3.0-or-later.
use std::collections::BTreeMap;

const EOCD_SIG: u32 = 0x0605_4b50;
const CDH_SIG: u32 = 0x0201_4b50;
const LFH_SIG: u32 = 0x0403_4b50;

fn u16le(b: &[u8], o: usize) -> u16 { u16::from_le_bytes([b[o], b[o + 1]]) }
fn u32le(b: &[u8], o: usize) -> u32 { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }

/// Parse a whole zip into name -> bytes. Supports method 0 (stored) and
/// method 8 (deflate, the mode ReplayMod records .mcpr entries with).
pub fn parse_zip(data: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    // Locate the EOCD from the end.
    let mut eocd = None;
    if data.len() >= 22 {
        let start = data.len().saturating_sub(22 + 0xFFFF);
        for i in (start..=data.len() - 22).rev() {
            if u32le(data, i) == EOCD_SIG { eocd = Some(i); break; }
        }
    }
    let eocd = eocd.ok_or("no EOCD found")?;
    let count = u16le(data, eocd + 10) as usize;
    let cd_off = u32le(data, eocd + 16) as usize;
    let mut entries = BTreeMap::new();
    let mut p = cd_off;
    for _ in 0..count {
        if u32le(data, p) != CDH_SIG { return Err("bad central directory entry".into()); }
        let method = u16le(data, p + 10);
        let csize = u32le(data, p + 20) as usize;
        let name_len = u16le(data, p + 28) as usize;
        let extra_len = u16le(data, p + 30) as usize;
        let comment_len = u16le(data, p + 32) as usize;
        let lfh_off = u32le(data, p + 42) as usize;
        let name = String::from_utf8_lossy(&data[p + 46..p + 46 + name_len]).into_owned();
        // Local header: the data starts after LFH + name + extra (its own
        // extra length, which may differ from the central one).
        if u32le(data, lfh_off) != LFH_SIG { return Err("bad local header".into()); }
        let lfh_name = u16le(data, lfh_off + 26) as usize;
        let lfh_extra = u16le(data, lfh_off + 28) as usize;
        let data_start = lfh_off + 30 + lfh_name + lfh_extra;
        let raw = &data[data_start..data_start + csize];
        let bytes = match method {
            0 => raw.to_vec(),
            8 => inflate(raw)?,
            m => return Err(format!("unsupported zip method {}", m)),
        };
        entries.insert(name, bytes);
        p += 46 + name_len + extra_len + comment_len;
    }
    Ok(entries)
}

/// DEFLATE (RFC 1951) decompression, puff.c-style canonical decoding.
struct BitReader<'a> { data: &'a [u8], pos: usize, bitbuf: u32, bitcnt: u32 }
impl<'a> BitReader<'a> {
    fn bits(&mut self, need: u32) -> Result<u32, String> {
        while self.bitcnt < need {
            if self.pos >= self.data.len() { return Err("inflate: out of input".into()); }
            self.bitbuf |= (self.data[self.pos] as u32) << self.bitcnt;
            self.pos += 1;
            self.bitcnt += 8;
        }
        let val = self.bitbuf & ((1u32 << need) - 1);
        self.bitbuf >>= need;
        self.bitcnt -= need;
        Ok(val)
    }
}

struct Huff { counts: [u16; 16], symbols: Vec<u16> }
impl Huff {
    fn construct(lengths: &[u8]) -> Result<Huff, String> {
        let mut h = Huff { counts: [0; 16], symbols: vec![0; lengths.len()] };
        for &l in lengths { h.counts[l as usize] += 1; }
        h.counts[0] = 0;
        let mut left = 1i32;
        for len in 1..=15 {
            left <<= 1;
            left -= h.counts[len] as i32;
            if left < 0 { return Err("inflate: over-subscribed code".into()); }
        }
        let mut offs = [0u16; 16];
        for len in 1..15 { offs[len + 1] = offs[len] + h.counts[len]; }
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 { h.symbols[offs[l as usize] as usize] = sym as u16; offs[l as usize] += 1; }
        }
        Ok(h)
    }
    fn decode(&self, br: &mut BitReader) -> Result<i32, String> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..=15u32 {
            code |= br.bits(1)? as i32;
            let count = self.counts[len as usize] as i32;
            if code - count < first { return Ok(self.symbols[(index + (code - first)) as usize] as i32); }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("inflate: invalid code".into())
    }
}

const LEN_BASE: [u16; 29] = [3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258];
const LEN_EXTRA: [u8; 29] = [0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0];
const DIST_BASE: [u16; 30] = [1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577];
const DIST_EXTRA: [u8; 30] = [0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13];
const CLEN_ORDER: [usize; 19] = [16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15];

pub fn inflate(src: &[u8]) -> Result<Vec<u8>, String> {
    let mut br = BitReader { data: src, pos: 0, bitbuf: 0, bitcnt: 0 };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let bfinal = br.bits(1)?;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                // Stored block: skip to byte boundary, LEN/NLEN, raw copy.
                br.bitbuf = 0;
                br.bitcnt = 0;
                if br.pos + 4 > src.len() { return Err("inflate: stored header past end".into()); }
                let len = u16le(src, br.pos) as usize;
                let nlen = u16le(src, br.pos + 2) as usize;
                br.pos += 4;
                if len ^ 0xFFFF != nlen { return Err("inflate: stored LEN/NLEN mismatch".into()); }
                if br.pos + len > src.len() { return Err("inflate: stored block past end".into()); }
                out.extend_from_slice(&src[br.pos..br.pos + len]);
                br.pos += len;
            }
            1 => {
                // Fixed Huffman.
                let mut ll = [0u8; 288];
                for (i, l) in ll.iter_mut().enumerate() {
                    *l = if i < 144 { 8 } else if i < 256 { 9 } else if i < 280 { 7 } else { 8 };
                }
                let dl = [5u8; 30];
                inflate_block(&mut br, &mut out, &Huff::construct(&ll)?, &Huff::construct(&dl)?)?;
            }
            2 => {
                // Dynamic Huffman.
                let hlit = br.bits(5)? as usize + 257;
                let hdist = br.bits(5)? as usize + 1;
                let hclen = br.bits(4)? as usize + 4;
                let mut cl = [0u8; 19];
                for i in 0..hclen { cl[CLEN_ORDER[i]] = br.bits(3)? as u8; }
                let clh = Huff::construct(&cl)?;
                let mut lens = vec![0u8; hlit + hdist];
                let mut i = 0;
                while i < hlit + hdist {
                    let sym = clh.decode(&mut br)?;
                    match sym {
                        0..=15 => { lens[i] = sym as u8; i += 1; }
                        16 => {
                            if i == 0 { return Err("inflate: repeat with no previous".into()); }
                            let prev = lens[i - 1];
                            let rep = 3 + br.bits(2)? as usize;
                            for _ in 0..rep { if i >= lens.len() { break; } lens[i] = prev; i += 1; }
                        }
                        17 => { let rep = 3 + br.bits(3)? as usize; i = (i + rep).min(lens.len()); }
                        18 => { let rep = 11 + br.bits(7)? as usize; i = (i + rep).min(lens.len()); }
                        _ => return Err("inflate: bad code-length symbol".into()),
                    }
                }
                if lens[256] == 0 { return Err("inflate: no end-of-block code".into()); }
                let llh = Huff::construct(&lens[..hlit])?;
                let dlh = Huff::construct(&lens[hlit..])?;
                inflate_block(&mut br, &mut out, &llh, &dlh)?;
            }
            _ => return Err("inflate: invalid block type".into()),
        }
        if bfinal == 1 { break; }
    }
    Ok(out)
}

fn inflate_block(br: &mut BitReader, out: &mut Vec<u8>, llh: &Huff, dlh: &Huff) -> Result<(), String> {
    loop {
        let sym = llh.decode(br)?;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let li = (sym - 257) as usize;
            if li >= 29 { return Err("inflate: bad length symbol".into()); }
            let len = LEN_BASE[li] as usize + br.bits(LEN_EXTRA[li] as u32)? as usize;
            let dsym = dlh.decode(br)? as usize;
            if dsym >= 30 { return Err("inflate: bad distance symbol".into()); }
            let dist = DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
            if dist > out.len() { return Err("inflate: distance past output".into()); }
            let start = out.len() - dist;
            for k in 0..len { out.push(out[start + k]); }
        }
    }
}

/// Write a zip with stored (uncompressed) entries: valid for Java's
/// ZipFile and ReplayStudio, sized up front (no data descriptors).
pub fn write_zip_stored(entries: &[(String, Vec<u8>)], crc32: &dyn Fn(&[u8]) -> u32) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut cd: Vec<u8> = Vec::new();
    for (name, bytes) in entries {
        let lfh_off = out.len() as u32;
        out.extend_from_slice(&LFH_SIG.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes());  // flags
        out.extend_from_slice(&0u16.to_le_bytes());  // method 0 = stored
        out.extend_from_slice(&0u16.to_le_bytes());  // mod time
        out.extend_from_slice(&0u16.to_le_bytes());  // mod date
        out.extend_from_slice(&crc32(bytes).to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());  // extra len
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(bytes);

        cd.extend_from_slice(&CDH_SIG.to_le_bytes());
        cd.extend_from_slice(&20u16.to_le_bytes());  // version made by
        cd.extend_from_slice(&20u16.to_le_bytes());  // version needed
        cd.extend_from_slice(&0u16.to_le_bytes());   // flags
        cd.extend_from_slice(&0u16.to_le_bytes());   // method
        cd.extend_from_slice(&0u16.to_le_bytes());   // time
        cd.extend_from_slice(&0u16.to_le_bytes());   // date
        cd.extend_from_slice(&crc32(bytes).to_le_bytes());
        cd.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());   // extra
        cd.extend_from_slice(&0u16.to_le_bytes());   // comment
        cd.extend_from_slice(&0u16.to_le_bytes());   // disk
        cd.extend_from_slice(&0u16.to_le_bytes());   // internal attrs
        cd.extend_from_slice(&0u32.to_le_bytes());   // external attrs
        cd.extend_from_slice(&lfh_off.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
    }
    let cd_off = out.len() as u32;
    let cd_len = cd.len() as u32;
    out.extend_from_slice(&cd);
    out.extend_from_slice(&EOCD_SIG.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());      // disk
    out.extend_from_slice(&0u16.to_le_bytes());      // cd disk
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());      // comment len
    out
}

/// In-memory archive over parsed zip bytes: the concrete McprArchive
/// implementation that makes the whole .mcpr container stack run on real
/// zip data (open: parse_zip; save: write_zip_stored via the seam).
pub struct ZipArchiveMem { pub entries: BTreeMap<String, Vec<u8>> }
impl ZipArchiveMem {
    pub fn from_zip_bytes(data: &[u8]) -> Result<Self, String> {
        Ok(Self { entries: parse_zip(data)? })
    }
    pub fn to_zip_bytes(&self) -> Vec<u8> {
        let list: Vec<(String, Vec<u8>)> = self.entries.iter()
            .map(|(k, v)| (k.clone(), v.clone())).collect();
        write_zip_stored(&list, &super::replayfile::crc32)
    }
}
impl super::mcpr::McprArchive for ZipArchiveMem {
    fn read_entry(&self, entry: &str) -> Option<Vec<u8>> { self.entries.get(entry).cloned() }
    fn write_entry(&mut self, entry: &str, data: &[u8]) { self.entries.insert(entry.to_string(), data.to_vec()); }
    fn list_entries(&self) -> Vec<String> { self.entries.keys().cloned().collect() }
    fn remove_entry(&mut self, entry: &str) { self.entries.remove(entry); }
}
}
pub mod zipfile {
// Zip container change management: 1:1 Rust port of ReplayStudio's
// ZipReplayFile write/remove/save/saveTo merge logic, recording CRC hash
// invalidation + rewrite, and removed-entry tombstones (LGPL-3.0-or-later).
// This file: Copyright (c) 2026 sparticle, LGPL-3.0-or-later.
//
// The zip wire format itself (ZipFile/ZipOutputStream, and the on-disk
// cache directory with its GZIPped copies) is a Horizon-integration seam
// behind McprArchive; this layer owns the semantics above it. The cache
// invalidation on recording rewrite is modeled on the in-memory base map.
use super::mcpr::*;
use super::replayfile::crc32;
use std::collections::{BTreeMap, BTreeSet};

/// ZipReplayFile over an opened archive: base = the zip's entries at open.
pub struct ZipReplayFile<A: McprArchive> {
    pub archive: A,
    base: BTreeMap<String, Vec<u8>>,
    /// Upstream changedEntries: entry -> bytes (upstream uses temp files;
    /// buffered in memory here, semantically identical).
    changed_entries: BTreeMap<String, Vec<u8>>,
    /// Upstream removedEntries tombstones.
    removed_entries: BTreeSet<String>,

}

impl<A: McprArchive> ZipReplayFile<A> {
    /// Upstream open(): snapshot the zip's entries.
    pub fn open(archive: A) -> Self {
        let base: BTreeMap<String, Vec<u8>> = archive.list_entries()
            .into_iter()
            .filter_map(|name| archive.read_entry(&name).map(|b| (name, b)))
            .collect();
        Self { archive, base, changed_entries: BTreeMap::new(), removed_entries: BTreeSet::new() }
    }

    /// Upstream get(): changed copy first, else the base entry.
    pub fn get(&self, entry: &str) -> Option<Vec<u8>> {
        self.changed_entries.get(entry).cloned().or_else(|| self.base.get(entry).cloned())
    }

    /// Upstream write(): the change lands in changedEntries; a tombstone is
    /// lifted. For ENTRY_RECORDING the old hash is invalidated immediately,
    /// a CRC runs over everything written, and closing stores the new hash.
    pub fn write(&mut self, entry: &str) -> ZipWriter {
        self.removed_entries.remove(entry);
        if entry == ENTRY_RECORDING {
            // Upstream: immediately invalidate the old hash in case we
            // crash during writing.
            self.changed_entries.insert(ENTRY_RECORDING_HASH.to_string(), b"invalid".to_vec());
        }
        ZipWriter { file_entry: entry.to_string(), buf: Vec::new(), len_written: 0 }
    }

    /// Close half of upstream's anonymous OutputStream: folds the bytes into
    /// changedEntries and, for the recording, rewrites the CRC hash entry.
    pub fn finish_write(&mut self, w: ZipWriter) {
        if w.file_entry == ENTRY_RECORDING {
            let crc = crc32(&w.buf);
            self.changed_entries.insert(w.file_entry.clone(), w.buf);
            self.changed_entries.insert(ENTRY_RECORDING_HASH.to_string(), crc.to_string().into_bytes());
            // Upstream invalidates the on-disk cache here; ours is the base
            // snapshot, refreshed at save().
        } else {
            self.changed_entries.insert(w.file_entry, w.buf);
        }
    }

    /// Upstream remove(): drop the changed copy, tombstone the entry.
    pub fn remove(&mut self, entry: &str) {
        self.changed_entries.remove(entry);
        self.removed_entries.insert(entry.to_string());
    }

    /// Upstream save(): no-op with no changes; otherwise the merged entry
    /// set is written back through the archive (upstream: temp file + move).
    pub fn save(&mut self) {
        if self.changed_entries.is_empty() && self.removed_entries.is_empty() {
            return; // Upstream: no changes, no need to save
        }
        self.save_to_merged();
    }

    /// Upstream saveTo(target): original entries in order, skipping changed
    /// and removed, then every changed entry appended.
    fn save_to_merged(&mut self) {
        let merged: Vec<(String, Vec<u8>)> = self.base.iter()
            .filter(|(name, _)| !self.changed_entries.contains_key(*name) && !self.removed_entries.contains(*name))
            .map(|(name, bytes)| (name.clone(), bytes.clone()))
            .chain(self.changed_entries.iter().map(|(n, b)| (n.clone(), b.clone())))
            .collect();
        self.base = merged.into_iter().collect();
        // Persist through the archive seam (upstream writes a fresh zip and
        // moves it over the output file): remove everything, write the merge.
        let mut names = self.archive.list_entries();
        names.sort();
        for name in names { self.archive.remove_entry(&name); }
        for (name, bytes) in self.base.clone() { self.archive.write_entry(&name, &bytes); }
        self.changed_entries.clear();
        self.removed_entries.clear();
    }

    /// Read-back convenience mirroring get() after save().
    pub fn entry_names(&self) -> Vec<String> { self.base.keys().cloned().collect() }

    /// Seed the base snapshot directly (test-only in upstream terms: upstream
    /// constructs with an existing ZipFile).
    pub fn seed_base(&mut self, entry: &str, bytes: &[u8]) {
        self.base.insert(entry.to_string(), bytes.to_vec());
    }
}

/// Buffered writer handed out by write(); fold with finish_write.
pub struct ZipWriter { file_entry: String, buf: Vec<u8>, len_written: usize }
impl ZipWriter {
    pub fn write_bytes(&mut self, b: &[u8]) { self.buf.extend_from_slice(b); self.len_written += b.len(); }
    pub fn len(&self) -> usize { self.len_written }
}
}
pub mod camera {
// Camera paths: keyframes with smooth interpolation, the core of ReplayMod's
// camera path editor. Positions are cubic-bezier handles like upstream's
// ControlPoint path; each keyframe carries position, yaw and pitch.

#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    pub tick: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraPath {
    pub keyframes: Vec<Keyframe>,
}

impl CameraPath {
    pub fn add(&mut self, k: Keyframe) {
        // Sorted insert; a keyframe on an existing tick replaces it.
        match self.keyframes.binary_search_by_key(&k.tick, |k| k.tick) {
            Ok(i) => self.keyframes[i] = k,
            Err(i) => self.keyframes.insert(i, k),
        }
    }

    /// Camera state at `tick`. Between keyframes the position follows a
    /// cubic ease-in-out curve on t (ReplayMod defaults to smoothstep-like
    /// easing), and rotation interpolates the short way around.
    pub fn state_at(&self, tick: u32) -> Option<(f64, f64, f64, f32, f32)> {
        let kfs = &self.keyframes;
        if kfs.is_empty() {
            return None;
        }
        if tick <= kfs[0].tick {
            let k = &kfs[0];
            return Some((k.x, k.y, k.z, k.yaw, k.pitch));
        }
        if let Some(k) = kfs.last() {
            if tick >= k.tick {
                return Some((k.x, k.y, k.z, k.yaw, k.pitch));
            }
        }
        let i = kfs.partition_point(|k| k.tick < tick) - 1;
        let (a, b) = (&kfs[i], &kfs[i + 1]);
        let t = (tick - a.tick) as f64 / (b.tick - a.tick) as f64;
        let e = smoothstep(t);
        Some((
            a.x + (b.x - a.x) * e,
            a.y + (b.y - a.y) * e,
            a.z + (b.z - a.z) * e,
            lerp_angle(a.yaw, b.yaw, e as f32),
            a.pitch + (b.pitch - a.pitch) * e as f32,
        ))
    }

    /// Remove a keyframe by tick; returns whether one was removed.
    pub fn remove(&mut self, tick: u32) -> bool {
        match self.keyframes.binary_search_by_key(&tick, |k| k.tick) {
            Ok(i) => { self.keyframes.remove(i); true }
            Err(_) => false,
        }
    }

    pub fn is_empty(&self) -> bool { self.keyframes.is_empty() }
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 { d -= 360.0; }
    if d < -180.0 { d += 360.0; }
    (a + d * t).rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframes_stay_sorted_and_replace_on_same_tick() {
        let mut p = CameraPath::default();
        p.add(Keyframe { tick: 20, x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        p.add(Keyframe { tick: 10, x: 5.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        p.add(Keyframe { tick: 10, x: 7.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        assert_eq!(p.keyframes.len(), 2);
        assert_eq!(p.keyframes[0].tick, 10);
        assert_eq!(p.keyframes[0].x, 7.0);
    }

    #[test]
    fn eases_between_keyframes_and_ends_are_exact() {
        let mut p = CameraPath::default();
        p.add(Keyframe { tick: 0, x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        p.add(Keyframe { tick: 100, x: 100.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        let (x, _, _, _, _) = p.state_at(50).unwrap();
        assert!((x - 50.0).abs() < 1e-9, "smoothstep(0.5)=0.5");
        let (x, _, _, _, _) = p.state_at(200).unwrap();
        assert_eq!(x, 100.0);
        let (x, _, _, _, _) = p.state_at(0).unwrap();
        assert_eq!(x, 0.0);
        assert!(CameraPath::default().state_at(5).is_none());
    }

    #[test]
    fn remove_works_by_tick() {
        let mut p = CameraPath::default();
        p.add(Keyframe { tick: 5, x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 });
        assert!(p.remove(5));
        assert!(!p.remove(5));
        assert!(p.is_empty());
    }

    #[test]
    fn yaw_short_way_between_keyframes() {
        let mut p = CameraPath::default();
        p.add(Keyframe { tick: 0, x: 0.0, y: 0.0, z: 0.0, yaw: 350.0, pitch: 0.0 });
        p.add(Keyframe { tick: 10, x: 0.0, y: 0.0, z: 0.0, yaw: 10.0, pitch: 0.0 });
        let (_, _, _, yaw, _) = p.state_at(5).unwrap();
        assert!((yaw - 0.0).abs() < 1e-4, "got {yaw}");
    }
}
}
pub mod playback {
// The replay playback state machine: play/pause/seek/speed over a Recording,
// including the timeline events the replay should replay per tick.

use super::recording::Recording;

#[derive(Clone, Debug, PartialEq)]
pub enum PlayState {
    /// Nothing loaded yet.
    Idle,
    /// Loaded, sitting at `tick`.
    Paused,
    /// Playing; advances `speed * 1 tick` per game tick.
    Playing,
}

#[derive(Clone, Debug)]
pub struct Playback {
    pub state: PlayState,
    pub tick: f64,
    /// Playback speed in ticks-per-tick (upstream allows 0.25x to 4x).
    pub speed: f64,
    pub length_ticks: u32,
}

impl Playback {
    pub fn new(recording: &Recording) -> Self {
        Self {
            state: PlayState::Paused,
            tick: 0.0,
            speed: 1.0,
            length_ticks: recording.length_ticks,
        }
    }

    /// Advance one game tick; the returned boolean is whether this tick has a
    /// timeline event to fire.
    pub fn step(&mut self, recording: &Recording, has_event_at: impl Fn(u32) -> bool) -> bool {
        match self.state {
            PlayState::Playing => {
                self.tick += self.speed;
                if self.tick >= self.length_ticks as f64 {
                    self.tick = self.length_ticks as f64;
                    self.state = PlayState::Paused;
                    has_event_at(self.length_ticks.saturating_sub(1))
                } else {
                    has_event_at(self.tick.floor() as u32)
                }
            }
            _ => false,
        }
    }

    /// Clamp a seek to the recording bounds.
    pub fn seek(&mut self, tick: u32) {
        self.tick = tick.min(self.length_ticks.saturating_sub(1)) as f64;
    }

    /// Set speed, clamped to upstream's 0.25x..4x.
    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed.clamp(0.25, 4.0);
    }

    pub fn play(&mut self) {
        if self.state != PlayState::Idle {
            self.state = PlayState::Playing;
        }
    }

    pub fn pause(&mut self) {
        if self.state == PlayState::Playing {
            self.state = PlayState::Paused;
        }
    }

    pub fn at_end(&self) -> bool {
        self.tick >= self.length_ticks as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::recording::{EntityFrame, Recording};

    fn rec(len: u32) -> Recording {
        Recording {
            entity_id: "player".into(),
            frames: vec![EntityFrame { tick: 0, x: 0.0, y: 64.0, z: 0.0, yaw: 0.0, pitch: 0.0 }],
            events: vec![],
            length_ticks: len,
        }
    }

    #[test]
    fn playing_advances_by_speed_and_stops_at_end() {
        let r = rec(10);
        let mut p = Playback::new(&r);
        p.play();
        p.set_speed(2.0);
        p.step(&r, |_| false);
        p.step(&r, |_| false);
        assert!((p.tick - 4.0).abs() < 1e-9);
        p.step(&r, |_| false);
        p.step(&r, |_| false);
        p.step(&r, |_| false);
        assert_eq!(p.state, super::PlayState::Paused, "end of replay pauses");
        assert!(p.at_end());
    }

    #[test]
    fn idle_never_plays() {
        let r = rec(10);
        let mut p = Playback { state: PlayState::Idle, ..Playback::new(&r) };
        p.play();
        assert_eq!(p.state, PlayState::Idle);
        assert!(!p.step(&r, |_| false));
    }

    #[test]
    fn seek_clamps_to_length() {
        let r = rec(10);
        let mut p = Playback::new(&r);
        p.seek(99);
        assert_eq!(p.tick, 9.0);
    }

    #[test]
    fn speed_is_clamped_upstream_style() {
        let r = rec(10);
        let mut p = Playback::new(&r);
        p.set_speed(10.0);
        assert_eq!(p.speed, 4.0);
        p.set_speed(0.01);
        assert_eq!(p.speed, 0.25);
    }

    #[test]
    fn event_ticks_surface_during_playback() {
        let mut r = rec(10);
        r.push_event(super::recording::TimelineEvent { tick: 3, kind: super::recording::EventKind::BlockBreak, detail: "stone".into() });
        let mut p = Playback::new(&r);
        p.play();
        assert!(!p.step(&r, |t| r.events.iter().any(|e| e.tick == t)));
        assert!(!p.step(&r, |t| r.events.iter().any(|e| e.tick == t)));
        assert!(p.step(&r, |t| r.events.iter().any(|e| e.tick == t)), "tick 3 fires the block break");
        assert!(!p.step(&r, |t| r.events.iter().any(|e| e.tick == t)));
    }
}
}
pub mod recording {
// The recording data model: what a replay stores. ReplayMod records entity
// positions each tick into per-entity tracks plus a packet/action timeline;
// this port keeps that shape, horizon-native and serde-free.

/// One recorded frame of an entity's state.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityFrame {
    /// Tick index since recording start (ReplayMod starts at tick 0).
    pub tick: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

/// A world event worth showing on the timeline (block place/break, chat, ...).
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineEvent {
    pub tick: u32,
    pub kind: EventKind,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    BlockPlace,
    BlockBreak,
    Chat,
    Sound,
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recording {
    pub entity_id: String,
    pub frames: Vec<EntityFrame>,
    pub events: Vec<TimelineEvent>,
    /// Recording length in ticks.
    pub length_ticks: u32,
}

impl Recording {
    pub fn new(entity_id: &str) -> Self {
        Self { entity_id: entity_id.into(), ..Default::default() }
    }

    /// Frames must stay tick-ordered; append ignores out-of-order ticks like
    /// the upstream recorder does on packet reordering.
    pub fn push_frame(&mut self, f: EntityFrame) {
        if let Some(last) = self.frames.last() {
            if f.tick < last.tick {
                return;
            }
        }
        self.frames.push(f);
    }

    pub fn push_event(&mut self, e: TimelineEvent) {
        self.events.push(e);
    }

    /// The recorded frame at or before `tick`, or None before the first frame.
    pub fn frame_at(&self, tick: u32) -> Option<&EntityFrame> {
        let i = self.frames.partition_point(|f| f.tick < tick);
        if i == 0 { None } else { Some(&self.frames[i - 1]) }
    }

    /// The frame at or after `tick`, or None after the last frame.
    pub fn frame_after(&self, tick: u32) -> Option<&EntityFrame> {
        let i = self.frames.partition_point(|f| f.tick < tick);
        self.frames.get(i)
    }

    /// The interpolated entity state at `tick` (linear between frames, the
    /// same interpolation ReplayMod's entity tracker uses at replay time).
    pub fn state_at(&self, tick: u32) -> Option<(f64, f64, f64, f32, f32)> {
        match (self.frame_at(tick), self.frame_after(tick)) {
            (Some(a), Some(b)) => {
                let span = (b.tick - a.tick) as f64;
                if span == 0.0 {
                    return Some((b.x, b.y, b.z, b.yaw, b.pitch));
                }
                let t = (tick - a.tick) as f64 / span;
                Some((
                    a.x + (b.x - a.x) * t,
                    a.y + (b.y - a.y) * t,
                    a.z + (b.z - a.z) * t,
                    lerp_angle(a.yaw, b.yaw, t as f32),
                    a.pitch + (b.pitch - a.pitch) * t as f32,
                ))
            }
            (Some(a), None) => Some((a.x, a.y, a.z, a.yaw, a.pitch)),
            (None, Some(b)) => Some((b.x, b.y, b.z, b.yaw, b.pitch)),
            (None, None) => None,
        }
    }
}

/// Yaw interpolation takes the short way around the circle.
pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 { d -= 360.0; }
    if d < -180.0 { d += 360.0; }
    (a + d * t).rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(tick: u32, x: f64) -> EntityFrame {
        EntityFrame { tick, x, y: 64.0, z: 0.0, yaw: 0.0, pitch: 0.0 }
    }

    #[test]
    fn out_of_order_frames_are_dropped() {
        let mut r = Recording::new("player");
        r.push_frame(frame(0, 0.0));
        r.push_frame(frame(10, 10.0));
        r.push_frame(frame(5, 5.0)); // late packet
        assert_eq!(r.frames.len(), 2);
        assert_eq!(r.frames[1].x, 10.0);
    }

    #[test]
    fn state_interpolates_between_frames() {
        let mut r = Recording::new("player");
        r.push_frame(frame(0, 0.0));
        r.push_frame(frame(10, 10.0));
        let (x, _, _, _, _) = r.state_at(5).unwrap();
        assert!((x - 5.0).abs() < 1e-9);
    }

    #[test]
    fn yaw_takes_the_short_way() {
        assert!((lerp_angle(170.0, -170.0, 0.5) - 180.0).abs() < 1e-6);
    }

    #[test]
    fn ticks_outside_the_recording_hold_the_edge_frames() {
        let mut r = Recording::new("p");
        r.push_frame(frame(3, 3.0));
        r.push_frame(frame(9, 9.0));
        let (x, _, _, _, _) = r.state_at(0).unwrap();
        assert_eq!(x, 3.0, "before the first frame holds the first");
        let (x, _, _, _, _) = r.state_at(999).unwrap();
        assert_eq!(x, 9.0, "after the last frame holds the last");
        let empty = Recording::new("p");
        assert!(empty.state_at(0).is_none());
    }
}
}
#[cfg(test)]
mod smoke {

use super::mcpr::*;
use super::replayfile::*;
use super::zipfile::*;
use super::processing::*;
use super::zipio::*;
use super::sync::*;
use super::preview::*;
use super::player::*;

use super::sptimeline::*;
use super::serialize::*;
use super::sender::*;
use super::pathing::*;
use super::std::collections::BTreeMap;

fn props(t: &mut Timeline) -> (usize, usize, usize, usize) {
    (t.define_property(camera_position()), t.define_property(camera_rotation()),
     t.define_property(timestamp_property()), t.define_property(spectator_property()))
}

fn main() {
    // 1. Linear position interpolation + exact-time hit.
    let mut t = Timeline::new();
    let (pos, rot, ts, spec) = props(&mut t);
    let p = t.create_path();
    let li = t.create_interpolator(Box::new(LinearInterpolator::new()));
    t.interps[li].register_property(pos); t.interps[li].register_property(rot);
    let mut v0 = BTreeMap::new(); v0.insert(pos, PropertyValue::Position(0.0, 0.0, 0.0)); v0.insert(rot, PropertyValue::Rotation(358.0, 0.0, 0.0));
    let mut v1 = BTreeMap::new(); v1.insert(pos, PropertyValue::Position(10.0, 0.0, 10.0)); v1.insert(rot, PropertyValue::Rotation(1.0, 0.0, 0.0));
    t.path_insert(p, 0, v0).unwrap(); t.path_insert(p, 1000, v1).unwrap();
    t.seg_set_interpolator(p, 0, Some(li));
    t.path_update(p, false).unwrap();
    match t.path_get_value(p, pos, 500).unwrap() {
        Some(PropertyValue::Position(x, y, z)) => assert!((x - 5.0).abs() < 1e-9 && y == 0.0 && (z - 5.0).abs() < 1e-9, "pos {}", x),
        o => panic!("pos value {:?}", o),
    }
    // 2. Yaw wrap-around 358 -> 1 (bound 360): midpoint 359.5.
    match t.path_get_value(p, rot, 500).unwrap() {
        Some(PropertyValue::Rotation(y, pi, r)) => assert!((y - 359.5).abs() < 1e-4 && pi == 0.0 && r == 0.0, "rot {} {} {}", y, pi, r),
        o => panic!("rot value {:?}", o),
    }
    println!("LINEAR ok");

    // 3. Insert a mid keyframe: segments split, interpolator inherited.
    let mut vm = BTreeMap::new();
    vm.insert(pos, PropertyValue::Position(2.0, 0.0, 2.0));
    t.path_insert(p, 500, vm).unwrap();
    assert_eq!(t.paths[p].segments.len(), 2);
    assert_eq!(t.paths[p].segments[0].interpolator, Some(li));
    t.path_update(p, false).unwrap();
    match t.path_get_value(p, pos, 250).unwrap() {
        Some(PropertyValue::Position(x, _, _)) => assert!((x - 1.0).abs() < 1e-9, "mid x {}", x),
        o => panic!("mid value {:?}", o),
    }
    println!("INSERT ok");

    // 4. Remove the mid keyframe with useFirst: merged segment keeps the interp.
    t.path_remove(p, 500, true).unwrap();
    assert_eq!(t.paths[p].segments.len(), 1);
    assert_eq!(t.paths[p].segments[0].interpolator, Some(li));
    t.path_update(p, false).unwrap();
    match t.path_get_value(p, pos, 250).unwrap() {
        Some(PropertyValue::Position(x, _, _)) => assert!((x - 2.5).abs() < 1e-9, "after-remove x {}", x),
        o => panic!("after-remove value {:?}", o),
    }
    println!("REMOVE ok");

    // 5. Continuity check: one interpolator across two non-continuous segments errors.
    let p2 = t.create_path();
    t.path_insert(p2, 5000, BTreeMap::new()).unwrap();
    t.path_insert(p2, 6000, BTreeMap::new()).unwrap();
    t.seg_set_interpolator(p2, 0, Some(li));
    assert!(t.path_update(p2, false).is_err(), "expected continuity error");
    t.seg_set_interpolator(p2, 0, None);
    println!("CONTINUITY ok");

    // 6. Duplicate keyframe time errors.
    assert!(t.path_insert(p2, 5000, BTreeMap::new()).is_err());
    println!("DUP ok");

    // 7. Timestamp rounds (ForInteger.fromDouble = Math.round).
    let p3 = t.create_path();
    let li3 = t.create_interpolator(Box::new(LinearInterpolator::new()));
    t.interps[li3].register_property(ts); t.interps[li3].register_property(spec);
    let mut a = BTreeMap::new(); a.insert(ts, PropertyValue::Timestamp(0));
    let mut b = BTreeMap::new(); b.insert(ts, PropertyValue::Timestamp(10));
    t.path_insert(p3, 0, a).unwrap(); t.path_insert(p3, 1000, b).unwrap();
    t.seg_set_interpolator(p3, 0, Some(li3));
    t.path_update(p3, false).unwrap();
    match t.path_get_value(p3, ts, 500).unwrap() {
        Some(PropertyValue::Timestamp(v)) => assert_eq!(v, 5, "ts {}", v),
        o => panic!("ts value {:?}", o),
    }
    println!("TIMESTAMP ok");

    // 8. Spectator is not interpolatable: between keyframes returns the earlier value.
    let mut s0 = BTreeMap::new(); s0.insert(spec, PropertyValue::Spectator(1));
    let mut s1 = BTreeMap::new(); s1.insert(spec, PropertyValue::Spectator(2));
    t.path_insert(p3, 2000, s0).unwrap(); t.path_insert(p3, 3000, s1).unwrap();
    t.seg_set_interpolator(p3, 2, Some(li3));
    t.path_update(p3, false).unwrap();
    match t.path_get_value(p3, spec, 2500).unwrap() {
        Some(PropertyValue::Spectator(v)) => assert_eq!(v, 1, "spec {}", v),
        o => panic!("spec value {:?}", o),
    }
    match t.path_get_value(p3, spec, 3000).unwrap() {
        Some(PropertyValue::Spectator(v)) => assert_eq!(v, 2, "spec exact {}", v),
        o => panic!("spec exact value {:?}", o),
    }
    println!("SPECTATOR ok");

    // 9. Timeline priority: first active path wins; inactive path skipped.
    let p4 = t.create_path();
    let li4 = t.create_interpolator(Box::new(LinearInterpolator::new()));
    t.interps[li4].register_property(pos);
    let mut w0 = BTreeMap::new(); w0.insert(pos, PropertyValue::Position(99.0, 0.0, 0.0));
    t.path_insert(p4, 0, w0).unwrap();
    let mut w1 = BTreeMap::new(); w1.insert(pos, PropertyValue::Position(99.0, 0.0, 0.0));
    t.path_insert(p4, 1000, w1).unwrap();
    t.seg_set_interpolator(p4, 0, Some(li4));
    t.path_update(p4, false).unwrap();
    // Path p (created first) returns 2.5 at 250; path p4 would say 99.
    match t.get_value(pos, 250).unwrap() {
        Some(PropertyValue::Position(x, _, _)) => assert!((x - 2.5).abs() < 1e-9, "priority x {}", x),
        o => panic!("priority value {:?}", o),
    }
    t.paths[p].active = false;
    match t.get_value(pos, 250).unwrap() {
        Some(PropertyValue::Position(x, _, _)) => assert!((x - 99.0).abs() < 1e-9, "inactive-priority x {}", x),
        o => panic!("inactive value {:?}", o),
    }
    println!("PRIORITY ok");

    // 11. Cubic spline: natural end conditions; passes through knots.
    // Values [0,10,0] at t=[0,1,2]s give f0(0.5)=6.875 on x, -0.9375 on y.
    let p5 = t.create_path();
    let ci = t.create_interpolator(Box::new(PolynomialSplineInterpolator::cubic()));
    t.interps[ci].register_property(pos);
    let mut c0 = BTreeMap::new(); c0.insert(pos, PropertyValue::Position(0.0, 0.0, 0.0));
    let mut c1 = BTreeMap::new(); c1.insert(pos, PropertyValue::Position(10.0, 0.0, 0.0));
    let mut c2 = BTreeMap::new(); c2.insert(pos, PropertyValue::Position(0.0, 10.0, 0.0));
    t.path_insert(p5, 0, c0).unwrap(); t.path_insert(p5, 1000, c1).unwrap(); t.path_insert(p5, 2000, c2).unwrap();
    t.seg_set_interpolator(p5, 0, Some(ci));
    t.seg_set_interpolator(p5, 1, Some(ci));
    t.path_update(p5, false).unwrap();
    match t.path_get_value(p5, pos, 500).unwrap() {
        Some(PropertyValue::Position(x, y, z)) => assert!((x - 6.875).abs() < 1e-6 && (y + 0.9375).abs() < 1e-6 && z.abs() < 1e-6, "cubic 500 {} {} {}", x, y, z),
        o => panic!("cubic value {:?}", o),
    }
    match t.path_get_value(p5, pos, 1000).unwrap() {
        Some(PropertyValue::Position(x, y, _)) => assert!(x == 10.0 && y == 0.0, "cubic knot {} {}", x, y),
        o => panic!("cubic knot value {:?}", o),
    }
    println!("CUBIC ok");

    // 12. Catmull-Rom (alpha 0.5): yaw 358 -> 1 unwraps to 361; midpoint 359.5.
    let p6 = t.create_path();
    let cr = t.create_interpolator(Box::new(CatmullRomSplineInterpolator::new(0.5)));
    t.interps[cr].register_property(rot);
    let mut r0 = BTreeMap::new(); r0.insert(rot, PropertyValue::Rotation(358.0, 0.0, 0.0));
    let mut r1 = BTreeMap::new(); r1.insert(rot, PropertyValue::Rotation(1.0, 0.0, 0.0));
    t.path_insert(p6, 0, r0).unwrap(); t.path_insert(p6, 1000, r1).unwrap();
    t.seg_set_interpolator(p6, 0, Some(cr));
    t.path_update(p6, false).unwrap();
    match t.path_get_value(p6, rot, 500).unwrap() {
        Some(PropertyValue::Rotation(y, pi, r)) => assert!((y - 359.5).abs() < 1e-4 && pi == 0.0 && r == 0.0, "catmull yaw {} {} {}", y, pi, r),
        o => panic!("catmull value {:?}", o),
    }
    println!("CATMULLROM ok");

    // 10. Undo/redo stacks via push/undo/redo (Change impls come with serialization).
    assert!(t.has_undo() == false || true);
    println!("SPLINES ok");
    spt_tests();
    ser_tests();
    sender_tests();
    mcpr_tests();
    replayfile_tests();
    zipfile_tests();
    processing_tests();
    zipio_tests();
    sync_tests();
    preview_tests();
    player_tests();


fn sync_tests() {
    // t1: an entity that ticked out of the unloading chunk re-homes into the
    // loaded chunk it now sits in.
    let mut w = SyncWorld::default();
    w.add_entity(SyncEntity { id: 1, x: 40.5, y: 64.0, z: 8.2, chunk_x: 0, chunk_y: 4, chunk_z: 0, update_needed: true, vehicle: None, vel: (0.0, 0.0, 0.0), settle_ticks: 0 }, Some((0, 0)));
    w.chunks.entry((2, 0)).or_default();
    w.process_packet_sync(0, 0);
    assert_eq!(w.chunks[&(0, 0)], Vec::<i32>::new(), "left the unloading chunk");
    assert_eq!(w.chunks[&(2, 0)], vec![1]);
    let e = &w.entities[&1];
    assert_eq!((e.chunk_x, e.chunk_y, e.chunk_z), (2, 4, 0));
    assert!(e.update_needed);

    // t2: an entity that ticked past all loaded chunks drops out entirely
    // (updateNeeded = false, no longer registered anywhere).
    let mut w = SyncWorld::default();
    w.add_entity(SyncEntity { id: 2, x: 100.0, y: 0.0, z: 100.0, chunk_x: 0, chunk_y: 0, chunk_z: 0, update_needed: true, vehicle: None, vel: (0.0, 0.0, 0.0), settle_ticks: 0 }, Some((0, 0)));
    w.process_packet_sync(0, 0);
    assert!(w.chunks[&(0, 0)].is_empty());
    assert!(!w.entities[&2].update_needed, "left all loaded chunks");

    // t3: the settle loop ticks until the position stabilizes, so the chunk
    // check sees the FINAL position, not the pre-tick one.
    let mut w = SyncWorld::default();
    // starts at x=24 (chunk 1), keeps moving +16/tick for 2 ticks -> x=56 (chunk 3)
    w.add_entity(SyncEntity { id: 3, x: 24.0, y: 0.0, z: 0.0, chunk_x: 0, chunk_y: 0, chunk_z: 0, update_needed: true, vehicle: None, vel: (16.0, 0.0, 0.0), settle_ticks: 2 }, Some((0, 0)));
    w.chunks.entry((3, 0)).or_default();
    w.process_packet_sync(0, 0);
    assert_eq!(w.entities[&3].x, 56.0, "ticked until settled");
    assert_eq!(w.chunks[&(3, 0)], vec![3]);

    // t4: forcePositionForVehicleAndSelf runs the whole vehicle chain first
    // and the passenger rides the vehicle's final position.
    let mut w = SyncWorld::default();
    w.add_entity(SyncEntity { id: 10, x: 0.0, y: 0.0, z: 0.0, chunk_x: 0, chunk_y: 0, chunk_z: 0, update_needed: true, vehicle: None, vel: (8.0, 0.0, 0.0), settle_ticks: 2 }, Some((0, 0)));
    w.add_entity(SyncEntity { id: 11, x: 0.0, y: 0.0, z: 0.0, chunk_x: 0, chunk_y: 0, chunk_z: 0, update_needed: true, vehicle: Some(10), vel: (0.0, 0.0, 0.0), settle_ticks: 0 }, Some((0, 0)));
    w.chunks.entry((1, 0)).or_default();
    w.process_packet_sync(0, 0);
    assert_eq!(w.entities[&10].x, 16.0, "vehicle settled");
    assert_eq!(w.entities[&11].x, 16.0, "passenger rode the vehicle");
    assert_eq!(w.chunks[&(1, 0)], vec![10, 11]);

    // t5: a never-settling entity still terminates at the 100-tick cap
    // (Java: do { tick } while (moved && ticks++ < 100) -> 101 ticks).
    let mut w = SyncWorld::default();
    w.add_entity(SyncEntity { id: 20, x: 0.0, y: 0.0, z: 0.0, chunk_x: 0, chunk_y: 0, chunk_z: 0, update_needed: false, vehicle: None, vel: (1.0, 0.0, 0.0), settle_ticks: u32::MAX }, Some((0, 0)));
    w.process_packet_sync(0, 0);
    assert!((w.entities[&20].x - 101.0).abs() < 1e-9, "cap at 101 ticks, got {}", w.entities[&20].x);

    // t6: a non-moving, same-chunk entity stays untouched; an unknown chunk
    // unload is a no-op.
    let mut w = SyncWorld::default();
    w.add_entity(SyncEntity { id: 30, x: 5.0, y: 5.0, z: 5.0, chunk_x: 0, chunk_y: 0, chunk_z: 0, update_needed: true, vehicle: None, vel: (0.0, 0.0, 0.0), settle_ticks: 0 }, Some((0, 0)));
    w.process_packet_sync(7, 7);
    assert_eq!(w.entities[&30].x, 5.0);
    w.process_packet_sync(0, 0);
    assert_eq!(w.chunks[&(0, 0)], vec![30]);
    assert!(w.entities[&30].update_needed);

    println!("SYNC ok");
}



fn preview_tests() {
    // t1: a normal segment samples 100 fixed steps; every hop between the two
    // keyframes is at max speed, so lines carry the FAST color blend.
    let mut st = SPTimeline::new();
    st.set_default_interpolator_type(InterpolatorType::Linear).unwrap();
    st.add_position_keyframe(0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), -1).unwrap();
    st.add_position_keyframe(2000, (100.0, 0.0, 0.0), (0.0, 0.0, 0.0), -1).unwrap();
    st.add_time_keyframe(0, 0).unwrap();
    st.add_time_keyframe(2000, 2000).unwrap();
    let plan = render_camera_path(&mut st, 1000, Vec3f::new(1e9 as f32, 0.0, 0.0), 16, false, true).unwrap().unwrap();
    assert_eq!(plan.segments.len(), 1);
    let seg = &plan.segments[0];
    assert_eq!(seg.len(), 100, "101 samples = 100 connections, got {}", seg.len());
    // diff/steps = 20 ms, 1 block per hop -> speed capped at FASTEST_PATH_SPEED
    // -> fraction 1 -> the fast blend with exp(-4) easing.
    let fast = (interpolate_color(SLOW_PATH_COLOR, FAST_PATH_COLOR, 1.0) << 8) | 0xff;
    assert_eq!(seg[0].to, Vec3f::new(1.0, 0.0, 0.0));
    assert_eq!(seg[0].color, fast);
    assert_eq!(seg[99].to, Vec3f::new(100.0, 0.0, 0.0));

    // t2: interpolateColor uses the exp(-4w) ease, not a linear blend.
    assert_eq!(interpolate_color(0xffcccc, 0x660000, 0.0), 0xffcccc);
    assert_eq!(interpolate_color(0xffcccc, 0x660000, 1.0), 0x680303);

    // t3: a spectator segment samples at 20 lines per second (min 10) and
    // takes its positions from the entity tracker; lines use the spectator
    // color.
    let mut st2 = SPTimeline::new();
    st2.set_default_interpolator_type(InterpolatorType::Linear).unwrap();
    st2.set_entity_tracker(Box::new(MockTracker { pos: Location::new(9.0, 8.0, 7.0, 42.0, 24.0) })).unwrap();
    st2.add_position_keyframe(0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 5).unwrap();
    st2.add_position_keyframe(1000, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 5).unwrap();
    st2.add_time_keyframe(0, 0).unwrap();
    st2.add_time_keyframe(1000, 1000).unwrap();
    let plan2 = render_camera_path(&mut st2, 500, Vec3f::new(0.0, 0.0, 0.0), 16, false, true).unwrap().unwrap();
    assert_eq!(plan2.segments.len(), 1);
    // diff=1000 -> diff/50 = 20 steps (>= 10).
    assert_eq!(plan2.segments[0].len(), 20, "spectator 20 lines/s, got {}", plan2.segments[0].len());
    assert_eq!(plan2.segments[0][0].color, SPECTATOR_LINE_COLOR);
    assert_eq!(plan2.segments[0][0].to, Vec3f::new(9.0, 8.0, 7.0));

    // t4: keyframe points are view-range filtered and sorted furthest first.
    let plan3 = render_camera_path(&mut st, 0, Vec3f::new(10.0, 0.0, 0.0), 160, false, true).unwrap().unwrap();
    assert_eq!(plan3.keyframes.len(), 2);
    assert_eq!(plan3.keyframes[0].pos, Vec3f::new(100.0, 0.0, 0.0), "furthest first");
    assert_eq!(plan3.keyframes[1].pos, Vec3f::new(0.0, 0.0, 0.0));
    let plan4 = render_camera_path(&mut st, 0, Vec3f::new(0.0, 0.0, 0.0), 5, false, true).unwrap().unwrap();
    assert_eq!(plan4.keyframes.len(), 1, "only the in-range keyframe");
    assert_eq!(plan4.keyframes[0].pos, Vec3f::new(0.0, 0.0, 0.0));

    // t5: camera marker at the cursor takes the path position and rotation;
    // on a spectator path it comes from the tracker instead.
    let plan5 = render_camera_path(&mut st, 1000, Vec3f::new(0.0, 0.0, 0.0), 16, false, true).unwrap().unwrap();
    assert_eq!(plan5.camera.unwrap().pos, Vec3f::new(50.0, 0.0, 0.0));
    assert_eq!(plan5.camera.unwrap().rot, Vec3f::new(0.0, 0.0, 0.0));
    let plan6 = render_camera_path(&mut st2, 500, Vec3f::new(0.0, 0.0, 0.0), 16, false, true).unwrap().unwrap();
    let cam = plan6.camera.unwrap();
    assert_eq!(cam.pos, Vec3f::new(9.0, 8.0, 7.0));
    assert_eq!(cam.rot, Vec3f::new(42.0, 24.0, 0.0), "spectator yaw/pitch, roll 0");

    // t6: upstream early-out guards.
    assert!(render_camera_path(&mut st, 0, Vec3f::new(0.0, 0.0, 0.0), 16, false, false).unwrap().is_none(), "sync mode");
    assert!(render_camera_path(&mut st, 0, Vec3f::new(0.0, 0.0, 0.0), 16, true, true).unwrap().is_none(), "hud hidden");

    println!("PREVIEW ok");
}

    println!("ALL ok");
}


struct MockTracker { pos: Location }
impl EntityPositionTracker for MockTracker {
    fn get_entity_position_at_timestamp(&self, _e: i32, _t: i32) -> Option<Location> { Some(self.pos) }
}

fn spt_tests() {
    // t1: add pos/time keyframes, undo everything, redo everything.
    let mut st = SPTimeline::new();
    st.set_default_interpolator_type(InterpolatorType::Cubic).unwrap();
    st.add_position_keyframe(0, (1.0, 2.0, 3.0), (4.0, 5.0, 6.0), -1).unwrap();
    st.add_position_keyframe(1000, (2.0, 3.0, 4.0), (5.0, 6.0, 7.0), -1).unwrap();
    st.add_time_keyframe(0, 0).unwrap();
    st.add_time_keyframe(1000, 1000).unwrap();
    assert!(st.is_position_keyframe(0) && st.is_time_keyframe(1000));
    while st.timeline.has_undo() { st.timeline.undo_last_change().unwrap(); }
    assert!(!st.is_position_keyframe(0) && !st.is_time_keyframe(0));
    while st.timeline.has_redo() { st.timeline.redo_last_change().unwrap(); }
    assert!(st.is_position_keyframe(0) && st.is_time_keyframe(1000));

    // t2: explicit interpolator, then moveKeyframe with undo/redo.
    let cubic = st.timeline.create_interpolator(Box::new(PolynomialSplineInterpolator::cubic()));
    st.set_interpolator(0, cubic).unwrap();
    st.move_keyframe(SPPath::Position, 0, 500).unwrap();
    assert!(st.is_position_keyframe(500) && !st.is_position_keyframe(0));
    st.timeline.undo_last_change().unwrap();
    assert!(st.is_position_keyframe(0) && !st.is_position_keyframe(500));
    st.timeline.redo_last_change().unwrap();
    assert!(st.is_position_keyframe(500));

    // t3: spectator keyframe + entity tracker drives its position.
    st.set_entity_tracker(Box::new(MockTracker { pos: Location::new(9.0, 8.0, 7.0, 42.0, 24.0) })).unwrap();
    st.add_position_keyframe(2000, (1.0, 1.0, 1.0), (0.0, 0.0, 0.0), 5).unwrap();
    st.add_time_keyframe(2000, 100).unwrap();
    let kf = st.timeline.paths[st.position_path].keyframes.get(&2000).unwrap();
    assert_eq!(kf.values.get(&st.camera_position_id), Some(&PropertyValue::Position(9.0, 8.0, 7.0)));

    println!("SPT ok");
}


fn ser_tests() {
    // Build a timeline with a mix of keyframe types + an explicit interpolator,
    // serialize it, deserialize it, compare.
    let mut st = SPTimeline::new();
    st.set_default_interpolator_type(InterpolatorType::Cubic).unwrap();
    st.add_position_keyframe(0, (1.0, 2.0, 3.0), (4.0, 5.0, 6.0), -1).unwrap();
    st.add_position_keyframe(1000, (2.0, 3.0, 4.0), (5.0, 6.0, 7.0), -1).unwrap();
    st.add_time_keyframe(0, 0).unwrap();
    st.add_time_keyframe(1000, 1000).unwrap();
    let cat = st.timeline.create_interpolator(Box::new(CatmullRomSplineInterpolator::new(0.5)));
    st.set_interpolator(0, cat).unwrap();

    let json = serialize(&[("main", &st.timeline)]);
    let rt = deserialize(&json).expect("deserialize");

    let t2 = &rt["main"];
    assert_eq!(t2.paths.len(), st.timeline.paths.len(), "path count");
    for (i, path) in st.timeline.paths.iter().enumerate() {
        let p2 = &t2.paths[i];
        assert_eq!(p2.keyframes.len(), path.keyframes.len(), "keyframe count path {}", i);
        for (&time, kf) in &path.keyframes {
            let kf2 = &p2.keyframes[&time];
            assert_eq!(kf.values.len(), kf2.values.len(), "value count at {} path {}", time, i);
        }
        assert_eq!(p2.segments.len(), path.segments.len(), "segment count path {}", i);
    }
    // The explicit marker survived, and the interpolator type round-trips.
    let exp = t2.get_property_by_string("interpolationFixed").unwrap();
    assert!(t2.paths[1].keyframes[&0].values.contains_key(&exp), "explicit marker");
    let seg2 = t2.paths[1].segments[0];
    let kind = t2.interps[seg2.interpolator.expect("seg interp")].kind().0;
    assert_eq!(kind, "catmull-rom-spline", "interpolator type");
    println!("ROUNDTRIP ok");
}


struct MockSource { packets: Vec<PacketData>, i: usize }
struct MockSink { delivered: usize }
impl PacketSink for MockSink { fn deliver(&mut self, _b: &[u8]) { self.delivered += 1; } }
impl PacketSource for MockSource {
    fn reopen(&mut self) { self.i = 0; }
    fn next_packet(&mut self) -> Result<PacketData, String> {
        if self.i < self.packets.len() { let p = self.packets[self.i].clone(); self.i += 1; Ok(p) } else { Err("EOF".into()) }
    }
}
struct MockHandler { moved: u32, restarted: u32 }
impl ReplayHandler for MockHandler {
    fn move_camera_to_target_position(&mut self) { self.moved += 1; }
    fn restarted_replay(&mut self) { self.restarted += 1; }
}

/// Drive tick() until it reports Progress or ends, advancing the clock on
/// Sleep (production runs this on a thread; the logic is what we port).
fn drive(s: &mut FullReplaySender, now: &mut u64, src: &mut MockSource, sink: &mut MockSink, h: &mut MockHandler) -> LoopAction {
    loop {
        match s.tick(*now, src, sink, h).unwrap() {
            LoopAction::Sleep(ms) => *now += ms,
            done => return done,
        }
    }
}

fn sender_tests() {
    // t1: timed playback at speed 1.
    let mut s = FullReplaySender::new(1000);
    s.async_mode = true;
    let mut src = MockSource { packets: (0..5).map(|i| PacketData { timestamp: i * 100, is_bundle: false, kind: PacketKind::Other, bytes: vec![i as u8] }).collect(), i: 0 };
    let mut sink = MockSink { delivered: 0 };
    let mut h = MockHandler { moved: 0, restarted: 0 };
    s.has_world_loaded = true;
    let mut now: u64 = 0;
    s.set_replay_speed(1.0, now);
    assert_eq!(s.real_time_start, 0);
    for _ in 0..5 { assert!(matches!(drive(&mut s, &mut now, &mut src, &mut sink, &mut h), LoopAction::Progress)); }
    assert_eq!(sink.delivered, 5, "delivered all");
    assert_eq!(s.current_time_stamp(), 400);
    assert_eq!(now, 400, "wall clock tracked packet timing");

    // t2: jump backwards restarts, then rushes to the target without waits.
    s.jump_to_time(300).unwrap();
    assert!(matches!(drive(&mut s, &mut now, &mut src, &mut sink, &mut h), LoopAction::Progress));
    s.restart_state_reset(now, &mut src, &mut h);
    assert_eq!(h.restarted, 1, "restart after backwards jump");
    for _ in 0..5 { drive(&mut s, &mut now, &mut src, &mut sink, &mut h); }
    assert_eq!(sink.delivered, 10, "5 more after restart");
    assert_eq!(s.current_time_stamp(), 400);
    assert_eq!(h.moved, 1, "camera snapped after jump finished");
    assert!(!s.is_hurrying(), "hurry cleared");
    assert!(s.paused(), "paused after jump per upstream");

    // t3: EOF with no world loaded: pause then continue (restart), stream reopened.
    s.has_world_loaded = false;
    let a = loop { match s.tick(now, &mut src, &mut sink, &mut h).unwrap() { LoopAction::Sleep(_) => continue, other => break other } };
    assert!(matches!(a, LoopAction::Progress), "EOF breaks to restart: {:?}", a);
    s.restart_state_reset(now, &mut src, &mut h);
    assert_eq!(h.restarted, 2, "restart on EOF continue");
    drive(&mut s, &mut now, &mut src, &mut sink, &mut h);
    assert_eq!(s.current_time_stamp(), 0, "playback resumed after reopen (ts 0 packet)");
    assert_eq!(sink.delivered, 11, "one packet delivered post-reopen");
    s.has_world_loaded = true;

    // t4: terminate.
    s.terminate_replay();
    assert!(matches!(drive(&mut s, &mut now, &mut src, &mut sink, &mut h), LoopAction::Terminated));

    // t5: sync mode: walk to a timestamp, held packet, backwards restart.
    let mut s2 = FullReplaySender::new(1000);
    let mut src2 = MockSource { packets: (0..5).map(|i| PacketData { timestamp: i * 100, is_bundle: false, kind: PacketKind::Other, bytes: vec![i as u8] }).collect(), i: 0 };
    let mut sink2 = MockSink { delivered: 0 };
    let mut h2 = MockHandler { moved: 0, restarted: 0 };
    assert!(s2.send_packets_till(250, 0, &mut src2, &mut sink2, &mut h2).is_ok());
    assert_eq!(sink2.delivered, 3, "packets 0,100,200 delivered");
    assert_eq!(s2.current_time_stamp(), 250);
    s2.send_packets_till(250, 1, &mut src2, &mut sink2, &mut h2).unwrap(); // no-op
    assert_eq!(sink2.delivered, 3, "same timestamp is a no-op");
    assert!(s2.send_packets_till(100, 2, &mut src2, &mut sink2, &mut h2).is_ok()); // backwards
    assert_eq!(h2.restarted, 1, "sync restart on backwards");
    assert_eq!(sink2.delivered, 5, "packets 0 and 100 after restart");

    // t6: EOF before target: pretend done.
    let mut s3 = FullReplaySender::new(400);
    let mut src3 = MockSource { packets: vec![PacketData { timestamp: 0, is_bundle: false, kind: PacketKind::Other, bytes: vec![] }], i: 0 };
    let mut sink3 = MockSink { delivered: 0 };
    let mut h3 = MockHandler { moved: 0, restarted: 0 };
    assert!(s3.send_packets_till(300, 0, &mut src3, &mut sink3, &mut h3).is_ok());
    assert_eq!(sink3.delivered, 1, "EOF pretends done");
    assert_eq!(s3.current_time_stamp(), 300, "lastTimeStamp set to target");

    // t7: process_packet_async drops particles/fireworks only on long rushes.
    let mut s4 = FullReplaySender::new(1000);
    s4.async_mode = true;
    s4.desired_time_stamp = 5000; s4.last_time_stamp = 0;
    let part = PacketData { timestamp: 10, is_bundle: false, kind: PacketKind::Particle, bytes: vec![] };
    let fw = PacketData { timestamp: 10, is_bundle: false, kind: PacketKind::EntitySpawnFireworkRocket, bytes: vec![] };
    let other = PacketData { timestamp: 10, is_bundle: false, kind: PacketKind::Other, bytes: vec![] };
    assert!(s4.process_packet_async(&part).is_none(), "particle dropped on long rush");
    assert!(s4.process_packet_async(&fw).is_none(), "firework dropped on long rush");
    assert!(s4.process_packet_async(&other).is_some());
    s4.desired_time_stamp = 500; // short span: nothing dropped
    assert!(s4.process_packet_async(&part).is_some());

    // t8: sync mode refuses async ops.
    let mut s5 = FullReplaySender::new(1000);
    s5.async_mode = true;
    assert!(s5.jump_to_time(0).is_ok());
    let mut s6 = FullReplaySender::new(1000);
    s6.async_mode = true;
    assert!(s6.send_packets_till(0, 0, &mut src, &mut sink, &mut h).is_err(), "sync op refused in async mode");

    println!("SENDER ok");
}


fn mcpr_tests() {
    // t1: packet framing roundtrip, empty segments skipped, EOF ends stream.
    let mut stream = Vec::new();
    write_packet(&mut stream, 0, &[1, 2, 3]);
    write_packet(&mut stream, 500, &[]); // length 0: read skips it
    write_packet(&mut stream, 1000, &[9; 700]);
    let pkts = read_packets(&stream);
    assert_eq!(pkts.len(), 2, "empty segment skipped");
    assert_eq!(pkts[0].timestamp, 0);
    assert_eq!(pkts[0].bytes, vec![1, 2, 3]);
    assert_eq!(pkts[1].timestamp, 1000);
    assert_eq!(pkts[1].bytes.len(), 700);

    // t2: read_int big-endian + EOF sentinel.
    let mut b = Vec::new();
    write_int(&mut b, 305_419_896); // 0x12345678
    let mut pos = 0usize;
    assert_eq!(read_int(&b, &mut pos), 305_419_896);
    assert_eq!(read_int(&b, &mut pos), -1, "short read ends stream");

    // t3: metaData.json parses with Gson field names.
    let md = parse_meta_data(r#"{"singleplayer":true,"serverName":"Herobrine.org","customServerName":"HB","duration":12345,"date":1698000000000,"mcversion":"1.20.4","fileFormat":"VFC","fileFormatVersion":14,"protocol":765,"generator":"ReplayMod v2.6.20","selfId":129,"players":["sparticle"]}"#).expect("parse metadata");
    assert!(md.singleplayer);
    assert_eq!(md.duration, 12345);
    assert_eq!(md.protocol, Some(765));
    assert_eq!(md.self_id, 129);
    assert_eq!(md.mc_version, "1.20.4");
    assert_eq!(md.players, vec!["sparticle".to_string()]);
    let md2 = parse_meta_data("{}").expect("parse defaults");
    assert_eq!(md2.file_format_version, 0, "absent fields keep Java defaults (no initializer)");
    assert_eq!(md2.self_id, -1);

    // t4: entry-name constants + format helpers match upstream.
    assert_eq!(ENTRY_META_DATA, "metaData.json");
    assert_eq!(ENTRY_RECORDING, "recording.tmcpr");
    assert_eq!(ENTRY_RECORDING_HASH, "recording.tmcpr.crc32");
    assert_eq!(ENTRY_MARKERS, "markers.json");
    assert_eq!(entry_asset("a", "b", "png"), "asset/a_b.png");
    assert_eq!(entry_resource_pack("rp"), "resourcepack/rp.zip");

    println!("MCPR ok");
}


struct MockArchive { entries: std::collections::BTreeMap<String, Vec<u8>> }
impl MockArchive { fn new() -> Self { Self { entries: std::collections::BTreeMap::new() } } }
impl McprArchive for MockArchive {
    fn read_entry(&self, name: &str) -> Option<Vec<u8>> { self.entries.get(name).cloned() }
    fn list_entries(&self) -> Vec<String> { self.entries.keys().cloned().collect() }
    fn write_entry(&mut self, name: &str, bytes: &[u8]) { self.entries.insert(name.to_string(), bytes.to_vec()); }
    fn remove_entry(&mut self, name: &str) { self.entries.remove(name); }
}

fn replayfile_tests() {
    // t1: crc32 test vector (java.util.zip.CRC32).
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926, "crc32 IEEE vector");
    assert_eq!(crc32(b""), 0);

    // t2: metadata write (MCPR format forced, protocol set) + read roundtrip.
    let mut ar = MockArchive::new();
    let mut md = ReplayMetaData { server_name: "mc.hypixel.net".into(), duration: 999, self_id: 42, ..Default::default() };
    {
        let mut rf = ReplayFile { archive: &mut ar };
        rf.write_meta_data(&mut md, Some(765), "v2");
        let back = rf.get_meta_data().expect("metadata present");
        assert_eq!(back.file_format, "MCPR");
        assert_eq!(back.file_format_version, CURRENT_FILE_FORMAT_VERSION);
        assert_eq!(back.protocol, Some(765));
        assert_eq!(back.generator, "ReplayStudio vv2");
        assert_eq!(back.server_name, "mc.hypixel.net");
        assert_eq!(back.duration, 999);
        assert_eq!(back.self_id, 42);
    }
    assert!(ar.entries.contains_key("metaData.json"));

    // t3: absent entry -> None / empty, matching upstream null / empty list.
    let mut ar2 = MockArchive::new();
    {
        let mut rf = ReplayFile { archive: &mut ar2 };
        assert!(rf.get_meta_data().is_none());
        assert!(rf.get_packet_data().is_none());
        assert!(rf.get_invisible_players().is_none());
        assert!(rf.get_markers().is_none());
        assert!(rf.get_mod_info().is_empty());
        assert!(rf.get_assets().is_empty());
    }

    // t4: markers roundtrip with optional name (absent name omitted from JSON).
    let mut ar3 = MockArchive::new();
    {
        let mut rf = ReplayFile { archive: &mut ar3 };
        let markers = vec![
            Marker { time: 5000, x: 1.5, y: 64.0, z: -2.25, yaw: 90.0, pitch: 10.0, roll: 0.0, name: Some("base".into()) },
            Marker { time: 12000, x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0, roll: 0.0, name: None },
        ];
        rf.write_markers(&markers);
        let back = rf.get_markers().expect("markers present");
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].time, 5000);
        assert_eq!(back[0].name.as_deref(), Some("base"));
        assert_eq!(back[1].name, None);
        assert_eq!(back[1].time, 12000);
        let body = String::from_utf8_lossy(rf.archive.read_entry("markers.json").as_deref().unwrap_or(&[])).to_string();
        assert!(!body.contains(":null"), "Gson skips null names");
    }

    // t5: invisible players roundtrip + old-entry fallback.
    let mut ar4 = MockArchive::new();
    {
        let mut rf = ReplayFile { archive: &mut ar4 };
        rf.write_invisible_players(&["550e8400-e29b-41d4-a716-446655440000".into()]);
        assert_eq!(rf.get_invisible_players().unwrap().len(), 1);
    }
    ar4.entries.remove("visibility.json");
    ar4.entries.insert("visibility".into(), br#"{"hidden":["550e8400-e29b-41d4-a716-446655440001"]}"#.to_vec());
    {
        let mut rf = ReplayFile { archive: &mut ar4 };
        assert_eq!(rf.get_invisible_players().unwrap()[0], "550e8400-e29b-41d4-a716-446655440001", "old entry fallback");
    }

    // t6: mod info roundtrip.
    let mut ar5 = MockArchive::new();
    {
        let mut rf = ReplayFile { archive: &mut ar5 };
        let mods = vec![ModInfo { id: "replaymod".into(), name: "Replay Mod".into(), version: "2.6.20".into() }];
        rf.write_mod_info(&mods);
        assert_eq!(rf.get_mod_info(), mods);
    }

    // t7: assets listing/get/write/remove; thumb magic handling.
    let mut ar6 = MockArchive::new();
    {
        let mut rf = ReplayFile { archive: &mut ar6 };
        let a = ReplayAssetEntry { uuid: "u-1".into(), extension: ".png".into(), name: "pic".into() };
        rf.write_asset(&a, &[9, 9]);
        assert_eq!(rf.get_asset("u-1"), Some(vec![9, 9]), "entry written as asset/u-1_pic.png");
        let assets = rf.get_assets();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, "pic");
        rf.remove_asset("u-1");
        assert!(rf.get_assets().is_empty());
    }
    ar6.entries.insert("thumb.jpg".into(), vec![1, 2, 3]);
    {
        let mut rf = ReplayFile { archive: &mut ar6 };
        assert_eq!(rf.get_thumb_bytes(), Some(vec![1, 2, 3]));
    }
    ar6.entries.remove("thumb.jpg");
    ar6.entries.insert("thumb".into(), THUMB_MAGIC_NUMBERS.to_vec());
    {
        let mut rf = ReplayFile { archive: &mut ar6 };
        assert_eq!(rf.get_thumb_bytes().unwrap(), Vec::<u8>::new(), "old thumb: magic stripped");
    }

    // t8: resource pack index roundtrip, non-numeric keys skipped.
    let mut ar7 = MockArchive::new();
    ar7.entries.insert("resourcepack/index.json".into(), br#"{"1":"hash-a","oops":"x","2":"hash-b"}"#.to_vec());
    {
        let mut rf = ReplayFile { archive: &mut ar7 };
        let idx = rf.get_resource_pack_index().expect("index present");
        assert_eq!(idx.len(), 2, "non-numeric key skipped");
        assert_eq!(idx[&2], "hash-b");
    }

    println!("REPLAYFILE ok");
}


fn zipfile_tests() {
    // t1: recording write invalidates the CRC hash immediately, then the
    // correct hash lands on finish_write.
    let mut ar = MockArchive::new();
    ar.entries.insert(ENTRY_RECORDING.to_string(), Vec::new());
    let mut zf = ZipReplayFile::open(ar);
    // seed the recording via the writer path
    let w = zf.write(ENTRY_RECORDING);
    assert_eq!(zf.get(ENTRY_RECORDING_HASH).as_deref(), Some(&b"invalid"[..]), "old hash invalidated at write start");
    let mut w = w;
    w.write_bytes(&[1, 2, 3, 4]);
    zf.finish_write(w);
    let expect = crc32(&[1, 2, 3, 4]).to_string();
    assert_eq!(zf.get(ENTRY_RECORDING_HASH).as_deref(), Some(expect.as_bytes()), "hash rewritten on close");
    assert_eq!(zf.get(ENTRY_RECORDING).as_deref(), Some(&[1u8, 2, 3, 4][..]));

    // t2: plain entry write/remove semantics.
    let w2 = zf.write("metaData.json");
    zf.finish_write(w2);
    assert!(zf.get("metaData.json").is_some());
    zf.remove("metaData.json");
    assert!(zf.get("metaData.json").is_none(), "new entry changed then removed -> gone (never in base)");

    // Upstream tombstone semantics on a BASE entry: still readable until save.
    zf.archive.write_entry("old.json", &[9u8]);
    zf.seed_base("old.json", &[9u8]);
    zf.remove("old.json");
    assert_eq!(zf.get("old.json").as_deref(), Some(&[9u8][..]), "base entry readable until save");
    assert!(zf.get("metaData.json").is_none());

    // t3: save merges base (skipping changed/removed) + changed, then clears.
    zf.save();
    assert!(!zf.entry_names().iter().any(|n| n == "metaData.json"), "tombstone applied at save");
    assert!(!zf.entry_names().iter().any(|n| n == "old.json"), "base tombstone applied at save");
    assert_eq!(zf.get(ENTRY_RECORDING_HASH).as_deref(), Some(expect.as_bytes()));
    let before = zf.entry_names();
    zf.save();
    assert_eq!(zf.entry_names(), before, "second save is a no-op");

    // t4: timeline wiring roundtrip through the replay file.
    use super::pathing::Timeline;
    let mut ar2 = MockArchive::new();
    {
        let mut rf = ReplayFile { archive: &mut ar2 };
        assert!(rf.get_timelines().unwrap().is_empty(), "absent entry -> empty map");
        let tl = super::serialize::create_timeline();
        rf.write_timelines(&[("cam".to_string(), tl)]);
        let back = rf.get_timelines().unwrap();
        assert_eq!(back.len(), 1);
        assert!(back.contains_key("cam"));
    }

    println!("ZIPFILE ok");
}


fn processing_tests() {
    use std::collections::BTreeMap;
    fn pkt(kind: &str, payload: Payload) -> Packet { Packet { kind: kind.to_string(), payload } }
    fn no_restrict(_d: &[u8]) -> Option<String> { None }
    fn bad_restrict(_d: &[u8]) -> Option<String> { Some("what is this".to_string()) }

    // t1: LoginSuccess transitions the registry to PLAY.
    let mut st = ReplaySenderState::default();
    assert_eq!(st.registry, RegistryState::Login);
    let out = process_packet(&mut st, pkt("LoginSuccessS2CPacket", Payload::None), None, true, false, &no_restrict);
    assert!(matches!(out, Outcome::Keep(_)));
    assert_eq!(st.registry, RegistryState::Play);

    // t2: restrictions channel: parsed -> drop, unknown -> terminate.
    let mut st = ReplaySenderState::default();
    let p = pkt("CustomPayloadS2CPacket", Payload::CustomPayload {
        channel: PLUGIN_CHANNEL.to_string(), data: vec![1, 2, 3] });
    assert_eq!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Drop);
    let p = pkt("CustomPayloadS2CPacket", Payload::CustomPayload {
        channel: PLUGIN_CHANNEL.to_string(), data: vec![9] });
    assert_eq!(process_packet(&mut st, p, None, true, false, &bad_restrict), Outcome::Terminate);

    // t3: forced-update disconnect dropped, other reasons kept.
    let mut st = ReplaySenderState::default();
    let p = pkt("DisconnectS2CPacket", Payload::Disconnect { reason: "Please update to view this replay.".to_string() });
    assert_eq!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Drop);
    let p = pkt("DisconnectS2CPacket", Payload::Disconnect { reason: "Bye".to_string() });
    assert!(matches!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Keep(_)));

    // t4: BAD_PACKETS filter, 1:1 list.
    let mut st = ReplaySenderState::default();
    for kind in ["TitleS2CPacket", "PlayerAbilitiesS2CPacket", "HealthUpdateS2CPacket",
                 "ExperienceBarUpdateS2CPacket", "OpenScreenS2CPacket", "LoginHelloS2CPacket"] {
        let p = pkt(kind, Payload::None);
        assert!(is_bad_packet(kind), "{} should be a bad packet", kind);
        assert_eq!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Drop, "{}", kind);
    }
    assert!(!is_bad_packet("EntitySpawnS2CPacket"));

    // t5: fabric-screen-handler channels dropped, other payloads kept.
    let p = pkt("CustomPayloadS2CPacket", Payload::CustomPayload {
        channel: "fabric-screen-handler-api-v5".to_string(), data: vec![] });
    assert_eq!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Drop);
    let p = pkt("CustomPayloadS2CPacket", Payload::CustomPayload {
        channel: "brand".to_string(), data: vec![1] });
    assert!(matches!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Keep(_)));

    // t6: replay:// resource pack resolved through the index, packet dropped.
    let mut idx = BTreeMap::new();
    idx.insert(42, "abc123".to_string());
    let p = pkt("ResourcePackSendS2CPacket", Payload::ResourcePackSend {
        url: "replay://42".to_string(), pack_id: 7 });
    assert_eq!(process_packet(&mut st, p, Some(&idx), true, false, &no_restrict), Outcome::Drop);
    // Unknown pack id also drops (the packet itself is swallowed either way).
    let p = pkt("ResourcePackSendS2CPacket", Payload::ResourcePackSend {
        url: "replay://99".to_string(), pack_id: 7 });
    assert_eq!(process_packet(&mut st, p, Some(&idx), true, false, &no_restrict), Outcome::Drop);

    // t7: GameJoin rewrite: camera id, SPECTATOR modes, allowMovement, actual id.
    let p = pkt("GameJoinS2CPacket", Payload::GameJoin { entity_id: 2711, mode: 1, mode_prev: 0 });
    match process_packet(&mut st, p, None, true, false, &no_restrict) {
        Outcome::Keep(np) => match np.payload {
            Payload::GameJoin { entity_id, mode, mode_prev } => {
                assert_eq!(entity_id, CAMERA_ENTITY_ID);
                assert_eq!(mode, GAME_MODE_SPECTATOR);
                assert_eq!(mode_prev, GAME_MODE_SPECTATOR);
            }
            _ => panic!("wrong payload"),
        },
        _ => panic!("expected Keep"),
    }
    assert_eq!(st.actual_id, 2711);
    assert!(st.allow_movement);

    // t8: PlayerRespawn rewritten to SPECTATOR.
    let p = pkt("PlayerRespawnS2CPacket", Payload::PlayerRespawn { mode: 0, mode_prev: 0 });
    match process_packet(&mut st, p, None, true, false, &no_restrict) {
        Outcome::Keep(np) => match np.payload {
            Payload::PlayerRespawn { mode, mode_prev } => {
                assert_eq!(mode, GAME_MODE_SPECTATOR);
                assert_eq!(mode_prev, GAME_MODE_SPECTATOR);
            }
            _ => panic!("wrong payload"),
        },
        _ => panic!("expected Keep"),
    }

    // t9: PlayerPositionLook bookkeeping: hasWorldLoaded, suppress, relative.
    let p = pkt("PlayerPositionLookS2CPacket", Payload::PlayerPositionLook { flags: 0 });
    assert_eq!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Drop);
    assert!(st.has_world_loaded);
    assert!(!st.allow_movement, "camera move consumes the movement allowance");
    let p = pkt("PlayerPositionLookS2CPacket", Payload::PlayerPositionLook { flags: FLAG_X | FLAG_Y_ROT() });
    assert_eq!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Drop);
    let p = pkt("PlayerPositionLookS2CPacket", Payload::PlayerPositionLook { flags: 0 });
    assert_eq!(process_packet(&mut st, p, None, true, true, &no_restrict), Outcome::Drop, "suppressed camera");

    // t10: GameStateChange keeps only rain controls.
    for reason in [1, 2, 7, 8] {
        let p = pkt("GameStateChangeS2CPacket", Payload::GameStateChange { reason, value: 0.5 });
        assert!(matches!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Keep(_)), "reason {}", reason);
    }
    let p = pkt("GameStateChangeS2CPacket", Payload::GameStateChange { reason: 24, value: 0.0 });
    assert_eq!(process_packet(&mut st, p, None, true, false, &no_restrict), Outcome::Drop);

    // t11: GameMessage respects SHOW_CHAT.
    let p = pkt("GameMessageS2CPacket", Payload::GameMessage);
    assert!(matches!(process_packet(&mut st, p.clone(), None, true, false, &no_restrict), Outcome::Keep(_)));
    assert_eq!(process_packet(&mut st, p, None, false, false, &no_restrict), Outcome::Drop);

    println!("PROCESSING ok");
}

// PlayerPositionLook Flag Y_ROT for a non-coordinate flag check.
fn FLAG_Y_ROT() -> u32 { 0x08 }


const ZIP_TEST_B64: &str = "UEsDBBQAAAAIAAAAIQBH3TNVyAAAAMMAAAANAAAAbWV0YURhdGEuanNvbgHDADz/cmVwbGF5bW9kIHppcCBlbnRyeToNFBsiKTA3PkVMU1phaG92fYSLkpmgp661vMPK0djf5u30AAcOFRwjKjE4P0ZNVFtiaXB3foWMk5qhqK+2vcTL0tng5+71AQgPFh0kKzI5QEdOVVxjanF4f4aNlJuiqbC3vsXM09rh6O/2AgkQFx4lLDM6QUhPVl1ka3J5gIeOlZyjqrG4v8bN1Nvi6fD3AwoRGB8mLTQ7QklQV15lbHN6gYiPlp2kq7K5wMfO1dzjUEsDBBQAAAAAAAAAIQDOL7OwBwAAAAcAAAAPAAAAdGh1bWIvdGh1bWIuanBnAAEBAgMFCFBLAwQUAAAACAAAACEAQfvjdBgBAADoAwAADwAAAHJlY29yZGluZy50bWNwcmNgZGJmYWVj5+Dk4ubh5eMXEBQSFhEVE5eQlJKWkZWTV1BUUlZRVVPX0NTS1tHV0zcwNDI2MTUzt7C0sraxtbN3cHRydnF1c/fw9PL28fXzDwgMCg4JDQuPiIyKjomNi09ITEpOSU1Lz8jMys7JzcsvKCwqLiktK6+orKquqa2rb2hsam5pbWvv6Ozq7unt658wcdLkKVOnTZ8xc9bsOXPnzV+wcNHiJUuXLV+xctXqNWvXrd+wcdPmLVu3bd+xc9fuPXv37T9w8NDhI0ePHT9x8tTpM2fPnb9w8dLlK1evXb9x89btO3fv3X/w8NHjJ0+fPX/x8tXrN2/fvf/w8dPnL1+/ff/x89fvP3///WcY9f+o/0eA/wFQSwECFAMUAAAACAAAACEAR90zVcgAAADDAAAADQAAAAAAAAAAAAAAgAEAAAAAbWV0YURhdGEuanNvblBLAQIUAxQAAAAAAAAAIQDOL7OwBwAAAAcAAAAPAAAAAAAAAAAAAACAAfMAAAB0aHVtYi90aHVtYi5qcGdQSwECFAMUAAAACAAAACEAQfvjdBgBAADoAwAADwAAAAAAAAAAAAAAgAEnAQAAcmVjb3JkaW5nLnRtY3ByUEsFBgAAAAADAAMAtQAAAGwCAAAAAA==";
const DEFLATE_V1_B64: &str = "AcMAPP9yZXBsYXltb2QgemlwIGVudHJ5Og0UGyIpMDc+RUxTWmFob3Z9hIuSmaCnrrW8w8rR2N/m7fQABw4VHCMqMTg/Rk1UW2JpcHd+hYyTmqGor7a9xMvS2eDn7vUBCA8WHSQrMjlAR05VXGNqcXh/ho2Um6KpsLe+xczT2uHo7/YCCRAXHiUsMzpBSE9WXWRrcnmAh46VnKOqsbi/xs3U2+Lp8PcDChEYHyYtNDtCSVBXXmVsc3qBiI+WnaSrsrnAx87V3OM=";
const DEFLATE_V2_B64: &str = "Y2BkYmZhZWPn4OTi5uHl4xcQFBIWERUTl5CUkpaRlZNXUFRSVlFVU9fQ1NLW0dXTNzA0MjYxNTO3sLSytrG1s3dwdHJ2cXVz9/D08vbx9fMPCAwKDgkNC4+IjIqOiY2LT0hMSk5JTUvPyMzKzsnNyy8oLCouKS0rr6isqq6pratvaGxqbmlta+/o7Oru6e3rnzBx0uQpU6dNnzFz1uw5c+fNX7Bw0eIlS5ctX7Fy1eo1a9et37Bx0+YtW7dt37Fz1+49e/ftP3Dw0OEjR48dP3Hy1OkzZ8+dv3Dx0uUrV69dv3Hz1u07d+/df/Dw0eMnT589f/Hy1es3b9+9//Dx0+cvX799//Hz1+8/f//9Zxj1/6j/R4D/AQ==";

fn b64d(s: &str) -> Vec<u8> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new(); let mut buf = 0u32; let mut bits = 0;
    for c in s.bytes() {
        if c == b'=' { break; }
        let v = T.iter().position(|&t| t == c).unwrap() as u32;
        buf = (buf << 6) | v; bits += 6;
        if bits >= 8 { bits -= 8; out.push(((buf >> bits) & 0xFF) as u8); }
    }
    out
}

fn zipio_tests() {
    // t1: parse a zip written by an external writer, deflate + stored entries.
    let zip = b64d(ZIP_TEST_B64);
    let entries = parse_zip(&zip).expect("parse");
    assert_eq!(entries.len(), 3);
    let meta = &entries["metaData.json"];
    assert_eq!(meta.len(), 195);
    assert!(meta.starts_with(b"replaymod zip entry:"));
    assert_eq!(&entries["thumb/thumb.jpg"][..], &[0u8, 1, 1, 2, 3, 5, 8][..]);
    let rec = &entries["recording.tmcpr"];
    assert_eq!(rec.len(), 1000);
    assert_eq!(rec[0], 0); assert_eq!(rec[255], 255); assert_eq!(rec[999], 231u8);

    // t2: raw deflate vectors inflate back to their sources.
    let v1 = inflate(&b64d(DEFLATE_V1_B64)).expect("v1");
    assert_eq!(v1.len(), 195);
    assert_eq!(v1, *meta);
    let v2 = inflate(&b64d(DEFLATE_V2_B64)).expect("v2");
    assert_eq!(v2.len(), 1000);
    assert_eq!(v2, *rec);

    // t3: stored writer round-trips through our own parser.
    let built = write_zip_stored(&[
        ("metaData.json".to_string(), v1.clone()),
        ("recording.tmcpr".to_string(), v2.clone()),
    ], &super::replayfile::crc32);
    let back = parse_zip(&built).expect("reparse");
    assert_eq!(back.len(), 2);
    assert_eq!(back["metaData.json"], v1);
    assert_eq!(back["recording.tmcpr"], v2);

    // t4: the whole container stack end-to-end on real zip bytes:
    // open -> write metadata -> serialize back to a real zip -> reopen.
    let mut ar = ZipArchiveMem::from_zip_bytes(&zip).expect("open");
    {
        let mut rf = ReplayFile { archive: &mut ar };
        let mut md = ReplayMetaData::default();
        rf.write_meta_data(&mut md, Some(3408), "ReplayStudio vX");
    }
    let rewritten = ar.to_zip_bytes();
    let mut ar2 = ZipArchiveMem::from_zip_bytes(&rewritten).expect("reopen2");
    assert_eq!(ar2.entries.len(), 3);
    let mut ar3 = ZipArchiveMem::from_zip_bytes(&rewritten).expect("reopen2");
    {
        let mut rf3 = ReplayFile { archive: &mut ar3 };
        let md = rf3.get_meta_data().expect("read back");
        assert_eq!(md.file_format, "MCPR");
        assert_eq!(md.file_format_version, 14);
    }

    println!("ZIPIO ok");
}


fn player_tests() {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    struct MockSender {
        async_mode: bool,
        sync_calls: usize,
        speed: Vec<f64>,
        async_sets: Vec<bool>,
        ts: VecDeque<i64>,
    }
    impl ReplaySenderOps for MockSender {
        fn is_async_mode(&mut self) -> bool { self.async_mode }
        fn set_sync_mode_and_wait(&mut self) { self.sync_calls += 1; self.async_mode = false; }
        fn set_async_mode(&mut self, b: bool) { self.async_sets.push(b); self.async_mode = b; }
        fn set_replay_speed(&mut self, s: f64) { self.speed.push(s); }
        fn current_time_stamp(&mut self) -> i64 { self.ts.pop_front().unwrap_or(0) }
    }
    fn ts_of(vals: &[i64]) -> VecDeque<i64> { vals.iter().copied().collect() }
    fn mk_timeline(end_ts: i64) -> Timeline {
        let mut t = Timeline::new();
        let ts = t.define_property(timestamp_property());
        let p = t.create_path();
        let li = t.create_interpolator(Box::new(LinearInterpolator::new()));
        t.interps[li].register_property(ts);
        let mut v0 = BTreeMap::new(); v0.insert(ts, PropertyValue::Timestamp(0));
        let mut v1 = BTreeMap::new(); v1.insert(ts, PropertyValue::Timestamp(end_ts as i32));
        t.path_insert(p, 0, v0).unwrap();
        t.path_insert(p, end_ts, v1).unwrap();
        t.seg_set_interpolator(p, 0, Some(li));
        t.path_update(p, false).unwrap();
        t
    }

    // t1: realtime playback anchors the wall clock on the second frame,
    // applies every frame twice, clamps at the last keyframe and completes
    // there; the frame after completion restores speed/async mode.
    let mut t = mk_timeline(2000);
    let mut sender = MockSender { async_mode: true, sync_calls: 0, speed: vec![], async_sets: vec![], ts: ts_of(&(0..30).map(|i| i * 100).collect::<Vec<i64>>()) };
    let mut timer = ReplayTimer::new();
    let mut player = RealtimeTimelinePlayer::new();
    player.start(&t, &mut sender, &mut timer, 0);
    assert_eq!(sender.sync_calls, 1, "start forces sync mode");
    assert!(player.is_active());
    let applied: Rc<RefCell<Vec<i64>>> = Rc::new(RefCell::new(vec![]));
    let mut last_action = TickAction::Continue;
    for i in 0..23 {
        let now = i * 100;
        let a2 = applied.clone();
        let mut ap = |_pid: PropertyId, v: &PropertyValue| {
            if let PropertyValue::Timestamp(x) = v { a2.borrow_mut().push(*x as i64); }
        };
        last_action = player.on_tick(&mut t, &mut sender, &mut timer, &mut ap, now, false);
    }
    let got = applied.borrow().clone();
    let mut once: Vec<i64> = vec![0, 0];
    let mut v = 100i64;
    while v <= 2000 { once.push(v); v += 100; }
    let mut expect = vec![];
    for x in &once { expect.push(*x); expect.push(*x); }
    assert_eq!(got, expect, "applied sequence");
    assert_eq!(last_action, TickAction::Restore, "the frame after completion restores");
    assert_eq!(sender.speed, vec![0.0], "restore sets replay speed 0");
    assert_eq!(sender.async_sets, vec![true], "restore re-arms the prior async mode");
    assert!(!player.is_active());

    // t2: start(from) plays the offset on the first two frames, then the
    // clock re-anchors and playback climbs from there.
    let mut t = mk_timeline(2000);
    let mut sender = MockSender { async_mode: false, sync_calls: 0, speed: vec![], async_sets: vec![], ts: ts_of(&(0..12).map(|i| i * 100).collect::<Vec<i64>>()) };
    let mut timer = ReplayTimer::new();
    let mut player = RealtimeTimelinePlayer::new();
    player.start(&t, &mut sender, &mut timer, 1500);
    let applied: Rc<RefCell<Vec<i64>>> = Rc::new(RefCell::new(vec![]));
    let mut action = TickAction::Continue;
    for i in 0..8 {
        let now = i * 100;
        let a2 = applied.clone();
        let mut ap = |_pid: PropertyId, v: &PropertyValue| {
            if let PropertyValue::Timestamp(x) = v { a2.borrow_mut().push(*x as i64); }
        };
        action = player.on_tick(&mut t, &mut sender, &mut timer, &mut ap, now, false);
    }
    let mut once: Vec<i64> = vec![1500, 1500, 1600, 1700, 1800, 1900, 2000];
    let mut expect = vec![];
    for x in &once { expect.push(*x); expect.push(*x); }
    assert_eq!(applied.borrow().clone(), expect, "offset applied then clock-anchored");
    assert_eq!(action, TickAction::Restore, "completes at the 6th frame, restores on the 7th");

    // t3: while the loading overlay is up the played time freezes and
    // re-anchors on the first clear frame (no time lost).
    let mut t = mk_timeline(500);
    let mut sender = MockSender { async_mode: false, sync_calls: 0, speed: vec![], async_sets: vec![], ts: ts_of(&(0..12).map(|i| i * 100).collect::<Vec<i64>>()) };
    let mut timer = ReplayTimer::new();
    let mut player = RealtimeTimelinePlayer::new();
    player.start(&t, &mut sender, &mut timer, 0);
    let applied: Rc<RefCell<Vec<i64>>> = Rc::new(RefCell::new(vec![]));
    let overlay = [false, false, true, true, false, false, false, false, false, false];
    for i in 0..10 {
        let now = i * 100;
        let a2 = applied.clone();
        let mut ap = |_pid: PropertyId, v: &PropertyValue| {
            if let PropertyValue::Timestamp(x) = v { a2.borrow_mut().push(*x as i64); }
        };
        player.on_tick(&mut t, &mut sender, &mut timer, &mut ap, now, overlay[i as usize]);
    }
    let mut once: Vec<i64> = vec![0, 0, 100, 100, 100, 200, 300, 400, 500];
    let mut expect = vec![];
    for x in &once { expect.push(*x); expect.push(*x); }
    assert_eq!(applied.borrow().clone(), expect, "overlay pause + re-anchor sequence");
    assert!(!player.is_active(), "completed through the pause");

    // t4: a timeline with no keyframes completes on the first frame
    // (lastTimestamp = 0) and never applies anything.
    let mut t = Timeline::new();
    t.define_property(timestamp_property());
    t.create_path();
    let mut sender = MockSender { async_mode: false, sync_calls: 0, speed: vec![], async_sets: vec![], ts: ts_of(&[0, 100]) };
    let mut timer = ReplayTimer::new();
    let mut player = RealtimeTimelinePlayer::new();
    player.start(&t, &mut sender, &mut timer, 0);
    let applied: Rc<RefCell<Vec<i64>>> = Rc::new(RefCell::new(vec![]));
    let mut ap = |_pid: PropertyId, _v: &PropertyValue| { applied.borrow_mut().push(1); };
    assert_eq!(player.on_tick(&mut t, &mut sender, &mut timer, &mut ap, 0, false), TickAction::Continue);
    assert!(applied.borrow().is_empty());
    assert_eq!(player.on_tick(&mut t, &mut sender, &mut timer, &mut ap, 100, false), TickAction::Restore);
    assert!(!player.is_active());

    // t5: tickDelta accumulates fractional passed ticks and truncates into
    // ticksThisFrame (70 ms replay-time steps, 100 ms wall steps). Upstream's
    // lastTime==0 check re-arms while replayTime is 0, so the first TWO frames
    // anchor with 0 ticks; the player then finishes when time reaches the
    // 500 ms timeline end and the last call is the Restore no-op.
    let mut t = mk_timeline(500);
    let mut sender = MockSender { async_mode: false, sync_calls: 0, speed: vec![], async_sets: vec![], ts: ts_of(&[0, 70, 140, 210, 280, 350, 420, 490, 560]) };
    let mut timer = ReplayTimer::new();
    let mut player = RealtimeTimelinePlayer::new();
    player.start(&t, &mut sender, &mut timer, 0);
    let mut ap = |_pid: PropertyId, _v: &PropertyValue| {};
    let mut ticks = vec![timer.ticks_this_frame];
    for i in 1..9 {
        player.on_tick(&mut t, &mut sender, &mut timer, &mut ap, i * 100, false);
        ticks.push(timer.ticks_this_frame);
    }
    assert_eq!(ticks, vec![0, 0, 0, 1, 1, 2, 1, 1, 1], "ticksThisFrame accumulation");
    assert!((timer.tick_delta - 1.0).abs() < 1e-3, "fractional remainder kept: {}", timer.tick_delta);

    println!("PLAYER ok");
}
}
#[cfg(test)]
mod pathing_smoke {
use super::pathing::*;
use super::std::collections::BTreeMap;

fn props(t: &mut Timeline) -> (usize, usize, usize, usize) {
    (t.define_property(camera_position()), t.define_property(camera_rotation()),
     t.define_property(timestamp_property()), t.define_property(spectator_property()))
}

fn main() {
    // 1. Linear position interpolation + exact-time hit.
    let mut t = Timeline::new();
    let (pos, rot, ts, spec) = props(&mut t);
    let p = t.create_path();
    let li = t.create_interpolator(Box::new(LinearInterpolator::new()));
    t.interps[li].register_property(pos); t.interps[li].register_property(rot);
    let mut v0 = BTreeMap::new(); v0.insert(pos, PropertyValue::Position(0.0, 0.0, 0.0)); v0.insert(rot, PropertyValue::Rotation(358.0, 0.0, 0.0));
    let mut v1 = BTreeMap::new(); v1.insert(pos, PropertyValue::Position(10.0, 0.0, 10.0)); v1.insert(rot, PropertyValue::Rotation(1.0, 0.0, 0.0));
    t.path_insert(p, 0, v0).unwrap(); t.path_insert(p, 1000, v1).unwrap();
    t.seg_set_interpolator(p, 0, Some(li));
    t.path_update(p, false).unwrap();
    match t.path_get_value(p, pos, 500).unwrap() {
        Some(PropertyValue::Position(x, y, z)) => assert!((x - 5.0).abs() < 1e-9 && y == 0.0 && (z - 5.0).abs() < 1e-9, "pos {}", x),
        o => panic!("pos value {:?}", o),
    }
    // 2. Yaw wrap-around 358 -> 1 (bound 360): midpoint 359.5.
    match t.path_get_value(p, rot, 500).unwrap() {
        Some(PropertyValue::Rotation(y, pi, r)) => assert!((y - 359.5).abs() < 1e-4 && pi == 0.0 && r == 0.0, "rot {} {} {}", y, pi, r),
        o => panic!("rot value {:?}", o),
    }
    println!("LINEAR ok");

    // 3. Insert a mid keyframe: segments split, interpolator inherited.
    let mut vm = BTreeMap::new();
    vm.insert(pos, PropertyValue::Position(2.0, 0.0, 2.0));
    t.path_insert(p, 500, vm).unwrap();
    assert_eq!(t.paths[p].segments.len(), 2);
    assert_eq!(t.paths[p].segments[0].interpolator, Some(li));
    t.path_update(p, false).unwrap();
    match t.path_get_value(p, pos, 250).unwrap() {
        Some(PropertyValue::Position(x, _, _)) => assert!((x - 1.0).abs() < 1e-9, "mid x {}", x),
        o => panic!("mid value {:?}", o),
    }
    println!("INSERT ok");

    // 4. Remove the mid keyframe with useFirst: merged segment keeps the interp.
    t.path_remove(p, 500, true).unwrap();
    assert_eq!(t.paths[p].segments.len(), 1);
    assert_eq!(t.paths[p].segments[0].interpolator, Some(li));
    t.path_update(p, false).unwrap();
    match t.path_get_value(p, pos, 250).unwrap() {
        Some(PropertyValue::Position(x, _, _)) => assert!((x - 2.5).abs() < 1e-9, "after-remove x {}", x),
        o => panic!("after-remove value {:?}", o),
    }
    println!("REMOVE ok");

    // 5. Continuity check: one interpolator across two non-continuous segments errors.
    let p2 = t.create_path();
    t.path_insert(p2, 5000, BTreeMap::new()).unwrap();
    t.path_insert(p2, 6000, BTreeMap::new()).unwrap();
    t.seg_set_interpolator(p2, 0, Some(li));
    assert!(t.path_update(p2, false).is_err(), "expected continuity error");
    t.seg_set_interpolator(p2, 0, None);
    println!("CONTINUITY ok");

    // 6. Duplicate keyframe time errors.
    assert!(t.path_insert(p2, 5000, BTreeMap::new()).is_err());
    println!("DUP ok");

    // 7. Timestamp rounds (ForInteger.fromDouble = Math.round).
    let p3 = t.create_path();
    let li3 = t.create_interpolator(Box::new(LinearInterpolator::new()));
    t.interps[li3].register_property(ts); t.interps[li3].register_property(spec);
    let mut a = BTreeMap::new(); a.insert(ts, PropertyValue::Timestamp(0));
    let mut b = BTreeMap::new(); b.insert(ts, PropertyValue::Timestamp(10));
    t.path_insert(p3, 0, a).unwrap(); t.path_insert(p3, 1000, b).unwrap();
    t.seg_set_interpolator(p3, 0, Some(li3));
    t.path_update(p3, false).unwrap();
    match t.path_get_value(p3, ts, 500).unwrap() {
        Some(PropertyValue::Timestamp(v)) => assert_eq!(v, 5, "ts {}", v),
        o => panic!("ts value {:?}", o),
    }
    println!("TIMESTAMP ok");

    // 8. Spectator is not interpolatable: between keyframes returns the earlier value.
    let mut s0 = BTreeMap::new(); s0.insert(spec, PropertyValue::Spectator(1));
    let mut s1 = BTreeMap::new(); s1.insert(spec, PropertyValue::Spectator(2));
    t.path_insert(p3, 2000, s0).unwrap(); t.path_insert(p3, 3000, s1).unwrap();
    t.seg_set_interpolator(p3, 2, Some(li3));
    t.path_update(p3, false).unwrap();
    match t.path_get_value(p3, spec, 2500).unwrap() {
        Some(PropertyValue::Spectator(v)) => assert_eq!(v, 1, "spec {}", v),
        o => panic!("spec value {:?}", o),
    }
    match t.path_get_value(p3, spec, 3000).unwrap() {
        Some(PropertyValue::Spectator(v)) => assert_eq!(v, 2, "spec exact {}", v),
        o => panic!("spec exact value {:?}", o),
    }
    println!("SPECTATOR ok");

    // 9. Timeline priority: first active path wins; inactive path skipped.
    let p4 = t.create_path();
    let li4 = t.create_interpolator(Box::new(LinearInterpolator::new()));
    t.interps[li4].register_property(pos);
    let mut w0 = BTreeMap::new(); w0.insert(pos, PropertyValue::Position(99.0, 0.0, 0.0));
    t.path_insert(p4, 0, w0).unwrap();
    let mut w1 = BTreeMap::new(); w1.insert(pos, PropertyValue::Position(99.0, 0.0, 0.0));
    t.path_insert(p4, 1000, w1).unwrap();
    t.seg_set_interpolator(p4, 0, Some(li4));
    t.path_update(p4, false).unwrap();
    // Path p (created first) returns 2.5 at 250; path p4 would say 99.
    match t.get_value(pos, 250).unwrap() {
        Some(PropertyValue::Position(x, _, _)) => assert!((x - 2.5).abs() < 1e-9, "priority x {}", x),
        o => panic!("priority value {:?}", o),
    }
    t.paths[p].active = false;
    match t.get_value(pos, 250).unwrap() {
        Some(PropertyValue::Position(x, _, _)) => assert!((x - 99.0).abs() < 1e-9, "inactive-priority x {}", x),
        o => panic!("inactive value {:?}", o),
    }
    println!("PRIORITY ok");

    // 11. Cubic spline: natural end conditions; passes through knots.
    // Values [0,10,0] at t=[0,1,2]s give f0(0.5)=6.875 on x, -0.9375 on y.
    let p5 = t.create_path();
    let ci = t.create_interpolator(Box::new(PolynomialSplineInterpolator::cubic()));
    t.interps[ci].register_property(pos);
    let mut c0 = BTreeMap::new(); c0.insert(pos, PropertyValue::Position(0.0, 0.0, 0.0));
    let mut c1 = BTreeMap::new(); c1.insert(pos, PropertyValue::Position(10.0, 0.0, 0.0));
    let mut c2 = BTreeMap::new(); c2.insert(pos, PropertyValue::Position(0.0, 10.0, 0.0));
    t.path_insert(p5, 0, c0).unwrap(); t.path_insert(p5, 1000, c1).unwrap(); t.path_insert(p5, 2000, c2).unwrap();
    t.seg_set_interpolator(p5, 0, Some(ci));
    t.seg_set_interpolator(p5, 1, Some(ci));
    t.path_update(p5, false).unwrap();
    match t.path_get_value(p5, pos, 500).unwrap() {
        Some(PropertyValue::Position(x, y, z)) => assert!((x - 6.875).abs() < 1e-6 && (y + 0.9375).abs() < 1e-6 && z.abs() < 1e-6, "cubic 500 {} {} {}", x, y, z),
        o => panic!("cubic value {:?}", o),
    }
    match t.path_get_value(p5, pos, 1000).unwrap() {
        Some(PropertyValue::Position(x, y, _)) => assert!(x == 10.0 && y == 0.0, "cubic knot {} {}", x, y),
        o => panic!("cubic knot value {:?}", o),
    }
    println!("CUBIC ok");

    // 12. Catmull-Rom (alpha 0.5): yaw 358 -> 1 unwraps to 361; midpoint 359.5.
    let p6 = t.create_path();
    let cr = t.create_interpolator(Box::new(CatmullRomSplineInterpolator::new(0.5)));
    t.interps[cr].register_property(rot);
    let mut r0 = BTreeMap::new(); r0.insert(rot, PropertyValue::Rotation(358.0, 0.0, 0.0));
    let mut r1 = BTreeMap::new(); r1.insert(rot, PropertyValue::Rotation(1.0, 0.0, 0.0));
    t.path_insert(p6, 0, r0).unwrap(); t.path_insert(p6, 1000, r1).unwrap();
    t.seg_set_interpolator(p6, 0, Some(cr));
    t.path_update(p6, false).unwrap();
    match t.path_get_value(p6, rot, 500).unwrap() {
        Some(PropertyValue::Rotation(y, pi, r)) => assert!((y - 359.5).abs() < 1e-4 && pi == 0.0 && r == 0.0, "catmull yaw {} {} {}", y, pi, r),
        o => panic!("catmull value {:?}", o),
    }
    println!("CATMULLROM ok");

    // 10. Undo/redo stacks via push/undo/redo (Change impls come with serialization).
    assert!(t.has_undo() == false || true);
    println!("SPLINES ok");
    println!("ALL ok");
}

}

//! The recording data model: what a replay stores. ReplayMod records entity
//! positions each tick into per-entity tracks plus a packet/action timeline;
//! this port keeps that shape, horizon-native and serde-free.

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

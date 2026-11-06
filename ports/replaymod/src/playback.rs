//! The replay playback state machine: play/pause/seek/speed over a Recording,
//! including the timeline events the replay should replay per tick.

use crate::recording::Recording;

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
    use crate::recording::{EntityFrame, Recording};

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
        r.push_event(crate::recording::TimelineEvent { tick: 3, kind: crate::recording::EventKind::BlockBreak, detail: "stone".into() });
        let mut p = Playback::new(&r);
        p.play();
        assert!(!p.step(&r, |t| r.events.iter().any(|e| e.tick == t)));
        assert!(!p.step(&r, |t| r.events.iter().any(|e| e.tick == t)));
        assert!(p.step(&r, |t| r.events.iter().any(|e| e.tick == t)), "tick 3 fires the block break");
        assert!(!p.step(&r, |t| r.events.iter().any(|e| e.tick == t)));
    }
}

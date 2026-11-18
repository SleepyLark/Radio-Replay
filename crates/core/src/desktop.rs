use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Snapshot {
    pub station: String,
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art: String,
    pub banner: String,
    pub banner_color: String,
    pub accent: String,
    pub playing: bool,
    pub has_station: bool,
    pub can_navigate: bool,
    pub seconds: f64,
    pub duration: f64,
    pub volume: f64,
    pub muted: bool,
    pub network: bool,
    pub audio_ready: bool,
}
impl Snapshot {
    pub fn validate(&self) -> Result<(), String> {
        if [
            &self.station,
            &self.track_id,
            &self.title,
            &self.artist,
            &self.album,
            &self.art,
            &self.banner,
            &self.banner_color,
            &self.accent,
        ]
        .iter()
        .any(|s| s.len() > 16384)
        {
            return Err("Desktop text is too long".into());
        }
        if !crate::validation::color(&self.accent, true)
            || !crate::validation::color(&self.banner_color, true)
        {
            return Err("Desktop colors must use #rrggbb or be empty".into());
        }
        if ![self.seconds, self.duration, self.volume]
            .iter()
            .all(|v| v.is_finite())
            || self.seconds < 0.0
            || self.seconds > 172800.0
            || self.duration < 0.0
            || self.duration > 172800.0
            || !(0.0..=1.0).contains(&self.volume)
        {
            return Err("Invalid desktop playback state".into());
        }
        Ok(())
    }
}
/// Successful OS publications only. The system advances a playing timeline;
/// resending it is needed for controls, track changes or a material clock jump.
#[derive(Default)]
pub struct MediaPublication {
    metadata: Option<Snapshot>,
    playback: Option<(Snapshot, std::time::Instant)>,
    volume: Option<f64>,
}
impl MediaPublication {
    pub fn metadata_changed(&self, next: &Snapshot) -> bool {
        self.metadata.as_ref().is_none_or(|previous| {
            previous.has_station != next.has_station
                || previous.track_id != next.track_id
                || previous.station != next.station
                || previous.title != next.title
                || previous.artist != next.artist
                || previous.album != next.album
                || previous.duration != next.duration
        })
    }
    pub fn metadata_sent(&mut self, snapshot: &Snapshot) {
        self.metadata = Some(snapshot.clone());
    }
    pub fn playback_due(&self, next: &Snapshot, now: std::time::Instant) -> bool {
        self.playback.as_ref().is_none_or(|(previous, sent_at)| {
            if previous.has_station != next.has_station
                || previous.playing != next.playing
                || previous.track_id != next.track_id
                || previous.station != next.station
            {
                return true;
            }
            if !next.has_station {
                return false;
            }
            let expected = previous.seconds
                + if previous.playing {
                    now.saturating_duration_since(*sent_at).as_secs_f64()
                } else {
                    0.0
                };
            (next.seconds - expected).abs() > 2.0
        })
    }
    pub fn playback_sent(&mut self, snapshot: &Snapshot, now: std::time::Instant) {
        self.playback = Some((snapshot.clone(), now));
    }
    pub fn volume_changed(&self, next: &Snapshot) -> bool {
        self.volume != Some(if next.muted { 0.0 } else { next.volume })
    }
    pub fn volume_sent(&mut self, snapshot: &Snapshot) {
        self.volume = Some(if snapshot.muted { 0.0 } else { snapshot.volume });
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Action {
    Toggle,
    Play,
    Pause,
    Previous,
    Next,
    Mute,
    Volume(f64),
}
impl Action {
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Volume(v) = self {
            if !v.is_finite() || !(0.0..=1.0).contains(v) {
                return Err("Invalid volume".into());
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}
pub fn visible(position: Position, width: u32, height: u32, area: (i32, i32, u32, u32)) -> bool {
    let (x, y, w, h) = area;
    let px = i64::from(position.x);
    let py = i64::from(position.y);
    px >= i64::from(x)
        && py >= i64::from(y)
        && px + i64::from(width) <= i64::from(x) + i64::from(w)
        && py + i64::from(height) <= i64::from(y) + i64::from(h)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_timeline_extrapolates_until_controls_tracks_or_clock_jumps_change_it() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let mut next = Snapshot {
            has_station: true,
            playing: true,
            track_id: "a".into(),
            seconds: 10.0,
            duration: 100.0,
            ..Default::default()
        };
        let mut state = MediaPublication::default();
        assert!(state.metadata_changed(&next));
        assert!(state.playback_due(&next, now));
        state.metadata_sent(&next);
        state.playback_sent(&next, now);
        assert!(!state.metadata_changed(&next));
        next.seconds = 70.0;
        assert!(!state.playback_due(&next, now + Duration::from_secs(60)));
        next.seconds = 73.0;
        assert!(state.playback_due(&next, now + Duration::from_secs(60)));
        next.seconds = 67.0;
        assert!(state.playback_due(&next, now + Duration::from_secs(60)));
        next.seconds = 70.0;
        next.playing = false;
        assert!(state.playback_due(&next, now + Duration::from_secs(60)));
        state.playback_sent(&next, now + Duration::from_secs(60));
        assert!(!state.playback_due(&next, now + Duration::from_secs(120)));
        next.track_id = "b".into();
        assert!(state.playback_due(&next, now + Duration::from_secs(120)));
        assert!(state.metadata_changed(&next)); // Even identical displayed titles.
        state.metadata_sent(&next);
        next.duration += 1.0;
        assert!(state.metadata_changed(&next));
        next.has_station = false;
        assert!(state.playback_due(&next, now + Duration::from_secs(120)));
        state.playback_sent(&next, now);
        assert!(!state.playback_due(&next, now + Duration::from_secs(120)));
    }
    #[test]
    fn failed_media_updates_remain_due_and_volume_tracks_the_audible_level() {
        let now = std::time::Instant::now();
        let mut next = Snapshot {
            volume: 0.8,
            ..Default::default()
        };
        let mut state = MediaPublication::default();
        for _ in 0..2 {
            assert!(state.playback_due(&next, now));
            assert!(state.metadata_changed(&next));
            assert!(state.volume_changed(&next));
        }
        state.volume_sent(&next);
        assert!(!state.volume_changed(&next));
        next.muted = true;
        assert!(state.volume_changed(&next));
        state.volume_sent(&next);
        next.volume = 0.4;
        assert!(!state.volume_changed(&next));
        next.muted = false;
        assert!(state.volume_changed(&next));
    }
    #[test]
    fn rejects_unbounded_and_non_hex_desktop_colors() {
        let mut snapshot = Snapshot::default();
        snapshot.accent = "#aBc123".into();
        snapshot.banner_color = "#123456".into();
        snapshot.validate().unwrap();
        snapshot.accent = "x".repeat(16385);
        assert!(snapshot.validate().unwrap_err().contains("too long"));
        snapshot.accent = "#abcdef;display:none".into();
        assert!(snapshot.validate().is_err());
        snapshot.accent.clear();
        snapshot.banner_color = "red".into();
        assert!(snapshot.validate().is_err());
        snapshot.banner_color.clear();
        snapshot.validate().unwrap();
    }
    #[test]
    fn rejects_invalid_desktop_messages() {
        assert!(serde_json::from_str::<Action>(r#"{"type":"erase"}"#).is_err());
        assert!(Action::Volume(2.0).validate().is_err());
        let mut s = Snapshot::default();
        s.volume = f64::NAN;
        assert!(s.validate().is_err());
        s.volume = 0.8;
        s.seconds = f64::MAX;
        assert!(s.validate().is_err());
    }
    #[test]
    fn restores_positions_only_on_visible_work_areas() {
        assert!(visible(
            Position { x: -1800, y: 20 },
            480,
            160,
            (-1920, 0, 1920, 1080)
        ));
        assert!(!visible(
            Position { x: i32::MAX, y: 20 },
            480,
            160,
            (0, 0, 1920, 1080)
        ));
        assert!(!visible(
            Position { x: 1900, y: 10 },
            480,
            160,
            (0, 0, 1920, 1080)
        ));
    }
}

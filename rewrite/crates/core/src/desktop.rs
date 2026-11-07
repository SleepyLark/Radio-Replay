use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Snapshot {
    pub station: String,
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
}
impl Snapshot {
    pub fn validate(&self) -> Result<(), String> {
        if [
            &self.station,
            &self.title,
            &self.artist,
            &self.album,
            &self.art,
            &self.banner,
            &self.banner_color,
        ]
        .iter()
        .any(|s| s.len() > 16384)
        {
            return Err("Desktop text is too long".into());
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

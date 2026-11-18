use super::{validate_track_count, MAX_BUMPER_TRACKS, MAX_STATION_TRACKS};

/// Renderer plan scalars. File/slot authority is checked separately against the
/// prepared lineup and fresh filesystem identities before any decoder load.
pub struct PlaybackRequest<'a> {
    pub count: usize,
    pub index: usize,
    pub seconds: f64,
    pub fade: f64,
    pub bumper_count: usize,
    pub programme_context: Option<&'a str>,
}
impl PlaybackRequest<'_> {
    pub fn validate_bounds(&self) -> Result<usize, String> {
        if self.count == 0
            || self.count > MAX_STATION_TRACKS + MAX_BUMPER_TRACKS
            || self.index >= self.count
            || !self.seconds.is_finite()
            || !(0.0..=172800.0).contains(&self.seconds)
            || !self.fade.is_finite()
            || !(0.0..=30.0).contains(&self.fade)
        {
            return Err("Invalid native playback plan".into());
        }
        if self.bumper_count >= self.count || self.bumper_count > MAX_BUMPER_TRACKS {
            return Err("Invalid bumper count".into());
        }
        Ok(self.count - self.bumper_count)
    }
    pub fn validate_station(&self, network: bool, group: &str) -> Result<usize, String> {
        let music_count = self.validate_bounds()?;
        validate_track_count(if network { "network" } else { "tracks" }, music_count)?;
        if network && (self.bumper_count > 0 || self.seconds != 0.0 || self.fade != 0.0) {
            return Err(
                "Network stations do not support virtual seeking, crossfades or bumpers".into(),
            );
        }
        if self.index >= music_count {
            return Err("Initial playback must select a music track".into());
        }
        if self.bumper_count > 0 {
            let context = self.programme_context.unwrap_or(group);
            if context.len() > 1024 || context != group {
                return Err("Invalid programme context".into());
            }
        }
        Ok(music_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request<'a>() -> PlaybackRequest<'a> {
        PlaybackRequest {
            count: 3,
            index: 0,
            seconds: 0.0,
            fade: 0.0,
            bumper_count: 0,
            programme_context: None,
        }
    }
    #[test]
    fn invalid_plan_scalars_and_bumper_roles_are_rejected_before_loading() {
        for change in [
            |r: &mut PlaybackRequest| r.count = 0,
            |r: &mut PlaybackRequest| r.count = MAX_STATION_TRACKS + MAX_BUMPER_TRACKS + 1,
            |r: &mut PlaybackRequest| r.index = 3,
            |r: &mut PlaybackRequest| r.seconds = f64::NAN,
            |r: &mut PlaybackRequest| r.seconds = f64::INFINITY,
            |r: &mut PlaybackRequest| r.seconds = -1.0,
            |r: &mut PlaybackRequest| r.seconds = 172801.0,
            |r: &mut PlaybackRequest| r.fade = f64::NAN,
            |r: &mut PlaybackRequest| r.fade = -1.0,
            |r: &mut PlaybackRequest| r.fade = 30.1,
            |r: &mut PlaybackRequest| r.bumper_count = 3,
        ] {
            let mut request = request();
            change(&mut request);
            assert!(request.validate_bounds().is_err());
        }
        let mut valid = request();
        valid.bumper_count = 1;
        valid.seconds = 172800.0;
        valid.fade = 30.0;
        assert_eq!(valid.validate_station(false, "all").unwrap(), 2);
        valid.index = 2;
        assert!(valid.validate_station(false, "all").is_err());
        valid.index = 0;
        valid.programme_context = Some("different");
        assert!(valid.validate_station(false, "all").is_err());
        let long = "x".repeat(1025);
        valid.programme_context = Some(&long);
        assert!(valid.validate_station(false, &long).is_err());
    }
    #[test]
    fn network_and_local_track_limits_and_network_only_constraints_hold() {
        let mut valid = request();
        valid.count = 1000;
        assert!(valid.validate_station(true, "all").is_ok());
        valid.count = 1001;
        assert!(valid.validate_station(true, "all").is_err());
        valid.count = MAX_STATION_TRACKS;
        assert!(valid.validate_station(false, "all").is_ok());
        valid.count += 1;
        assert!(valid.validate_station(false, "all").is_err());
        valid.bumper_count = 1;
        assert!(valid.validate_station(false, "all").is_ok());
        valid.count = MAX_STATION_TRACKS + MAX_BUMPER_TRACKS;
        valid.bumper_count = MAX_BUMPER_TRACKS;
        assert!(valid.validate_station(false, "all").is_ok());
        valid.bumper_count += 1;
        assert!(valid.validate_bounds().is_err());
        for change in [
            |r: &mut PlaybackRequest| r.seconds = 0.1,
            |r: &mut PlaybackRequest| r.fade = 0.1,
            |r: &mut PlaybackRequest| r.bumper_count = 1,
        ] {
            let mut invalid = request();
            change(&mut invalid);
            assert!(invalid.validate_station(true, "all").is_err());
        }
    }
}

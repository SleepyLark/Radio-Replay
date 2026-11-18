//! Prepared metadata authority, separated from picker grants and playback slots.
use crate::{
    library::{PlayableTrack, MAX_BUMPER_TRACKS, MAX_STATION_TRACKS},
    Config,
};
use std::{collections::HashMap, sync::Arc};

const LIMIT: usize = MAX_STATION_TRACKS + MAX_BUMPER_TRACKS;

#[derive(Default)]
pub struct PreparedAudioGrants {
    stations: HashMap<String, HashMap<String, Arc<PlayableTrack>>>,
    count: usize,
}
impl PreparedAudioGrants {
    pub fn get(&self, key: &str, path: &str) -> Result<Arc<PlayableTrack>, String> {
        self.stations
            .get(key)
            .and_then(|tracks| tracks.get(path))
            .cloned()
            .ok_or_else(|| "Audio file is not in the prepared station".into())
    }
    pub fn remember(
        &mut self,
        key: &str,
        tracks: impl ExactSizeIterator<Item = Arc<PlayableTrack>>,
    ) -> Result<(), String> {
        self.remember_bounded(key, tracks, LIMIT)
    }
    fn remember_bounded(
        &mut self,
        key: &str,
        tracks: impl ExactSizeIterator<Item = Arc<PlayableTrack>>,
        limit: usize,
    ) -> Result<(), String> {
        if tracks.len() > limit {
            return Err("Prepared audio exceeds the grant limit".into());
        }
        if tracks.len() == 0 {
            return Ok(());
        }
        if self.count + tracks.len() > limit {
            self.clear();
        }
        let station = self.stations.entry(key.into()).or_default();
        for track in tracks {
            if station.insert(track.track.path.clone(), track).is_none() {
                self.count += 1;
            }
        }
        Ok(())
    }
    pub fn remove_station(&mut self, key: &str) {
        if let Some(tracks) = self.stations.remove(key) {
            self.count -= tracks.len();
        }
    }
    pub fn clear(&mut self) {
        self.stations.clear();
        self.count = 0;
    }

    /// Call only after the new library was successfully saved. Appearance-only
    /// edits retain metadata authority; ordered path or bumper changes revoke it.
    pub fn after_station_save(&mut self, before: &Config, after: &Config, key: &str) {
        let previous = before.custom.iter().find(|station| station.key == key);
        let current = after.custom.iter().find(|station| station.key == key);
        let changed = match (previous, current) {
            (Some(previous), Some(current)) => {
                previous
                    .tracks
                    .iter()
                    .map(|track| &track.path)
                    .ne(current.tracks.iter().map(|track| &track.path))
                    || previous
                        .extra
                        .get("bumpers")
                        .filter(|value| !value.is_null())
                        != current
                            .extra
                            .get("bumpers")
                            .filter(|value| !value.is_null())
            }
            _ => true,
        };
        if changed {
            self.remove_station(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(path: &str) -> Arc<PlayableTrack> {
        Arc::new(PlayableTrack {
            track: crate::Track {
                path: path.into(),
                name: path.into(),
                extra: Default::default(),
            },
            size: 1,
            modified: 1,
            duration: Some(1.0),
            cache_signature: Some("identity".into()),
        })
    }
    fn config() -> Config {
        let mut config = Config::default();
        config.custom.push(crate::Station {
            key: "a".into(),
            station: "A".into(),
            tracks: vec![
                track("song.wav").track.clone(),
                track("next.wav").track.clone(),
            ],
            extra: Default::default(),
        });
        config
    }
    #[test]
    fn authority_is_exact_and_separate_for_each_station_and_path() {
        let mut grants = PreparedAudioGrants::default();
        let shared = track("same.wav");
        grants.remember("a", [shared.clone()].into_iter()).unwrap();
        grants
            .remember("ab", [track("other.wav")].into_iter())
            .unwrap();
        assert!(Arc::ptr_eq(&shared, &grants.get("a", "same.wav").unwrap()));
        grants.remember("a", [track("x\0y")].into_iter()).unwrap();
        assert!(grants.get("a\0x", "y").is_err());
        for (key, path) in [
            ("b", "same.wav"),
            ("a", "other.wav"),
            ("a", "same.wav/child"),
            ("a\0same.wav", ""),
        ] {
            assert!(grants.get(key, path).is_err());
        }
        grants.remove_station("a");
        assert!(grants.get("a", "same.wav").is_err());
        assert!(grants.get("ab", "other.wav").is_ok());
        grants.clear();
        assert!(grants.get("ab", "other.wav").is_err());
    }
    #[test]
    fn bounded_preparation_replaces_identities_and_rejects_oversize_without_granting() {
        let mut grants = PreparedAudioGrants::default();
        grants.remember("empty", std::iter::empty()).unwrap();
        assert!(grants.stations.is_empty());
        grants
            .remember_bounded("a", [track("one"), track("two")].into_iter(), 3)
            .unwrap();
        let replacement = track("one");
        grants
            .remember_bounded("a", [replacement.clone()].into_iter(), 3)
            .unwrap();
        assert!(Arc::ptr_eq(&grants.get("a", "one").unwrap(), &replacement));
        assert_eq!(grants.count, 2);
        assert!(grants
            .remember_bounded(
                "b",
                [track("1"), track("2"), track("3"), track("4")].into_iter(),
                3
            )
            .is_err());
        assert!(grants.get("a", "two").is_ok());
        grants
            .remember_bounded("b", [track("three"), track("three")].into_iter(), 3)
            .unwrap();
        assert!(grants.get("a", "one").is_err());
        assert_eq!(grants.count, 1);
    }
    #[test]
    fn station_edits_revoke_only_changed_playback_sources_after_commit() {
        let before = config();
        let mut grants = PreparedAudioGrants::default();
        grants
            .remember("a", [track("song.wav"), track("clip.wav")].into_iter())
            .unwrap();
        grants
            .remember("ab", [track("other.wav")].into_iter())
            .unwrap();
        let mut appearance = before.clone();
        appearance.custom[0].station = "Renamed".into();
        appearance.custom[0]
            .extra
            .insert("accent".into(), "#abcdef".into());
        grants.after_station_save(&before, &appearance, "a");
        assert!(grants.get("a", "song.wav").is_ok());
        for change in [
            |config: &mut Config| {
                config.custom[0].tracks.reverse();
            },
            |config: &mut Config| {
                config.custom[0].tracks.pop();
            },
            |config: &mut Config| {
                config.custom[0]
                    .extra
                    .insert("bumpers".into(), serde_json::json!({"folder":"different"}));
            },
            |config: &mut Config| {
                config.custom.clear();
            },
        ] {
            let mut changed = before.clone();
            change(&mut changed);
            grants
                .remember("a", [track("song.wav"), track("clip.wav")].into_iter())
                .unwrap();
            grants.after_station_save(&before, &changed, "a");
            assert!(grants.get("a", "song.wav").is_err());
            assert!(grants.get("a", "clip.wav").is_err());
            assert!(grants.get("ab", "other.wav").is_ok());
        }
    }
}

use super::{PlayableTrack, TrackProblem};
use crate::Config;
use serde::Serialize;
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

#[derive(Serialize)]
pub struct IndexedTrack {
    pub slot: usize,
    pub id: String,
    pub name: String,
    pub duration: Option<f64>,
}
#[derive(Serialize)]
pub struct IndexedStation {
    pub revision: String,
    pub tracks: Vec<IndexedTrack>,
    pub bumpers: Vec<IndexedTrack>,
    pub problems: Vec<TrackProblem>,
}

/// File identities and paths stay in Rust. Slots authorize only this generation,
/// station, group and music/bumper partition, never a caller-supplied path.
pub struct IndexedLineup {
    pub generation: u64,
    pub key: String,
    pub config: Arc<Config>,
    pub group: String,
    pub tracks: Vec<Arc<PlayableTrack>>,
    pub music_count: usize,
}
impl IndexedLineup {
    pub fn verify_files(
        selected: &[&PlayableTrack],
        current: impl Fn() -> bool + Sync,
    ) -> Result<Vec<std::path::PathBuf>, String> {
        let inspect = |tracks: &[&PlayableTrack]| -> Result<Vec<std::path::PathBuf>, String> {
            tracks
                .iter()
                .map(|track| {
                    if !current() {
                        return Err("Tuning changed".into());
                    }
                    let metadata =
                        std::fs::metadata(&track.track.path).map_err(|e| e.to_string())?;
                    if crate::metadata::Cache::signature_from_metadata(&metadata)
                        != track.cache_signature
                    {
                        return Err("Audio changed after station preparation; tune again".into());
                    }
                    Ok(std::path::PathBuf::from(&track.track.path))
                })
                .collect()
        };
        let workers = if selected.len() >= 1024 {
            std::thread::available_parallelism()
                .map_or(1, usize::from)
                .min(4)
        } else {
            1
        };
        let paths = if workers == 1 {
            inspect(selected)?
        } else {
            std::thread::scope(|scope| {
                let handles: Vec<_> = selected
                    .chunks(selected.len().div_ceil(workers))
                    .map(|chunk| {
                        let inspect = &inspect;
                        scope.spawn(move || inspect(chunk))
                    })
                    .collect();
                let mut paths = Vec::with_capacity(selected.len());
                for handle in handles {
                    paths.extend(
                        handle
                            .join()
                            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))?,
                    );
                }
                Ok::<_, String>(paths)
            })?
        };
        if !current() {
            return Err("Tuning changed".into());
        }
        Ok(paths)
    }
    pub fn new(
        generation: u64,
        key: String,
        config: Arc<Config>,
        group: String,
        music: Vec<PlayableTrack>,
        bumpers: Vec<PlayableTrack>,
    ) -> Self {
        let music_count = music.len();
        Self {
            generation,
            key,
            config,
            group,
            tracks: music.into_iter().chain(bumpers).map(Arc::new).collect(),
            music_count,
        }
    }
    pub fn describe(&self, problems: Vec<TrackProblem>) -> IndexedStation {
        let mut tracks = Vec::with_capacity(self.music_count);
        let mut bumpers = Vec::new();
        let mut revision = std::collections::hash_map::DefaultHasher::new();
        if let Some(station) = self
            .config
            .custom
            .iter()
            .find(|station| station.key == self.key)
        {
            station
                .extra
                .get("mode")
                .and_then(|mode| mode.as_str())
                .hash(&mut revision);
            if self.tracks.len() > self.music_count {
                if let Some(settings) = station.extra.get("bumpers") {
                    settings
                        .get("method")
                        .and_then(|v| v.as_str())
                        .hash(&mut revision);
                    for field in ["min", "max"] {
                        settings
                            .get(field)
                            .and_then(|v| v.as_f64())
                            .map(f64::to_bits)
                            .hash(&mut revision);
                    }
                }
            }
        }
        for (slot, track) in self.tracks.iter().enumerate() {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            track.track.path.hash(&mut hash);
            track.cache_signature.hash(&mut hash);
            let identity = hash.finish();
            identity.hash(&mut revision);
            let value = IndexedTrack {
                slot,
                id: format!("{identity:016x}"),
                name: track.track.name.clone(),
                duration: track.duration,
            };
            if slot < self.music_count {
                tracks.push(value);
            } else {
                bumpers.push(value);
            }
        }
        IndexedStation {
            revision: format!(
                "{}:{}:{:016x}",
                self.music_count,
                self.tracks.len() - self.music_count,
                revision.finish()
            ),
            tracks,
            bumpers,
            problems,
        }
    }
    pub fn select(
        &self,
        generation: u64,
        key: &str,
        slots: &[usize],
        bumper_count: usize,
    ) -> Result<Vec<&PlayableTrack>, String> {
        if generation != self.generation || key != self.key {
            return Err("Tuning changed; prepare this station again".into());
        }
        if slots.len() != self.tracks.len() || bumper_count != self.tracks.len() - self.music_count
        {
            return Err("Native lineup does not match its preparation".into());
        }
        let mut seen = vec![false; self.tracks.len()];
        let mut selected = Vec::with_capacity(slots.len());
        let mut music_paths = std::collections::HashSet::new();
        let mut bumper_paths = std::collections::HashSet::new();
        for (position, &slot) in slots.iter().enumerate() {
            let Some(track) = self.tracks.get(slot) else {
                return Err("Unprepared native track index".into());
            };
            if seen[slot] || (position < self.music_count) != (slot < self.music_count) {
                return Err("Duplicate or misclassified native track index".into());
            }
            let paths = if position < self.music_count {
                &mut music_paths
            } else {
                &mut bumper_paths
            };
            if !paths.insert(track.track.path.as_str()) {
                return Err("Duplicate native playback path".into());
            }
            seen[slot] = true;
            selected.push(track.as_ref());
        }
        Ok(selected)
    }
    pub fn validate_config(&self, current: &Arc<Config>) -> Result<(), String> {
        validate_preparation(&self.config, current, &self.key, &self.group)
    }
}
pub fn validate_preparation(
    prepared: &Arc<Config>,
    current: &Arc<Config>,
    key: &str,
    selected_group: &str,
) -> Result<(), String> {
    let previous = prepared.custom.iter().find(|s| s.key == key);
    if previous.is_none() {
        return Err("Station unavailable during preparation".into());
    }
    if Arc::ptr_eq(prepared, current) {
        return Ok(());
    }
    let next = current.custom.iter().find(|s| s.key == key);
    fn group<'a>(config: &'a Config, id: &str) -> Option<&'a serde_json::Value> {
        config.groups.iter().find(|g| g["id"].as_str() == Some(id))
    }
    if let (Some(previous), Some(next)) = (previous, next) {
        if previous.tracks == next.tracks
            && previous.extra.get("mode") == next.extra.get("mode")
            && previous.extra.get("bumpers") == next.extra.get("bumpers")
            && (selected_group == "all"
                || group(prepared, selected_group) == group(current, selected_group))
        {
            return Ok(());
        }
    }
    Err("Station or group changed after preparation; tune again".into())
}
#[cfg(test)]
mod tests;

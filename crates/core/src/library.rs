use crate::{Config, Station, Track};
mod indexed;
pub use indexed::{validate_preparation, IndexedLineup, IndexedStation, IndexedTrack};
mod request;
mod scan;
pub use request::PlaybackRequest;
pub use scan::{collect_audio, scan_folder, ScanNotices, ScanReport};

pub const MAX_STATION_TRACKS: usize = 100_000;
pub const MAX_BUMPER_TRACKS: usize = 1_000;
pub const MAX_NETWORK_STREAMS: usize = 1_000;
pub fn validate_track_count(mode: &str, count: usize) -> Result<(), String> {
    if mode == "network" && count > MAX_NETWORK_STREAMS {
        return Err("Network stations are limited to 1,000 streams".into());
    }
    if count > MAX_STATION_TRACKS {
        return Err("A station can hold up to 100,000 files".into());
    }
    Ok(())
}
use serde::Deserialize;
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub fn audio_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "mp3" | "m4a" | "aac" | "ogg" | "wav" | "flac"
    )
}
pub fn image_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif"
    )
}

#[derive(Serialize, Clone)]
pub struct PlayableTrack {
    #[serde(flatten)]
    pub track: Track,
    pub size: u64,
    pub modified: u64,
    pub duration: Option<f64>,
    #[serde(skip)]
    pub cache_signature: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct TrackProblem {
    pub path: String,
    pub name: String,
    pub reason: String,
    pub kind: &'static str,
}
#[derive(Serialize)]
pub struct PreparedStation {
    pub tracks: Vec<PlayableTrack>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bumpers: Vec<PlayableTrack>,
    pub problems: Vec<TrackProblem>,
}

/// Resolve saved tracks while retaining diagnostics for files that cannot be used.
pub fn playback_files(config: &Config, key: &str) -> Result<Vec<PlayableTrack>, String> {
    Ok(playback_report(config, key)?.tracks)
}
fn playback_report(config: &Config, key: &str) -> Result<PreparedStation, String> {
    playback_report_selected(config, key, None)
}
fn playback_report_selected(
    config: &Config,
    key: &str,
    paths: Option<&HashSet<&str>>,
) -> Result<PreparedStation, String> {
    let station = config
        .custom
        .iter()
        .find(|s| s.key == key)
        .ok_or("Station no longer exists")?;
    if station.extra.get("mode").and_then(|v| v.as_str()) == Some("network") {
        if station.tracks.len() > MAX_NETWORK_STREAMS {
            return Err("Network stations are limited to 1,000 streams".into());
        }
        let mut report = report_tracks(&[]);
        for track in station
            .tracks
            .iter()
            .filter(|t| paths.is_none_or(|paths| paths.contains(t.path.as_str())))
        {
            match radio_replay_network::validate(&track.path) {
                Ok(_) => {
                    let mut track = track.clone();
                    track.extra.insert("source".into(), "network".into());
                    report.tracks.push(PlayableTrack {
                        track,
                        duration: None,
                        size: 0,
                        modified: 0,
                        cache_signature: None,
                    })
                }
                Err(reason) => report.problems.push(TrackProblem {
                    path: track.path.clone(),
                    name: track.name.clone(),
                    reason,
                    kind: "file",
                }),
            }
        }
        return Ok(report);
    }
    Ok(report_tracks_iter(station.tracks.iter().filter(|t| {
        paths.is_none_or(|paths| paths.contains(t.path.as_str()))
    })))
}
pub fn report_tracks(tracks: &[Track]) -> PreparedStation {
    report_tracks_iter(tracks.iter())
}
fn report_tracks_iter<'a>(tracks: impl Iterator<Item = &'a Track>) -> PreparedStation {
    let tracks: Vec<_> = tracks.collect();
    let workers = if tracks.len() >= 1024 {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(4)
    } else {
        1
    };
    if workers == 1 {
        return report_tracks_serial(tracks.into_iter());
    }
    std::thread::scope(|scope| {
        let handles: Vec<_> = tracks
            .chunks(tracks.len().div_ceil(workers))
            .map(|chunk| scope.spawn(move || report_tracks_serial(chunk.iter().copied())))
            .collect();
        let mut report = PreparedStation {
            tracks: Vec::new(),
            bumpers: Vec::new(),
            problems: Vec::new(),
        };
        // Join in input order; completion order never reshuffles long broadcasts
        // or their diagnostics. Every worker uses the same full identity checks.
        for handle in handles {
            let mut chunk = handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            report.tracks.append(&mut chunk.tracks);
            report.problems.append(&mut chunk.problems);
        }
        report
    })
}
fn report_tracks_serial<'a>(tracks: impl Iterator<Item = &'a Track>) -> PreparedStation {
    let mut files = Vec::new();
    let mut problems = Vec::new();
    for track in tracks {
        let path = Path::new(&track.path);
        let resolved = (|| -> Result<_, String> {
            if !audio_file(path) {
                return Err("This file type is not supported.".into());
            }
            let access_error = |error: std::io::Error| {
                match error.kind() {
                std::io::ErrorKind::NotFound=>"File not found. It may have been moved or deleted, or its drive is disconnected.".into(),
                std::io::ErrorKind::PermissionDenied=>"Access denied. Check this file's permissions.".into(),
                _=>format!("Could not access this file: {error}"),
            }
            };
            let path = fs::canonicalize(path).map_err(access_error)?;
            let meta = fs::metadata(&path).map_err(access_error)?;
            if !meta.is_file() {
                return Err("This path is a folder, not an audio file.".into());
            }
            Ok((path, meta))
        })();
        let (path, meta) = match resolved {
            Ok(value) => value,
            Err(reason) => {
                problems.push(TrackProblem {
                    path: track.path.clone(),
                    name: track.name.clone(),
                    reason,
                    kind: "file",
                });
                continue;
            }
        };
        let mut track = track.clone();
        track.path = path.to_string_lossy().into_owned();
        files.push(PlayableTrack {
            duration: None,
            track,
            size: meta.len(),
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
                .unwrap_or(0),
            cache_signature: crate::metadata::Cache::signature_from_metadata(&meta),
        });
    }
    PreparedStation {
        tracks: files,
        bumpers: Vec::new(),
        problems,
    }
}
pub fn bumper_report(config: &Config, key: &str) -> PreparedStation {
    bumper_report_with(config, key, scan_folder)
}
fn bumper_report_with(
    config: &Config,
    key: &str,
    scan: impl FnOnce(&Path) -> Result<ScanReport, String>,
) -> PreparedStation {
    let mut report = report_tracks(&[]);
    let result = (|| {
        let station = config
            .custom
            .iter()
            .find(|s| s.key == key)
            .ok_or("Station unavailable")?;
        let Some(settings) = crate::bumpers::Settings::from_station(station)? else {
            return Ok(());
        };
        let scan = scan(Path::new(&settings.folder))?;
        let mut tracks = scan.tracks;
        if tracks.len() > MAX_BUMPER_TRACKS {
            return Err("Bumper folders are limited to 1,000 audio files".to_string());
        }
        if tracks.is_empty() {
            return Err(if scan.notices.count() > 0 {
                format!(
                    "No supported audio clips in the bumper folder. {}",
                    scan.notices.summary()
                )
            } else {
                "No supported audio clips in the bumper folder".into()
            });
        }
        tracks.sort_by(|a, b| a.path.cmp(&b.path));
        report = report_tracks(&tracks);
        if scan.notices.count() > 0 {
            report.problems.push(TrackProblem {
                path: settings.folder.clone(),
                name: "Bumper folder".into(),
                reason: scan.notices.summary(),
                kind: "scan",
            });
        }
        Ok::<(), String>(())
    })();
    if let Err(reason) = result {
        report.problems.push(TrackProblem {
            path: config
                .custom
                .iter()
                .find(|s| s.key == key)
                .and_then(|s| s.extra.get("bumpers"))
                .and_then(|b| b.get("folder"))
                .and_then(|f| f.as_str())
                .unwrap_or("")
                .into(),
            name: "Bumper folder".into(),
            reason,
            kind: "scan",
        });
    }
    report
}
pub fn playback_files_for_group(
    config: Config,
    key: &str,
    group: Option<&str>,
) -> Result<Vec<PlayableTrack>, String> {
    Ok(playback_report_for_group(config, key, group)?.tracks)
}
pub fn playback_report_for_group(
    config: Config,
    key: &str,
    group: Option<&str>,
) -> Result<PreparedStation, String> {
    playback_report_for_group_shared(&config, key, group)
}
pub fn playback_report_for_group_shared(
    config: &Config,
    key: &str,
    group: Option<&str>,
) -> Result<PreparedStation, String> {
    if let Some(id) = group.filter(|g| *g != "all") {
        let members = config
            .groups
            .iter()
            .find(|g| g["id"].as_str() == Some(id))
            .and_then(|g| g["members"].as_array())
            .ok_or("Group unavailable")?
            .iter()
            .filter_map(|member| member.as_str())
            .collect::<HashSet<&str>>();
        if !members.contains(key) {
            let prefix = format!("{key}|");
            let paths = members
                .iter()
                .filter_map(|member| member.strip_prefix(&prefix))
                .collect();
            return playback_report_selected(config, key, Some(&paths));
        }
    }
    playback_report(config, key)
}
pub fn track(path: PathBuf) -> Result<Track, String> {
    if !audio_file(&path) || !path.is_file() {
        return Err("Choose a supported audio file".into());
    }
    let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
    Ok(Track {
        name: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: path.to_string_lossy().into_owned(),
        extra: Default::default(),
    })
}

/// Recover archive-era definitions without moving audio or overwriting existing stations.
pub fn recover_legacy(config: &mut Config) -> Result<(usize, ScanNotices), String> {
    if config.root.is_empty() {
        return Err("This configuration has no legacy archive root".into());
    }
    let original = PathBuf::from(&config.root);
    let root = fs::canonicalize(&original).map_err(|e| e.to_string())?;
    let scan = scan_folder(&root)?;
    let mut stations: std::collections::BTreeMap<String, Station> = Default::default();
    for mut track in scan.tracks {
        let relative = Path::new(&track.path)
            .strip_prefix(&root)
            .map_err(|e| e.to_string())?;
        let parts: Vec<_> = relative
            .components()
            .map(|p| p.as_os_str().to_string_lossy().into_owned())
            .collect();
        let game = if parts.len() > 1 {
            parts[0].clone()
        } else {
            "Library".into()
        };
        let station = if parts.len() > 2 {
            parts[parts.len() - 2].clone()
        } else {
            track.name.clone()
        };
        let key = format!("{game}::{station}");
        track.path = original.join(relative).to_string_lossy().into_owned();
        #[cfg(windows)]
        {
            track.path = track.path.replace('/', "\\");
        }
        let entry = stations.entry(key.clone()).or_insert_with(|| {
            let mut extra = serde_json::Map::new();
            extra.insert("game".into(), game.into());
            extra.insert("custom".into(), true.into());
            extra.insert("legacyRoot".into(), config.root.clone().into());
            Station {
                key,
                station,
                tracks: vec![],
                extra,
            }
        });
        entry.tracks.push(track);
    }
    let existing: HashSet<String> = config.custom.iter().map(|s| s.key.clone()).collect();
    let mut added = 0;
    for (key, mut station) in stations {
        if existing.contains(&key) {
            continue;
        }
        station.extra.insert(
            "mode".into(),
            if station.tracks.len() == 1 {
                "longset"
            } else {
                "tracks"
            }
            .into(),
        );
        config.custom.push(station);
        added += 1;
    }
    Ok((added, scan.notices))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationEdit {
    pub key: String,
    pub name: String,
    pub mode: String,
    pub tracks: Vec<Track>,
    pub logo_path: Option<String>,
    #[serde(default)]
    pub options: serde_json::Map<String, serde_json::Value>,
}

pub fn upsert(
    config: &mut Config,
    edit: StationEdit,
    audio: &HashSet<String>,
    images: &HashSet<String>,
) -> Result<(), String> {
    crate::validation::identifier(&edit.key, "Station identifier")?;
    crate::validation::name(&edit.name, "Station name")?;
    if !matches!(edit.mode.as_str(), "tracks" | "longset" | "network") {
        return Err("Invalid playback mode".into());
    }
    if edit.tracks.is_empty() {
        return Err("Add at least one audio file".into());
    }
    validate_track_count(&edit.mode, edit.tracks.len())?;
    for (key, value) in &edit.options {
        let valid = match key.as_str() {
            "bumpers" => {
                if value.is_null() {
                    true
                } else if let Ok(settings) =
                    serde_json::from_value::<crate::bumpers::Settings>(value.clone())
                {
                    let retained = config.custom.iter().any(|s| {
                        s.key == edit.key
                            && s.extra
                                .get("bumpers")
                                .and_then(|b| b.get("folder"))
                                .and_then(|f| f.as_str())
                                == Some(settings.folder.as_str())
                    });
                    settings.validate().is_ok()
                        && (!settings.enabled || edit.mode == "tracks")
                        && (settings.folder.is_empty()
                            || retained
                            || audio.contains(&crate::bumpers::folder_grant(&settings.folder)))
                } else {
                    false
                }
            }
            "crossfade" => value.is_null() || crate::validation::crossfade(value),
            "textMode" => matches!(
                value.as_str(),
                Some("auto" | "titles" | "lyrics" | "ignore")
            ),
            "spinLogo" => value.is_boolean(),
            "accent" | "bannerColor" | "bannerAccent" => value
                .as_str()
                .map(|s| crate::validation::color(s, true))
                .unwrap_or(false),
            "bannerPath" => value
                .as_str()
                .map(|path| {
                    path.is_empty()
                        || (image_file(Path::new(path))
                            && (images.contains(path)
                                || config.custom.iter().any(|s| {
                                    s.key == edit.key
                                        && s.extra.get("bannerPath").and_then(|v| v.as_str())
                                            == Some(path)
                                })))
                })
                .unwrap_or(false),
            _ => false,
        };
        if !valid {
            return Err(format!("Invalid station option: {key}"));
        }
    }
    let existing = config.custom.iter().position(|s| s.key == edit.key);
    let retained_paths: HashSet<&str> = edit.tracks.iter().map(|t| t.path.as_str()).collect();
    let removed_members: HashSet<String> = existing
        .map(|i| {
            config.custom[i]
                .tracks
                .iter()
                .filter(|t| !retained_paths.contains(t.path.as_str()))
                .map(|t| format!("{}|{}", edit.key, t.path))
                .collect()
        })
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let previous_paths: HashSet<&str> = existing
        .map(|i| {
            config.custom[i]
                .tracks
                .iter()
                .map(|t| t.path.as_str())
                .collect()
        })
        .unwrap_or_default();
    for track in &edit.tracks {
        let retained = previous_paths.contains(track.path.as_str());
        let valid_source = if edit.mode == "network" {
            radio_replay_network::validate(&track.path).is_ok()
        } else {
            audio_file(Path::new(&track.path))
                && !track.path.starts_with("http:")
                && !track.path.starts_with("https:")
        };
        if (!retained && !audio.contains(&track.path)) || !valid_source {
            return Err("Audio file was not selected".into());
        }
        if !seen.insert(&track.path) {
            return Err("A track occurs more than once".into());
        }
    }
    if let Some(path) = edit.logo_path.as_ref().filter(|s| !s.is_empty()) {
        let retained = config.logos.get(&edit.key).and_then(|v| v.as_str()) == Some(path.as_str());
        if !retained && !images.contains(path) {
            return Err("Artwork was not selected".into());
        }
    }
    // All validation above is complete. Retain extensions without cloning the
    // old track list just before replacing it with the editor's list.
    let mut station = Station {
        key: edit.key.clone(),
        station: edit.name.trim().into(),
        tracks: edit.tracks,
        extra: existing
            .map(|i| std::mem::take(&mut config.custom[i].extra))
            .unwrap_or_default(),
    };
    station.extra.insert("mode".into(), edit.mode.into());
    station.extra.insert("custom".into(), true.into());
    station.extra.extend(edit.options);
    if let Some(path) = edit.logo_path {
        if path.is_empty() {
            config.logos.remove(&edit.key);
        } else {
            config.logos.insert(edit.key.clone(), path.into());
        }
    }
    match existing {
        Some(i) => config.custom[i] = station,
        None => config.custom.push(station),
    }
    for group in &mut config.groups {
        if let Some(members) = group.get_mut("members").and_then(|m| m.as_array_mut()) {
            members.retain(|m| {
                m.as_str()
                    .map(|m| !removed_members.contains(m))
                    .unwrap_or(true)
            });
        }
    }
    Ok(())
}

/// Remove station entries only, including the last track. Audio files are never touched.
pub fn remove_tracks(config: &mut Config, key: &str, paths: &[String]) -> Result<(), String> {
    if paths.len() > 100_000 {
        return Err("Too many files selected".into());
    }
    let station = config
        .custom
        .iter_mut()
        .find(|s| s.key == key)
        .ok_or("Station no longer exists")?;
    let selected: HashSet<&str> = paths.iter().map(String::as_str).collect();
    let removed: HashSet<String> = station
        .tracks
        .iter()
        .filter(|t| selected.contains(t.path.as_str()))
        .map(|t| format!("{key}|{}", t.path))
        .collect();
    station
        .tracks
        .retain(|t| !selected.contains(t.path.as_str()));
    for group in &mut config.groups {
        if let Some(members) = group.get_mut("members").and_then(|m| m.as_array_mut()) {
            members.retain(|m| !m.as_str().is_some_and(|s| removed.contains(s)));
        }
    }
    Ok(())
}

pub fn remove_many(config: &mut Config, keys: &[String]) -> Result<(), String> {
    if keys.is_empty()
        || keys
            .iter()
            .any(|key| !config.custom.iter().any(|s| &s.key == key))
    {
        return Err("Select existing stations to remove".into());
    }
    let unique: HashSet<&str> = keys.iter().map(String::as_str).collect();
    for key in unique {
        remove(config, key)?;
    }
    Ok(())
}

pub fn remove(config: &mut Config, key: &str) -> Result<(), String> {
    let station = config
        .custom
        .iter()
        .find(|s| s.key == key)
        .ok_or("Station no longer exists")?;
    let members_to_remove: HashSet<String> = station
        .tracks
        .iter()
        .map(|t| format!("{key}|{}", t.path))
        .chain(std::iter::once(key.to_string()))
        .collect();
    config.custom.retain(|s| s.key != key);
    config.hidden.retain(|s| s != key);
    config.logos.remove(key);
    for group in &mut config.groups {
        if let Some(members) = group.get_mut("members").and_then(|m| m.as_array_mut()) {
            members.retain(|m| {
                m.as_str()
                    .map(|m| !members_to_remove.contains(m))
                    .unwrap_or(true)
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parallel_inspection_preserves_order_diagnostics_and_canonical_file_identities() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("folder")).unwrap();
        let audio = dir.path().join("audio.wav");
        fs::write(&audio, b"fixture").unwrap();
        let input: Vec<_> = (0..1100)
            .map(|i| Track {
                path: if i % 17 == 0 {
                    dir.path().join(format!("missing-{i}.wav"))
                } else {
                    dir.path().join("folder/../audio.wav")
                }
                .to_string_lossy()
                .into_owned(),
                name: format!("Track {i}"),
                extra: Default::default(),
            })
            .collect();
        let original = input.clone();
        let serial = report_tracks_serial(input.iter());
        let parallel = report_tracks(&input);
        assert_eq!(
            serde_json::to_value(&serial).unwrap(),
            serde_json::to_value(&parallel).unwrap()
        );
        let canonical = fs::canonicalize(&audio)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(parallel
            .tracks
            .iter()
            .all(|t| t.track.path == canonical && t.cache_signature.is_some()));
        assert_eq!(input, original);
        let selected: Vec<_> = parallel.tracks.iter().collect();
        assert!(IndexedLineup::verify_files(&selected, || true)
            .unwrap()
            .iter()
            .all(|path| path.to_string_lossy() == canonical));
        assert!(IndexedLineup::verify_files(&selected, || false)
            .unwrap_err()
            .contains("Tuning changed"));
        let signature = parallel.tracks[0].cache_signature.clone();
        fs::File::options()
            .write(true)
            .open(&audio)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(2))
            .unwrap();
        assert_ne!(report_tracks(&input).tracks[0].cache_signature, signature);
        assert!(IndexedLineup::verify_files(&selected, || true)
            .unwrap_err()
            .contains("Audio changed"));
    }
    #[test]
    fn rejects_ambiguous_station_keys_before_grant_lookup() {
        let mut config = Config::default();
        let mut request = edit();
        request.key = "new\0song.mp3".into();
        assert!(
            upsert(&mut config, request, &HashSet::new(), &HashSet::new())
                .unwrap_err()
                .contains("identifier")
        );
        assert!(config.custom.is_empty());
        let mut request = edit();
        request.name = "x".repeat(201);
        assert!(
            upsert(&mut config, request, &HashSet::new(), &HashSet::new())
                .unwrap_err()
                .contains("name")
        );
        assert!(config.custom.is_empty());
    }
    #[test]
    fn track_limits_apply_before_edit_authorization_and_to_loaded_configs() {
        assert!(validate_track_count("tracks", MAX_STATION_TRACKS).is_ok());
        assert!(validate_track_count("network", MAX_NETWORK_STREAMS).is_ok());
        assert!(validate_track_count("network", MAX_NETWORK_STREAMS + 1).is_err());
        let mut request = edit();
        request.tracks = vec![
            Track {
                path: "song.mp3".into(),
                name: "Song".into(),
                extra: Default::default()
            };
            MAX_STATION_TRACKS + 1
        ];
        let mut config = Config::default();
        assert!(
            upsert(&mut config, request, &HashSet::new(), &HashSet::new())
                .unwrap_err()
                .contains("100,000")
        );
        assert!(config.custom.is_empty());
        config.custom.push(Station {
            key: "s".into(),
            station: "S".into(),
            tracks: vec![
                Track {
                    path: "song.mp3".into(),
                    name: "Song".into(),
                    extra: Default::default()
                };
                MAX_STATION_TRACKS + 1
            ],
            extra: Default::default(),
        });
        assert!(config.validate().unwrap_err().contains("100,000"));
        let bytes = serde_json::to_vec(&config).unwrap();
        assert!(Config::parse(&bytes).unwrap_err().contains("100,000"));
    }
    #[test]
    fn large_retained_station_still_rejects_unselected_additions() {
        let tracks: Vec<_> = (0..20_000)
            .map(|i| Track {
                path: format!("track-{i}.mp3"),
                name: format!("Track {i}"),
                extra: Default::default(),
            })
            .collect();
        let mut config = Config::default();
        config.custom.push(Station {
            key: "large".into(),
            station: "Large".into(),
            tracks: tracks.clone(),
            extra: Default::default(),
        });
        let mut request = edit();
        request.key = "large".into();
        request.tracks = tracks;
        upsert(&mut config, request, &HashSet::new(), &HashSet::new()).unwrap();
        assert_eq!(config.custom[0].tracks.len(), 20_000);
        let mut request = edit();
        request.key = "large".into();
        request.tracks = config.custom[0].tracks.clone();
        request.tracks.push(Track {
            path: "not-picked.mp3".into(),
            name: "Not picked".into(),
            extra: Default::default(),
        });
        assert!(upsert(&mut config, request, &HashSet::new(), &HashSet::new()).is_err());
        assert_eq!(config.custom[0].tracks.len(), 20_000);
    }
    #[test]
    fn recovers_legacy_layout_without_overwriting_custom_stations_or_references() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Game/Station")).unwrap();
        std::fs::create_dir_all(dir.path().join("Game/Extras")).unwrap();
        for file in [
            "Game/Station/a.mp3",
            "Game/Station/b.mp3",
            "Game/Broadcast.mp3",
            "Game/Extras/skip.mp3",
        ] {
            std::fs::write(dir.path().join(file), b"fixture").unwrap();
        }
        let mut config = Config::default();
        config.root = dir.path().to_string_lossy().into_owned();
        let member = format!(
            "Game::Station|{}",
            dir.path()
                .join("Game")
                .join("Station")
                .join("a.mp3")
                .display()
        );
        config
            .groups
            .push(serde_json::json!({"members":[member.clone()]}));
        assert_eq!(recover_legacy(&mut config).unwrap().0, 2);
        assert_eq!(
            config
                .custom
                .iter()
                .find(|s| s.key == "Game::Station")
                .unwrap()
                .tracks
                .len(),
            2
        );
        assert_eq!(
            config
                .custom
                .iter()
                .find(|s| s.key == "Game::Broadcast")
                .unwrap()
                .extra["mode"],
            "longset"
        );
        assert_eq!(config.groups[0]["members"][0], member);
        config.groups[0]["id"] = "archive-group".into();
        assert_eq!(
            playback_files_for_group(config.clone(), "Game::Station", Some("archive-group"))
                .unwrap()
                .len(),
            1
        );
        config.custom[0].station = "Edited".into();
        assert_eq!(recover_legacy(&mut config).unwrap().0, 0);
        assert_eq!(config.custom[0].station, "Edited");
    }
    #[test]
    fn playback_resolves_only_saved_audio_and_skips_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.wav");
        fs::write(&path, b"test").unwrap();
        let mut config = Config::default();
        let mut request = edit();
        request.tracks[0].path = path.to_string_lossy().into_owned();
        let grants = HashSet::from([request.tracks[0].path.clone()]);
        upsert(&mut config, request, &grants, &HashSet::new()).unwrap();
        assert!(playback_files(&config, "not-saved").is_err());
        let files = playback_files(&config, "new").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].size, 4);
        config.groups.push(serde_json::json!({"id":"subset","name":"Subset","members":[format!("new|{}",config.custom[0].tracks[0].path)]}));
        assert_eq!(
            playback_files_for_group(config.clone(), "new", Some("subset"))
                .unwrap()
                .len(),
            1
        );
        config.groups[0]["members"] = serde_json::json!([]);
        assert!(
            playback_files_for_group(config.clone(), "new", Some("subset"))
                .unwrap()
                .is_empty()
        );
        assert!(playback_files_for_group(config.clone(), "new", Some("unknown")).is_err());
        fs::remove_file(path).unwrap();
        assert!(playback_files(&config, "new").unwrap().is_empty());
    }
    #[test]
    fn scans_nested_supported_files_in_order() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("b")).unwrap();
        for file in ["a.MP3", "b/z.flac", "ignored.txt"] {
            fs::write(dir.path().join(file), b"fixture").unwrap();
        }
        let report = scan_folder(dir.path()).unwrap();
        assert_eq!(report.notices.count(), 0);
        let tracks = report.tracks;
        assert_eq!(
            tracks.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["a", "z"]
        );
    }
    #[test]
    fn playback_report_names_bad_files_and_respects_group_selection() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.mp3");
        let missing = dir.path().join("missing.mp3");
        let unsupported = dir.path().join("notes.txt");
        fs::write(&good, b"audio").unwrap();
        fs::write(&unsupported, b"notes").unwrap();
        let mut config = Config::default();
        config.custom.push(Station {
            key: "station".into(),
            station: "Station".into(),
            tracks: [&good, &missing, &unsupported]
                .into_iter()
                .map(|path| Track {
                    path: path.to_string_lossy().into_owned(),
                    name: path.file_name().unwrap().to_string_lossy().into_owned(),
                    extra: Default::default(),
                })
                .collect(),
            extra: Default::default(),
        });
        let report = playback_report_for_group(config.clone(), "station", None).unwrap();
        assert_eq!(report.tracks.len(), 1);
        assert_eq!(report.problems.len(), 2);
        assert_eq!(report.problems[0].path, missing.to_string_lossy());
        assert!(report.problems[0].reason.contains("File not found"));
        assert!(report.problems[1].reason.contains("not supported"));
        config.groups.push(serde_json::json!({"id":"good-only","members":[format!("station|{}",good.to_string_lossy())]}));
        let subset =
            playback_report_for_group(config.clone(), "station", Some("good-only")).unwrap();
        assert_eq!(subset.tracks.len(), 1);
        assert!(subset.problems.is_empty());
        config
            .groups
            .push(serde_json::json!({"id":"whole-station","members":["station"]}));
        let whole = playback_report_for_group(config, "station", Some("whole-station")).unwrap();
        assert_eq!(whole.tracks.len(), 1);
        assert_eq!(whole.problems.len(), 2);
    }
    #[test]
    fn dropped_audio_collects_folders_and_deduplicates_selected_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        let audio = dir.path().join("nested/song.mp3");
        fs::write(&audio, b"fixture").unwrap();
        let text = dir.path().join("notes.txt");
        fs::write(&text, b"notes").unwrap();
        let tracks = collect_audio(&[dir.path().into(), audio, text.clone()])
            .unwrap()
            .tracks;
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].name, "song");
        assert!(collect_audio(&[text]).is_err());
    }
    #[test]
    fn bulk_removal_validates_the_entire_selection_before_mutation() {
        let mut config = Config::default();
        let mut first = edit();
        first.key = "first".into();
        upsert(
            &mut config,
            first,
            &HashSet::from(["song.mp3".into()]),
            &HashSet::new(),
        )
        .unwrap();
        upsert(
            &mut config,
            edit(),
            &HashSet::from(["song.mp3".into()]),
            &HashSet::new(),
        )
        .unwrap();
        config
            .groups
            .push(serde_json::json!({"id":"g","members":["first","new|song.mp3","other"]}));
        assert!(remove_many(&mut config, &["first".into(), "missing".into()]).is_err());
        assert_eq!(config.custom.len(), 2);
        remove_many(&mut config, &["first".into(), "new".into()]).unwrap();
        assert!(config.custom.is_empty());
        assert_eq!(config.groups[0]["members"], serde_json::json!(["other"]));
    }
    fn edit() -> StationEdit {
        StationEdit {
            key: "new".into(),
            name: " FM ".into(),
            mode: "tracks".into(),
            tracks: vec![Track {
                path: "song.mp3".into(),
                name: "Song".into(),
                extra: Default::default(),
            }],
            logo_path: None,
            options: Default::default(),
        }
    }
    #[test]
    fn rejects_unselected_files_and_artwork() {
        let mut config = Config::default();
        let mut audio = HashSet::new();
        let images = HashSet::new();
        assert!(upsert(&mut config, edit(), &audio, &images).is_err());
        audio.insert("song.mp3".into());
        let mut request = edit();
        request.logo_path = Some("secret.png".into());
        assert!(upsert(&mut config, request, &audio, &images).is_err());
        assert!(config.custom.is_empty());
    }
    #[test]
    fn station_visuals_validate_colors_and_retain_only_authorized_banners() {
        let mut config = Config::default();
        let audio = HashSet::from(["song.mp3".into()]);
        let images = HashSet::from(["banner.png".into()]);
        let mut request = edit();
        request.options = serde_json::from_value(
            serde_json::json!({"bannerPath":"private.png","accent":"#c7f464","spinLogo":true}),
        )
        .unwrap();
        assert!(upsert(&mut config, request, &audio, &images).is_err());
        let mut request = edit();
        request.options = serde_json::from_value(serde_json::json!({"bannerPath":"banner.png","bannerColor":"#244b42","accent":"#c7f464","spinLogo":true})).unwrap();
        upsert(&mut config, request, &audio, &images).unwrap();
        let mut request = edit();
        request
            .options
            .insert("bannerPath".into(), "banner.png".into());
        upsert(&mut config, request, &HashSet::new(), &HashSet::new()).unwrap();
        let mut request = edit();
        request
            .options
            .insert("accent".into(), "red;display:none".into());
        assert!(upsert(&mut config, request, &audio, &images).is_err());
        assert_eq!(config.custom[0].extra["bannerPath"], "banner.png");
        assert_eq!(config.custom[0].extra["accent"], "#c7f464");
    }
    #[test]
    fn edits_preserve_options_and_removal_cleans_references() {
        let mut config = Config::default();
        let audio = HashSet::from(["song.mp3".into()]);
        let images = HashSet::new();
        upsert(&mut config, edit(), &audio, &images).unwrap();
        config.custom[0].extra.insert("crossfade".into(), 5.into());
        upsert(&mut config, edit(), &HashSet::new(), &images).unwrap();
        assert_eq!(config.custom[0].station, "FM");
        assert_eq!(config.custom[0].extra["crossfade"], 5);
        config
            .groups
            .push(serde_json::json!({"members":["new", "new|song.mp3", "other"]}));
        let mut replacement = edit();
        replacement.tracks[0].path = "replacement.mp3".into();
        replacement
            .options
            .insert("textMode".into(), "titles".into());
        upsert(
            &mut config,
            replacement,
            &HashSet::from(["replacement.mp3".into()]),
            &images,
        )
        .unwrap();
        assert_eq!(
            config.groups[0]["members"],
            serde_json::json!(["new", "other"])
        );
        assert_eq!(config.custom[0].extra["textMode"], "titles");
        remove(&mut config, "new").unwrap();
        assert_eq!(config.groups[0]["members"], serde_json::json!(["other"]));
        assert!(config.custom.is_empty());
    }
    #[test]
    fn station_and_global_crossfade_edits_share_the_same_millisecond_range() {
        for number in [-1.0, 0.0, 1.0, 20.0, 30.0, 99.0, 100.0, 30000.0, 30001.0] {
            let mut station = Config::default();
            let mut global = Config::default();
            let value = serde_json::json!(number);
            let mut request = edit();
            request.options.insert("crossfade".into(), value.clone());
            let station_result = upsert(
                &mut station,
                request,
                &HashSet::from(["song.mp3".into()]),
                &HashSet::new(),
            );
            let global_result = crate::preferences::save_settings(
                &mut global,
                serde_json::Map::from_iter([("crossfade".into(), value)]),
            );
            assert_eq!(station_result.is_ok(), global_result.is_ok(), "{number}");
            assert_eq!(
                global_result.is_ok(),
                (0.0..=30000.0).contains(&number),
                "{number}"
            );
        }
    }
    #[test]
    fn removing_problem_tracks_preserves_files_station_options_and_other_stations() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("keep-on-disk.wav");
        std::fs::write(&audio, b"audio fixture").unwrap();
        let path = audio.to_string_lossy().into_owned();
        let mut config: Config=serde_json::from_value(serde_json::json!({"schemaVersion":crate::SCHEMA_VERSION,"custom":[
            {"key":"a","station":"A","tracks":[{"path":path,"name":"Bad"},{"path":"good.wav","name":"Good"}],"mode":"tracks","accent":"#abcdef","bumpers":{"enabled":false}},
            {"key":"b","station":"B","tracks":[{"path":path,"name":"Other station"}]}],
            "groups":[{"id":"g","name":"G","members":["a",format!("a|{path}"),"a|good.wav",format!("b|{path}")]}],"logos":{"a":"art.png"}})).unwrap();
        let options = config.custom[0].extra.clone();
        remove_tracks(&mut config, "a", &[path.clone(), "stale.wav".into()]).unwrap();
        assert_eq!(config.custom[0].tracks.len(), 1);
        assert_eq!(config.custom[0].tracks[0].path, "good.wav");
        assert_eq!(config.custom[0].extra, options);
        assert_eq!(config.logos["a"], "art.png");
        assert_eq!(config.custom[1].tracks[0].path, path);
        assert_eq!(
            config.groups[0]["members"],
            serde_json::json!(["a", "a|good.wav", format!("b|{path}")])
        );
        assert_eq!(std::fs::read(audio).unwrap(), b"audio fixture");
        remove_tracks(&mut config, "a", &["good.wav".into()]).unwrap();
        assert_eq!(config.custom.len(), 2);
        assert!(config.custom[0].tracks.is_empty());
        let store = crate::ConfigStore::new(dir.path().join("stations.json"));
        store.save(&config).unwrap();
        assert!(store.load().unwrap().custom[0].tracks.is_empty());
        let before = serde_json::to_string(&config).unwrap();
        assert!(remove_tracks(&mut config, "unknown", &[path]).is_err());
        assert_eq!(serde_json::to_string(&config).unwrap(), before);
    }
}

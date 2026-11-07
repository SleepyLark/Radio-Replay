use crate::{Config, Station, Track};
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

/// Resolve only tracks belonging to a saved station; missing files are skipped.
pub fn playback_files(config: &Config, key: &str) -> Result<Vec<PlayableTrack>, String> {
    let station = config
        .custom
        .iter()
        .find(|s| s.key == key)
        .ok_or("Station no longer exists")?;
    let mut files = Vec::new();
    for track in &station.tracks {
        let path = Path::new(&track.path);
        if !audio_file(path) {
            continue;
        }
        let Ok(path) = fs::canonicalize(path) else {
            continue;
        };
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
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
    Ok(files)
}
pub fn playback_files_for_group(
    mut config: Config,
    key: &str,
    group: Option<&str>,
) -> Result<Vec<PlayableTrack>, String> {
    if let Some(id) = group.filter(|g| *g != "all") {
        let members = config
            .groups
            .iter()
            .find(|g| g["id"].as_str() == Some(id))
            .and_then(|g| g["members"].as_array())
            .ok_or("Group unavailable")?
            .clone();
        let station = config
            .custom
            .iter_mut()
            .find(|s| s.key == key)
            .ok_or("Station unavailable")?;
        if !members.iter().any(|m| m.as_str() == Some(key)) {
            station.tracks.retain(|t| {
                members
                    .iter()
                    .any(|m| m.as_str() == Some(&format!("{key}|{}", t.path)))
            });
        }
    }
    playback_files(&config, key)
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

/// Deterministic depth-first scan. Skip symlinks/junctions to avoid escaping the chosen tree.
pub fn scan_folder(root: &Path) -> Result<Vec<Track>, String> {
    fn visit(root: &Path, depth: usize, output: &mut Vec<Track>) -> Result<(), String> {
        if depth > 64 {
            return Err("Folder nesting exceeds 64 levels".into());
        }
        let mut entries = fs::read_dir(root)
            .map_err(|e| format!("{}: {e}", root.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    continue;
                }
            }
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if matches!(
                    entry
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .as_str(),
                    "extra" | "extras"
                ) {
                    continue;
                }
                visit(&entry.path(), depth + 1, output)?;
            } else if metadata.is_file() && audio_file(&entry.path()) {
                if output.len() >= 50_000 {
                    return Err("Choose a smaller folder (maximum 50,000 tracks)".into());
                }
                output.push(track(entry.path())?);
            }
        }
        Ok(())
    }
    let mut tracks = Vec::new();
    visit(root, 0, &mut tracks)?;
    Ok(tracks)
}

/// Recover archive-era definitions without moving audio or overwriting existing stations.
pub fn recover_legacy(config: &mut Config) -> Result<usize, String> {
    if config.root.is_empty() {
        return Err("This configuration has no legacy archive root".into());
    }
    let original = PathBuf::from(&config.root);
    let root = fs::canonicalize(&original).map_err(|e| e.to_string())?;
    let tracks = scan_folder(&root)?;
    let mut stations: std::collections::BTreeMap<String, Station> = Default::default();
    for mut track in tracks {
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
    Ok(added)
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
    if edit.key.trim().is_empty() || edit.name.trim().is_empty() {
        return Err("Station name is required".into());
    }
    if !matches!(edit.mode.as_str(), "tracks" | "longset") {
        return Err("Invalid playback mode".into());
    }
    if edit.tracks.is_empty() {
        return Err("Add at least one audio file".into());
    }
    for (key, value) in &edit.options {
        let valid = match key.as_str() {
            "crossfade" => {
                value.is_null()
                    || value
                        .as_f64()
                        .map(|n| n == 0.0 || (100.0..=30000.0).contains(&n))
                        .unwrap_or(false)
            }
            "textMode" => matches!(
                value.as_str(),
                Some("auto" | "titles" | "lyrics" | "ignore")
            ),
            "spinLogo" => value.is_boolean(),
            "accent" | "bannerColor" | "bannerAccent" => value
                .as_str()
                .map(|s| {
                    s.is_empty()
                        || (s.len() == 7
                            && s.starts_with('#')
                            && s[1..].bytes().all(|b| b.is_ascii_hexdigit()))
                })
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
    for track in &edit.tracks {
        let retained = existing
            .map(|i| config.custom[i].tracks.iter().any(|t| t.path == track.path))
            .unwrap_or(false);
        if (!retained && !audio.contains(&track.path)) || !audio_file(Path::new(&track.path)) {
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
    let mut station = existing
        .map(|i| config.custom[i].clone())
        .unwrap_or(Station {
            key: edit.key.clone(),
            station: String::new(),
            tracks: vec![],
            extra: Default::default(),
        });
    station.station = edit.name.trim().into();
    station.tracks = edit.tracks;
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

pub fn collect_audio(paths: &[PathBuf]) -> Result<Vec<Track>, String> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        let tracks = if path.is_dir() {
            scan_folder(path)?
        } else if audio_file(path) {
            vec![track(path.clone())?]
        } else {
            continue;
        };
        for track in tracks {
            if seen.insert(track.path.clone()) {
                result.push(track);
            }
        }
        if result.len() > 100_000 {
            return Err("Too many audio files in this drop".into());
        }
    }
    if result.is_empty() {
        return Err("No supported audio files in this drop".into());
    }
    Ok(result)
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
        assert_eq!(recover_legacy(&mut config).unwrap(), 2);
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
        assert_eq!(recover_legacy(&mut config).unwrap(), 0);
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
        let tracks = scan_folder(dir.path()).unwrap();
        assert_eq!(
            tracks.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["a", "z"]
        );
    }
    #[test]
    fn dropped_audio_collects_folders_and_deduplicates_selected_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        let audio = dir.path().join("nested/song.mp3");
        fs::write(&audio, b"fixture").unwrap();
        let text = dir.path().join("notes.txt");
        fs::write(&text, b"notes").unwrap();
        let tracks = collect_audio(&[dir.path().into(), audio, text.clone()]).unwrap();
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
}

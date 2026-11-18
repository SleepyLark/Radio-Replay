//! Portable domain and persistence layer. No windowing or playback dependencies.
pub mod artwork;
pub mod audio_preferences;
pub mod bumpers;
pub mod changes;
pub mod desktop;
pub mod grants;
pub mod library;
pub mod m3u;
pub mod metadata;
pub mod path_policy;
#[cfg(test)]
mod performance_tests;
mod persistence;
pub mod preferences;
pub mod validation;
pub use persistence::{ConfigStore, RecoveryAction, RecoveryInfo};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;
pub const SCHEMA_VERSION: u32 = 2;
pub const MAX_CONFIG_BYTES: u64 = 256 * 1024 * 1024;
fn legacy_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub path: String,
    pub name: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Station {
    pub key: String,
    pub station: String,
    pub tracks: Vec<Track>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Keep unported settings and fields losslessly, including station/group extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(rename = "schemaVersion", default = "legacy_schema_version")]
    pub schema_version: u32,
    pub root: String,
    pub custom: Vec<Station>,
    pub groups: Vec<Value>,
    pub logos: Map<String, Value>,
    pub hidden: Vec<String>,
    pub settings: Map<String, Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            root: String::new(),
            custom: Vec::new(),
            groups: Vec::new(),
            logos: Map::new(),
            hidden: Vec::new(),
            settings: Map::new(),
            extra: Map::new(),
        }
    }
}

impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err("Library file exceeds the 256 MiB limit".into());
        }
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
            return Err("Expected a stations.json object".into());
        }
        let mut config: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if config.schema_version == 1 {
            // Preserve the old preview's seconds heuristic once, at the schema
            // boundary. Schema 2 stores milliseconds without runtime guessing.
            for value in std::iter::once(config.settings.get_mut("crossfade"))
                .chain(
                    config
                        .custom
                        .iter_mut()
                        .map(|station| station.extra.get_mut("crossfade")),
                )
                .flatten()
            {
                if let Some(number) = value.as_f64().filter(|n| *n > 0.0 && *n <= 30.0) {
                    *value = (number * 1000.0).into();
                }
            }
            config.schema_version = SCHEMA_VERSION;
        }
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "Library schema version {} requires a newer Radio Replay build",
                self.schema_version
            ));
        }
        let mut keys = std::collections::HashSet::new();
        for station in &self.custom {
            if station
                .extra
                .get("crossfade")
                .is_some_and(|value| !value.is_null() && !validation::crossfade(value))
            {
                return Err("Invalid station crossfade".into());
            }
            validation::identifier(&station.key, "Station identifier")?;
            validation::name(&station.station, "Station name")?;
            if station.key.is_empty() || !keys.insert(&station.key) {
                return Err("Station keys must be nonempty and unique".into());
            }
            library::validate_track_count(
                station
                    .extra
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("tracks"),
                station.tracks.len(),
            )?;
            for key in ["accent", "bannerColor", "bannerAccent"] {
                if let Some(value) = station.extra.get(key) {
                    if !value
                        .as_str()
                        .is_some_and(|value| validation::color(value, true))
                    {
                        return Err(format!("Invalid station color: {key}"));
                    }
                }
            }
        }
        let mut group_ids = std::collections::HashSet::new();
        for group in &self.groups {
            let id = group
                .get("id")
                .and_then(Value::as_str)
                .ok_or("Group identifier is required")?;
            validation::identifier(id, "Group identifier")?;
            if id == "all" || !group_ids.insert(id) {
                return Err("Group identifiers must be unique and cannot be all".into());
            }
            validation::name(
                group
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or("Group name is required")?,
                "Group name",
            )?;
        }
        if let Some(accent) = self.settings.get("accent") {
            if !accent
                .as_str()
                .is_some_and(|value| validation::color(value, false))
            {
                return Err("Invalid global accent color".into());
            }
        }
        if self
            .settings
            .get("crossfade")
            .is_some_and(|value| !validation::crossfade(value))
        {
            return Err("Invalid global crossfade".into());
        }
        Ok(())
    }
}
impl Config {
    fn validate_import_paths(&self) -> Result<(), String> {
        let config = self;
        for path in std::iter::once(config.root.as_str())
            .chain(config.logos.values().filter_map(Value::as_str))
            .chain(config.settings.get("staticPath").and_then(Value::as_str))
            .chain(config.custom.iter().flat_map(|station| {
                station
                    .tracks
                    .iter()
                    .map(|track| track.path.as_str())
                    .chain(station.extra.get("bannerPath").and_then(Value::as_str))
                    .chain(
                        station
                            .extra
                            .get("bumpers")
                            .and_then(|value| value.get("folder"))
                            .and_then(Value::as_str),
                    )
            }))
        {
            path_policy::validate_import(Path::new(path))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn crossfade_units_upgrade_once_and_current_schema_keeps_small_milliseconds() {
        for version in [None, Some(1)] {
            let mut input = serde_json::json!({"settings":{"crossfade":20},"custom":[{"key":"a","station":"A","tracks":[],"crossfade":5.5},{"key":"b","station":"B","tracks":[],"crossfade":null}]});
            if let Some(version) = version {
                input["schemaVersion"] = version.into();
            }
            let upgraded = Config::parse(&serde_json::to_vec(&input).unwrap()).unwrap();
            assert_eq!(upgraded.schema_version, SCHEMA_VERSION);
            assert_eq!(upgraded.settings["crossfade"], 20000.0);
            assert_eq!(upgraded.custom[0].extra["crossfade"], 5500.0);
            assert!(upgraded.custom[1].extra["crossfade"].is_null());
            let mut current = upgraded;
            current.settings.insert("crossfade".into(), 20.into());
            current.custom[0].extra.insert("crossfade".into(), 5.into());
            let reopened = Config::parse(&serde_json::to_vec(&current).unwrap()).unwrap();
            assert_eq!(reopened.settings["crossfade"], 20);
            assert_eq!(reopened.custom[0].extra["crossfade"], 5);
        }
        for value in [
            serde_json::json!(-1),
            serde_json::json!(30001),
            serde_json::json!("20"),
        ] {
            for input in [
                serde_json::json!({"schemaVersion":SCHEMA_VERSION,"settings":{"crossfade":value}}),
                serde_json::json!({"schemaVersion":SCHEMA_VERSION,"custom":[{"key":"a","station":"A","tracks":[],"crossfade":value}]}),
            ] {
                assert!(Config::parse(&serde_json::to_vec(&input).unwrap()).is_err());
            }
        }
    }
    #[test]
    fn imported_identifiers_names_and_colors_use_the_same_validation_as_edits() {
        for value in [
            serde_json::json!({"custom":[{"key":"a\u{0}b","station":"S","tracks":[]}]}),
            serde_json::json!({"custom":[{"key":"a|b","station":"S","tracks":[]}]}),
            serde_json::json!({"custom":[{"key":"s","station":"x".repeat(201),"tracks":[]}]}),
            serde_json::json!({"custom":[{"key":"s","station":"S","tracks":[],"accent":"red"}]}),
            serde_json::json!({"groups":[{"id":"g\u{0}","name":"G","members":[]}]}),
            serde_json::json!({"groups":[{"id":"g","name":"x".repeat(201),"members":[]}]}),
            serde_json::json!({"settings":{"accent":"#fff"}}),
        ] {
            assert!(
                Config::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{value}"
            );
        }
    }
    #[test]
    fn importing_network_filesystem_references_never_replaces_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        store.save(&Config::default()).unwrap();
        let before = fs::read(&store.path).unwrap();
        let remote = r"\\host\share\media.mp3";
        let source = dir.path().join("import.json");
        for value in [
            serde_json::json!({"root":remote}),
            serde_json::json!({"logos":{"station":remote}}),
            serde_json::json!({"settings":{"staticPath":remote}}),
            serde_json::json!({"custom":[{"key":"s","station":"S","tracks":[{"path":remote,"name":"Song"}]}]}),
            serde_json::json!({"custom":[{"key":"s","station":"S","tracks":[],"bannerPath":remote}]}),
            serde_json::json!({"custom":[{"key":"s","station":"S","tracks":[],"bumpers":{"folder":remote}}]}),
        ] {
            fs::write(&source, serde_json::to_vec(&value).unwrap()).unwrap();
            assert!(store.import(&source).unwrap_err().contains("Network"));
            assert_eq!(fs::read(&store.path).unwrap(), before);
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(&source).unwrap()).unwrap(),
                value
            );
        }
    }
    #[test]
    fn explicitly_selected_network_paths_can_still_be_saved_normally() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        let remote = r"\\host\share\song.mp3";
        let mut config = Config::default();
        crate::library::upsert(
            &mut config,
            crate::library::StationEdit {
                key: "nas".into(),
                name: "NAS library".into(),
                mode: "tracks".into(),
                tracks: vec![Track {
                    path: remote.into(),
                    name: "Song".into(),
                    extra: Default::default(),
                }],
                logo_path: None,
                options: Default::default(),
            },
            &std::collections::HashSet::from([remote.to_string()]),
            &Default::default(),
        )
        .unwrap();
        store.save(&config).unwrap();
        assert_eq!(store.load().unwrap().custom[0].tracks[0].path, remote);
    }
    #[test]
    fn legacy_roundtrip_preserves_extensions() {
        let mut value = serde_json::json!({"root":"D:/Radio", "custom":[{"key":"a","station":"FM","tracks":[{"path":"D:/song.mp3","name":"Song","duration":42}],"crossfade":5,"mode":"tracks"}],"settings":{"miniPosition":{"x":20}},"future":true});
        let config = Config::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        let output = serde_json::to_value(config).unwrap();
        value["custom"][0]["crossfade"] = 5000.0.into();
        assert_eq!(output["custom"], value["custom"]);
        assert_eq!(output["settings"], value["settings"]);
        assert_eq!(output["future"], true);
    }
    #[test]
    fn rejects_invalid_shape_and_duplicate_keys() {
        for input in [
            "null",
            "[]",
            "{\"custom\":null}",
            r#"{"custom":[{"key":"a","station":"A","tracks":[]},{"key":"a","station":"B","tracks":[]}]}"#,
        ] {
            assert!(Config::parse(input.as_bytes()).is_err());
        }
    }
    #[test]
    fn saves_backup_and_never_overwrites_corrupt_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stations.json");
        let store = ConfigStore::new(path.clone());
        store.save(&Config::default()).unwrap();
        let before = fs::read(&path).unwrap();
        let mut next = Config::default();
        next.root = "D:/Music".into();
        store.save(&next).unwrap();
        assert!(fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .any(|entry| fs::read(entry.unwrap().path()).unwrap() == before));
        assert_eq!(store.load().unwrap().root, next.root);
        fs::write(&path, "broken").unwrap();
        assert!(store.save(&next).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken");
    }
    #[test]
    fn successful_import_leaves_source_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("electron.json");
        let original = br#"{"root":"D:/Radio","custom":[],"groups":[{"id":"favorites","name":"Favorites","members":[]}],"settings":{"crossfade":5}}"#;
        fs::write(&source, original).unwrap();
        let store = ConfigStore::new(dir.path().join("preview/stations.json"));
        let imported = store.import(&source).unwrap();
        assert_eq!(imported.root, "D:/Radio");
        assert_eq!(store.load().unwrap().groups, imported.groups);
        assert_eq!(store.load().unwrap().settings["crossfade"], 5000.0);
        assert_eq!(fs::read(&source).unwrap(), original);
    }
    #[test]
    fn invalid_import_does_not_replace_library() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        store.save(&Config::default()).unwrap();
        let source = dir.path().join("bad.json");
        fs::write(&source, "[]").unwrap();
        assert!(store.import(&source).is_err());
        assert!(store.load().unwrap().custom.is_empty());
    }
}

//! Portable domain and persistence layer. No windowing or playback dependencies.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub path: String,
    pub name: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub key: String,
    pub station: String,
    pub tracks: Vec<Track>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Keep unported settings and fields losslessly, including station/group extensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub root: String,
    pub custom: Vec<Station>,
    pub groups: Vec<Value>,
    pub logos: Map<String, Value>,
    pub hidden: Vec<String>,
    pub settings: Map<String, Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if !value.is_object() {
            return Err("Expected a stations.json object".into());
        }
        let config: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let mut keys = std::collections::HashSet::new();
        for station in &config.custom {
            if station.key.is_empty() || !keys.insert(&station.key) {
                return Err("Station keys must be nonempty and unique".into());
            }
        }
        Ok(config)
    }
}

pub struct ConfigStore {
    path: PathBuf,
}
impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn load(&self) -> Result<Config, String> {
        match fs::read(&self.path) {
            Ok(bytes) => Config::parse(&bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn import(&self, source: &Path) -> Result<Config, String> {
        let config = Config::parse(&fs::read(source).map_err(|e| e.to_string())?)?;
        self.save(&config)?;
        Ok(config)
    }
    pub fn save(&self, config: &Config) -> Result<(), String> {
        // Refuse to overwrite unreadable/corrupt existing data.
        self.load()?;
        let bytes = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
        Config::parse(&bytes)?;
        let parent = self
            .path
            .parent()
            .ok_or("Missing config parent directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        if self.path.exists() {
            fs::copy(&self.path, self.path.with_extension("json.bak"))
                .map_err(|e| e.to_string())?;
        }
        let mut pending = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        pending.write_all(&bytes).map_err(|e| e.to_string())?;
        pending.as_file().sync_all().map_err(|e| e.to_string())?;
        pending.persist(&self.path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_roundtrip_preserves_extensions() {
        let value = serde_json::json!({"root":"D:/Radio", "custom":[{"key":"a","station":"FM","tracks":[{"path":"D:/song.mp3","name":"Song","duration":42}],"crossfade":5,"mode":"tracks"}],"settings":{"miniPosition":{"x":20}},"future":true});
        let config = Config::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        let output = serde_json::to_value(config).unwrap();
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
        assert_eq!(fs::read(path.with_extension("json.bak")).unwrap(), before);
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
        assert_eq!(store.load().unwrap().settings["crossfade"], 5);
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

use crate::{Config, MAX_CONFIG_BYTES};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const SAVE_BACKUPS: usize = 5;
const DAILY_BACKUPS: usize = 7;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    pub error: String,
    pub can_restore: bool,
    pub can_reset: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryAction {
    RestoreBackup,
    StartFresh,
}

pub struct ConfigStore {
    pub(crate) path: PathBuf,
    cached: Mutex<Option<CachedConfig>>,
}
struct CachedConfig {
    stamp: Option<FileStamp>,
    config: Arc<Config>,
    bytes: Option<Arc<[u8]>>,
}
#[derive(PartialEq, Eq)]
struct FileStamp {
    length: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    change: (i64, i64),
}
fn file_stamp(path: &Path) -> Result<Option<FileStamp>, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() {
        return Err("Library path is not a file".into());
    }
    Ok(Some(stamp_metadata(&metadata)))
}
fn stamp_metadata(metadata: &fs::Metadata) -> FileStamp {
    FileStamp {
        length: metadata.len(),
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
        #[cfg(unix)]
        change: {
            use std::os::unix::fs::MetadataExt;
            (metadata.ctime(), metadata.ctime_nsec())
        },
    }
}

fn read_open(file: fs::File, limit: u64) -> Result<Vec<u8>, String> {
    if file.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err("Library file exceeds the 256 MiB limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Library file exceeds the 256 MiB limit".into());
    }
    Ok(bytes)
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::File::open(path) {
        Ok(file) => read_open(file, MAX_CONFIG_BYTES).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    read_optional(path)?.ok_or_else(|| "Library file does not exist".into())
}

fn pending(path: &Path, bytes: &[u8]) -> Result<tempfile::NamedTempFile, String> {
    let parent = path.parent().ok_or("Missing config parent directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    Ok(file)
}

fn persist(mut file: tempfile::NamedTempFile, path: &Path) -> Result<fs::File, String> {
    for attempt in 0..4 {
        match file.persist(path) {
            Ok(file) => return Ok(file),
            Err(error) => {
                let transient = cfg!(windows)
                    && (error.error.kind() == io::ErrorKind::PermissionDenied
                        || matches!(error.error.raw_os_error(), Some(32 | 33)));
                if !transient || attempt == 3 {
                    return Err(error.error.to_string());
                }
                file = error.file;
                std::thread::sleep(std::time::Duration::from_millis(25 << attempt));
            }
        }
    }
    unreachable!()
}

pub(crate) fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    persist(pending(path, bytes)?, path).map(|_| ())
}

impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cached: Mutex::default(),
        }
    }
    pub fn load(&self) -> Result<Config, String> {
        Ok((*self.load_shared()?).clone())
    }
    /// Shared immutable reads for native tuning. Stat the library on every call;
    /// explicit reload/recovery/export and every save still verify file bytes.
    pub fn load_shared(&self) -> Result<Arc<Config>, String> {
        let mut cached = self.cached.lock().unwrap_or_else(|p| p.into_inner());
        for _ in 0..3 {
            let before = file_stamp(&self.path)?;
            let reusable = before.as_ref().is_none_or(|stamp| stamp.modified.is_some());
            if reusable {
                if let Some(value) = cached.as_ref().filter(|value| value.stamp == before) {
                    return Ok(value.config.clone());
                }
            }
            *cached = None;
            let bytes = read_optional(&self.path)?;
            let config = Arc::new(match &bytes {
                None => Config::default(),
                Some(bytes) => Config::parse(bytes)?,
            });
            if file_stamp(&self.path)? != before {
                continue;
            }
            *cached = Some(CachedConfig {
                stamp: before,
                config: config.clone(),
                bytes: bytes.map(Arc::from),
            });
            return Ok(config);
        }
        Err("Library changed while reading; try again".into())
    }
    pub fn reload(&self) -> Result<Config, String> {
        *self.cached.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.load()
    }
    fn invalidate(&self) {
        let mut cached = self.cached.lock().unwrap_or_else(|p| p.into_inner());
        // Do not label an external replacement as this save's contents. The next
        // read loads the committed file through the normal validation path.
        *cached = None;
    }
    pub fn import(&self, source: &Path) -> Result<Config, String> {
        let config = Config::parse(&read(source)?)?;
        config.validate_import_paths()?;
        self.save(&config)?;
        Ok(config)
    }
    fn bytes(config: &Config) -> Result<Vec<u8>, String> {
        config.validate()?;
        let bytes = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err("Library file exceeds the 256 MiB limit".into());
        }
        Ok(bytes)
    }
    pub fn save(&self, config: &Config) -> Result<(), String> {
        self.save_shared(Arc::new(config.clone())).map(|_| ())
    }
    pub fn save_owned(&self, config: Config) -> Result<Arc<Config>, String> {
        self.save_shared(Arc::new(config))
    }
    /// Verify a no-op mutation without cloning/serializing a whole library.
    /// Exact fresh bytes, not a metadata stamp, establish that nothing changed.
    pub fn confirm_unchanged(&self, snapshot: &Arc<Config>) -> Result<Arc<Config>, String> {
        let previous = read_optional(&self.path)?;
        let known = self
            .cached
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .filter(|cached| {
                Arc::ptr_eq(&cached.config, snapshot)
                    && cached.bytes.as_deref() == previous.as_deref()
            })
            .map(|cached| cached.config.clone());
        if let Some(config) = known {
            return Ok(config);
        }
        self.reload()?;
        Err("Library changed while saving; retry with the current library".into())
    }
    fn save_shared(&self, config: Arc<Config>) -> Result<Arc<Config>, String> {
        // Never overwrite unreadable data outside the explicit recovery flow.
        let previous = read_optional(&self.path)?;
        let known = self
            .cached
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .filter(|cached| cached.bytes.as_deref() == previous.as_deref())
            .map(|cached| cached.config.clone());
        if known.is_none() {
            if let Some(previous) = &previous {
                Config::parse(previous)?;
            }
        }
        let bytes = Self::bytes(&config)?;
        if previous.as_deref() == Some(bytes.as_slice()) {
            // Preserve the existing immutable snapshot for no-op saves.
            return Ok(known.unwrap_or(config));
        }
        let file = pending(&self.path, &bytes)?;
        let written = stamp_metadata(&file.as_file().metadata().map_err(|e| e.to_string())?);
        if let Some(previous) = previous {
            self.backup(&previous)?;
        }
        let committed = persist(file, &self.path)?;
        let stamp = committed
            .metadata()
            .ok()
            .map(|metadata| stamp_metadata(&metadata));
        self.invalidate();
        if let Some(stamp) = stamp.filter(|stamp| {
            stamp.length == written.length
                && stamp.modified == written.modified
                && file_stamp(&self.path).ok().flatten().as_ref() == Some(stamp)
        }) {
            *self.cached.lock().unwrap_or_else(|p| p.into_inner()) = Some(CachedConfig {
                stamp: Some(stamp),
                config: config.clone(),
                bytes: Some(Arc::from(bytes)),
            });
        }
        Ok(config)
    }
    fn backup(&self, bytes: &[u8]) -> Result<(), String> {
        let directory = self.path.with_file_name("backups");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        let mut file = tempfile::Builder::new()
            .prefix(&format!("stations-{:020}-", now.as_nanos()))
            .suffix(".save.json.bak")
            .tempfile_in(&directory)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.keep().map_err(|e| e.error.to_string())?;
        let daily = directory.join(format!(
            "stations-{:010}.day.json.bak",
            now.as_secs() / 86400
        ));
        if !daily.exists() {
            pending(&daily, bytes)?
                .persist_noclobber(&daily)
                .map_err(|e| e.error.to_string())?;
        }
        self.prune(".save.json.bak", SAVE_BACKUPS)?;
        self.prune(".day.json.bak", DAILY_BACKUPS)
    }
    fn backups(&self) -> Result<Vec<PathBuf>, String> {
        let directory = self.path.with_file_name("backups");
        let mut paths = match fs::read_dir(directory) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|name| {
                            name.starts_with("stations-")
                                && (name.ends_with(".save.json.bak")
                                    || name.ends_with(".day.json.bak"))
                        })
                })
                .collect::<Vec<_>>(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.to_string()),
        };
        paths.sort_by_cached_key(|path| {
            (
                fs::metadata(path)
                    .and_then(|m| m.modified())
                    .unwrap_or(UNIX_EPOCH),
                path.clone(),
            )
        });
        paths.reverse();
        Ok(paths)
    }
    fn prune(&self, suffix: &str, keep: usize) -> Result<(), String> {
        for path in self
            .backups()?
            .into_iter()
            .filter(|path| path.to_string_lossy().ends_with(suffix))
            .skip(keep)
        {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn readable_backup(&self) -> Option<Vec<u8>> {
        self.backups()
            .unwrap_or_default()
            .into_iter()
            .chain(std::iter::once(self.path.with_extension("json.bak")))
            .find_map(|path| {
                read(&path)
                    .ok()
                    .filter(|bytes| Config::parse(bytes).is_ok())
            })
    }
    pub fn recovery_info(&self) -> RecoveryInfo {
        RecoveryInfo {
            error: self.reload().err().unwrap_or_default(),
            can_restore: self.readable_backup().is_some(),
            can_reset: fs::File::open(&self.path).is_ok(),
        }
    }
    pub fn recover(&self, action: RecoveryAction) -> Result<Config, String> {
        if self.reload().is_ok() {
            return Err(
                "The library is readable now. Reload it before attempting recovery.".into(),
            );
        }
        let config = match action {
            RecoveryAction::RestoreBackup => Config::parse(
                &self
                    .readable_backup()
                    .ok_or("No readable library backup is available")?,
            )?,
            RecoveryAction::StartFresh => Config::default(),
        };
        let file = pending(&self.path, &Self::bytes(&config)?)?;
        // Preserve the original before replacement. A failed write leaves it in place too.
        let parent = self
            .path
            .parent()
            .ok_or("Missing config parent directory")?;
        let mut damaged = tempfile::Builder::new()
            .prefix("stations.corrupt-")
            .suffix(".json")
            .tempfile_in(parent)
            .map_err(|e| e.to_string())?;
        io::copy(
            &mut fs::File::open(&self.path).map_err(|e| e.to_string())?,
            &mut damaged,
        )
        .map_err(|e| e.to_string())?;
        damaged.as_file().sync_all().map_err(|e| e.to_string())?;
        damaged.keep().map_err(|e| e.error.to_string())?;
        persist(file, &self.path)?;
        self.invalidate();
        Ok(config)
    }
    pub fn export(&self, destination: &Path) -> Result<(), String> {
        if destination == self.path
            || (destination.exists()
                && fs::canonicalize(destination).ok() == fs::canonicalize(&self.path).ok())
        {
            return Err("Choose a different file for the library export".into());
        }
        let bytes = Self::bytes(&self.reload()?)?;
        persist(pending(destination, &bytes)?, destination).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_saves_publish_validated_snapshots_and_noops_preserve_files_and_backups() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        let mut config = Config::default();
        config
            .extra
            .insert("future".into(), serde_json::json!({"kept":true}));
        let saved = store.save_owned(config).unwrap();
        assert!(Arc::ptr_eq(&saved, &store.load_shared().unwrap()));
        let before = fs::metadata(&store.path).unwrap().modified().unwrap();
        let backups = store.backups().unwrap();
        assert!(Arc::ptr_eq(
            &saved,
            &store.confirm_unchanged(&saved).unwrap()
        ));
        assert!(Arc::ptr_eq(
            &saved,
            &store.save_owned((*saved).clone()).unwrap()
        ));
        assert_eq!(
            fs::metadata(&store.path).unwrap().modified().unwrap(),
            before
        );
        assert_eq!(store.backups().unwrap(), backups);
        assert_eq!(
            store.load().unwrap().extra["future"],
            serde_json::json!({"kept":true})
        );
        let mut external = (*saved).clone();
        external.root = "external".into();
        fs::write(&store.path, serde_json::to_vec(&external).unwrap()).unwrap();
        assert!(store
            .confirm_unchanged(&saved)
            .unwrap_err()
            .contains("changed"));
        assert_eq!(store.load().unwrap().root, "external");
        let current = store.load_shared().unwrap();
        fs::write(
            &store.path,
            serde_json::to_vec(&serde_json::json!({"schemaVersion":crate::SCHEMA_VERSION+1}))
                .unwrap(),
        )
        .unwrap();
        assert!(store.confirm_unchanged(&current).is_err());
    }
    #[test]
    fn shared_reads_reuse_the_parse_and_invalidate_after_save_external_edit_or_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        let mut config = Config::default();
        config.root = "first".into();
        store.save(&config).unwrap();
        let first = store.load_shared().unwrap();
        assert!(Arc::ptr_eq(&first, &store.load_shared().unwrap()));
        let mut owned = store.load().unwrap();
        owned.root = "owned copy".into();
        assert_eq!(store.load_shared().unwrap().root, "first");
        config.root = "saved second".into();
        store.save(&config).unwrap();
        let saved = store.load_shared().unwrap();
        assert!(!Arc::ptr_eq(&first, &saved));
        assert_eq!(saved.root, "saved second");
        config.root = "external third".into();
        fs::write(&store.path, serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(store.load_shared().unwrap().root, "external third");
        fs::write(&store.path, b"broken").unwrap();
        assert!(store.load_shared().is_err());
        fs::remove_file(&store.path).unwrap();
        assert!(store.load_shared().unwrap().custom.is_empty());
        assert_eq!(
            first.root, "first",
            "Existing readers retain an immutable snapshot"
        );
    }
    #[test]
    fn explicit_reload_reads_bytes_and_cached_reads_never_authorize_overwriting_future_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        let mut config = Config::default();
        config.root = "first".into();
        store.save(&config).unwrap();
        store.load_shared().unwrap();
        let modified = fs::metadata(&store.path).unwrap().modified().unwrap();
        let mut external = fs::read(&store.path).unwrap();
        let offset = external.windows(5).position(|s| s == b"first").unwrap();
        external[offset..offset + 5].copy_from_slice(b"later");
        fs::write(&store.path, &external).unwrap();
        fs::File::options()
            .write(true)
            .open(&store.path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_eq!(store.reload().unwrap().root, "later");
        let mut future = serde_json::to_vec(
            &serde_json::json!({"schemaVersion":crate::SCHEMA_VERSION+1,"custom":[]}),
        )
        .unwrap();
        future.resize(external.len(), b' ');
        fs::write(&store.path, &future).unwrap();
        fs::File::options()
            .write(true)
            .open(&store.path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert!(store.save(&config).is_err());
        assert_eq!(fs::read(&store.path).unwrap(), future);
    }
    fn damaged_copies(directory: &Path) -> Vec<Vec<u8>> {
        fs::read_dir(directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("stations.corrupt-")
            })
            .map(|entry| fs::read(entry.path()).unwrap())
            .collect()
    }
    #[test]
    fn restores_the_newest_readable_backup_and_preserves_the_damaged_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        let mut config = Config::default();
        for name in ["first", "second", "third"] {
            config.root = name.into();
            store.save(&config).unwrap();
        }
        fs::write(
            dir.path().join("backups/stations-zz.save.json.bak"),
            b"invalid backup",
        )
        .unwrap();
        fs::write(&store.path, b"{broken").unwrap();
        let info = store.recovery_info();
        assert!(info.can_restore && info.can_reset && !info.error.is_empty());
        let recovered = store.recover(RecoveryAction::RestoreBackup).unwrap();
        assert_eq!(recovered.root, "second");
        assert_eq!(store.load().unwrap().root, "second");
        assert_eq!(damaged_copies(dir.path()), vec![b"{broken".to_vec()]);
    }
    #[test]
    fn starting_fresh_keeps_the_original_and_will_not_reset_a_readable_library() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        fs::write(&store.path, b"broken").unwrap();
        assert!(!store.recovery_info().can_restore);
        assert!(store.recover(RecoveryAction::RestoreBackup).is_err());
        assert_eq!(fs::read(&store.path).unwrap(), b"broken");
        assert!(damaged_copies(dir.path()).is_empty());
        let fresh = store.recover(RecoveryAction::StartFresh).unwrap();
        assert!(fresh.custom.is_empty());
        assert_eq!(fresh.schema_version, crate::SCHEMA_VERSION);
        assert_eq!(damaged_copies(dir.path()), vec![b"broken".to_vec()]);
        assert!(store.recover(RecoveryAction::StartFresh).is_err());
        assert_eq!(damaged_copies(dir.path()).len(), 1);
    }
    #[test]
    fn backups_are_bounded_and_identical_saves_do_not_rotate_them() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        let mut config = Config::default();
        for i in 0..20 {
            config.root = i.to_string();
            store.save(&config).unwrap();
        }
        let paths = store.backups().unwrap();
        assert_eq!(
            paths
                .iter()
                .filter(|p| p.to_string_lossy().ends_with(".save.json.bak"))
                .count(),
            SAVE_BACKUPS
        );
        assert_eq!(
            paths
                .iter()
                .filter(|p| p.to_string_lossy().ends_with(".day.json.bak"))
                .count(),
            1
        );
        for _ in 0..10 {
            store.save(&config).unwrap();
        }
        assert_eq!(store.backups().unwrap(), paths);
        let directory = dir.path().join("backups");
        for day in 0..20 {
            fs::write(
                directory.join(format!("stations-{day:010}.day.json.bak")),
                b"{}",
            )
            .unwrap();
        }
        store.prune(".day.json.bak", DAILY_BACKUPS).unwrap();
        assert_eq!(
            store
                .backups()
                .unwrap()
                .iter()
                .filter(|p| p.to_string_lossy().ends_with(".day.json.bak"))
                .count(),
            DAILY_BACKUPS
        );
        assert_eq!(store.load().unwrap().root, "19");
    }
    #[test]
    fn exports_preserve_the_current_library_and_reject_its_own_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        let mut config = Config::default();
        config.root = "music".into();
        store.save(&config).unwrap();
        let before = fs::read(&store.path).unwrap();
        let destination = dir.path().join("export.json");
        store.export(&destination).unwrap();
        assert_eq!(
            Config::parse(&fs::read(destination).unwrap()).unwrap().root,
            "music"
        );
        assert_eq!(fs::read(&store.path).unwrap(), before);
        assert!(store.export(&store.path).is_err());
        assert!(store.backups().unwrap().is_empty());
    }
    #[test]
    fn bounded_reads_and_unknown_future_schemas_cannot_be_saved_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stations.json");
        fs::write(&path, b"123456789").unwrap();
        assert!(read_open(fs::File::open(&path).unwrap(), 8).is_err());
        let future = serde_json::to_vec(
            &serde_json::json!({"schemaVersion":crate::SCHEMA_VERSION+1,"custom":[]}),
        )
        .unwrap();
        fs::write(&path, &future).unwrap();
        let store = ConfigStore::new(path.clone());
        assert!(store.load().unwrap_err().contains("newer"));
        assert!(store.save(&Config::default()).is_err());
        assert_eq!(fs::read(path).unwrap(), future);
        assert!(damaged_copies(dir.path()).is_empty());
    }
}

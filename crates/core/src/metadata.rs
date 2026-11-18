use lofty::{
    file::{AudioFile, TaggedFileExt},
    tag::Accessor,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::sync::{Mutex, MutexGuard};
use std::{fs, io::Read, path::Path};
mod prepare;
pub use prepare::{measure_durations, remember_prepared_durations, DurationPreparation};

pub const CACHE_CAPACITY: usize = 250_000;
const CACHE_SCHEMA: i64 = 1;
static NEXT_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct Metadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<f64>,
    #[serde(default)]
    pub tags_read: bool,
}
#[derive(Serialize)]
pub struct Details {
    pub tags: Metadata,
    pub sidecar: Option<String>,
}
pub struct Cache {
    db: Option<Connection>,
    path: std::path::PathBuf,
    clock: Cell<i64>,
    epoch: u64,
}
struct Write {
    path: std::path::PathBuf,
    signature: String,
    value: Metadata,
}

/// Only derived cache state is recovered after poisoning; library locks stay strict.
pub fn lock(cache: &Mutex<Cache>) -> MutexGuard<'_, Cache> {
    cache.lock().unwrap_or_else(|poisoned| {
        let mut guard = poisoned.into_inner();
        let path = guard.path.clone();
        guard.db = None;
        *guard = Cache::open(&path);
        cache.clear_poison();
        guard
    })
}

pub fn read_cached(cache: &Mutex<Cache>, path: &Path) -> Metadata {
    read_cached_with(cache, path, parse_file)
}

fn read_cached_with(
    cache: &Mutex<Cache>,
    path: &Path,
    parse: impl FnOnce(&Path, Metadata) -> Metadata,
) -> Metadata {
    let Some(signature) = Cache::signature(path) else {
        return Metadata::default();
    };
    let (value, epoch) = {
        let cache = lock(cache);
        let value = cache.lookup(path, &signature).unwrap_or_default();
        let _ = cache.touch([path.to_string_lossy().as_ref()]);
        (value, cache.epoch)
    };
    if value.tags_read {
        return value;
    }
    // Parsing may read arbitrary media and panic. No cache guard crosses this call.
    let value = parse_safely(path, value, parse);
    if Cache::signature(path).as_deref() == Some(signature.as_str()) {
        let cache = lock(cache);
        if cache.epoch == epoch {
            let _ = cache.put(path, &signature, &value);
        }
    }
    value
}

fn parse_safely(
    path: &Path,
    mut value: Metadata,
    parse: impl FnOnce(&Path, Metadata) -> Metadata,
) -> Metadata {
    value.tags_read = true;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse(path, value.clone())))
        .unwrap_or(value)
}

fn parse_file(path: &Path, mut value: Metadata) -> Metadata {
    if let Ok(file) = lofty::probe::Probe::open(path).and_then(|probe| {
        probe
            .options(lofty::config::ParseOptions::new().read_cover_art(false))
            .read()
    }) {
        let seconds = file.properties().duration().as_secs_f64();
        if seconds.is_finite() && seconds > 0.0 {
            value.duration = Some(seconds);
        }
        if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
            value.title = tag.title().unwrap_or_default().into_owned();
            value.artist = tag.artist().unwrap_or_default().into_owned();
            value.album = tag.album().unwrap_or_default().into_owned();
        }
    }
    value
}
impl Cache {
    pub fn open(path: &Path) -> Self {
        let open = || -> rusqlite::Result<(Connection, i64)> {
            let db = Connection::open(path)?;
            db.busy_timeout(std::time::Duration::from_secs(2))?;
            let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
            if version != 0 && version != CACHE_SCHEMA {
                // Derived data only: an incompatible cache never blocks a library.
                db.execute_batch("DROP TABLE IF EXISTS metadata_v1;")?;
            }
            db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
                PRAGMA cache_size=-2048; PRAGMA mmap_size=0;
                CREATE TABLE IF NOT EXISTS metadata_v1(path TEXT PRIMARY KEY, signature TEXT NOT NULL, value TEXT NOT NULL, used INTEGER NOT NULL);
                CREATE INDEX IF NOT EXISTS metadata_used ON metadata_v1(used);")?;
            db.pragma_update(None, "user_version", CACHE_SCHEMA)?;
            let clock =
                db.query_row("SELECT COALESCE(MAX(used),0) FROM metadata_v1", [], |row| {
                    row.get(0)
                })?;
            Ok((db, clock))
        };
        let opened = open().or_else(|error| {
            // Do not rename a healthy database that is locked, read-only or on
            // an unavailable drive. Only SQLite's corruption codes allow recovery.
            if matches!(&error, rusqlite::Error::SqliteFailure(code, _)
                if matches!(code.code, rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase))
            {
                match preserve_corrupt_cache(path) {
                    Ok(directory) => {
                        eprintln!("Corrupt metadata cache preserved in {}", directory.display());
                        return open();
                    }
                    Err(failure) => eprintln!("Could not preserve corrupt metadata cache: {failure}"),
                }
            }
            Err(error)
        });
        let (db, clock) = match opened {
            Ok((db, clock)) => (Some(db), clock),
            Err(_) => (None, 0),
        };
        Self {
            db,
            path: path.to_path_buf(),
            clock: Cell::new(clock),
            epoch: NEXT_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }
    pub fn clear(&mut self) -> Result<(), String> {
        self.epoch = NEXT_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.db.take(); // Close before unlinking, including SQLite's WAL/SHM files.
        for suffix in ["", "-wal", "-shm"] {
            let mut path = self.path.as_os_str().to_os_string();
            path.push(suffix);
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        *self = Self::open(&self.path);
        if self.db.is_none() {
            return Err("Metadata cache could not be rebuilt".into());
        }
        Ok(())
    }
    fn signature(path: &Path) -> Option<String> {
        let meta = fs::metadata(path).ok()?;
        Self::signature_from_metadata(&meta)
    }
    pub fn signature_from_metadata(meta: &fs::Metadata) -> Option<String> {
        let modified = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        #[cfg(unix)]
        let change = {
            use std::os::unix::fs::MetadataExt;
            format!("{}:{}", meta.ctime(), meta.ctime_nsec())
        };
        #[cfg(not(unix))]
        let change = creation_stamp(meta.created().ok());
        Some(format!("{}:{modified}:{change}", meta.len()))
    }
    fn put(&self, path: &Path, signature: &str, value: &Metadata) -> Result<(), String> {
        self.put_batch(&[Write {
            path: path.into(),
            signature: signature.into(),
            value: value.clone(),
        }])
    }
    fn tick(&self) -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(i64::MAX as u128) as i64;
        let next = self.clock.get().saturating_add(1).max(now);
        self.clock.set(next);
        next
    }
    fn put_batch(&self, entries: &[Write]) -> Result<(), String> {
        if entries.is_empty() {
            return Ok(());
        }
        let db = self.db.as_ref().ok_or("Duration cache is unavailable")?;
        let used = self.tick();
        let transaction = db.unchecked_transaction().map_err(|e| e.to_string())?;
        {
            let mut statement = transaction
                .prepare_cached("INSERT OR REPLACE INTO metadata_v1 VALUES(?1,?2,?3,?4)")
                .map_err(|e| e.to_string())?;
            for entry in entries {
                let mut value = entry.value.clone();
                if let Some(previous) = self.lookup(&entry.path, &entry.signature) {
                    if !value.tags_read && previous.tags_read {
                        value.title = previous.title;
                        value.artist = previous.artist;
                        value.album = previous.album;
                        value.tags_read = true;
                    }
                    if value.duration.is_none() {
                        value.duration = previous.duration;
                    }
                }
                let json = serde_json::to_string(&value).map_err(|e| e.to_string())?;
                statement
                    .execute(params![
                        entry.path.to_string_lossy(),
                        entry.signature,
                        json,
                        used
                    ])
                    .map_err(|e| e.to_string())?;
            }
        }
        Self::prune(&transaction, CACHE_CAPACITY).map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
    fn prune(db: &Connection, capacity: usize) -> rusqlite::Result<()> {
        let count: i64 = db.query_row("SELECT COUNT(*) FROM metadata_v1", [], |row| row.get(0))?;
        if count > capacity as i64 {
            db.execute("DELETE FROM metadata_v1 WHERE rowid IN (SELECT rowid FROM metadata_v1 ORDER BY used ASC,rowid ASC LIMIT ?1)", [count-capacity as i64])?;
        }
        Ok(())
    }
    fn touch<'a>(&self, paths: impl IntoIterator<Item = &'a str>) -> Result<(), String> {
        let paths: std::collections::HashSet<_> = paths.into_iter().collect();
        if paths.is_empty() {
            return Ok(());
        }
        let db = self.db.as_ref().ok_or("Duration cache is unavailable")?;
        let used = self.tick();
        let transaction = db.unchecked_transaction().map_err(|e| e.to_string())?;
        {
            let mut statement = transaction
                .prepare_cached("UPDATE metadata_v1 SET used=?1 WHERE path=?2")
                .map_err(|e| e.to_string())?;
            for path in paths {
                statement
                    .execute(params![used, path])
                    .map_err(|e| e.to_string())?;
            }
        }
        transaction.commit().map_err(|e| e.to_string())
    }
    pub fn peek(&self, path: &Path) -> Option<Metadata> {
        let signature = Self::signature(path)?;
        self.lookup(path, &signature)
    }
    fn lookup(&self, path: &Path, signature: &str) -> Option<Metadata> {
        if let Some(db) = &self.db {
            let cached: Option<String> = db
                .prepare_cached("SELECT value FROM metadata_v1 WHERE path=?1 AND signature=?2")
                .and_then(|mut statement| {
                    statement
                        .query_row(params![path.to_string_lossy(), signature], |row| row.get(0))
                })
                .optional()
                .ok()
                .flatten();
            if let Some(value) = cached.and_then(|s| serde_json::from_str(&s).ok()) {
                return Some(value);
            }
        }
        None
    }
    /// A station's files have already been statted and canonicalized. Reuse those
    /// signatures and one prepared statement. Refresh recency in one transaction.
    pub fn populate_durations(&self, tracks: &mut [crate::library::PlayableTrack]) {
        let Some(db) = &self.db else {
            return;
        };
        let Ok(mut statement) =
            db.prepare_cached("SELECT value,used FROM metadata_v1 WHERE path=?1 AND signature=?2")
        else {
            return;
        };
        let mut hits = Vec::new();
        // Recency is approximate to one minute. Rapid warm retunes must not
        // rewrite every row merely to say it was read a few seconds later.
        let refresh_before = self.tick().saturating_sub(60_000);
        for track in tracks {
            let Some(signature) = &track.cache_signature else {
                continue;
            };
            let value: Option<(String, i64)> = statement
                .query_row(params![track.track.path, signature], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .optional()
                .ok()
                .flatten();
            let used = value.as_ref().map(|(_, used)| *used);
            track.duration = value
                .and_then(|(json, _)| serde_json::from_str::<Metadata>(&json).ok())
                .and_then(|m| m.duration)
                .filter(|n| n.is_finite() && *n > 0.0 && *n <= 172800.0);
            if track.duration.is_some() && used.is_some_and(|used| used < refresh_before) {
                hits.push(track.track.path.as_str());
            }
        }
        drop(statement);
        let _ = self.touch(hits);
    }
    pub fn read(&self, path: &Path) -> Metadata {
        let Some(signature) = Self::signature(path) else {
            return Metadata::default();
        };
        let mut value = self.lookup(path, &signature).unwrap_or_default();
        let _ = self.touch([path.to_string_lossy().as_ref()]);
        if value.tags_read {
            return value;
        }
        value = parse_safely(path, value, parse_file);
        if Self::signature(path).as_deref() == Some(signature.as_str()) {
            let _ = self.put(path, &signature, &value);
        }
        value
    }
    pub fn remember_duration(&self, path: &Path, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds <= 0.0 || seconds > 48.0 * 3600.0 {
            return Err("Invalid duration".into());
        }
        let signature = Self::signature(path).ok_or("Audio file unavailable")?;
        let mut value = self.lookup(path, &signature).unwrap_or_default();
        value.duration = Some(seconds);
        if Self::signature(path).as_deref() == Some(signature.as_str()) {
            self.put(path, &signature, &value)?;
        } else {
            return Err("Audio changed while caching its duration".into());
        }
        Ok(())
    }
}

// Keep database and journal files together; no damaged bytes become a fresh WAL.
// The unique directory is retained even if moving or rolling back a file fails.
fn preserve_corrupt_cache(path: &Path) -> std::io::Result<std::path::PathBuf> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = tempfile::Builder::new()
        .prefix("metadata-cache-corrupt-")
        .tempdir_in(parent)?
        .keep();
    let mut moved = Vec::new();
    let result = (|| {
        for suffix in ["", "-wal", "-shm"] {
            let mut source = path.as_os_str().to_os_string();
            source.push(suffix);
            let source = std::path::PathBuf::from(source);
            match fs::symlink_metadata(&source) {
                Ok(metadata) if metadata.is_file() => {}
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound && !suffix.is_empty() =>
                {
                    continue
                }
                Err(error) => return Err(error),
                _ => {
                    return Err(std::io::Error::other(
                        "Cache recovery requires regular files",
                    ))
                }
            }
            let destination = directory.join(
                source
                    .file_name()
                    .ok_or_else(|| std::io::Error::other("Missing cache filename"))?,
            );
            fs::rename(&source, &destination)?;
            moved.push((source, destination));
        }
        Ok(())
    })();
    if let Err(error) = result {
        for (source, destination) in moved.into_iter().rev() {
            let _ = fs::rename(destination, source);
        }
        let _ = fs::remove_dir(&directory); // Only if empty; never discard preserved data.
        return Err(error);
    }
    Ok(directory)
}

#[cfg(any(not(unix), test))]
fn creation_stamp(time: Option<std::time::SystemTime>) -> String {
    match time.map(|time| time.duration_since(std::time::UNIX_EPOCH)) {
        Some(Ok(duration)) => duration.as_nanos().to_string(),
        Some(Err(before)) => format!("-{}", before.duration().as_nanos()),
        None => "unknown".into(),
    }
}

/// Sidecars are read fresh, never from the metadata cache. Limit allocations to 1 MiB.
pub fn sidecar(path: &Path) -> Option<String> {
    for ext in ["lrc", "txt"] {
        let candidate = path.with_extension(ext);
        let file = match fs::File::open(candidate) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return None,
        };
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 1024 * 1024 {
            return None;
        }
        return String::from_utf8(bytes).ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thirty_thousand_rows_survive_and_batched_hits_refresh_eviction_order() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(&dir.path().join("cache.sqlite"));
        let entries: Vec<_> = (0..30000)
            .map(|i| Write {
                path: format!("song-{i}").into(),
                signature: "same".into(),
                value: Metadata {
                    duration: Some(9.0),
                    ..Default::default()
                },
            })
            .collect();
        cache.put_batch(&entries).unwrap();
        let db = cache.db.as_ref().unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM metadata_v1", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            30000
        );
        cache
            .touch(
                entries[..100]
                    .iter()
                    .map(|entry| entry.path.to_str().unwrap()),
            )
            .unwrap();
        Cache::prune(db, 29900).unwrap();
        for i in 0..30000 {
            assert_eq!(
                cache
                    .lookup(Path::new(&format!("song-{i}")), "same")
                    .is_some(),
                !(100..200).contains(&i)
            );
        }
    }
    #[test]
    fn clear_reclaims_storage_and_incompatible_schema_rebuilds_derived_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        let mut cache = Cache::open(&path);
        cache
            .put(
                Path::new("large"),
                "same",
                &Metadata {
                    title: "x".repeat(1024 * 1024),
                    ..Default::default()
                },
            )
            .unwrap();
        cache
            .db
            .as_ref()
            .unwrap()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        let before = fs::metadata(&path).unwrap().len();
        let epoch = cache.epoch;
        cache.clear().unwrap();
        assert_ne!(cache.epoch, epoch);
        cache
            .db
            .as_ref()
            .unwrap()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        assert!(fs::metadata(&path).unwrap().len() < before / 2);
        cache
            .put(Path::new("future"), "same", &Metadata::default())
            .unwrap();
        cache
            .db
            .as_ref()
            .unwrap()
            .pragma_update(None, "user_version", 999)
            .unwrap();
        drop(cache);
        let cache = Cache::open(&path);
        assert_eq!(
            cache
                .db
                .as_ref()
                .unwrap()
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            CACHE_SCHEMA
        );
        assert!(cache.lookup(Path::new("future"), "same").is_none());
    }
    #[test]
    fn parser_panics_are_contained_without_holding_or_poisoning_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("malformed.mp3");
        fs::write(&audio, b"ID3 truncated").unwrap();
        let cache = Mutex::new(Cache::open(&dir.path().join("cache.sqlite")));
        let parsed_without_lock = Cell::new(false);
        let value = read_cached_with(&cache, &audio, |_, _| {
            parsed_without_lock.set(cache.try_lock().is_ok());
            panic!("Injected parser panic");
        });
        assert!(parsed_without_lock.get(), "Parsing held the cache lock");
        assert!(value.duration.is_none());
        assert!(!cache.is_poisoned());
        lock(&cache).remember_duration(&audio, 12.0).unwrap();
        assert_eq!(read_cached(&cache, &audio).duration, Some(12.0));
    }
    #[test]
    fn poisoned_cache_reopens_and_remains_usable() {
        let dir = tempfile::tempdir().unwrap();
        let cache = std::sync::Arc::new(Mutex::new(Cache::open(&dir.path().join("cache.sqlite"))));
        let other = cache.clone();
        assert!(std::thread::spawn(move || {
            let _guard = other.lock().unwrap();
            panic!("Injected cache panic");
        })
        .join()
        .is_err());
        assert!(cache.is_poisoned());
        let audio = dir.path().join("song.mp3");
        fs::write(&audio, b"fixture").unwrap();
        lock(&cache).remember_duration(&audio, 42.0).unwrap();
        assert!(!cache.is_poisoned());
        assert_eq!(read_cached(&cache, &audio).duration, Some(42.0));
    }
    #[test]
    fn malformed_media_does_not_prevent_later_metadata_reads() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Mutex::new(Cache::open(&dir.path().join("cache.sqlite")));
        for (extension, bytes) in [
            ("mp3", &b"ID3"[..]),
            ("flac", &b"fLaC"[..]),
            ("m4a", &b"\0\0\0\x18ftyp"[..]),
            ("wav", &b""[..]),
        ] {
            let audio = dir.path().join(format!("truncated.{extension}"));
            fs::write(&audio, bytes).unwrap();
            assert!(read_cached(&cache, &audio).duration.is_none());
            lock(&cache).remember_duration(&audio, 7.0).unwrap();
            assert_eq!(read_cached(&cache, &audio).duration, Some(7.0));
        }
    }
    #[test]
    fn reads_real_wav_duration_and_embedded_riff_tags() {
        fn chunk(out: &mut Vec<u8>, id: &[u8; 4], bytes: &[u8]) {
            out.extend(id);
            out.extend((bytes.len() as u32).to_le_bytes());
            out.extend(bytes);
            if bytes.len() % 2 != 0 {
                out.push(0);
            }
        }
        let mut wav = b"RIFF\0\0\0\0WAVE".to_vec();
        let mut fmt = Vec::new();
        fmt.extend(1u16.to_le_bytes());
        fmt.extend(1u16.to_le_bytes());
        fmt.extend(8000u32.to_le_bytes());
        fmt.extend(16000u32.to_le_bytes());
        fmt.extend(2u16.to_le_bytes());
        fmt.extend(16u16.to_le_bytes());
        chunk(&mut wav, b"fmt ", &fmt);
        let mut info = b"INFO".to_vec();
        chunk(&mut info, b"INAM", b"Fixture title\0");
        chunk(&mut info, b"IART", b"Fixture artist\0");
        chunk(&mut info, b"IPRD", b"Fixture album\0");
        chunk(&mut wav, b"LIST", &info);
        chunk(&mut wav, b"data", &vec![0; 16000]);
        let size = (wav.len() - 8) as u32;
        wav[4..8].copy_from_slice(&size.to_le_bytes());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.wav");
        fs::write(&path, wav).unwrap();
        let cache = Mutex::new(Cache::open(&dir.path().join("cache.sqlite")));
        let mut tracks = vec![crate::library::PlayableTrack {
            track: crate::Track {
                path: path.to_string_lossy().into_owned(),
                name: "Fixture".into(),
                extra: Default::default(),
            },
            size: 0,
            modified: 0,
            duration: None,
            cache_signature: Cache::signature(&path),
        }];
        measure_durations(&cache, &mut tracks, || true, |_, _| {}).unwrap();
        assert!((tracks[0].duration.unwrap() - 1.0).abs() < 0.01);
        let before = lock(&cache).peek(&path).unwrap();
        assert!(!before.tags_read);
        assert!(before.title.is_empty());
        let tags = read_cached(&cache, &path);
        assert_eq!(tags.title, "Fixture title");
        assert_eq!(tags.artist, "Fixture artist");
        assert_eq!(tags.album, "Fixture album");
        assert!((tags.duration.unwrap() - 1.0).abs() < 0.01);
        assert_eq!(lock(&cache).peek(&path).unwrap().title, tags.title);
    }
    #[test]
    fn persisted_duration_invalidates_when_file_changes_and_clear_works() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("test.mp3");
        fs::write(&audio, b"invalid fixture").unwrap();
        let db = dir.path().join("cache.sqlite");
        {
            let cache = Cache::open(&db);
            cache.remember_duration(&audio, 42.0).unwrap();
        }
        let mut cache = Cache::open(&db);
        assert_eq!(cache.read(&audio).duration, Some(42.0));
        fs::write(&audio, b"changed size").unwrap();
        assert_eq!(cache.read(&audio).duration, None);
        cache.remember_duration(&audio, 15.0).unwrap();
        cache.clear().unwrap();
        assert_eq!(cache.read(&audio).duration, None);
        assert!(cache.remember_duration(&audio, f64::NAN).is_err());
    }
    #[test]
    fn prepared_station_reuses_persisted_durations_and_batches_recency_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.mp3");
        fs::write(&audio, b"fixture").unwrap();
        let mut config = crate::Config::default();
        config.custom.push(crate::Station {
            key: "station".into(),
            station: "Station".into(),
            tracks: vec![crate::Track {
                path: audio.to_string_lossy().into_owned(),
                name: "Song".into(),
                extra: Default::default(),
            }],
            extra: Default::default(),
        });
        let db = dir.path().join("cache.sqlite");
        let prepared = crate::library::playback_files(&config, "station").unwrap();
        {
            let cache = Cache::open(&db);
            cache
                .remember_duration(Path::new(&prepared[0].track.path), 42.0)
                .unwrap();
        }
        let cache = Cache::open(&db);
        // Simulate a row that has not been refreshed in the last minute.
        cache
            .db
            .as_ref()
            .unwrap()
            .execute("UPDATE metadata_v1 SET used=0", [])
            .unwrap();
        let changes = cache.db.as_ref().unwrap().total_changes();
        let started = std::time::Instant::now();
        // A large warm lineup reuses metadata, touching recency once per unique path.
        let mut warm = vec![prepared[0].clone(); 5000];
        cache.populate_durations(&mut warm);
        assert!(warm.iter().all(|track| track.duration == Some(42.0)));
        assert_eq!(cache.db.as_ref().unwrap().total_changes(), changes + 1);
        cache.populate_durations(&mut warm);
        assert_eq!(
            cache.db.as_ref().unwrap().total_changes(),
            changes + 1,
            "Recent warm hits must not rewrite recency"
        );
        assert_eq!(
            cache
                .peek(Path::new(&prepared[0].track.path))
                .unwrap()
                .duration,
            Some(42.0)
        );
        assert_eq!(cache.db.as_ref().unwrap().total_changes(), changes + 1);
        println!(
            "5000 cached duration lookups after reopen: {:?}",
            started.elapsed()
        );
        fs::write(&audio, b"changed fixture").unwrap();
        let mut changed = crate::library::playback_files(&config, "station").unwrap();
        cache.populate_durations(&mut changed);
        assert_eq!(changed[0].duration, None);
    }
    #[test]
    fn unavailable_cache_reports_failed_duration_writes() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("cache.sqlite");
        fs::create_dir(&db).unwrap();
        let audio = dir.path().join("song.mp3");
        fs::write(&audio, b"fixture").unwrap();
        let cache = Cache::open(&db);
        assert!(cache.remember_duration(&audio, 42.0).is_err());
    }
    #[test]
    fn sidecars_prefer_lrc_and_bound_reads() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("test.wav");
        fs::write(audio.with_extension("txt"), "[00:01]txt").unwrap();
        assert!(sidecar(&audio).unwrap().contains("txt"));
        fs::write(audio.with_extension("lrc"), "[00:01]lrc").unwrap();
        assert!(sidecar(&audio).unwrap().contains("lrc"));
        fs::write(audio.with_extension("lrc"), vec![0; 1024 * 1024 + 1]).unwrap();
        assert!(sidecar(&audio).is_none());
    }
    #[test]
    fn corrupt_cache_is_preserved_and_rebuilt_without_manual_clear() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        fs::write(&path, b"broken").unwrap();
        let cache = Cache::open(&path);
        assert!(cache
            .read(&dir.path().join("missing.mp3"))
            .duration
            .is_none());
        let backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read(backups[0].path().join("cache.sqlite")).unwrap(),
            b"broken"
        );
        let audio = dir.path().join("fixture.mp3");
        fs::write(&audio, b"fixture").unwrap();
        cache.remember_duration(&audio, 12.0).unwrap();
        assert_eq!(cache.peek(&audio).unwrap().duration, Some(12.0));
        drop(cache);
        assert_eq!(
            Cache::open(&path).peek(&audio).unwrap().duration,
            Some(12.0)
        );
    }
    #[test]
    fn corrupt_cache_preservation_keeps_journals_and_uses_unique_destinations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        let mut previous = None;
        for _ in 0..2 {
            for suffix in ["", "-wal", "-shm"] {
                fs::write(
                    dir.path().join(format!("cache.sqlite{suffix}")),
                    format!("broken{suffix}"),
                )
                .unwrap();
            }
            let backup = preserve_corrupt_cache(&path).unwrap();
            assert_ne!(Some(&backup), previous.as_ref());
            for suffix in ["", "-wal", "-shm"] {
                let filename = format!("cache.sqlite{suffix}");
                assert!(!dir.path().join(&filename).exists());
                assert_eq!(
                    fs::read(backup.join(filename)).unwrap(),
                    format!("broken{suffix}").as_bytes()
                );
            }
            previous = Some(backup);
        }
    }
    #[test]
    fn failed_cache_preservation_restores_already_moved_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        fs::write(&path, b"broken").unwrap();
        // A non-file journal must not be moved or deleted, even after the main
        // file was moved successfully.
        fs::create_dir(dir.path().join("cache.sqlite-wal")).unwrap();
        assert!(preserve_corrupt_cache(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
        assert!(dir.path().join("cache.sqlite-wal").is_dir());
    }
    #[test]
    fn busy_cache_is_not_mistaken_for_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        let audio = dir.path().join("song.mp3");
        fs::write(&audio, b"fixture").unwrap();
        let cache = Cache::open(&path);
        cache.remember_duration(&audio, 12.0).unwrap();
        cache
            .db
            .as_ref()
            .unwrap()
            .execute_batch("BEGIN IMMEDIATE")
            .unwrap();
        let blocked = Cache::open(&path);
        assert!(blocked.db.is_none());
        assert!(fs::read_dir(dir.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_type()
            .unwrap()
            .is_dir()));
        cache
            .db
            .as_ref()
            .unwrap()
            .execute_batch("ROLLBACK")
            .unwrap();
        drop(cache);
        assert_eq!(
            Cache::open(&path).peek(&audio).unwrap().duration,
            Some(12.0)
        );
    }
    #[test]
    fn creation_signatures_use_explicit_epoch_nanoseconds() {
        use std::time::{Duration, UNIX_EPOCH};
        assert_eq!(creation_stamp(None), "unknown");
        assert_eq!(creation_stamp(Some(UNIX_EPOCH)), "0");
        assert_eq!(
            creation_stamp(Some(UNIX_EPOCH + Duration::new(42, 100))),
            "42000000100"
        );
        assert_eq!(
            creation_stamp(Some(UNIX_EPOCH - Duration::new(42, 100))),
            "-42000000100"
        );
        #[cfg(not(unix))]
        {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("audio.mp3");
            fs::write(&path, b"fixture").unwrap();
            let meta = fs::metadata(path).unwrap();
            let modified = meta
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            assert_eq!(
                Cache::signature_from_metadata(&meta).unwrap(),
                format!("7:{modified}:{}", creation_stamp(meta.created().ok()))
            );
        }
    }
}

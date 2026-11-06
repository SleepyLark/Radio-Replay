use lofty::{
    file::{AudioFile, TaggedFileExt},
    tag::Accessor,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

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
}
impl Cache {
    pub fn open(path: &Path) -> Self {
        let db = (|| -> rusqlite::Result<Connection> {
            let db = Connection::open(path)?;
            db.execute_batch("PRAGMA cache_size=-2048; PRAGMA mmap_size=0;
                CREATE TABLE IF NOT EXISTS metadata_v1(path TEXT PRIMARY KEY, signature TEXT NOT NULL, value TEXT NOT NULL, used INTEGER NOT NULL);
                CREATE INDEX IF NOT EXISTS metadata_used ON metadata_v1(used);")?;
            Ok(db)
        })().ok();
        Self {
            db,
            path: path.to_path_buf(),
        }
    }
    pub fn clear(&mut self) -> Result<(), String> {
        if let Some(db) = &self.db {
            db.execute("DELETE FROM metadata_v1", [])
                .map_err(|e| e.to_string())?;
        } else {
            match fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
            self.db = Self::open(&self.path).db;
            if self.db.is_none() {
                return Err("Metadata cache could not be rebuilt".into());
            }
        }
        Ok(())
    }
    fn signature(path: &Path) -> Option<String> {
        let meta = fs::metadata(path).ok()?;
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
        let change = format!("{:?}", meta.created().ok());
        Some(format!("{}:{modified}:{change}", meta.len()))
    }
    fn put(&self, path: &Path, signature: &str, value: &Metadata) {
        if let Some(db) = &self.db {
            if let Ok(json) = serde_json::to_string(value) {
                let _ = db.execute(
                    "INSERT OR REPLACE INTO metadata_v1 VALUES(?1,?2,?3,unixepoch())",
                    params![path.to_string_lossy(), signature, json],
                );
                if db
                    .query_row("SELECT COUNT(*) FROM metadata_v1", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap_or(0)
                    > 20000
                {
                    let _ = db.execute("DELETE FROM metadata_v1 WHERE rowid IN (SELECT rowid FROM metadata_v1 ORDER BY used ASC,rowid ASC LIMIT 1000)", []);
                }
            }
        }
    }
    pub fn peek(&self, path: &Path) -> Option<Metadata> {
        let signature = Self::signature(path)?;
        if let Some(db) = &self.db {
            let cached: Option<String> = db
                .query_row(
                    "SELECT value FROM metadata_v1 WHERE path=?1 AND signature=?2",
                    params![path.to_string_lossy(), signature],
                    |row| row.get(0),
                )
                .optional()
                .ok()
                .flatten();
            if let Some(value) = cached.and_then(|s| serde_json::from_str(&s).ok()) {
                let _ = db.execute(
                    "UPDATE metadata_v1 SET used=unixepoch() WHERE path=?1",
                    params![path.to_string_lossy()],
                );
                return Some(value);
            }
        }
        None
    }
    pub fn read(&self, path: &Path) -> Metadata {
        let Some(signature) = Self::signature(path) else {
            return Metadata::default();
        };
        let mut value = self.peek(path).unwrap_or_default();
        if value.tags_read {
            return value;
        }
        value.tags_read = true;
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
        if Self::signature(path).as_deref() == Some(signature.as_str()) {
            self.put(path, &signature, &value);
        }
        value
    }
    pub fn remember_duration(&self, path: &Path, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds <= 0.0 || seconds > 48.0 * 3600.0 {
            return Err("Invalid duration".into());
        }
        let signature = Self::signature(path).ok_or("Audio file unavailable")?;
        let mut value = self.peek(path).unwrap_or_default();
        value.duration = Some(seconds);
        if Self::signature(path).as_deref() == Some(signature.as_str()) {
            self.put(path, &signature, &value);
        }
        Ok(())
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
        let cache = Cache::open(&dir.path().join("cache.sqlite"));
        let tags = cache.read(&path);
        assert_eq!(tags.title, "Fixture title");
        assert_eq!(tags.artist, "Fixture artist");
        assert_eq!(tags.album, "Fixture album");
        assert!((tags.duration.unwrap() - 1.0).abs() < 0.01);
        assert_eq!(cache.peek(&path).unwrap().title, tags.title);
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
    fn corrupt_cache_never_blocks_uncached_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        fs::write(&path, b"broken").unwrap();
        let mut cache = Cache::open(&path);
        assert!(cache
            .read(&dir.path().join("missing.mp3"))
            .duration
            .is_none());
        cache.clear().unwrap();
        let audio = dir.path().join("fixture.mp3");
        fs::write(&audio, b"fixture").unwrap();
        cache.remember_duration(&audio, 12.0).unwrap();
        assert_eq!(cache.peek(&audio).unwrap().duration, Some(12.0));
    }
}

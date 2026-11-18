//! Small, versioned audio preferences. Volume changes never rewrite the library.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Levels {
    pub volume: f64,
    pub muted: bool,
}
impl Default for Levels {
    fn default() -> Self {
        Self {
            volume: 0.8,
            muted: false,
        }
    }
}
impl Levels {
    pub fn validate(self) -> Result<(), String> {
        if self.volume.is_finite() && (0.0..=1.0).contains(&self.volume) {
            Ok(())
        } else {
            Err("Invalid saved audio volume".into())
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    #[serde(flatten)]
    levels: Levels,
}
fn read(path: &Path) -> Result<Levels, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Levels::default()),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(4097)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4096 {
        return Err("Audio preferences exceed the size limit".into());
    }
    let saved: Saved = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if saved.version != 1 {
        return Err("Unsupported audio preferences version".into());
    }
    saved.levels.validate()?;
    Ok(saved.levels)
}
enum Request {
    Changed,
    Flush(Sender<Result<(), String>>),
}
pub struct AudioPreferences {
    levels: Arc<Mutex<Levels>>,
    queued: Arc<AtomicBool>,
    sender: Sender<Request>,
    warning: Option<String>,
}
impl AudioPreferences {
    pub fn open(path: PathBuf) -> Self {
        let (initial, warning) = match read(&path) {
            Ok(levels) => (levels, None),
            Err(error) => (Levels::default(), Some(error)),
        };
        let levels = Arc::new(Mutex::new(initial));
        let current = levels.clone();
        let queued = Arc::new(AtomicBool::new(false));
        let pending = queued.clone();
        let blocked = warning.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut written = initial;
            let mut deadline: Option<Instant> = None;
            let mut save = || {
                pending.store(false, Ordering::SeqCst);
                let value = *current.lock().unwrap_or_else(|p| p.into_inner());
                // Never replace an unreadable file or a future format.
                if let Some(error) = &blocked {
                    return Err(error.clone());
                }
                if value == written {
                    return Ok(());
                }
                let bytes = serde_json::to_vec(&Saved {
                    version: 1,
                    levels: value,
                })
                .map_err(|e| e.to_string())?;
                crate::persistence::atomic_bytes(&path, &bytes)?;
                written = value;
                Ok(())
            };
            loop {
                let request = match deadline {
                    Some(deadline) => {
                        receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    }
                    None => receiver
                        .recv()
                        .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
                };
                match request {
                    Ok(Request::Changed) => {
                        deadline.get_or_insert_with(|| Instant::now() + Duration::from_millis(250));
                    }
                    Ok(Request::Flush(reply)) => {
                        let _ = reply.send(save());
                        deadline = None;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Err(error) = save() {
                            eprintln!("Audio preferences could not be saved: {error}");
                        }
                        deadline = None;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = save();
                        break;
                    }
                }
            }
        });
        Self {
            levels,
            queued,
            sender,
            warning,
        }
    }
    pub fn get(&self) -> Levels {
        *self.levels.lock().unwrap_or_else(|p| p.into_inner())
    }
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }
    pub fn set(&self, value: Levels) -> Result<(), String> {
        value.validate()?;
        let mut current = self.levels.lock().unwrap_or_else(|p| p.into_inner());
        if *current == value {
            return Ok(());
        }
        *current = value;
        drop(current);
        if !self.queued.swap(true, Ordering::SeqCst) {
            self.sender
                .send(Request::Changed)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn flush(&self) -> Result<(), String> {
        let (reply, receiver) = mpsc::channel();
        self.sender
            .send(Request::Flush(reply))
            .map_err(|e| e.to_string())?;
        receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn levels_survive_reopen_and_flush_keeps_the_last_value_of_a_burst() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio-preferences.json");
        let preferences = AudioPreferences::open(path.clone());
        assert_eq!(preferences.get(), Levels::default());
        preferences.flush().unwrap();
        assert!(!path.exists());
        for i in 0..100 {
            preferences
                .set(Levels {
                    volume: i as f64 / 100.0,
                    muted: i % 2 == 0,
                })
                .unwrap();
        }
        let last = Levels {
            volume: 0.37,
            muted: true,
        };
        preferences.set(last).unwrap();
        preferences.flush().unwrap();
        let reopened = AudioPreferences::open(path);
        assert_eq!(reopened.get(), last);
    }
    #[test]
    fn bad_or_future_preferences_are_preserved_and_invalid_controls_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio-preferences.json");
        for bytes in [
            b"broken".as_slice(),
            br#"{"version":99,"volume":0.1,"muted":true}"#,
            br#"{"version":1,"volume":2,"muted":false}"#,
        ] {
            fs::write(&path, bytes).unwrap();
            let preferences = AudioPreferences::open(path.clone());
            assert!(preferences.warning().is_some());
            preferences
                .set(Levels {
                    volume: 0.5,
                    muted: false,
                })
                .unwrap();
            assert!(preferences.flush().is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            for volume in [-1.0, 1.1, f64::NAN, f64::INFINITY] {
                assert!(preferences
                    .set(Levels {
                        volume,
                        muted: false
                    })
                    .is_err());
            }
            assert_eq!(preferences.get().volume, 0.5);
        }
    }
    #[test]
    fn failed_writes_keep_live_levels_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio-preferences.json");
        let preferences = AudioPreferences::open(path.clone());
        fs::create_dir(&path).unwrap();
        let levels = Levels {
            volume: 0.15,
            muted: true,
        };
        preferences.set(levels).unwrap();
        assert!(preferences.flush().is_err());
        assert_eq!(preferences.get(), levels);
        fs::remove_dir(&path).unwrap();
        preferences.flush().unwrap();
        assert_eq!(read(&path).unwrap(), levels);
    }
}

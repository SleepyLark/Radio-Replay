use radio_replay_audio::player::{Control, Plan, Player, Snapshot};
use radio_replay_core::{
    library::{self, PreparedStation, TrackProblem},
    metadata,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use tauri::{Emitter, Manager, WebviewWindow};

pub struct NativeAudio(Player);
fn main_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Native audio commands are restricted to the main player".into())
    }
}
pub fn setup(app: &tauri::AppHandle) -> Result<(), String> {
    let path = if let Some(path) = std::env::var_os("RADIO_REPLAY_MPV_LIBRARY") {
        PathBuf::from(path)
    } else {
        #[cfg(windows)]
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vendor/mpv/libmpv-2.dll");
        #[cfg(target_os = "macos")]
        let path = PathBuf::from("libmpv.2.dylib");
        #[cfg(not(any(windows, target_os = "macos")))]
        let path = PathBuf::from("libmpv.so.2");
        path
    };
    let handle = app.clone();
    app.manage(NativeAudio(Player::start(path, move |state| {
        let _ = handle.emit_to("main", "native-audio-state", state);
    })));
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title("Radio Replay — libmpv Preview");
    }
    Ok(())
}
#[tauri::command]
pub async fn native_begin(
    window: WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Snapshot, String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || app.state::<NativeAudio>().0.begin())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn native_snapshot(window: WebviewWindow, app: tauri::AppHandle) -> Result<Snapshot, String> {
    main_only(&window)?;
    app.state::<NativeAudio>().0.snapshot()
}
#[tauri::command]
pub async fn native_control(
    window: WebviewWindow,
    app: tauri::AppHandle,
    control: Control,
    generation: Option<u64>,
) -> Result<Snapshot, String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let native = app.state::<NativeAudio>();
        if matches!(control, Control::Stop) {
            if let Some(id) = generation {
                return native.0.stop_if_current(id);
            }
        }
        native.0.control(control)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn native_prepare(
    window: WebviewWindow,
    app: tauri::AppHandle,
    generation: u64,
    key: String,
    group: Option<String>,
) -> Result<PreparedStation, String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let native = app.state::<NativeAudio>();
        let library = app.state::<super::Library>();
        let cache = app.state::<super::MetadataCache>();
        let current = || {
            if native.0.current(generation) {
                Ok(())
            } else {
                Err("Tuning changed".to_string())
            }
        };
        current()?;
        let config = library.0.lock().map_err(|e| e.to_string())?.load()?;
        let mut report = library::playback_report_for_group(config, &key, group.as_deref())?;
        cache
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .populate_durations(&mut report.tracks);
        let mut playable = Vec::new();
        for mut track in report.tracks {
            current()?;
            if track.duration.is_none() {
                track.duration = cache
                    .0
                    .lock()
                    .map_err(|e| e.to_string())?
                    .read(Path::new(&track.track.path))
                    .duration;
            }
            if track.duration.is_none() {
                match native.0.probe(generation, PathBuf::from(&track.track.path)) {
                    Ok(seconds) => {
                        track.duration = Some(seconds);
                        if let Err(reason) = cache
                            .0
                            .lock()
                            .map_err(|e| e.to_string())?
                            .remember_duration(Path::new(&track.track.path), seconds)
                        {
                            report.problems.push(TrackProblem {
                                path: track.track.path.clone(),
                                name: track.track.name.clone(),
                                reason,
                                kind: "cache",
                            });
                        }
                    }
                    Err(reason) => {
                        current()?;
                        report.problems.push(TrackProblem {
                            path: track.track.path.clone(),
                            name: track.track.name.clone(),
                            reason,
                            kind: "file",
                        });
                        continue;
                    }
                }
            }
            if track
                .duration
                .is_some_and(|n| n.is_finite() && n > 0.0 && n <= 172800.0)
            {
                playable.push(track);
            } else {
                report.problems.push(TrackProblem {
                    path: track.track.path.clone(),
                    name: track.track.name.clone(),
                    reason: "No finite audio duration could be measured".into(),
                    kind: "file",
                });
            }
        }
        current()?;
        // Native audio needs no browser asset grants. Retain prepared-file identity
        // for metadata IPC and validate it again before handing paths to libmpv.
        let mut grants = library.3.lock().map_err(|e| e.to_string())?;
        if grants.len() + playable.len() > 100000 {
            grants.clear();
        }
        for track in &playable {
            grants.insert(format!("{key}\0{}", track.track.path), track.clone());
        }
        Ok(PreparedStation {
            tracks: playable,
            problems: report.problems,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn native_load(
    window: WebviewWindow,
    app: tauri::AppHandle,
    generation: u64,
    key: String,
    paths: Vec<String>,
    index: usize,
    seconds: f64,
) -> Result<Snapshot, String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let native = app.state::<NativeAudio>();
        if !native.0.current(generation) {
            return Err("Tuning changed".into());
        }
        if paths.is_empty()
            || paths.len() > 100000
            || index >= paths.len()
            || !seconds.is_finite()
            || !(0.0..=172800.0).contains(&seconds)
        {
            return Err("Invalid native playback plan".into());
        }
        let library = app.state::<super::Library>();
        let mut seen = HashSet::new();
        let mut approved = Vec::with_capacity(paths.len());
        for path in paths {
            if !seen.insert(path.clone()) {
                return Err("Duplicate native playback path".into());
            }
            let track = super::saved_audio(&library, &key, &path)?;
            let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
            if metadata::Cache::signature_from_metadata(&metadata) != track.cache_signature {
                return Err("Audio changed after station preparation; tune again".into());
            }
            approved.push(PathBuf::from(path));
        }
        native.0.load(
            generation,
            Plan {
                paths: approved,
                index,
                seconds,
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

use radio_replay_audio::player::{Control, Plan, Player, Snapshot, Tuning};
use radio_replay_audio::programme::{Bumpers, Interval};
use radio_replay_core::{
    library::{self, IndexedLineup, IndexedStation, TrackProblem},
    metadata,
};
use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager, WebviewWindow};

pub struct NativeAudio(
    Player,
    Result<PathBuf, String>,
    Mutex<Option<Arc<IndexedLineup>>>,
);
const _: () = assert!(radio_replay_audio::MAX_MUSIC_TRACKS == library::MAX_STATION_TRACKS);
const _: () = assert!(radio_replay_audio::MAX_BUMPER_TRACKS == library::MAX_BUMPER_TRACKS);
const _: () = assert!(radio_replay_audio::MAX_NETWORK_STREAMS == library::MAX_NETWORK_STREAMS);
impl NativeAudio {
    pub fn startup_error(&self) -> Option<String> {
        self.0.control(Control::Status).err().map(|error| {
            eprintln!("Audio runtime initialization failed: {error}");
            "Audio runtime unavailable. Keep the audio folder next to the program, or reinstall Radio Replay if it is already present. Restart the app after restoring the runtime.".into()
        })
    }
}
fn main_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Native audio commands are restricted to the main player".into())
    }
}
pub fn setup(app: &tauri::AppHandle) -> Result<(), String> {
    let path = library_path(app)?;
    let handle = app.clone();
    let bundled = bundled_tuning_source(app);
    let levels = app.state::<super::AudioPreferences>().get();
    app.manage(NativeAudio(
        Player::start_with_preferences(path, levels.volume, levels.muted, move |state| {
            let _ = handle.emit_to("main", "native-audio-state", state);
        }),
        bundled,
        Mutex::default(),
    ));
    Ok(())
}
pub(crate) fn library_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let path = if let Some(path) = std::env::var_os("RADIO_REPLAY_MPV_LIBRARY") {
        PathBuf::from(path)
    } else {
        #[cfg(windows)]
        let path = {
            let installed = app
                .path()
                .resource_dir()
                .map_err(|e| e.to_string())?
                .join("audio/libmpv-2.dll");
            if installed.is_file() {
                installed
            } else {
                #[cfg(debug_assertions)]
                {
                    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../vendor/mpv/libmpv-2.dll");
                    if development.is_file() {
                        return Ok(development);
                    }
                }
                installed
            }
        };
        #[cfg(target_os = "macos")]
        let path = PathBuf::from("libmpv.2.dylib");
        #[cfg(not(any(windows, target_os = "macos")))]
        let path = PathBuf::from("libmpv.so.2");
        path
    };
    #[cfg(not(windows))]
    let _ = app;
    Ok(path)
}
#[tauri::command]
pub async fn native_begin(
    window: WebviewWindow,
    app: tauri::AppHandle,
    delay_ms: Option<u64>,
) -> Result<Snapshot, String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let delay = delay_ms.unwrap_or(0);
        if delay > 1500 {
            return Err("Invalid tuning delay".into());
        }
        let config = app
            .state::<super::Library>()
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .load_shared()?;
        let mut tuning = Tuning {
            minimum: Duration::from_millis(delay),
            ..Default::default()
        };
        if config
            .settings
            .get("tuningTransition")
            .and_then(|v| v.as_bool())
            != Some(false)
        {
            let source = config
                .settings
                .get("staticPath")
                .and_then(|v| v.as_str())
                .unwrap_or("bundled");
            if !source.is_empty() && source != "off" {
                match tuning_source(&app, source) {
                    Ok(path) => tuning.sound = Some(path),
                    Err(error) => {
                        tuning.problem = Some((
                            source.to_owned(),
                            format!("Tuning sound unavailable: {error}"),
                        ))
                    }
                }
            }
        } else {
            tuning.minimum = Duration::ZERO;
        }
        let native = app.state::<NativeAudio>();
        let state = native.0.begin_tuning(tuning)?;
        let mut prepared = native.2.lock().map_err(|e| e.to_string())?;
        if native.0.current(state.generation) {
            *prepared = None;
        }
        Ok(state)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn tuning_source(app: &tauri::AppHandle, source: &str) -> Result<PathBuf, String> {
    if source == "bundled" {
        return app.state::<NativeAudio>().1.clone();
    }
    let path = Path::new(source);
    if !library::audio_file(path) {
        return Err("Invalid tuning audio extension".into());
    }
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
        return Err("Tuning audio exceeds 64 MiB".into());
    }
    Ok(path)
}
fn bundled_tuning_source(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let path = app
        .state::<super::DataDirectory>()
        .0
        .join("bundled-static.mp3");
    let bytes = include_bytes!("../../assets/audio/static.mp3");
    if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(path)
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
            let state = if let Some(id) = generation {
                native.0.stop_if_current(id)?
            } else {
                native.0.control(control)?
            };
            let mut prepared = native.2.lock().map_err(|e| e.to_string())?;
            if prepared
                .as_ref()
                .is_some_and(|prepared| prepared.generation != state.generation)
            {
                *prepared = None;
            }
            return Ok(state);
        }
        let save_levels = matches!(control, Control::Volume(_) | Control::Mute);
        let state = native.0.control(control)?;
        if save_levels {
            app.state::<super::AudioPreferences>().set(super::Levels {
                volume: state.volume,
                muted: state.muted,
            })?;
        }
        Ok(state)
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
) -> Result<IndexedStation, String> {
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
        let started = std::time::Instant::now();
        let config = library.0.lock().map_err(|e| e.to_string())?.load_shared()?;
        let config_elapsed = started.elapsed();
        let network = config
            .custom
            .iter()
            .find(|s| s.key == key)
            .and_then(|s| s.extra.get("mode"))
            .and_then(|v| v.as_str())
            == Some("network");
        let mut bumper_report = library::bumper_report(&config, &key);
        let mut report = library::playback_report_for_group_shared(&config, &key, group.as_deref())?;
        report.problems.append(&mut bumper_report.problems);
        let files_elapsed = started.elapsed();
        let music_count = report.tracks.len();
        report.tracks.append(&mut bumper_report.tracks);
        let last_progress = std::cell::Cell::new(("", std::time::Instant::now()));
        let progress = |phase: &'static str, done: usize, total: usize| {
            if native.0.current(generation) {
                let (previous, at) = last_progress.get();
                if previous == phase && done < total && at.elapsed() < Duration::from_millis(100) {
                    return;
                }
                last_progress.set((phase, std::time::Instant::now()));
                let _ = app.emit_to("main", "tuning-progress", serde_json::json!({
                    "generation": generation, "phase": phase, "done": done, "total": total
                }));
            }
        };
        let measured = if network {
            metadata::DurationPreparation::default()
        } else {
            metadata::measure_durations(
                &cache.0, &mut report.tracks,
                || native.0.current(generation),
                |done, total| progress("measuring", done, total),
            )?
        };
        let measured_elapsed = started.elapsed();
        let mut cache_error = measured.cache_error;
        let probe_total = report.tracks.iter().filter(|track| track.duration.is_none()).count();
        let mut probed = 0;
        let mut fallback = Vec::new();
        if !network && probe_total > 0 {
            progress("probing", 0, probe_total);
        }
        let mut playable = Vec::new();
        let mut prepared_bumpers = Vec::new();
        for (index, mut track) in report.tracks.into_iter().enumerate()
        {
            current()?;
            if network {
                playable.push(track);
                continue;
            }
            if track.duration.is_none() {
                let result = native.0.probe(generation, PathBuf::from(&track.track.path));
                probed += 1;
                progress("probing", probed, probe_total);
                match result {
                    Ok(seconds) => {
                        track.duration = Some(seconds);
                        fallback.push(track.clone());
                        if fallback.len() >= 256 {
                            current()?;
                            if cache_error.is_none() {
                                if let Err(reason) = metadata::remember_prepared_durations(&cache.0, measured.cache_epoch, &fallback) {
                                    cache_error = Some(reason);
                                }
                            }
                            fallback.clear();
                        }
                    }
                    Err(reason) => {
                        current()?;
                        if let Some(message) = reason.recovery_message() {
                            return Err(message.replace("press Play", "tune in again"));
                        }
                        report.problems.push(TrackProblem {
                            path: track.track.path.clone(),
                            name: track.track.name.clone(),
                            reason: reason.message,
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
                if index >= music_count {
                    prepared_bumpers.push(track);
                } else {
                    playable.push(track);
                }
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
        if cache_error.is_none() {
            if let Err(reason) = metadata::remember_prepared_durations(&cache.0, measured.cache_epoch, &fallback) {
                cache_error = Some(reason);
            }
        }
        if let Some(reason) = cache_error {
            report.problems.push(TrackProblem {
                path: String::new(), name: "Metadata cache".into(), reason, kind: "cache"
            });
        }
        if cfg!(debug_assertions) && std::env::var_os("RADIO_REPLAY_TUNING_TIMINGS").is_some() {
            eprintln!("Tuning timings: config={config_elapsed:?} filesystem={:?} metadata={:?} decoder={:?} total={:?} cached={} parsed={} probed={probed}",files_elapsed-config_elapsed,measured_elapsed-files_elapsed,started.elapsed()-measured_elapsed,started.elapsed(),measured.cache_hits,measured.parsed);
        }
        let lineup=Arc::new(IndexedLineup::new(generation,key.clone(),config,group.unwrap_or_else(||"all".into()),playable,prepared_bumpers));
        let response=lineup.describe(report.problems);
        // File identities are shared by metadata authorization and this native
        // generation. Loading indexes never clones or locks a grant per track.
        let mut prepared=native.2.lock().map_err(|e|e.to_string())?;
        current()?;
        let store = library.0.lock().map_err(|e| e.to_string())?;
        lineup.validate_config(&store.load_shared()?)?;
        let mut grants = library.3.lock().map_err(|e| e.to_string())?;
        grants.remember(&key, lineup.tracks.iter().cloned())?;
        *prepared=Some(lineup);
        Ok(response)
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
    slots: Vec<usize>,
    index: usize,
    seconds: f64,
    fade: Option<f64>,
    bumper_count: Option<usize>,
    programme_context: Option<String>,
    seed: Option<u32>,
) -> Result<Snapshot, String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let native = app.state::<NativeAudio>();
        if !native.0.current(generation) {
            return Err("Tuning changed".into());
        }
        let bumper_count = bumper_count.unwrap_or(0);
        let request = library::PlaybackRequest {
            count: slots.len(),
            index,
            seconds,
            fade: fade.unwrap_or(0.0),
            bumper_count,
            programme_context: programme_context.as_deref(),
        };
        request.validate_bounds()?;
        let library = app.state::<super::Library>();
        let lineup = native
            .2
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or("Station has not been prepared for this tune")?;
        let selected = lineup.select(generation, &key, &slots, bumper_count)?;
        let config = library.0.lock().map_err(|e| e.to_string())?.load_shared()?;
        lineup.validate_config(&config)?;
        let station = config
            .custom
            .iter()
            .find(|s| s.key == key)
            .ok_or("Station unavailable")?;
        let network = station.extra.get("mode").and_then(|v| v.as_str()) == Some("network");
        let music_count = request.validate_station(network, &lineup.group)?;
        let settings = if bumper_count > 0 {
            radio_replay_core::bumpers::Settings::from_station(station)?
        } else {
            None
        };
        if bumper_count > 0 && settings.is_none() {
            return Err("Bumpers are not enabled on this music station".into());
        }
        let approved = if network {
            selected
                .iter()
                .map(|track| {
                    radio_replay_network::validate(&track.track.path)?;
                    Ok(PathBuf::from(&track.track.path))
                })
                .collect::<Result<Vec<_>, String>>()?
        } else {
            IndexedLineup::verify_files(&selected, || native.0.current(generation))?
        };
        let mut lengths = Vec::with_capacity(slots.len());
        let mut revision = std::collections::hash_map::DefaultHasher::new();
        for track in selected {
            if !native.0.current(generation) {
                return Err("Tuning changed".into());
            }
            if network {
                continue;
            }
            lengths.push(track.duration.unwrap_or(0.0));
            track.cache_signature.hash(&mut revision);
        }
        let bumpers = if bumper_count > 0 {
            let settings = settings.unwrap();
            let context = programme_context.unwrap_or_else(|| lineup.group.clone());
            Some(Bumpers {
                music_count,
                lengths,
                interval: if settings.method == "songs" {
                    Interval::Songs {
                        min: settings.min as u32,
                        max: settings.max as u32,
                    }
                } else {
                    Interval::Time {
                        min: settings.min * 60.0,
                        max: settings.max * 60.0,
                    }
                },
                seed: seed.unwrap_or(1),
                context: format!("{key}\0{context}"),
                revision: revision.finish(),
            })
        } else {
            None
        };
        native.0.load(
            generation,
            Plan {
                paths: approved,
                index,
                seconds,
                fade: fade.unwrap_or(0.0),
                bumpers,
                network,
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

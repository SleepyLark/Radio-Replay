#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod desktop;
mod drops;
#[cfg(feature = "native-audio")]
mod native_audio;
#[cfg(not(feature = "native-smoke"))]
mod single_instance;
#[cfg(feature = "native-smoke")]
mod smoke;
struct DataDirectory(std::path::PathBuf);
use radio_replay_core::audio_preferences::{AudioPreferences, Levels};
use radio_replay_core::changes::LibraryUpdate;
use radio_replay_core::grants::PreparedAudioGrants;
use radio_replay_core::{
    library::{self, StationEdit},
    Track,
};
use radio_replay_core::{metadata, preferences};
use radio_replay_core::{Config, ConfigStore};
use std::sync::Mutex;
use std::{collections::HashSet, path::Path};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
struct MetadataCache(Mutex<metadata::Cache>);

async fn library_write<T: Send + 'static>(
    app: tauri::AppHandle,
    operation: impl FnOnce(&Library) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || operation(&app.state::<Library>()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn playback_backend(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<PlaybackStatus, String> {
    if window.label() != "main" {
        return Err("Main player required".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let preferences = app.state::<AudioPreferences>();
        #[cfg(feature = "native-audio")]
        let error = app.state::<native_audio::NativeAudio>().startup_error();
        #[cfg(not(feature = "native-audio"))]
        let error = None;
        let warning = preferences.warning().map(|_| "Saved volume and mute could not be read. Defaults are in use; check audio-preferences.json in the data folder.".to_string());
        PlaybackStatus {backend: if cfg!(feature="native-audio") {"mpv"} else {"web"}, error, warning, levels: preferences.get()}
    }).await.map_err(|e| e.to_string())
}
#[derive(serde::Serialize)]
struct PlaybackStatus {
    backend: &'static str,
    error: Option<String>,
    warning: Option<String>,
    levels: Levels,
}

fn saved_audio(
    library: &Library,
    key: &str,
    path: &str,
) -> Result<std::sync::Arc<library::PlayableTrack>, String> {
    library.3.lock().map_err(|e| e.to_string())?.get(key, path)
}

#[tauri::command]
async fn track_details(
    window: tauri::WebviewWindow,
    library: State<'_, Library>,
    cache: State<'_, MetadataCache>,
    key: String,
    path: String,
) -> Result<metadata::Details, String> {
    main_only(&window)?;
    let track = saved_audio(&library, &key, &path)?;
    if track.track.extra.get("source").and_then(|v| v.as_str()) == Some("network") {
        return Ok(metadata::Details {
            tags: metadata::Metadata {
                title: track.track.name.clone(),
                ..Default::default()
            },
            sidecar: None,
        });
    }
    let path = Path::new(&track.track.path);
    let tags = metadata::read_cached(&cache.0, path);
    Ok(metadata::Details {
        tags,
        sidecar: metadata::sidecar(path),
    })
}

#[tauri::command]
async fn remember_duration(
    window: tauri::WebviewWindow,
    library: State<'_, Library>,
    cache: State<'_, MetadataCache>,
    key: String,
    path: String,
    size: Option<u64>,
    modified: Option<u64>,
    duration: f64,
) -> Result<(), String> {
    main_only(&window)?;
    let track = saved_audio(&library, &key, &path)?;
    let current = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let stamp = current
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    if size.is_some_and(|size| track.size != size)
        || modified.is_some_and(|modified| track.modified != modified)
        || current.len() != track.size
        || stamp != track.modified
        || metadata::Cache::signature_from_metadata(&current) != track.cache_signature
    {
        return Err("Audio changed during probing".into());
    }
    metadata::lock(&cache.0).remember_duration(Path::new(&path), duration)
}

#[tauri::command]
async fn clear_metadata_cache(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        metadata::lock(&app.state::<MetadataCache>().0).clear()
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_group(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    group: preferences::Group,
) -> Result<LibraryUpdate, String> {
    main_only(&window)?;
    library_write(app, move |library| {
        let store = library.0.lock().map_err(|e| e.to_string())?;
        let baseline = store.load_shared()?;
        let mut config = (*baseline).clone();
        preferences::save_group(&mut config, group)?;
        let config = store.save_owned(config)?;
        Ok(LibraryUpdate::groups(&baseline, &config))
    })
    .await
}
#[tauri::command]
async fn delete_group(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: String,
) -> Result<LibraryUpdate, String> {
    main_only(&window)?;
    library_write(app, move |library| {
        let store = library.0.lock().map_err(|e| e.to_string())?;
        let baseline = store.load_shared()?;
        let mut config = (*baseline).clone();
        config.groups.retain(|g| g["id"].as_str() != Some(&id));
        let config = store.save_owned(config)?;
        Ok(LibraryUpdate::groups(&baseline, &config))
    })
    .await
}
#[tauri::command]
async fn recover_legacy_library(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<RecoveredLibrary, String> {
    main_only(&window)?;
    library_write(app, move |library| {
        let baseline = library.0.lock().map_err(|e| e.to_string())?.load()?;
        let existing: HashSet<String> = baseline.custom.iter().map(|s| s.key.clone()).collect();
        let mut recovered = baseline.clone();
        let (_, notices) = library::recover_legacy(&mut recovered)?;
        let store = library.0.lock().map_err(|e| e.to_string())?;
        let mut config = store.load()?;
        if config.root != baseline.root {
            return Err(
                "Archive root changed during recovery; retry with the current library".into(),
            );
        }
        for station in recovered.custom {
            if !existing.contains(&station.key)
                && !config.custom.iter().any(|s| s.key == station.key)
            {
                config.custom.push(station);
            }
        }
        store.save(&config)?;
        Ok(RecoveredLibrary { config, notices })
    })
    .await
}
#[derive(serde::Serialize)]
struct RecoveredLibrary {
    config: Config,
    notices: library::ScanNotices,
}
#[tauri::command]
async fn save_settings(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    patch: serde_json::Map<String, serde_json::Value>,
) -> Result<LibraryUpdate, String> {
    main_only(&window)?;
    let handle = app.clone();
    library_write(app, move |library| {
        let store = library.0.lock().map_err(|e| e.to_string())?;
        let baseline = store.load_shared()?;
        if let Some(path) = patch
            .get("staticPath")
            .and_then(|v| v.as_str())
            .filter(|p| !p.is_empty() && *p != "bundled" && *p != "off")
        {
            let retained =
                baseline.settings.get("staticPath").and_then(|v| v.as_str()) == Some(path);
            if !library::audio_file(Path::new(path))
                || (!retained && !library.1.lock().map_err(|e| e.to_string())?.contains(path))
            {
                return Err("Tuning audio was not selected".into());
            }
        }
        let mut settings = Config {
            settings: baseline.settings.clone(),
            ..Default::default()
        };
        preferences::save_settings(&mut settings, patch)?;
        if settings.settings == baseline.settings {
            let confirmed = store.confirm_unchanged(&baseline)?;
            handle
                .state::<desktop::Desktop>()
                .apply_settings(&confirmed);
            return Ok(LibraryUpdate::settings(&confirmed));
        }
        let mut config = (*baseline).clone();
        config.settings = settings.settings;
        let config = store.save_owned(config)?;
        handle.state::<desktop::Desktop>().apply_settings(&config);
        Ok(LibraryUpdate::settings(&config))
    })
    .await
}

struct Library(
    Mutex<ConfigStore>,
    Mutex<HashSet<String>>,
    Mutex<HashSet<String>>,
    Mutex<PreparedAudioGrants>,
);

#[tauri::command]
#[cfg(not(feature = "native-audio"))]
async fn resolve_tuning_sound(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library: State<'_, Library>,
) -> Result<Option<String>, String> {
    main_only(&window)?;
    let config = library.0.lock().map_err(|e| e.to_string())?.load_shared()?;
    let Some(path) = config
        .settings
        .get("staticPath")
        .and_then(|v| v.as_str())
        .filter(|p| !p.is_empty() && *p != "bundled" && *p != "off")
    else {
        return Ok(None);
    };
    if !library::audio_file(Path::new(path)) {
        return Err("Invalid tuning audio".into());
    }
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > 64 * 1024 * 1024 {
        return Err("Tuning audio exceeds 64 MiB".into());
    }
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

fn allow_art(app: &tauri::AppHandle, config: &Config) {
    let mut seen = HashSet::new();
    for path in config.logos.values().filter_map(|v| v.as_str()).chain(
        config
            .custom
            .iter()
            .filter_map(|s| s.extra.get("bannerPath").and_then(|v| v.as_str())),
    ) {
        if !seen.insert(path) {
            continue;
        }
        if radio_replay_core::artwork::validate(Path::new(path)).is_ok() {
            let _ = app.asset_protocol_scope().allow_file(path);
        }
    }
}

#[tauri::command]
fn load_config(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library: State<'_, Library>,
) -> Result<Config, String> {
    main_only(&window)?;
    let store = library.0.lock().map_err(|e| e.to_string())?;
    let config = store.reload()?;
    app.state::<desktop::Desktop>().apply_settings(&config);
    drop(store);
    allow_art(&app, &config);
    Ok(config)
}

fn main_only(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Main player required".into())
    }
}

#[tauri::command]
async fn library_recovery_info(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<radio_replay_core::RecoveryInfo, String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        Ok(app
            .state::<Library>()
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .recovery_info())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn recover_library(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    action: radio_replay_core::RecoveryAction,
) -> Result<Config, String> {
    main_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let library = app.state::<Library>();
        let store = library.0.lock().map_err(|e| e.to_string())?;
        let config = store.recover(action)?;
        app.state::<desktop::Desktop>().apply_settings(&config);
        drop(store);
        library.3.lock().map_err(|e| e.to_string())?.clear();
        allow_art(&app, &config);
        Ok(config)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn export_config(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    main_only(&window)?;
    let Some(file) = app
        .dialog()
        .file()
        .add_filter("Radio Replay library", &["json"])
        .set_file_name("stations-export.json")
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Library>()
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .export(&path)?;
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_config(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Option<Config>, String> {
    main_only(&window)?;
    // Picker runs in Rust: the renderer cannot supply an arbitrary filesystem path.
    let selected = app
        .dialog()
        .file()
        .add_filter("Radio Replay configuration", &["json"])
        .blocking_pick_file();
    let Some(file) = selected else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let handle = app.clone();
    library_write(app, move |library| {
        let store = library.0.lock().map_err(|e| e.to_string())?;
        let config = store.import(&path)?;
        handle.state::<desktop::Desktop>().apply_settings(&config);
        drop(store);
        library.3.lock().map_err(|e| e.to_string())?.clear();
        allow_art(&handle, &config);
        Ok(Some(config))
    })
    .await
}

#[tauri::command]
async fn choose_audio(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    folder: bool,
) -> Result<Option<library::ScanReport>, String> {
    main_only(&window)?;
    let picker = app.dialog().file();
    let paths = if folder {
        match picker.blocking_pick_folder() {
            Some(file) => vec![file.into_path().map_err(|e| e.to_string())?],
            None => return Ok(None),
        }
    } else {
        match picker
            .add_filter("Audio", &["mp3", "m4a", "aac", "ogg", "wav", "flac"])
            .blocking_pick_files()
        {
            Some(files) => files
                .into_iter()
                .map(|file| file.into_path().map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?,
            None => return Ok(None),
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        let report = if folder {
            library::scan_folder(&paths[0])?
        } else {
            library::collect_audio(&paths)?
        };
        app.state::<Library>()
            .1
            .lock()
            .map_err(|e| e.to_string())?
            .extend(report.tracks.iter().map(|track| track.path.clone()));
        Ok(Some(report))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn add_network_stream(
    window: tauri::WebviewWindow,
    library: State<'_, Library>,
    url: String,
    name: String,
) -> Result<Track, String> {
    if window.label() != "main" {
        return Err("Main player required".into());
    }
    let track = radio_replay_core::m3u::network_track(&url, &name)?;
    library
        .1
        .lock()
        .map_err(|e| e.to_string())?
        .insert(track.path.clone());
    Ok(track)
}

#[tauri::command]
async fn choose_playlist(
    network_only: Option<bool>,
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Option<radio_replay_core::m3u::Import>, String> {
    if window.label() != "main" {
        return Err("Main player required".into());
    }
    let selected = app
        .dialog()
        .file()
        .add_filter("M3U playlists", &["m3u", "m3u8"])
        .blocking_pick_file();
    let Some(file) = selected else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let report = if network_only.unwrap_or(false) {
            radio_replay_core::m3u::import_network(&path)?
        } else {
            radio_replay_core::m3u::import(&path)?
        };
        let library = app.state::<Library>();
        library
            .1
            .lock()
            .map_err(|e| e.to_string())?
            .extend(report.tracks.iter().map(|t| t.path.clone()));
        Ok(Some(report))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn choose_bumper_folder(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library: State<'_, Library>,
) -> Result<Option<String>, String> {
    if window.label() != "main" {
        return Err("Main player required".into());
    }
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = std::fs::canonicalize(folder.into_path().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if !path.is_dir() {
        return Err("Choose a folder of bumper clips".into());
    }
    let path = path.to_string_lossy().into_owned();
    library
        .1
        .lock()
        .map_err(|e| e.to_string())?
        .insert(radio_replay_core::bumpers::folder_grant(&path));
    Ok(Some(path))
}

#[tauri::command]
async fn choose_artwork(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library: State<'_, Library>,
) -> Result<Option<String>, String> {
    main_only(&window)?;
    let Some(file) = app
        .dialog()
        .file()
        .add_filter("Artwork", &["png", "jpg", "jpeg", "webp", "gif"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    radio_replay_core::artwork::validate(&path)?;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|e| e.to_string())?;
    let path = path.to_string_lossy().into_owned();
    library
        .2
        .lock()
        .map_err(|e| e.to_string())?
        .insert(path.clone());
    Ok(Some(path))
}

#[tauri::command]
#[cfg(not(feature = "native-audio"))]
async fn prepare_station(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library: State<'_, Library>,
    key: String,
    group: Option<String>,
    cache: State<'_, MetadataCache>,
) -> Result<library::PreparedStation, String> {
    main_only(&window)?;
    let config = library.0.lock().map_err(|e| e.to_string())?.load_shared()?;
    let mut report = library::playback_report_for_group_shared(&config, &key, group.as_deref())?;
    let tracks = &mut report.tracks;
    metadata::lock(&cache.0).populate_durations(tracks);
    let store = library.0.lock().map_err(|e| e.to_string())?;
    let current = store.load_shared()?;
    library::validate_preparation(&config, &current, &key, group.as_deref().unwrap_or("all"))?;
    for track in tracks.iter() {
        app.asset_protocol_scope()
            .allow_file(&track.track.path)
            .map_err(|e| e.to_string())?;
    }
    library
        .3
        .lock()
        .map_err(|e| e.to_string())?
        .remember(&key, tracks.iter().cloned().map(std::sync::Arc::new))?;
    Ok(report)
}

#[tauri::command]
async fn save_station(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    edit: StationEdit,
) -> Result<LibraryUpdate, String> {
    main_only(&window)?;
    library_write(app, move |library| {
        let store = library.0.lock().map_err(|e| e.to_string())?;
        let baseline = store.load_shared()?;
        let mut config = (*baseline).clone();
        let key = edit.key.clone();
        library::upsert(
            &mut config,
            edit,
            &*library.1.lock().map_err(|e| e.to_string())?,
            &*library.2.lock().map_err(|e| e.to_string())?,
        )?;
        let config = store.save_owned(config)?;
        library
            .3
            .lock()
            .map_err(|e| e.to_string())?
            .after_station_save(&baseline, &config, &key);
        Ok(LibraryUpdate::station(&baseline, &config, &key))
    })
    .await
}

#[tauri::command]
async fn remove_station_tracks(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    key: String,
    paths: Vec<String>,
) -> Result<LibraryUpdate, String> {
    main_only(&window)?;
    library_write(app, move |library| {
        let store = library.0.lock().map_err(|e| e.to_string())?;
        let baseline = store.load_shared()?;
        let mut config = (*baseline).clone();
        library::remove_tracks(&mut config, &key, &paths)?;
        let config = store.save_owned(config)?;
        library
            .3
            .lock()
            .map_err(|e| e.to_string())?
            .remove_station(&key);
        Ok(LibraryUpdate::station(&baseline, &config, &key))
    })
    .await
}

#[tauri::command]
async fn delete_station(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    key: String,
) -> Result<LibraryUpdate, String> {
    main_only(&window)?;
    delete_stations(window, app, vec![key]).await
}

#[tauri::command]
async fn delete_stations(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    keys: Vec<String>,
) -> Result<LibraryUpdate, String> {
    main_only(&window)?;
    library_write(app, move |library| {
        let store = library.0.lock().map_err(|e| e.to_string())?;
        let baseline = store.load_shared()?;
        let mut config = (*baseline).clone();
        library::remove_many(&mut config, &keys)?;
        let config = store.save_owned(config)?;
        let mut grants = library.3.lock().map_err(|e| e.to_string())?;
        for key in &keys {
            grants.remove_station(key);
        }
        Ok(LibraryUpdate::removed(&baseline, &config, keys))
    })
    .await
}

#[tauri::command]
async fn open_config(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    main_only(&window)?;
    let directory = app.state::<DataDirectory>().0.clone();
    library_write(app, move |library| {
        let path = directory.join("stations.json");
        if !path.exists() {
            let store = library.0.lock().map_err(|e| e.to_string())?;
            store.save(&store.load()?)?;
        }
        // Keep the existing IPC permission, but reveal the file instead of editing it.
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut command = std::process::Command::new("explorer.exe");
            command.arg("/select,").arg(&path);
            command
        };
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = std::process::Command::new("open");
            command.arg("-R").arg(&path);
            command
        };
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let mut command = {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(&directory);
            command
        };
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })
    .await
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            #[cfg(not(feature = "native-smoke"))]
            {
                let handle = app.handle().clone();
                let Some(instance) = single_instance::acquire(&directory, move || {
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || desktop::restore(&app));
                }).map_err(std::io::Error::other)? else { std::process::exit(0); };
                app.manage(instance);
            }
            #[cfg(feature = "native-smoke")]
            let directory = smoke::initialize(app.handle())?.unwrap_or(directory);
            app.manage(DataDirectory(directory.clone()));
            app.manage(AudioPreferences::open(directory.join("audio-preferences.json")));
            let path = directory.join("stations.json");
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            app.manage(MetadataCache(Mutex::new(metadata::Cache::open(
                &path.with_file_name("metadata-cache.sqlite"),
            ))));
            app.manage(Library(
                Mutex::new(ConfigStore::new(path)),
                Mutex::default(),
                Mutex::default(),
                Mutex::default(),
            ));
            desktop::setup(app.handle())?;
            #[cfg(feature = "native-audio")]
            native_audio::setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(|invoke| {
            #[cfg(feature = "native-audio")]
            if matches!(
                invoke.message.command(),
                "native_begin"
                    | "native_snapshot"
                    | "native_control"
                    | "native_prepare"
                    | "native_load"
            ) {
                let native_handler: fn(tauri::ipc::Invoke<tauri::Wry>)->bool = tauri::generate_handler![
                    native_audio::native_begin,
                    native_audio::native_snapshot,
                    native_audio::native_control,
                    native_audio::native_prepare,
                    native_audio::native_load
                ];
                return native_handler(invoke);
            }
            let standard_handler: fn(tauri::ipc::Invoke<tauri::Wry>)->bool = tauri::generate_handler![
                playback_backend,
                load_config,
                library_recovery_info,
                recover_library,
                export_config,
                import_config,
                choose_audio,
                choose_playlist,
                add_network_stream,
                choose_bumper_folder,
                choose_artwork,
                #[cfg(not(feature = "native-audio"))]
                prepare_station,
                track_details,
                remember_duration,
                clear_metadata_cache,
                save_group,
                delete_group,
                save_settings,
                #[cfg(not(feature = "native-audio"))]
                resolve_tuning_sound,
                recover_legacy_library,
                save_station,
                remove_station_tracks,
                delete_station,
                delete_stations,
                open_config,
                desktop::desktop_publish,
                desktop::desktop_snapshot,
                desktop::desktop_action,
                desktop::desktop_open_mini,
                desktop::desktop_restore,
                desktop::desktop_quit,
                #[cfg(feature = "native-smoke")]
                smoke::native_smoke_report
            ];
            standard_handler(invoke)
        })
        .on_page_load(|webview, payload| {
            #[cfg(feature = "native-smoke")]
            smoke::page_loaded(webview, payload);
            #[cfg(not(feature = "native-smoke"))]
            let _ = (webview, payload);
        })
        .on_window_event(|window, event| {
            desktop::on_window_event(window, event);
            drops::on_window_event(window, event);
        })
        .build(tauri::generate_context!())
        .expect("Could not start Radio Replay")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(preferences) = app.try_state::<AudioPreferences>() {
                    if let Err(error) = preferences.flush() { eprintln!("Audio preferences could not be saved on exit: {error}"); }
                }
            }
        });
}

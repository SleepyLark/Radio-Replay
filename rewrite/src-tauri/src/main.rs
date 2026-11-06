#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use radio_replay_core::{
    library::{self, StationEdit},
    Track,
};
use radio_replay_core::{metadata, preferences};
use radio_replay_core::{Config, ConfigStore};
use std::sync::Mutex;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
struct MetadataCache(Mutex<metadata::Cache>);

fn saved_audio(library: &Library, key: &str, path: &str) -> Result<library::PlayableTrack, String> {
    library
        .3
        .lock()
        .map_err(|e| e.to_string())?
        .get(&format!("{key}\0{path}"))
        .cloned()
        .ok_or("Audio file is not in the prepared station".into())
}

#[tauri::command]
async fn track_details(
    library: State<'_, Library>,
    cache: State<'_, MetadataCache>,
    key: String,
    path: String,
) -> Result<metadata::Details, String> {
    let track = saved_audio(&library, &key, &path)?;
    let path = Path::new(&track.track.path);
    let tags = cache.0.lock().map_err(|e| e.to_string())?.read(path);
    Ok(metadata::Details {
        tags,
        sidecar: metadata::sidecar(path),
    })
}

#[tauri::command]
async fn remember_duration(
    library: State<'_, Library>,
    cache: State<'_, MetadataCache>,
    key: String,
    path: String,
    size: u64,
    modified: u128,
    duration: f64,
) -> Result<(), String> {
    let track = saved_audio(&library, &key, &path)?;
    let current = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let stamp = current
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if track.size != size
        || track.modified != modified
        || current.len() != size
        || stamp != modified
    {
        return Err("Audio changed during probing".into());
    }
    cache
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .remember_duration(Path::new(&path), duration)
}

#[tauri::command]
fn clear_metadata_cache(cache: State<'_, MetadataCache>) -> Result<(), String> {
    cache.0.lock().map_err(|e| e.to_string())?.clear()
}

#[tauri::command]
fn save_group(library: State<'_, Library>, group: preferences::Group) -> Result<Config, String> {
    let store = library.0.lock().map_err(|e| e.to_string())?;
    let mut config = store.load()?;
    preferences::save_group(&mut config, group)?;
    store.save(&config)?;
    Ok(config)
}
#[tauri::command]
fn delete_group(library: State<'_, Library>, id: String) -> Result<Config, String> {
    let store = library.0.lock().map_err(|e| e.to_string())?;
    let mut config = store.load()?;
    config.groups.retain(|g| g["id"].as_str() != Some(&id));
    store.save(&config)?;
    Ok(config)
}
#[tauri::command]
fn save_settings(
    library: State<'_, Library>,
    patch: serde_json::Map<String, serde_json::Value>,
) -> Result<Config, String> {
    let store = library.0.lock().map_err(|e| e.to_string())?;
    let mut config = store.load()?;
    preferences::save_settings(&mut config, patch)?;
    store.save(&config)?;
    Ok(config)
}

struct Library(
    Mutex<ConfigStore>,
    Mutex<HashSet<String>>,
    Mutex<HashSet<String>>,
    Mutex<HashMap<String, library::PlayableTrack>>,
);

fn allow_art(app: &tauri::AppHandle, config: &Config) {
    for path in config.logos.values().filter_map(|v| v.as_str()) {
        if library::image_file(Path::new(path)) && Path::new(path).is_file() {
            let _ = app.asset_protocol_scope().allow_file(path);
        }
    }
}

#[tauri::command]
fn load_config(app: tauri::AppHandle, library: State<'_, Library>) -> Result<Config, String> {
    let config = library.0.lock().map_err(|e| e.to_string())?.load()?;
    allow_art(&app, &config);
    Ok(config)
}

#[tauri::command]
async fn import_config(
    app: tauri::AppHandle,
    library: State<'_, Library>,
) -> Result<Option<Config>, String> {
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
    let config = library.0.lock().map_err(|e| e.to_string())?.import(&path)?;
    library.3.lock().map_err(|e| e.to_string())?.clear();
    allow_art(&app, &config);
    Ok(Some(config))
}

#[tauri::command]
async fn choose_audio(
    app: tauri::AppHandle,
    library: State<'_, Library>,
    folder: bool,
) -> Result<Vec<Track>, String> {
    let picker = app.dialog().file();
    let tracks = if folder {
        match picker.blocking_pick_folder() {
            Some(file) => library::scan_folder(&file.into_path().map_err(|e| e.to_string())?)?,
            None => return Ok(vec![]),
        }
    } else {
        match picker
            .add_filter("Audio", &["mp3", "m4a", "aac", "ogg", "wav", "flac"])
            .blocking_pick_files()
        {
            Some(files) => files
                .into_iter()
                .map(|file| library::track(file.into_path().map_err(|e| e.to_string())?))
                .collect::<Result<Vec<_>, String>>()?,
            None => return Ok(vec![]),
        }
    };
    library
        .1
        .lock()
        .map_err(|e| e.to_string())?
        .extend(tracks.iter().map(|t| t.path.clone()));
    Ok(tracks)
}

#[tauri::command]
async fn choose_artwork(
    app: tauri::AppHandle,
    library: State<'_, Library>,
) -> Result<Option<String>, String> {
    let Some(file) = app
        .dialog()
        .file()
        .add_filter("Artwork", &["png", "jpg", "jpeg", "webp", "gif"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    if !library::image_file(&path) || !path.is_file() {
        return Err("Choose an image file".into());
    }
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
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
async fn prepare_station(
    app: tauri::AppHandle,
    library: State<'_, Library>,
    key: String,
    group: Option<String>,
    cache: State<'_, MetadataCache>,
) -> Result<Vec<library::PlayableTrack>, String> {
    let config = library.0.lock().map_err(|e| e.to_string())?.load()?;
    let mut tracks = library::playback_files_for_group(config, &key, group.as_deref())?;
    let metadata = cache.0.lock().map_err(|e| e.to_string())?;
    for track in &mut tracks {
        track.duration = metadata
            .peek(Path::new(&track.track.path))
            .and_then(|m| m.duration);
    }
    for track in &tracks {
        app.asset_protocol_scope()
            .allow_file(&track.track.path)
            .map_err(|e| e.to_string())?;
    }
    let mut grants = library.3.lock().map_err(|e| e.to_string())?;
    if grants.len() + tracks.len() > 100000 {
        grants.clear();
    }
    for track in &tracks {
        grants.insert(format!("{key}\0{}", track.track.path), track.clone());
    }
    Ok(tracks)
}

#[tauri::command]
fn save_station(library: State<'_, Library>, edit: StationEdit) -> Result<Config, String> {
    let store = library.0.lock().map_err(|e| e.to_string())?;
    let mut config = store.load()?;
    library::upsert(
        &mut config,
        edit,
        &*library.1.lock().map_err(|e| e.to_string())?,
        &*library.2.lock().map_err(|e| e.to_string())?,
    )?;
    store.save(&config)?;
    library.3.lock().map_err(|e| e.to_string())?.clear();
    Ok(config)
}

#[tauri::command]
fn delete_station(library: State<'_, Library>, key: String) -> Result<Config, String> {
    let store = library.0.lock().map_err(|e| e.to_string())?;
    let mut config = store.load()?;
    library::remove(&mut config, &key)?;
    store.save(&config)?;
    library.3.lock().map_err(|e| e.to_string())?.clear();
    Ok(config)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("stations.json");
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_config,
            import_config,
            choose_audio,
            choose_artwork,
            prepare_station,
            track_details,
            remember_duration,
            clear_metadata_cache,
            save_group,
            delete_group,
            save_settings,
            save_station,
            delete_station
        ])
        .run(tauri::generate_context!())
        .expect("Could not start Radio Replay");
}

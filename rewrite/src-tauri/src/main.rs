#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use radio_replay_core::{
    library::{self, StationEdit},
    Track,
};
use radio_replay_core::{Config, ConfigStore};
use std::sync::Mutex;
use std::{collections::HashSet, path::Path};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

struct Library(
    Mutex<ConfigStore>,
    Mutex<HashSet<String>>,
    Mutex<HashSet<String>>,
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
) -> Result<Vec<library::PlayableTrack>, String> {
    let config = library.0.lock().map_err(|e| e.to_string())?.load()?;
    let tracks = library::playback_files(&config, &key)?;
    for track in &tracks {
        app.asset_protocol_scope()
            .allow_file(&track.track.path)
            .map_err(|e| e.to_string())?;
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
    Ok(config)
}

#[tauri::command]
fn delete_station(library: State<'_, Library>, key: String) -> Result<Config, String> {
    let store = library.0.lock().map_err(|e| e.to_string())?;
    let mut config = store.load()?;
    library::remove(&mut config, &key)?;
    store.save(&config)?;
    Ok(config)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("stations.json");
            app.manage(Library(
                Mutex::new(ConfigStore::new(path)),
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
            save_station,
            delete_station
        ])
        .run(tauri::generate_context!())
        .expect("Could not start Radio Replay");
}

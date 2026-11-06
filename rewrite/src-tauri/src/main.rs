#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use radio_replay_core::{Config, ConfigStore};
use std::sync::Mutex;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

struct Library(Mutex<ConfigStore>);

#[tauri::command]
fn load_config(library: State<'_, Library>) -> Result<Config, String> {
    library.0.lock().map_err(|e| e.to_string())?.load()
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
    library
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .import(&path)
        .map(Some)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("stations.json");
            app.manage(Library(Mutex::new(ConfigStore::new(path))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![load_config, import_config])
        .run(tauri::generate_context!())
        .expect("Could not start Radio Replay");
}

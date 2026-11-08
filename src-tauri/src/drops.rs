//! OS-originated drops grant only the audio files actually supplied by the user.
use radio_replay_core::{library, Track};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{Emitter, Manager};

static NEXT_DROP: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Serialize)]
struct DropResult {
    id: u64,
    tracks: Vec<Track>,
    error: String,
}

pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }
    let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event else {
        return;
    };
    let app = window.app_handle().clone();
    let id = NEXT_DROP.fetch_add(1, Ordering::Relaxed);
    let _ = app.emit_to("main", "audio-drop-start", id);
    let paths = paths.clone();
    std::thread::spawn(move || {
        let result = library::collect_audio(&paths);
        let (tracks, error) = match result {
            Ok(tracks) => {
                let library = app.state::<super::Library>();
                let granted = match library.1.lock() {
                    Ok(mut grants) => {
                        grants.extend(tracks.iter().map(|t| t.path.clone()));
                        (tracks, String::new())
                    }
                    Err(error) => (vec![], error.to_string()),
                };
                granted
            }
            Err(error) => (vec![], error),
        };
        let _ = app.emit_to("main", "audio-drop", DropResult { id, tracks, error });
    });
}

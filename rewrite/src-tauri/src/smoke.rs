//! Opt-in integration harness. This module and its IPC handler are absent from normal builds.
use radio_replay_core::{Config, ConfigStore, Station, Track};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{Manager, State, WebviewWindow};
pub(crate) struct Smoke {
    directory: PathBuf,
    mini_done: AtomicBool,
    finished: AtomicBool,
    phases: Mutex<Vec<String>>,
}
pub fn active() -> bool {
    std::env::var_os("RADIO_REPLAY_NATIVE_SMOKE_DIR").is_some()
}
fn report(app: &tauri::AppHandle, ok: bool, message: &str) {
    let Some(smoke) = app.try_state::<Smoke>() else {
        return;
    };
    if smoke.finished.swap(true, Ordering::Relaxed) {
        return;
    }
    let (tray, media) = super::desktop::smoke_status(app);
    let value = serde_json::json!({"passed":ok,"message":message,"trayReady":tray,"nativeMediaReady":media,"miniCompleted":smoke.mini_done.load(Ordering::Relaxed),"phases":smoke.phases.lock().map(|p|p.clone()).unwrap_or_default()});
    let _ = std::fs::write(
        smoke.directory.join("report.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    );
    println!("Native smoke: {value}");
    app.state::<super::desktop::Desktop>().mark_quitting();
    app.exit(if ok { 0 } else { 1 });
}
pub fn initialize(app: &tauri::AppHandle) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let Some(directory) = std::env::var_os("RADIO_REPLAY_NATIVE_SMOKE_DIR").map(PathBuf::from)
    else {
        return Ok(None);
    };
    if !directory.is_absolute()
        || directory.file_name().and_then(|s| s.to_str()) != Some("native-smoke")
    {
        return Err("Native smoke requires an absolute directory named native-smoke".into());
    }
    std::fs::create_dir_all(&directory)?;
    let samples = 10 * 8000;
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + samples * 2u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(8000u32.to_le_bytes());
    wav.extend(16000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((samples * 2u32).to_le_bytes());
    wav.resize(44 + samples as usize * 2, 0);
    let mut tracks = Vec::new();
    for i in 0..2 {
        let file = directory.join(format!("fixture-{i}.wav"));
        std::fs::write(&file, &wav)?;
        std::fs::write(
            file.with_extension("lrc"),
            "[00:00]Native opening lyric\n[00:05]Native later lyric",
        )?;
        tracks.push(Track {
            path: file.to_string_lossy().into_owned(),
            name: format!("Native track {i}"),
            extra: Default::default(),
        });
    }
    let mut config = Config::default();
    let art = directory.join("fixture.png");
    std::fs::write(&art, include_bytes!("../../../assets/icons/radio.png"))?;
    for (key, name) in [("a", "Native A"), ("b", "Native B")] {
        let mut extra = serde_json::Map::new();
        extra.insert("mode".into(), "longset".into());
        extra.insert("textMode".into(), "lyrics".into());
        extra.insert(
            "bannerPath".into(),
            art.to_string_lossy().into_owned().into(),
        );
        config
            .logos
            .insert(key.into(), art.to_string_lossy().into_owned().into());
        config.custom.push(Station {
            key: key.into(),
            station: name.into(),
            tracks: tracks.clone(),
            extra,
        });
    }
    config
        .settings
        .insert("tuningTransition".into(), false.into());
    config.settings.insert("crossfade".into(), 0.into());
    config.settings.insert("closeToTray".into(), true.into());
    ConfigStore::new(directory.join("stations.json")).save(&config)?;
    app.manage(Smoke {
        directory: directory.clone(),
        mini_done: AtomicBool::new(false),
        finished: AtomicBool::new(false),
        phases: Mutex::default(),
    });
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        report(&handle, false, "Native smoke timed out");
    });
    Ok(Some(directory))
}
pub fn page_loaded(webview: &tauri::Webview, payload: &tauri::webview::PageLoadPayload<'_>) {
    if active() {
        if let Some(state) = webview.app_handle().try_state::<Smoke>() {
            if let Ok(mut phases) = state.phases.lock() {
                phases.push(format!(
                    "Page {}: {:?}: {}",
                    webview.label(),
                    payload.event(),
                    payload.url()
                ));
            }
        }
    }
    if active() && matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
        let _ = webview.eval(include_str!("../../tests/native-smoke.js"));
    }
}
#[tauri::command]
pub fn native_smoke_report(
    window: WebviewWindow,
    app: tauri::AppHandle,
    smoke: State<'_, Smoke>,
    phase: String,
    ok: bool,
    message: String,
) -> Result<(), String> {
    if !active() {
        return Err("Native smoke is not active".into());
    }
    smoke
        .phases
        .lock()
        .map_err(|e| e.to_string())?
        .push(format!("{}: {phase}: {message}", window.label()));
    if !ok {
        report(&app, false, &message);
        return Ok(());
    }
    if phase == "closeToTray" && window.label() == "main" {
        window.close().map_err(|e| e.to_string())?;
    }
    if phase == "drop" && window.label() == "main" {
        super::drops::on_window_event(
            &window.as_ref().window(),
            &tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop {
                paths: vec![
                    smoke.directory.join("fixture-0.wav"),
                    smoke.directory.join("fixture-1.wav"),
                ],
                position: tauri::PhysicalPosition::new(0.0, 0.0),
            }),
        );
    }
    if phase == "mini" && window.label() == "mini" {
        smoke.mini_done.store(true, Ordering::Relaxed);
    }
    if phase == "complete" && window.label() == "main" {
        let (tray, media) = super::desktop::smoke_status(&app);
        report(
            &app,
            smoke.mini_done.load(Ordering::Relaxed) && tray && media,
            "WebView2 playback, mini controls, restore, tray and native media initialization",
        );
    }
    Ok(())
}

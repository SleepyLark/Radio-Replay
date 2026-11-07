use radio_replay_core::desktop::{visible, Action, Position, Snapshot};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

pub struct Desktop {
    snapshot: Mutex<Snapshot>,
    media: Mutex<Option<MediaControls>>,
    media_title: Mutex<String>,
    tray_ready: AtomicBool,
    quitting: AtomicBool,
    mini_creation: Mutex<()>,
}
impl Default for Desktop {
    fn default() -> Self {
        Self {
            snapshot: Mutex::new(Snapshot {
                volume: 0.8,
                accent: "#c7f464".into(),
                ..Default::default()
            }),
            media: Mutex::new(None),
            media_title: Mutex::new(String::new()),
            tray_ready: AtomicBool::new(false),
            quitting: AtomicBool::new(false),
            mini_creation: Mutex::default(),
        }
    }
}
impl Desktop {
    #[cfg(feature = "native-smoke")]
    pub fn mark_quitting(&self) {
        self.quitting.store(true, Ordering::Relaxed);
    }
}
fn show_windows() -> bool {
    #[cfg(feature = "native-smoke")]
    if super::smoke::active() {
        return false;
    }
    true
}
#[cfg(feature = "native-smoke")]
pub fn smoke_status(app: &tauri::AppHandle) -> (bool, bool) {
    let state = app.state::<Desktop>();
    (
        state.tray_ready.load(Ordering::Relaxed),
        state.media.lock().map(|m| m.is_some()).unwrap_or(false),
    )
}
fn main_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the main player may perform this operation".into());
    }
    Ok(())
}
fn known_window(window: &WebviewWindow) -> Result<(), String> {
    if !matches!(window.label(), "main" | "mini") {
        return Err("Unknown player window".into());
    }
    Ok(())
}
fn dispatch(app: &tauri::AppHandle, action: Action) {
    let _ = app.emit_to("main", "desktop-command", action);
}
fn quit(app: &tauri::AppHandle) {
    save_position(app);
    app.state::<Desktop>()
        .quitting
        .store(true, Ordering::Relaxed);
    app.exit(0);
}
fn save_position(app: &tauri::AppHandle) {
    if let Some(mini) = app.get_webview_window("mini") {
        if let Ok(pos) = mini.outer_position() {
            let dir = app.state::<super::DataDirectory>().0.clone();
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(bytes) = serde_json::to_vec(&Position { x: pos.x, y: pos.y }) {
                let _ = std::fs::write(dir.join("mini-position.json"), bytes);
            }
        }
    }
}
pub fn restore(app: &tauri::AppHandle) {
    save_position(app);
    if let Some(mini) = app.get_webview_window("mini") {
        let _ = mini.hide();
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        if show_windows() {
            let _ = main.show();
            let _ = main.set_focus();
        }
    }
}
fn open_mini(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<Desktop>();
    let _creation = state.mini_creation.lock().map_err(|e| e.to_string())?;
    let mini = if let Some(mini) = app.get_webview_window("mini") {
        mini
    } else {
        let builder =
            WebviewWindowBuilder::new(app, "mini", WebviewUrl::App("index.html?mini=1".into()))
                .title("Radio Replay Mini")
                .inner_size(480.0, 160.0)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false);
        // WebView2 shares its browser process; all windows must use matching options.
        #[cfg(windows)]
        let builder = if let Some(args) = app
            .config()
            .app
            .windows
            .iter()
            .find(|w| w.label == "main")
            .and_then(|w| w.additional_browser_args.as_deref())
        {
            builder.additional_browser_args(args)
        } else {
            builder
        };
        let mini = builder.build().map_err(|e| e.to_string())?;
        let size = mini.outer_size().map_err(|e| e.to_string())?;
        let saved = std::fs::read(
            app.state::<super::DataDirectory>()
                .0
                .join("mini-position.json"),
        )
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Position>(&bytes).ok());
        let monitors = mini.available_monitors().map_err(|e| e.to_string())?;
        let valid = saved.filter(|pos| {
            monitors.iter().any(|m| {
                let a = m.work_area();
                visible(
                    *pos,
                    size.width,
                    size.height,
                    (a.position.x, a.position.y, a.size.width, a.size.height),
                )
            })
        });
        if let Some(pos) = valid {
            let _ = mini.set_position(PhysicalPosition::new(pos.x, pos.y));
        } else if let Some(monitor) = app
            .get_webview_window("main")
            .and_then(|w| w.current_monitor().ok().flatten())
            .or_else(|| monitors.first().cloned())
        {
            let a = monitor.work_area();
            let x = i64::from(a.position.x)
                + (i64::from(a.size.width) - i64::from(size.width) - 20).max(0);
            let y = i64::from(a.position.y)
                + (i64::from(a.size.height) - i64::from(size.height) - 20).max(0);
            let _ = mini.set_position(PhysicalPosition::new(x as i32, y as i32));
        }
        mini
    };
    if show_windows() {
        mini.show().map_err(|e| e.to_string())?;
        let _ = mini.set_focus();
    }
    if let Some(main) = app.get_webview_window("main") {
        main.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(Desktop::default());
    let show = MenuItem::with_id(app, "show", "Open Radio Replay", true, None::<&str>)?;
    let mini = MenuItem::with_id(app, "mini", "Mini player", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Play / pause", true, None::<&str>)?;
    let previous = MenuItem::with_id(app, "previous", "Previous station", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "Next station", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &mini, &toggle, &previous, &next, &exit])?;
    let tray = TrayIconBuilder::with_id("radio-replay")
        .icon(
            app.default_window_icon()
                .ok_or("Missing tray icon")?
                .clone(),
        )
        .tooltip("Radio Replay Preview")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => restore(app),
            "mini" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_mini(&handle);
                });
            }
            "toggle" => dispatch(app, Action::Toggle),
            "previous" => dispatch(app, Action::Previous),
            "next" => dispatch(app, Action::Next),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                restore(tray.app_handle());
            }
        })
        .build(app);
    app.state::<Desktop>()
        .tray_ready
        .store(tray.is_ok(), Ordering::Relaxed);
    if let Err(error) = tray {
        eprintln!("Tray unavailable: {error}");
    }
    #[cfg(windows)]
    let hwnd = app
        .get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .map(|handle| handle.0 as *mut std::ffi::c_void);
    #[cfg(not(windows))]
    let hwnd = None;
    match MediaControls::new(PlatformConfig {
        display_name: "Radio Replay Preview",
        dbus_name: "radio_replay_preview",
        hwnd,
    }) {
        Ok(mut controls) => {
            let handle = app.clone();
            if let Err(error) = controls.attach(move |event| match event {
                MediaControlEvent::Play => dispatch(&handle, Action::Play),
                MediaControlEvent::Pause | MediaControlEvent::Stop => {
                    dispatch(&handle, Action::Pause)
                }
                MediaControlEvent::Toggle => dispatch(&handle, Action::Toggle),
                MediaControlEvent::Next => dispatch(&handle, Action::Next),
                MediaControlEvent::Previous => dispatch(&handle, Action::Previous),
                MediaControlEvent::SetVolume(v) => {
                    if Action::Volume(v).validate().is_ok() {
                        dispatch(&handle, Action::Volume(v));
                    }
                }
                MediaControlEvent::Raise => restore(&handle),
                _ => {}
            }) {
                eprintln!("Media event subscription unavailable: {error:?}");
            } else {
                *app.state::<Desktop>()
                    .media
                    .lock()
                    .map_err(|e| e.to_string())? = Some(controls);
            }
        }
        Err(error) => eprintln!("Native media controls unavailable: {error:?}"),
    }
    Ok(())
}

pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    let app = window.app_handle();
    let Some(desktop) = app.try_state::<Desktop>() else {
        return;
    };
    if desktop.quitting.load(Ordering::Relaxed) {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        if window.label() == "mini" {
            api.prevent_close();
            restore(app);
        } else if window.label() == "main" {
            api.prevent_close();
            let close_to_tray = app
                .try_state::<super::Library>()
                .and_then(|library| library.0.lock().ok().and_then(|store| store.load().ok()))
                .map(|config| {
                    config
                        .settings
                        .get("closeToTray")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if close_to_tray && desktop.tray_ready.load(Ordering::Relaxed) {
                let _ = window.hide();
            } else {
                quit(app);
            }
        }
    }
}

#[tauri::command]
pub fn desktop_publish(
    window: WebviewWindow,
    app: tauri::AppHandle,
    desktop: State<'_, Desktop>,
    snapshot: Snapshot,
) -> Result<(), String> {
    main_only(&window)?;
    snapshot.validate()?;
    *desktop.snapshot.lock().map_err(|e| e.to_string())? = snapshot.clone();
    if let Some(mini) = app.get_webview_window("mini") {
        if mini.is_visible().unwrap_or(false) || !show_windows() {
            let _ = app.emit_to("mini", "mini-state", &snapshot);
        }
    }
    if let Some(media) = desktop.media.lock().map_err(|e| e.to_string())?.as_mut() {
        let identity = format!(
            "{}\0{}\0{}\0{}",
            snapshot.station, snapshot.title, snapshot.artist, snapshot.album
        );
        let mut previous = desktop.media_title.lock().map_err(|e| e.to_string())?;
        if *previous != identity {
            let _ = media.set_metadata(MediaMetadata {
                title: Some(&snapshot.title),
                artist: Some(if snapshot.artist.is_empty() {
                    &snapshot.station
                } else {
                    &snapshot.artist
                }),
                album: Some(&snapshot.album),
                duration: Some(Duration::from_secs_f64(snapshot.duration)),
                ..Default::default()
            });
            *previous = identity;
        }
        let progress = Some(MediaPosition(Duration::from_secs_f64(snapshot.seconds)));
        let _ = media.set_playback(if !snapshot.has_station {
            MediaPlayback::Stopped
        } else if snapshot.playing {
            MediaPlayback::Playing { progress }
        } else {
            MediaPlayback::Paused { progress }
        });
        #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
        let _ = media.set_volume(if snapshot.muted { 0.0 } else { snapshot.volume });
    }
    Ok(())
}
#[tauri::command]
pub fn desktop_snapshot(
    window: WebviewWindow,
    desktop: State<'_, Desktop>,
) -> Result<Snapshot, String> {
    known_window(&window)?;
    Ok(desktop.snapshot.lock().map_err(|e| e.to_string())?.clone())
}
#[tauri::command]
pub fn desktop_action(
    window: WebviewWindow,
    app: tauri::AppHandle,
    action: Action,
) -> Result<(), String> {
    if window.label() != "mini" {
        return Err("Mini player command required".into());
    }
    action.validate()?;
    app.emit_to("main", "desktop-command", action)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn desktop_open_mini(window: WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    main_only(&window)?;
    open_mini(&app)
}
#[tauri::command]
pub fn desktop_restore(window: WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    known_window(&window)?;
    restore(&app);
    Ok(())
}
#[tauri::command]
pub fn desktop_quit(window: WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    main_only(&window)?;
    quit(&app);
    Ok(())
}

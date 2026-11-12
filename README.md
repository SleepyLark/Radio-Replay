# Radio Replay

A local radio player built with Rust, Tauri 2 and Svelte 5. Create stations from your music collection, tune between broadcasts and shuffled music, and use the mini player or system media controls while listening.

The Rust application is now the primary project and runs from the repository root. The original Electron application remains available in Git history.

## Run on Windows

Install Node.js 24 LTS, stable Rust, Microsoft C++ Build Tools with the Desktop development with C++ workload, and the Microsoft Edge WebView2 runtime.

Open PowerShell in the repository root:

```powershell
npm ci
npm run desktop
```

`npm start` also launches the desktop app. `npm run dev` opens a browser layout preview without local filesystem access.

## Build an installer

```powershell
npm run package -- --bundles nsis
```

The installer is in `target/release/bundle/nsis/`. The standalone application is `target/release/radio-replay-desktop.exe`. Build from the repository root; the former `rewrite/` working directory is no longer used.

## Library and playback

- Create and edit stations from the sidebar. Manage Stations includes Import Library and bulk removal. Import selects a `stations.json` file and copies its configuration; music files stay in their original locations.
- Mini Player is beside the tuning hint on the main player screen. Quit Radio Replay is in Settings. Closing the main window exits by default; keeping playback in the tray is an opt-in setting.
- Mute displays zero volume while remembering the previous level. Unmute restores it; changing the volume control unmutes at the new level.
- Radio broadcast editors can move individual files or whole folders up, down, or to the beginning. Music stations shuffle playback and hide reorder controls. Show stations.json in Advanced settings reveals the configuration file in its folder.
- Space plays/pauses, up/down tune stations, M mutes, and +/− adjust volume. Keyboard media keys and system media controls also control playback.
- Duration and metadata caches use SQLite and file signatures. Errors appear in dismissible notifications with a searchable list of affected files.
- Audio uses the platform WebView's codecs. An `.m4a` extension alone does not guarantee support for the codec stored inside the container.

The existing `com.local.radioreplay.preview` data identifier is intentionally retained so current Rust users keep their stations, settings and SQLite cache. On Windows its data is under `%APPDATA%\com.local.radioreplay.preview`.

## Development

```powershell
npm run check
npm run test:playback
cargo test -p radio-replay-core --locked
cargo check -p radio-replay-desktop --locked
npm run test:ui
npm run test:audio
```

UI and audio integration tests use Playwright with installed Microsoft Edge, without Electron. To use a different supported browser channel, set `RADIO_REPLAY_BROWSER`. Tests use fixtures and do not edit your music library.

The optional native integration harness exercises WebView2, IPC, SQLite caching, tray behavior and mini-player controls with an isolated profile:

```powershell
npm exec tauri -- build --debug --no-bundle --features native-smoke --config src-tauri/tauri.smoke.conf.json
$env:RADIO_REPLAY_NATIVE_SMOKE_DIR = "$PWD\out\native-smoke"
& .\target\debug\radio-replay-desktop.exe
Remove-Item Env:RADIO_REPLAY_NATIVE_SMOKE_DIR
Get-Content .\out\native-smoke\report.json
```

`crates/core` contains library persistence, file validation, metadata, cache and station clock logic. `src-tauri` contains the native shell and restricted desktop commands. `ui` contains the Svelte interface and Web Audio playback engine. Theme styles and licensed fonts are preserved in `ui/theme`; shared assets are in `assets`. No LiMusic code or assets are included.

GitHub Actions checks Rust and the frontend across Windows, Linux and macOS. A manual workflow run or version tag also builds a Windows installer. Linux and macOS still need native playback and media-key testing on their target systems.

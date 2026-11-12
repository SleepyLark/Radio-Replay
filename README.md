# Radio Replay

Radio Replay turns your own audio collection into a desktop radio experience. Create shuffled music stations, tune into complete radio recordings, organize stations into groups, and give each one its own artwork and style.

The current application uses **Rust, Tauri 2, and Svelte 5**, with native audio playback through **libmpv**. WebView2 renders the interface; audio decoding and output run in Rust-owned native workers. The original Electron application remains available in Git history.

The libmpv migration is merged into `main` and selected by the normal launch and package commands. See [native audio setup and implementation details](docs/libmpv-migration.md).

The station-tuning experience is inspired by games such as Grand Theft Auto. No game audio, music, or game artwork is included; you supply your own collection.

> [!NOTE]
> This project is developed with substantial AI assistance.

<p align="center">
  <img src="docs/images/home.png" alt="Radio Replay Rust station browser" width="450">
  <img src="docs/images/player.png" alt="Radio Replay Rust player with artwork, lyrics, and volume controls" width="450">
</p>

## Download

Download the Rust build from [GitHub Releases](https://github.com/SleepyLark/Radio-Replay/releases). Windows x64 is currently the packaged and tested platform:

| File | Purpose |
| --- | --- |
| `Radio-Replay-<version>-Windows-x64-setup.exe` | Recommended installer |
| `Radio-Replay-<version>-Windows-x64-portable.zip` | Extract the complete ZIP and run the included executable |
| `SHA256SUMS.txt` | SHA-256 checksums for both downloads |

You do not need Rust, Node.js, or npm to use a release build. Both Windows downloads include the libmpv audio DLL and its license: audio needs no extra download or setup step. Keep the portable ZIP's `audio/` and `runtime/` folders beside the executable. WebView2 is still required for the interface; the installer can download it if missing. Local playback works offline without an account or music service. Runtime licensing and source provenance are included in `runtime/README.txt` and `runtime/SOURCES.txt`.

Release builds are currently unsigned; Windows may display a SmartScreen warning. Linux and macOS receive source/build checks in CI, but native playback and media controls still need testing before we offer packages for them.

Older releases, including the original `v0.5.0`, contain the Electron application. The Rust release prepared by this repository is `v0.6.5`.

## Features

- Create stations from files, folders, or audio dropped into the app.
- Shuffle songs as a **Music station**, or play recordings in order as a **Radio broadcast**.
- Keep station timelines moving while listening elsewhere, without playing hidden audio.
- Organize whole stations or individual music tracks into groups.
- Customize logos, GIFs, banners, colors, scanlines, motion, and tuning static.
- Display embedded track information, synchronized lyrics, and timestamped track listings.
- Set global or per-station crossfades.
- Use keyboard shortcuts, media keys, the tray, and a mini player.
- Cache metadata and durations in SQLite for reuse across launches.
- Inspect problematic files through dismissible notifications.

Recognized audio extensions are MP3, M4A, AAC, OGG, WAV, and FLAC. libmpv handles the codecs inside those containers, including ALAC in M4A. Network streams and M3U playlists are not enabled yet.

## Getting started

### Create and customize stations

Choose **Create Station**, add files or a folder, and select a playback style:

- **Music station** shuffles its tracks. Reorder controls are hidden because playback order is shuffled.
- **Radio broadcast** follows the playlist order. Move individual recordings or entire folders with **Up**, **Down**, or **First**. Folder moves preserve the relative order of their recordings.

Recordings stream through libmpv, with one next recording preloaded near a transition. Crossfades use native equal-power mixing; zero-fade stations hand off to the preloaded decoder at EOF. Transitions are not guaranteed to be sample-accurate gapless playback.

Add a logo, banner image or color, accent color, and playback options in the station editor. Supported artwork extensions are PNG, JPG, JPEG, WebP, and GIF.

Use **Manage Stations** to edit or remove station entries. Removing an entry never deletes the source audio or artwork. **Manage Stations → Import library** imports a `stations.json`, including one from the Electron version, without moving the original media.

### Tune in

Select a station from the home screen or sidebar. On its first tune in a session, a station begins at a random point. Its timeline advances while you listen to other stations, giving the lineup the feel of continuous radio. Station timelines reset when the application closes.

The bottom Now Playing bar appears on **All Stations**. The PIP button beside **↑ ↓ TO TUNE** opens the mini player; its exit-PIP button restores the main window.

Closing the main window exits by default. **Settings → General** can enable keeping playback in the tray. **Quit Radio Replay** is also available in Settings.

## Player controls

| Action | Control |
| --- | --- |
| Play / pause | **Space**, player button, or system media key |
| Next / previous station | **↓ / ↑**, player buttons, or system media keys |
| Adjust volume | **+ / −**, drag the volume meter, or scroll over it |
| Mute / unmute | **M** or speaker button |
| Tune with the mouse | Scroll over the player view, outside the volume controls |
| Restore the main window from mini | Exit-PIP button or **Escape** |

Mute moves the displayed volume to zero while remembering the previous level. Unmute restores that level. Moving the slider or changing volume while muted unmutes at the new value.

The progress bar shows position within the current recording and is read-only. The random-station setting changes Next into a random tuner within the selected group; Previous and Next retrace station history before choosing a new random station.

## Groups, lyrics, and track listings

Create and edit groups with the controls above the sidebar station list. A group can include entire stations or selected tracks from Music stations.

For timed text, place an `.lrc` file beside its recording with the same base name:

```text
My Broadcast.mp3
My Broadcast.lrc
```

```text
[00:00.00]Opening
[03:42.50]Next song
[120:00.00]Two hours into the recording
```

Timestamped `.txt` sidecars also work. Supported timestamps use **minutes:seconds**, with optional fractional seconds; hours must be expressed as total minutes. Two hours is **120 minutes**, not 240. Untimed lines and malformed timestamps are ignored.

Music stations use timed text as lyrics by default. Broadcasts use it as the current track or segment title. Change this in the station editor's Advanced options.

## Settings and saved data

Settings save automatically. **Settings → Advanced → Show stations.json** reveals the configuration file in its folder.

On Windows, the current Rust app stores its data here:

```text
%APPDATA%\com.local.radioreplay.preview\
  stations.json
  stations.json.bak
  metadata-cache.sqlite
```

The `.preview` identifier is intentionally retained so existing Rust installations keep their stations, settings, and cache. Audio and artwork stay in their original locations; the configuration stores paths to them. SQLite may also create `-wal` and `-shm` companion files while running.

**Clear metadata cache** removes cached information without removing your stations or media. Files will be measured again as needed. Back up the app-data folder before manually editing or moving its contents; close the app before copying an active SQLite database.

## Build from source

### Windows prerequisites

- Git and **Node.js 24 LTS**, including npm.
- A current stable [Rust toolchain](https://www.rust-lang.org/tools/install), using the MSVC target on Windows.
- Microsoft C++ Build Tools with **Desktop development with C++** and a Windows SDK.
- Microsoft Edge WebView2.

See [Tauri's platform prerequisites](https://v2.tauri.app/start/prerequisites/) for setup details and Linux/macOS development dependencies.

From the repository root:

```powershell
git clone https://github.com/SleepyLark/Radio-Replay.git
cd Radio-Replay
npm ci
npm run desktop
```

`npm start` is an alias for the desktop development app. Tauri/Vite reload frontend changes and rebuild native changes during development. `npm run dev` is a browser-only layout preview without native filesystem access.

Build a Windows installer:

```powershell
npm run package -- --bundles nsis -- --locked
```

The installer is in `target/release/bundle/nsis/`; the executable is `target/release/radio-replay-desktop.exe`. Normal launch/package commands automatically verify or install the developer runtime under ignored `vendor/mpv/`. Installed apps use `audio/libmpv-2.dll` in their installation folder. A bare executable is not a complete portable distribution: use the release packaging script below. The first build downloads Rust dependencies and bundling tools. Builds run from the repository root.

For the same named files and checksums used by GitHub Actions, run `pwsh -File scripts/package-windows.ps1` with PowerShell 7.3 or newer. Those files are written to `release/<version>/windows-x64/`.

## Development commands

| Command | Purpose |
| --- | --- |
| `npm run desktop` | Run the native desktop app with development reload |
| `npm run setup:mpv` | Verify/download the Windows x64 native audio runtime |
| `npm run dev` | Browser-only layout preview |
| `npm run check` | Check Svelte and TypeScript |
| `npm run build` | Build the frontend |
| `npm run test:playback` | Playback, timeline, playlist, and media-control regressions |
| `npm run test:ui` | Build and test the UI in Microsoft Edge |
| `npm run test:audio` | Legacy Web Audio integration checks for regression comparisons |
| `cargo test --workspace --locked` | Rust workspace tests |
| `cargo fmt --all -- --check` | Check Rust formatting |
| `npm run check:version` | Check all manifest and lockfile versions |
| `npm run version:set -- 0.6.6` | Set the next version in every manifest and lockfile |

Browser integration tests use Playwright with installed Microsoft Edge. Set `RADIO_REPLAY_BROWSER` to another installed Playwright-supported browser channel if needed. Tests use fixtures rather than your real library.

An isolated native integration harness covers libmpv playback/static, SQLite, tray behavior, mini-player IPC, and absence of browser audio fallback. See [development validation](docs/development.md) for its commands.

## Project layout

| Path | Purpose |
| --- | --- |
| `crates/core/` | Rust library persistence, validation, metadata, and SQLite cache |
| `crates/audio/` | libmpv client boundary, worker, tuning static, preloading, and crossfades |
| `src-tauri/` | Native shell, restricted IPC, file dialogs, tray, and system media controls |
| `ui/` | Svelte interface and native playback/clock adapter |
| `runtime/` | Pinned audio runtime manifest, developer download helper, and license/source information |
| `ui/theme/` | Theme styles and licensed fonts |
| `assets/` | App icons and bundled tuning static |
| `tests/` | Playback, browser UI/audio, and native integration fixtures |
| `scripts/` | Version validation and Windows release packaging |
| `.github/workflows/` | CI and GitHub Release automation |

Radio Replay's own code is distributed under the [MIT license](LICENSE). The bundled libmpv runtime has its own licenses and source-distribution requirements; see [runtime information](runtime/README.txt). No LiMusic code or assets are included.

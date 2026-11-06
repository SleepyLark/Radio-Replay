# Radio Replay Rust preview

An original Tauri 2 + Rust + Svelte 5 foundation, with a sidebar, station library,
and persistent player bar inspired by modern desktop music players. No LiMusic
code or assets are included. The Electron app at the repository root remains runnable.

## Run

Install a current stable Rust toolchain, Node.js 22.12+ and the Tauri platform
prerequisites (Windows: MSVC C++ build tools and WebView2).

From this directory:

```sh
npm ci
npm run desktop
```

`npm run dev` opens the browser-only layout preview (no filesystem access).
`npm run check` checks Svelte/TypeScript; `npm run build` builds the frontend;
`npm run test:playback` runs deterministic playback/timeline regressions with Node 22.18+;
`cargo test -p radio-replay-core` verifies configuration compatibility and persistence;
`cargo check -p radio-replay-desktop` checks the native shell.

## Foundation boundary

- `crates/core`: typed stations/tracks and configuration persistence without Tauri.
- `src-tauri`: desktop initialization and narrow native commands for loading/importing configuration.
- `ui`: typed IPC adapter and Svelte library shell; no Node/Electron dependencies.
- Imports are selected through a native JSON picker. Unknown settings, groups,
  station options and track fields survive round trips. Hidden stations are filtered.
- Writes are serialized by a Rust mutex, staged in the destination directory,
  synced, and atomically replaced. The preceding configuration is backed up.
  Corrupt destination data blocks writes.
- The preview uses `com.local.radioreplay.preview` and its own Tauri app-data
  directory. Import copies configuration without modifying Electron's source file.
- Create, rename and delete custom stations; add files or recursively scan a selected
  folder, remove/reorder tracks, choose music or broadcast mode and select artwork.
  Deletion removes group references but never deletes source media. Imported archive
  roots can be recovered explicitly from Settings. Remaining station visual options
  (banners, automatic artwork accents and logo motion) are still pending parity work.
- Existing numeric crossfade values are preserved as-is. Convert their units at
  the playback boundary once, rather than during every configuration load.
- The renderer has no general filesystem/shell capability. Rust grants exact selected
  audio paths for station saves and exact image files for the asset protocol. Artwork
  grants are restored from the saved library at startup. Imported missing media paths
  can be retained while editing. Audio access is granted only after Rust resolves a
  saved station key; the renderer cannot request an arbitrary audio path.
- Folder scans run off the UI thread, use deterministic filename order, skip symlinks
  and Windows reparse points, and fail visibly on unreadable directories or scan limits
  (64 levels / 50,000 audio files). Extra/Extras directories are skipped, matching
  Electron. Supported formats match Electron.

## Migration sequence

1. Characterize Electron station clock, shuffle, pause, broadcast and fade behavior
   with fixtures, including the uncommitted mini-player work in the original checkout.
2. Add selected-file grants, folder scanning, station CRUD and artwork access.
3. Connect the existing Web Audio behavior through a playback interface; preserve
   background station time and gapless/crossfade semantics before replacing audio.
4. Port metadata, duration cache, sidecars, groups and settings.
5. Add tray, media controls, mini player, packaging and platform regression checks.

## Phase 3 playback

Select a station to tune in. The player provides play/pause, previous/next station,
volume/mute and recording progress (read-only). Space toggles playback, Up/Down
change stations, and M toggles mute outside forms and editors.

Station clocks start at a random position and advance silently with wall time when
you tune elsewhere. Music shuffles once per retained session timeline; broadcasts
retain their saved order. Pause freezes the audible stream/context, while an inactive
station's wall clock continues. Station/group edits, imports, and global fade changes
reset playback; appearance-only edits and cache clearing leave current audio running.

Music uses equal-power fades between two streaming elements. Existing global and
per-station crossfade values are honored, including legacy seconds; music defaults
to five seconds and broadcasts to zero. Fades are clamped to half the shortest track.
Broadcasts with multiple recordings up to ten minutes / 64 MiB each can use scheduled
AudioBuffers for gapless handoffs. The decoded cache is capped at 64 MiB per buffer
and 128 MiB total; longer files, unsupported decodes, or budget overruns fall back
to streaming. Future decode failures drain scheduled audio before streaming continues.

Metadata probes run with at most six concurrent elements and cache duration by path,
size and modification time in memory. Missing or unsupported tracks are skipped, with
a visible notice. Rapid tuning cancels stale probes and clears old playback sources.
Webview codec support remains platform-dependent. Previous/next follow the selected group's lineup, with optional random
selection and bounded backward/forward history.

Tuning transitions support bundled static or a selected audio file, with configurable
minimum/maximum delay (0–1500 ms). They honor mute/volume, stop on cancellation, and
cannot restart after a later tune. Missing static audio never blocks station playback.

## Phase 4 library features

- Rust reads embedded title, artist, album and duration with Lofty, skipping cover
  art. Tags are parsed for the current track; tuning reads cached durations and
  falls back to the browser's bounded metadata probes. A failed tag read leaves
  filename playback available.
- `metadata-cache.sqlite` in the preview data directory stores derived metadata,
  keyed by canonical path and file signature (size, modification time and creation/
  change stamp). It evicts old entries in batches around 20,000 rows. Corrupt or
  unwritable caches fall back to uncached reads. Clear metadata cache also rebuilds
  an unavailable database and clears session duration/clock caches without stopping audio.
  This preview cache is independent of Electron's disposable duration cache.
- Adjacent `.lrc`, then timestamped `.txt`, files are read fresh with a 1 MiB limit.
  Standard minute/second timestamps, repeated timestamps and BOMs are supported;
  malformed/untimed lines are ignored and output is bounded to 20,000 entries.
  The lyric panel renders a small window around the current line. Broadcasts use
  timestamps as segment titles by default. Station editing exposes text-mode and
  crossfade overrides.
- Groups can include full stations or individual music tracks. Rust enforces track
  membership during preparation; each group retains separate station clocks. Removing
  tracks/stations cleans group references. Unknown saved extension fields survive edits.
- Settings currently expose accent, station density, reduced motion, recording
  progress, global music crossfade, random-station navigation and cache clearing.
  Existing settings not yet exposed remain in the configuration unchanged.

After `npm run build`, `tests/ui-electron.cjs` exercises the built Svelte UI with an
isolated mock backend and silent WAV fixture: tag/lyric display, one details request
per track, accent application, group editing/filtering, settings and cache clearing.
It saves `out/phase4-ui.png` for layout inspection. Run it with the same Electron
test-host command below, replacing `audio-electron.cjs` with `ui-electron.cjs`.

## Phase 5 desktop preview

- The tray offers restore, mini player, play/pause, previous/next station and Quit.
  Closing the main window keeps playing by default; disable this in Settings to
  exit on close. If tray creation fails, closing exits rather than leaving an
  inaccessible background process.
- Windows SMTC, Linux MPRIS and macOS media integration use Souvlaki. Native actions
  route to the same main playback engine; metadata and progress are updated from
  its state. OS seeking is intentionally ignored, matching read-only recording progress.
- Mini player is a separate 480×160 always-on-top window with snapshot updates,
  controls and volume. Drag its header to move it. Escape, its restore button, or
  closing the mini window returns to the main player. Its last position is saved
  and restored only if it still fits a monitor work area. It never owns audio.
- Main and mini windows have separate command capabilities: the mini cannot edit
  the library, select files or publish playback state. Window labels are also checked
  by the desktop command handlers. WebView2 browser options are inherited from the
  main window to prevent shared-profile environment mismatches.
- Settings can recover archive-era `Game/Station.mp3` and `Game/Station/track.mp3`
  layouts from an imported `root`. Recovery adds missing definitions only; it never
  overwrites edited stations or moves source files. Original keys and paths preserve
  group/artwork references. The scan runs without locking configuration for its duration.

Build the Windows preview with `npm run package` (or `npm exec tauri -- build --bundles nsis`).
The optimized executable is `target/release/radio-replay-desktop.exe`; the current-user
NSIS installer is under `target/release/bundle/nsis/`. These are unsigned preview builds.
They use the system WebView2 runtime; the installer can install that runtime if needed.
No Electron runtime is included. The Windows installer has not been installed automatically.

CI now configures Windows, Ubuntu and macOS compile/test jobs; a manual workflow run
also builds and uploads the Windows executable and installer. Linux/macOS jobs and
hardware media-key behavior still need validation on their target environments.

### Native Windows integration smoke

An opt-in `native-smoke` Cargo feature runs real Tauri/WebView2 windows, not an Electron
mock. It creates silent WAVs, sidecars and a separate fixture configuration; checks
asset playback, close-to-tray, mini controls, permission denial, tray creation and
native media-control initialization; writes `report.json`; and exits with a test result.
The harness and its test IPC handler are excluded from normal builds.

From `rewrite` in PowerShell:

```powershell
npm exec tauri -- build --debug --no-bundle --features native-smoke --config src-tauri/tauri.smoke.conf.json
$env:RADIO_REPLAY_NATIVE_SMOKE_DIR = Join-Path (Get-Location) 'out/native-smoke'
& .\target\debug\radio-replay-desktop.exe
```

The directory must be absolute and named `native-smoke`. Fixtures and the report
remain there for inspection. Browser smoke tests also verify mini rendering/control
forwarding and save `out/phase5-mini.png`.

`tests/audio-electron.cjs` additionally exercises the engine using real Chromium
Audio and AudioContext with generated silent WAVs. It uses the original checkout's
Electron development dependency solely as a test host; no Electron code ships in
the Tauri app. From this nested worktree on Windows, run:

```powershell
..\..\..\node_modules\.bin\electron.cmd tests/audio-electron.cjs
```

This smoke covers real decode, adjacent broadcast scheduling, pause/resume, crossfade
handoff and cleanup. It is not a substitute for codec and asset-protocol testing in
the packaged Tauri webview on each target platform.

This checkout branches from the committed Electron baseline. Uncommitted changes
in the original checkout were deliberately left there and must be included when
establishing feature parity. This is a local development fork, not a new GitHub repository.

## Preview 0.2: Electron-style radio UI

- Restores the Electron palette and bundled Oswald/DM Mono fonts, compact station
  cards, persistent searchable station lineup, and a dedicated on-air player.
  Returning to the library or selecting the current station preserves playback.
- The player displays a station banner/logo, track tags, synchronized lyrics,
  transport, volume and optional read-only progress. Arrow keys and mouse wheel
  tune stations; Space pauses, M mutes, and + / - adjust volume. Wheel tuning is
  throttled and does not intercept scrolling inside lyrics or form controls.
- Station editing opens a keyboard-accessible modal with a live preview and
  Essentials, Appearance and Advanced sections. Logo, banner image/color, station
  accent and optional logo rotation are editable. Native selection grants apply
  to banners as well as logos; imported images are scoped on library load.
- Name and visual edits preserve playback and prepared-media access. Changes to
  the playing station's files or playback options refresh its program. Editing
  another station leaves the current station's media permissions intact.
- Manage stations provides direct access to editing/deletion, and Settings now
  separates General, Appearance, Playback and Advanced preferences. Settings use
  an explicit Save button. Groups remain on their own management screen.
- A compact transport stays available while browsing other screens. The separate
  always-on-top mini window continues to control the same audio engine.

The UI smoke also checks player navigation without retuning, live editor preview,
saving without interrupted audio, and layout at 760×560. It saves `out/ui-home.png`,
`out/ui-player.png`, `out/ui-editor.png` and `out/ui-player-small.png`. Native smoke
checks the bundled fonts, restored image assets and actual Rust station save.
This is a UX parity pass; automatic banner color sampling and all original
appearance preferences are not yet exposed.

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
  Deletion removes group references but never deletes source media. Root-library
  reconstruction from legacy `root`, group editing, lyrics, metadata/cache,
  tray, media keys and mini player remain subsequent milestones.
- Existing numeric crossfade values are preserved as-is. Convert their units at
  the playback boundary once, rather than during every configuration load.
- The renderer has no general filesystem/shell capability. Rust grants exact selected
  audio paths for station saves and exact image files for the asset protocol. Artwork
  grants are restored from the saved library at startup. Imported missing media paths
  can be retained while editing. Audio access is granted only after Rust resolves a
  saved station key; the renderer cannot request an arbitrary audio path.
- Folder scans run off the UI thread, use deterministic filename order, skip symlinks
  and Windows reparse points, and fail visibly on unreadable directories or scan limits
  (64 levels / 50,000 audio files). Supported formats match Electron.

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
station's wall clock continues. Configuration changes/imports reset playback and clocks.

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
Webview codec support remains platform-dependent. Tuning static/delay effects,
random-station history, per-group playback and persistent metadata/duration caching
are still pending; previous/next currently follow the full visible station lineup.

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

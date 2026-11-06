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
  reconstruction from legacy `root`, group editing, playback, lyrics, metadata/cache,
  tray, media keys and mini player remain subsequent milestones. Playback buttons are disabled.
- Existing numeric crossfade values are preserved as-is. Convert their units at
  the playback boundary once, rather than during every configuration load.
- The renderer has no general filesystem/shell capability. Rust grants exact selected
  audio paths for station saves and exact image files for the asset protocol. Artwork
  grants are restored from the saved library at startup. Imported missing media paths
  can be retained while editing. No audio asset access is exposed yet.
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

This checkout branches from the committed Electron baseline. Uncommitted changes
in the original checkout were deliberately left there and must be included when
establishing feature parity. This is a local development fork, not a new GitHub repository.

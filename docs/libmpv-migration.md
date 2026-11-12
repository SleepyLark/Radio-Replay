# Native audio preview

This branch starts moving audio decoding and output from WebView2 into libmpv, while preserving the Svelte interface and Tauri shell. It is a development preview, not a release replacement. The application version remains 0.6.5; no new release tag is needed to test it.

## Run on Windows x64

Install the normal project prerequisites (Node.js 24+, Rust, and the Windows Tauri build tools described in the README). From this branch's checkout:

```powershell
npm ci
npm run setup:mpv
npm run desktop:mpv
```

The setup command downloads the official **mpv 0.42.0 Windows x64 LGPL** archive, verifies its pinned SHA-256 checksum, and extracts it into ignored `vendor/mpv/`. The runtime manifest is `runtime/mpv-windows-x64.json`. An unexpected checksum aborts extraction. The archive is cached in ignored `.mpv-cache/`.

On this development machine, the isolated checkout is:

```powershell
cd C:\Users\Skylark\source\repos\Radio-Replay\.rewrite\mpv
npm run desktop:mpv
```

The window title includes **libmpv Preview**. The first build may take several minutes. Do not run the stable and preview players at the same time when testing media keys.

The preview uses its own station settings and SQLite database under `%APPDATA%\com.local.radioreplay.mpv-preview`. Use **Manage Stations → Import library** to import `%APPDATA%\com.local.radioreplay.preview\stations.json`, or a library from another location. Importing references the existing audio files; it does not copy them. Preview edits do not modify the stable library.

`npm run desktop` still launches the existing Web Audio player. `npm run package` and the release workflow still package that player. Native runtime bundling and installer integration are pending; copying the preview executable alone is insufficient.

## Implemented

- Local MP3, M4A, AAC, OGG, WAV, and FLAC playback through a Rust-owned libmpv worker.
- Play/pause, mute with remembered volume, slider changes that unmute, native position/duration updates, and sequential track advancement with looping.
- Existing shuffled music and ordered broadcast station timelines, station switching, mini-player controls, desktop media-action routing, and file-problem notifications.
- Existing SQLite metadata/duration cache, with a paused native probe when cached or metadata-derived durations are unavailable.
- Cancellation of stale tune preparations and generation-checked stops, so an older asynchronous operation cannot stop a newer tune.

The previously reported `All Ur Luv.m4a` was tested directly: libmpv identified **ALAC**, loaded a duration of about 161.77 seconds, and advanced playback in a muted test. This verifies decoding and progression for that specific file; it is not an audible listening test of every supported format.

## Remaining before switching the default

- **Crossfades:** currently disabled in this preview, with a warning when configured.
- **Tuning static:** not yet routed through native audio.
- **Seamless transitions:** the worker advances at EOF; it does not yet preload and schedule gapless transitions. Brief gaps can occur.
- **Shipping:** bundle the runtime, preserve upstream licenses/notices and applicable source obligations, and test installer upgrades before publishing a native release.
- **Platforms and performance:** real native playback has been tested on Windows x64 only. RAM, CPU, and startup comparisons have not been measured. The pinned DLL alone is about **126 MB** uncompressed, so reduced distribution size is not an established benefit.
- **Network radio/M3U:** deliberately unavailable in this local-file stage. Add a distinct streaming station mode, URL validation, playlist resolution, reconnect behavior, and live-stream UI before enabling it.

## Design

`crates/audio` owns the dynamically loaded client API, its lifetime, and a bounded worker command queue. A libmpv handle stays on its creating thread. The client ABI follows the pinned upstream header, including platform-sized C types. `src-tauri/src/native_audio.rs` exposes prepared-station operations and controls to the main window; the mini player continues to send actions through the existing desktop bridge.

`ui/playback/native.ts` adapts native snapshots to the current UI and retains the station clock calculations. It does not create `Audio`, `AudioContext`, or browser audio URLs. The original Web Audio engine remains available through the default build; the Cargo `native-audio` feature explicitly selects libmpv.

The main window must prepare saved station files before loading them; native load checks those grants and file signatures. Local mode disables mpv configuration/scripts, external references, and network protocols, and forces known audio demuxers to reject disguised playlists. These restrictions are not an OS sandbox for native codecs. Library paths still need to exist and be readable by the current user, including mapped drives.

## Validation

```powershell
npm run check
npm run test:playback
npm run test:ui
cargo test --workspace --locked
cargo check -p radio-replay-desktop --features native-audio --locked
cargo run -p radio-replay-audio --example smoke -- vendor/mpv/libmpv-2.dll
```

The real-runtime smoke example generates silent WAV fixtures and checks duration, pause, seek, EOF, disguised-playlist rejection, worker advancement, stale-stop cancellation, and volume behavior. An optional second argument tests another local media file with output muted. Regular Rust tests do not require an installed libmpv runtime.

The opt-in `native-smoke` desktop harness can also be built with `native-audio` to exercise the actual Windows app, SQLite reopening, mini-player controls, main-window-only IPC permissions, and rejection of unprepared file paths. It is excluded from normal builds. CI compiles the native feature but does not yet download or execute the libmpv runtime.

One desktop validation run timed out waiting for the simulated file drop to open the station editor; the next two complete launches passed. The cause of that intermittent failure remains unconfirmed. It should be investigated if it reproduces during normal use.

Upstream runtime and API: [mpv 0.42.0 release](https://github.com/mpv-player/mpv/releases/tag/v0.42.0), [pinned client header](https://github.com/mpv-player/mpv/blob/v0.42.0/include/mpv/client.h).

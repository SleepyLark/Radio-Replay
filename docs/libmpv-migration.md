# Native audio

libmpv is the default audio engine. Rust owns decoding, output, tuning static, preloading, and crossfades; Svelte and WebView2 render the interface. The [package manifest](../package.json) is the source of the current development version.

## Development and releases

From the repository root:

```powershell
npm ci
npm run desktop
npm run package
```

On Windows x64, the normal launch/package commands verify or download the official mpv 0.42.0 LGPL runtime into ignored `vendor/mpv/`. `npm run setup:mpv` performs the same setup explicitly. The archive and extracted DLL are checked against the SHA-256 hashes in `runtime/mpv-windows-x64.json`. Only the DLL and upstream license are extracted. An unchanged runtime is reused without downloading it again.

The Windows installer and portable ZIP both include `audio/libmpv-2.dll`, its LGPL license, and runtime/license/source information. Extract the complete portable ZIP and run the executable; no audio setup command or subsequent audio download is needed. The download helper is only a developer build tool. WebView2 remains a UI prerequisite and may be downloaded by the installer if missing. See `runtime/README.txt` and `runtime/SOURCES.txt` for licensing and source provenance.

Installed/portable apps look for `audio/libmpv-2.dll` beside their resources; debug builds can fall back to `vendor/mpv/libmpv-2.dll`. `RADIO_REPLAY_MPV_LIBRARY` overrides the path, including for compatible replacement libraries. Checksums are enforced during official runtime setup, not while loading a user-provided replacement. A failed startup shows a runtime message while leaving library editing available. macOS/Linux need a compatible system libmpv client API 2 runtime or this environment override; only Windows x64 is packaged and playback-tested.

The app retains the existing `%APPDATA%\com.local.radioreplay.preview` station settings and SQLite cache. The experimental branch previously used `com.local.radioreplay.mpv-preview`; those files are preserved. If you edited that experimental library, use **Manage Stations → Import library** to import its `stations.json` explicitly. No app-data directories are removed during repository cleanup.

## Implemented

- Local MP3, M4A, AAC, OGG, WAV, and FLAC playback, including ALAC in M4A.
- Play/pause, volume/mute restored across launches, volume changes that unmute, native position/duration updates, and track advancement with looping.
- Shuffled music and ordered broadcast station timelines, station switching, mini-player controls, desktop media-action routing, and file-problem notifications.
- SQLite metadata/duration caching and event-driven native probes when durations are unavailable. Probes keep the worker available for controls and static playback.
- Generation-checked cancellation so an old tune or stop cannot affect a newer session.
- Looping bundled/custom tuning static, minimum tuning delays, and cleanup on completion/cancellation. Pause and mute affect static too.
- Next-track preloading and native equal-power crossfades. Both decks pause together; handoff preserves the incoming playback position. Fades are limited to half the shortest prepared recording.
- Failed preloads are reported and skipped while the current recording continues. The secondary context is released on stop or retune.

## Practical limits

Zero-fade stations hand off to a preloaded decoder at EOF, but separate output contexts and worker timing do not guarantee sample-accurate gapless playback. Subjective quality and device latency still need broader listening tests.

The user reported approximately 236 MiB → 92 MiB private working set for the two measured app/WebView2 processes. This is an encouraging snapshot, not a controlled benchmark of every child process or long-session peak. The pinned DLL is about 126 MB uncompressed; file size is distinct from resident RAM.

Local M3U/M3U8 imports and direct public HTTP(S) network stations are supported.
Network stations use guarded streaming, display unknown duration when necessary,
and do not use local-station virtual seeking, crossfades or bumpers. Remote playlist
fetching, HLS and nested playlist playback remain disabled. See the
[local playlist](local-playlists.md) and [network station](network-playlists.md) guides.

## Design and validation

`crates/audio` contains the dynamic client-API boundary, bounded command queue, and worker state machine. Handles stay on their creating thread. At most two contexts are used: the primary plays or probes station audio; the secondary plays static, then preloads a recording. Audio streams through mpv rather than whole-recording browser buffers. Per-context demuxer cache limits are not total-process RAM limits.

`src-tauri/src/native_audio.rs` validates saved/prepared file identity and exposes main-window-only commands. Mini-player actions use the existing desktop bridge. Local mode disables mpv config/scripts, external references, and network protocols, and forces known audio demuxers to reject disguised playlists. These restrictions are not an OS sandbox for native codecs.

`ui/playback/native.ts` adapts snapshots to the UI and retains virtual station clock calculations without creating browser Audio/AudioContext objects. `ui/playback-session.ts` owns backend startup, browser-media routing and cleanup; `ui/desktop-snapshot.ts` builds the mini/media projection. The previous Web Audio engine is retained for regression comparisons only. Select it using `tauri.web.conf.json` and pass Cargo's `--no-default-features` after the Tauri CLI's `--`; see [development validation](development.md).

```powershell
npm run check
npm run test:playback
npm run test:ui
cargo test --workspace --locked
cargo check -p radio-replay-desktop --no-default-features --locked
cargo run -p radio-replay-audio --example smoke -- vendor/mpv/libmpv-2.dll
$env:RADIO_REPLAY_MPV_LIBRARY = Join-Path $PWD 'vendor/mpv/libmpv-2.dll'
cargo test -p radio-replay-audio --locked -- --ignored --skip real_public_http_stream
```

The explicit native test uses silent fixtures to check looping static during probes, tuning delay, failed preload skipping, actual mpv volume properties, pausing both decks, crossfade handoff, zero-fade preloading, cancellation, and cleanup. Windows CI uses timed null output; local validation also uses the Windows output device. The override is available in audio unit tests and explicit debug `native-smoke` builds. Normal/release libraries ignore it, verified by a separate integration test.

The opt-in native desktop harness checks actual libmpv/static playback, SQLite reopening, mini-player IPC, file authorization, startup preferences and absence of browser audio fallback. Isolated missing-runtime and comparison-backend runs cover startup failure and persisted volume/mute. See [development validation](development.md) and the [audit progress](pre-1.0-audit-progress.md) for verified checks and remaining release work.

## Generated files

`npm run clean -- -WhatIf` previews removal of root build output and local caches; `npm run clean` removes them. Source, `node_modules`, the active developer audio runtime, release deliverables, and application data are preserved. Subsequent Rust builds take longer after removing `target/`.

Upstream: [mpv 0.42.0](https://github.com/mpv-player/mpv/releases/tag/v0.42.0), [client header](https://github.com/mpv-player/mpv/blob/v0.42.0/include/mpv/client.h).

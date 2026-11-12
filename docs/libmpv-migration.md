# Native audio

libmpv is the default audio engine on `main`. Rust owns decoding, output, tuning static, preloading, and crossfades; Svelte and WebView2 continue to render the interface. The project remains version 0.6.5.

## Development and releases

From the repository root:

```powershell
npm ci
npm run desktop
npm run package
```

On Windows x64, the normal launch/package commands verify or download the official mpv 0.42.0 LGPL runtime into ignored `vendor/mpv/`. `npm run setup:mpv` performs the same setup explicitly. The archive and extracted DLL are checked against the SHA-256 hashes in `runtime/mpv-windows-x64.json`. Only the DLL and upstream license are extracted. An unchanged runtime is reused without downloading it again.

The Windows installer includes the setup scripts and downloads the runtime directly from its official upstream release after installing the app. The portable release is a ZIP containing the executable and the same setup scripts: extract the complete ZIP and run `Install-AudioRuntime.cmd` once. Neither app package embeds or redistributes the runtime DLL. Initial setup needs internet (about 54 MB for the audio archive); local playback then works offline. See `runtime/README.txt` for source/license provenance and offline setup using a transferred archive.

Installed/portable apps first look for `audio/libmpv-2.dll` beside their resources; local builds can use `vendor/mpv/libmpv-2.dll`. `RADIO_REPLAY_MPV_LIBRARY` overrides the path, including for compatible replacement libraries. Checksums are enforced during official runtime setup, not while loading a user-provided replacement. macOS/Linux need a compatible system libmpv client API 2 runtime or this environment override; only Windows x64 is packaged and playback-tested.

The app retains the existing `%APPDATA%\com.local.radioreplay.preview` station settings and SQLite cache. The experimental branch previously used `com.local.radioreplay.mpv-preview`; those files are preserved. If you edited that experimental library, use **Manage Stations → Import library** to import its `stations.json` explicitly. No app-data directories are removed during repository cleanup.

## Implemented

- Local MP3, M4A, AAC, OGG, WAV, and FLAC playback, including ALAC in M4A.
- Play/pause, remembered mute volume, volume changes that unmute, native position/duration updates, and track advancement with looping.
- Shuffled music and ordered broadcast station timelines, station switching, mini-player controls, desktop media-action routing, and file-problem notifications.
- SQLite metadata/duration caching and event-driven native probes when durations are unavailable. Probes keep the worker available for controls and static playback.
- Generation-checked cancellation so an old tune or stop cannot affect a newer session.
- Looping bundled/custom tuning static, minimum tuning delays, and cleanup on completion/cancellation. Pause and mute affect static too.
- Next-track preloading and native equal-power crossfades. Both decks pause together; handoff preserves the incoming playback position. Fades are limited to half the shortest prepared recording.
- Failed preloads are reported and skipped while the current recording continues. The secondary context is released on stop or retune.

## Practical limits

Zero-fade stations hand off to a preloaded decoder at EOF, but separate output contexts and worker timing do not guarantee sample-accurate gapless playback. Subjective quality and device latency still need broader listening tests.

The user reported approximately 236 MiB → 92 MiB private working set for the two measured app/WebView2 processes. This is an encouraging snapshot, not a controlled benchmark of every child process or long-session peak. The pinned DLL is about 126 MB uncompressed; file size is distinct from resident RAM.

Network streams and M3U playlists remain disabled. A future streaming mode needs URL/playlist handling, reconnect behavior, buffering, and a live-stream UI.

## Design and validation

`crates/audio` contains the dynamic client-API boundary, bounded command queue, and worker state machine. Handles stay on their creating thread. At most two contexts are used: the primary plays or probes station audio; the secondary plays static, then preloads a recording. Audio streams through mpv rather than whole-recording browser buffers. Per-context demuxer cache limits are not total-process RAM limits.

`src-tauri/src/native_audio.rs` validates saved/prepared file identity and exposes main-window-only commands. Mini-player actions use the existing desktop bridge. Local mode disables mpv config/scripts, external references, and network protocols, and forces known audio demuxers to reject disguised playlists. These restrictions are not an OS sandbox for native codecs.

`ui/playback/native.ts` adapts snapshots to the UI and retains virtual station clock calculations without creating browser Audio/AudioContext objects. The previous Web Audio engine is retained for regression comparisons only; compile the desktop with `--no-default-features` to select it.

```powershell
npm run check
npm run test:playback
npm run test:ui
cargo test --workspace --locked
cargo check -p radio-replay-desktop --no-default-features --locked
cargo run -p radio-replay-audio --example smoke -- vendor/mpv/libmpv-2.dll
$env:RADIO_REPLAY_MPV_LIBRARY = Join-Path $PWD 'vendor/mpv/libmpv-2.dll'
cargo test -p radio-replay-audio --locked -- --ignored
```

The explicit native test uses silent fixtures to check looping static during probes, tuning delay, failed preload skipping, actual mpv volume properties, pausing both decks, crossfade handoff, zero-fade preloading, cancellation, and cleanup. Windows CI runs it with a timed null output; local validation also uses the Windows output device. The null-output override is compiled only into the audio unit-test binary.

The opt-in native desktop harness checks actual libmpv/static playback, SQLite reopening, mini-player IPC, file authorization, and absence of browser audio fallback. See [development validation](development.md). An earlier run intermittently timed out opening the editor after a simulated file drop; later runs passed, so that failure's cause remains unconfirmed.

## Generated files

`npm run clean -- -WhatIf` previews removal of root build output and local caches; `npm run clean` removes them. Source, `node_modules`, the active developer audio runtime, release deliverables, and application data are preserved. Subsequent Rust builds take longer after removing `target/`.

Upstream: [mpv 0.42.0](https://github.com/mpv-player/mpv/releases/tag/v0.42.0), [client header](https://github.com/mpv-player/mpv/blob/v0.42.0/include/mpv/client.h).

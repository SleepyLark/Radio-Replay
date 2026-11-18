# Development validation

## UI responsibilities

- `ui/library.ts` orders mutation IPC; `ui/library-update.ts` merges compact
  replies while preserving unrelated objects.
- `ui/playback-session.ts` creates/disposes the selected backend and browser
  media bridge, including cancellation during lazy comparison-engine loading.
- `ui/playback/native.ts` adapts native snapshots and station clocks;
  `ui/playback/engine.ts` is the comparison Web Audio engine.
- `ui/desktop-snapshot.ts` builds mini/media snapshots;
  `ui/desktop-publisher.ts` coalesces their IPC publication.

`App.svelte` keeps reactive view/selection state and connects these modules.
It does not own decoder or browser-media cleanup. The native frontend build
still excludes the comparison engine and duplicate audio asset.

## Checks

Run from the repository root after `npm ci`:

```powershell
npm run check:version
npm run check
npm run test:playback
npm run test:ui
npm run test:audio
cargo fmt --all -- --check
cargo test --workspace --locked
cargo check -p radio-replay-desktop --features native-smoke --locked
npm run setup:mpv
$env:RADIO_REPLAY_MPV_LIBRARY = Join-Path $PWD 'vendor/mpv/libmpv-2.dll'
cargo test -p radio-replay-audio --locked -- --ignored --skip real_public_http_stream
```

Browser integration uses installed Microsoft Edge. If it is missing, run
`npx playwright install msedge`. `RADIO_REPLAY_BROWSER` can select another
installed Playwright-supported channel. The tests serve fixtures locally and do
not edit your real station library.

## Isolated native integration (Windows)

Use the reusable runner from PowerShell 7.3 or newer:

```powershell
npm run test:native
# Use cached dependencies/runtime and timed decoding without a sound device:
npm run test:native -- -Offline -NullOutput
```

The runner always builds the explicit `native-smoke` feature/config, verifies
the pinned DLL, and copies the executable/runtime into a unique directory under
`out/native-integration/`. It uses synthetic profiles named `native-smoke`,
restores its environment variables, and kills a process that exceeds its deadline.
Reports and stdout/stderr logs are retained, including on failure. `-Offline`
requires cached Cargo dependencies and a verified `vendor/mpv` runtime.

The npm command runs twice against one profile to verify cache/preferences across
restarts, then tests a missing DLL with a separate profile. It checks playback,
grant retention/revocation, stale-lineup rejection, main/mini IPC, tray/media
controls and exit-on-close. `-NullOutput` selects timed decoding without a sound
device only in unit tests or explicit debug smoke builds; normal/release builds
ignore that environment hook. WebView2 and working native window/media APIs are
still required. Restore the ordinary debug executable afterwards with
`npm exec tauri -- build --debug --no-bundle -- --locked`.

`.github/workflows/native-smoke.yml` runs this on manual dispatch and nightly,
with reports/logs uploaded even after a failure. Its local equivalent is verified;
hosted-runner behavior still needs a GitHub run after these changes are pushed.

The coarse large-library guard is separate from ordinary tests:

```powershell
cargo test -p radio-replay-core --locked large_library_budget -- --ignored --nocapture
```

It creates 50,000 temporary on-disk entries and selects 25,000 through a group.
Fixture creation/cleanup are untimed. A save has a five-second budget and grouped
file inspection has ten seconds; these catch large regressions and are not
tuning-latency targets. The nightly workflow also runs this guard.

The default desktop build uses libmpv. The WebView2 UI is retained; `npm run test:audio`
exercises the old Web Audio implementation only for regression comparisons. A
desktop comparison build selects that engine with `--no-default-features` and
`--config src-tauri/tauri.web.conf.json`. That explicit configuration grants the
legacy audio commands and asset media/fetch access, which the native build omits.
When building with the Tauri CLI, put Cargo's `--no-default-features` after `--`.
Tauri packaging runs `npm run build:native`. That build excludes the legacy
engine and the duplicate browser MP3 asset, and checks the resulting artifacts.
`npm run build` retains a lazy Web Audio comparison chunk; `npm run test:ui`
checks that its engine stays out of the startup chunk. Direct Cargo builds use
the existing `dist`, so run the appropriate frontend build first. Development
selects the backend at runtime and imports the comparison engine only for `web`.

Native preloading uses a ten-second preparation margin beyond the fade window.
`PRELOAD_MARGIN` in `crates/audio/src/runtime.rs` is the source tuning knob for
storage profiling; the next-file load timeout includes additional headroom.
This keeps one silent auxiliary context and does not preload network streams.
The default main/mini windows can listen to Rust events but cannot emit them.
The native smoke config intentionally overgrants three library commands to the
mini window so the harness can verify the independent Rust window-label guards;
those grants are absent from ordinary builds.

`npm run clean -- -WhatIf` previews root cache/build cleanup. `npm run clean`
removes `target`, frontend/test output, and local build/download caches while
preserving source, dependencies, the developer audio DLL, release deliverables,
and application data. The next Rust build will take longer.

The main-window close handler reads an atomic copy of `closeToTray`, initialized
at startup and refreshed after successful settings saves, library reloads,
imports and recovery. It does not acquire the library lock or read config to
decide whether to hide. Manual config edits take effect after reloading the
library or restarting. A rejected settings save retains the previous behavior.

Metadata cache opening retries once after SQLite reports corruption or a
non-database file. Recovery closes the connection and preserves the database
and remaining WAL/SHM files together in a unique `metadata-cache-corrupt-*`
directory. Other open errors leave the files in place and permit uncached reads;
Clear metadata cache can retry later. Creation-time signatures use explicit
epoch nanoseconds on Windows; existing Windows cache rows may need one new
measurement after this change. No station or group path migration is involved.

Folder scans return `ScanReport { tracks, notices }`. Notices keep exact skipped
folder/entry counts and at most 50 path/reason details across a selection. Root
read errors, the 64-level nesting limit and the existing track caps remain fatal;
descendant enumeration, metadata and canonicalization failures are skipped.
Symlinks, Windows reparse points, Extras folders and unsupported files retain
their intentional exclusions and are not counted as failures. Picked audio and
drops grant only the successfully collected tracks. A cancelled picker returns
`null`; an empty partial scan returns its notices. Archive recovery returns its
config with scan notices, and bumper warnings use the non-removable `scan` kind.

Library schema 2 gives global/station crossfades the same finite 0–30,000 ms
range (a null station override inherits). `Config::parse` upgrades schema 1 and
unversioned libraries, converting the old `0 < value <= 30` seconds heuristic
once; playback reads milliseconds directly. The next changed save writes schema
2 through the existing backup/atomic-write path. Current-format small values
such as 20 mean 20 ms and are not converted on reopen.

`AudioPreferences` loads a versioned, at-most-4-KiB `audio-preferences.json` and
queues validated scalar changes to a dedicated writer. A fixed 250 ms window
coalesces bursts; unchanged levels cause no disk writes. Writes use the library's
atomic-file helper and an exit-event flush waits up to five seconds. This avoids
station-library cloning, serialization and backup rotation on volume changes.
Unreadable/future files are retained and writes are refused while default live
levels remain usable; startup reports that condition. Write failures retain live
levels and retry on a later change or flush.

Native controls persist the worker's acknowledged volume/mute. The comparison
backend persists initialized desktop snapshots; pre-initialization snapshots
cannot replace saved levels. `playback_backend` now returns backend, initial
levels and startup diagnostics. Its read-only native status control waits for
the actual worker initialization without tuning. A failed runtime leaves the
library usable and tuning disabled until restart. Windows release builds resolve
the bundled DLL beside the app; the vendor fallback is limited to debug builds.

For packaging-only edits, `pwsh -File scripts/package-windows.ps1 -BundleOnly`
can rebundle an existing matching `target/release/radio-replay-desktop.exe`
without compiling Rust again. Use the normal package command whenever the
application code has changed. Both paths verify that the portable ZIP contains
the expected audio DLL and licenses, and excludes the old end-user downloader.

For the first large-library optimization measurements and remaining audit work,
see [performance pass 1](performance-pass-1.md).

# Large-library fixes: first audit pass (0.3.6)

This pass addresses audit findings 1.4, 2.4, 2.5, 2.7, the argument-limit portion
of 2.8, and the raw-state/active-station portion of 2.6.

- Station updates and group filtering use sets for membership checks. Selection
  checks in group and management dialogs do too; bulk track removal uses a set.
- The file editor initially opens its first folder and renders 100 files. Other
  folders render their rows when expanded. Files are added in batches of 100;
  folder headers are added in batches of 50. Group track lists start collapsed
  and render in batches of 100. Closing a section releases its track rows.
  Selections and removal operations still cover the complete backing data.
- Configuration, playback snapshots and editor tracks use raw replacement state.
  Station rows derive the active key separately from progress updates.
- Zero-crossfade streaming reuses the preloaded element when available. An
  unready preload still falls back to loading. This removes a redundant reload;
  it does not promise sample-accurate gapless playback for every codec.
- Buffered playback estimates stereo float PCM using the context sample rate
  before fetching or decoding. A track estimated above the 64 MiB budget streams
  instead; prefetch also checks capacity against the 128 MiB retained-buffer
  budget. Mono may conservatively stream even when it would fit; actual channel
  counts and total retained buffers are still checked after decoding.
- Station-clock construction uses a loop instead of spreading an unbounded
  duration array into function arguments. A 200,000-entry regression covers it.

## Synthetic measurements

Windows, debug Rust build, individual local runs. These isolate membership work;
they exclude media I/O, IPC and config serialization. They are not click-to-sound
or release-build performance claims.

| Operation | Before, 3,000 tracks | After, 3,000 tracks | After, 20,000 tracks |
| --- | ---: | ---: | ---: |
| Filter against equally many nonmatching group members | 2,072 ms | 5.7 ms | 36.7 ms |
| Update a station retaining its tracks | 58.9 ms | 7.5 ms | 55.6 ms |

Reproduce the algorithm measurement from the repository root:

```powershell
cargo run -p radio-replay-core --example library_benchmark -- 3000
cargo run -p radio-replay-core --example library_benchmark -- 20000
```

The browser UI suite includes a synthetic 20,000-track configuration. It verifies
bounded initial rendering, expansion, pagination, folder removal, cancellation,
and saving selections that were hidden by collapsing a section. Audio regression
tests cover preloaded handoff, unready fallback and skipping oversized decodes;
the real-audio suite verifies preloaded handoff in the installed browser runtime.

Validation passed: 35 JavaScript tests, 26 Rust core tests, Svelte/TypeScript
checking, the production frontend build, browser UI/audio integration, and the
isolated native WebView2/SQLite/mini-player/tray integration harness.

## Remaining work

Cold duration indexing, whole-library persistence and IPC responses, periodic
publishing/timer cadence, SQLite capacity/maintenance and cache locking, asset
grant lifecycle, large clock-signature strings, and the P3 hardening items remain
for subsequent passes. No real-library cold/warm tuning, idle CPU or process RAM
baseline has been collected in this pass.

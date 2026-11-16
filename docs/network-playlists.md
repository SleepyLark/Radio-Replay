# Network playlist preview

Branch: `codex/network-playlists`, based on main after local playlist support.
The preview uses `com.local.radioreplay.network-preview` for separate application
data. Restore the intended release identifier before merging/publishing.

## Use it

```powershell
cd C:\Users\Skylark\source\repos\Radio-Replay\.rewrite\network
$env:CARGO_TARGET_DIR = (Resolve-Path ..\bumpers\target).Path
npm run desktop
```

Create a station, choose **Import playlist**, and select a local M3U/M3U8 containing
HTTP(S) audio URLs. The editor selects **Network station** for an empty station.
Review the import and save. A playlist remains one station, including playlists
with multiple live feeds or alternative relays. Reopening an already-playing
station preserves its current feed; tuning away and back creates a new shuffle.
Importing/preparing does not connect to the stream
hosts. Connecting begins when you tune in; importing is not a background scan.

Network entries are shuffled into a fresh order on each tune. Finite audio URLs
advance through that order at EOF. A live URL plays
until you stop, retune, or its connection ends. Pause/resume and volume/mute use
the same native player. Unknown total duration is shown as **STREAM** in the main
and mini players. Playlist `#EXTINF` names supply titles; duration estimates are
not treated as authoritative. No remote audio or durations are saved to disk.

A minimal live-stream playlist is:

```m3u
#EXTM3U
#EXTINF:-1,Nonstop Casiopea
https://nonstopcasiopea.radioca.st/;
```

The supplied `ggs.m3u` imported all 132 entries without fetching audio. Both its
first MP3 and the Casiopea live feed were tested through native playback.

## Supported in this phase

- Direct HTTP/HTTPS audio endpoints, including endpoints without extensions.
- MP3, AAC, OGG, FLAC, WAV, and streamable M4A/MOV containers. Containers that
  require arbitrary byte seeking (for example some non-fast-start M4A files)
  may fail in this forward-only preview.
- At most 1,000 entries per network station. Local stations retain their
  existing limits and playback behavior.
- Retuning starts a fresh connection at the beginning/current live feed. There
  are no virtual clocks, seeking, preloading, crossfades, bumpers or remote
  sidecars for network stations. Streams are not opened just to measure length.
- Failed entries are reported and skipped, with at most three failures attempted
  per tune. There is no infinite automatic reconnect loop; retuning retries.

Keep local audio and network URLs in separate stations. Mixed playlists are
rejected with an explanation. HLS/DASH, nested/remote playlist fetching, webpage
extraction, interleaved ICY metadata, authentication, direct URL input, and
private/LAN radio servers are not implemented. The existing Web Audio comparison
build explicitly rejects network stations; use the default libmpv build.

## Security boundary

The decoder never receives an Internet URL. Rust connects and supplies a bounded,
read-only custom stream through libmpv's
[stream callback API](https://github.com/mpv-player/mpv/blob/master/include/mpv/stream_cb.h).
Local audio retains its file-only protocol restriction. Network custom IO permits
only the expected audio demuxers; nested protocols, scripts, yt-dlp, and external
media references remain disabled. Playlist options are never executed.

The transport enforces:

- HTTP(S) only; no URL credentials, fragments, raw whitespace/control characters,
  ambiguous backslashes, or port zero. URLs are bounded at 8 KiB.
- Public destinations only. Loopback, private, link-local, multicast,
  documentation and common special/transition address ranges are blocked for
  IPv4 and IPv6. Every DNS answer is checked, and the actual HTTP connection is
  pinned to the checked addresses, preserving the original TLS hostname. DNS
  changes cannot substitute an unchecked destination between validation and
  connection.
- Redirects are manual, limited to four, and repeat URL/DNS/address checks.
  HTTPS-to-HTTP downgrades are rejected.
- Explicit Rustls certificate validation. No proxy inheritance, cookies,
  Authorization/Referer headers, or automatic HTTP content decompression.
  HTTP is still plaintext: prefer HTTPS where the station offers it.
- Five-second DNS/connect limits and a ten-second idle read limit; cancellation
  checks approximately every 25 ms. The native startup watchdog also applies.
- A four-slot, 32 KiB-per-chunk queue with backpressure, alongside the existing
  bounded mpv demuxer buffers. At most four network readers may exist while old
  connections/DNS operations wind down. Playback does not wait for a complete song download; there is no next-song prefetch.
- Playback requires a URL from the saved, prepared station. The renderer cannot
  load arbitrary URLs through the native IPC endpoint.

This is defense in depth, not a decoder sandbox or a promise of zero risk.
FFmpeg/libmpv still parses untrusted audio in the application process and needs
security updates. A station server also sees the listener's IP address and
requested URL, as with other Internet radio clients. URL query tokens, if used,
are part of the saved station configuration; avoid sharing that file publicly.

## Validation

Regular workspace tests are offline. They cover URL/IP classification, mixed
DNS answers, redirect policy, pinned local HTTP fixtures with automatic redirects
and ambient credentials disabled, cancellation, import/save grants, and network
adapter behavior. Real-decoder fixtures exercise finite queue handoff, pause,
stop, and rejection of a disguised playlist. Existing local/static/crossfade and
bumper regressions remain in the native test suite.

The opt-in native smoke harness passed with the supplied live feed through real
Tauri IPC, including prepared-URL enforcement, unknown durations, and pause/resume.
Its ordinary run never contacts external servers. To opt in, set
`RADIO_REPLAY_NETWORK_SMOKE_URL` before running the harness described in
[development validation](development.md).

External decoder checks are deliberately separate from CI:

```powershell
$env:RADIO_REPLAY_MPV_LIBRARY = Join-Path $PWD 'vendor/mpv/libmpv-2.dll'
$env:RADIO_REPLAY_AUDIO_TEST_NULL_OUTPUT = '1'
$env:RADIO_REPLAY_NETWORK_TEST_URL = 'https://nonstopcasiopea.radioca.st/;'
cargo test -p radio-replay-audio real_public_http_stream --locked -- --ignored
```

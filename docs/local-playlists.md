# Local playlist import (phase 1)

This preview lives on `codex/local-m3u`, based on main including the bumper work.
Its application identifier is `com.local.radioreplay.playlists-preview`, so its
library, preferences and metadata cache are separate from other builds. Restore
main's intended release identifier before merging or packaging a normal release.

## Try it

From this worktree, run `npm run desktop`. To reuse the existing development
Rust cache rather than create another one, use:

```powershell
cd C:\Users\Skylark\source\repos\Radio-Replay\.rewrite\m3u
$env:CARGO_TARGET_DIR = (Resolve-Path ..\bumpers\target).Path
npm run desktop
```

Create or edit a station and choose **Import playlist** beside Add files / Add
folder. Select a local `.m3u` or `.m3u8` file. The valid audio entries are appended
in playlist order; an empty station name is filled from the playlist filename.
Review the files and import report, then **Save station**. Cancelling the editor
discards those changes. Importing does not start playback or overwrite the source
playlist, and does not copy/move the audio files.

The existing station playback choices still apply: **Music station** shuffles;
**Radio broadcast** plays the saved file order. Import does not change that
choice. An independent ordered-music playback style is outside this phase.

## Compatibility

- Plain and extended M3U, including a UTF-8 BOM and Windows line endings.
- Relative file references resolved against the playlist's folder, absolute
  filesystem paths, and `file:` URLs appropriate to the current platform.
- Percent-escaped relative URLs such as `Music/First%20Song.mp3`, as produced by
  VLC. An existing literal filename is preferred when it also contains `%`.
- MP3, M4A, AAC, OGG, WAV and FLAC, using the existing audio engine and cache.
- `#EXTINF` display names as filename fallbacks; embedded metadata keeps its
  normal precedence. Playlist duration estimates are not used for scheduling.
- UTF-8 for M3U8; UTF-8 or a reported Windows-1252 fallback for legacy M3U.
  UTF-16 and other legacy encodings should be re-exported as UTF-8 M3U8.

M3U8 is recommended for new exports to keep non-English filenames unambiguous.
VLC's [playlist exporter](https://github.com/videolan/vlc/blob/master/modules/misc/playlist/m3u.c)
writes relative escaped references as well as full file URLs; this importer
handles both without interpreting any VLC playback directives.

The report lists missing/inaccessible files, unsupported files, duplicate entries,
and network URLs by source line. Duplicate files retain their first occurrence,
matching the existing rule that each station contains a path once. Entries
already in the station are also omitted and counted. Empty/all-invalid playlists
add no files and remain reviewable; HLS manifests are rejected with a specific
explanation because their segments are not standalone songs.

Imports are snapshots. Editing the M3U later does not automatically update a
station. Nested playlists are reported rather than expanded. Import currently
uses the explicit playlist picker, not drag-and-drop or OS file associations.

## Scope and limits

For the subsequent network implementation, see [network playlist preview](network-playlists.md). The local-only scope below describes phase 1.

No HTTP/HTTPS playlists, URL fetching, network-stream playback, direct URL input,
HLS playback, new audio formats, or playlist exporting are added. These remain
separate phases requiring approval. Existing mounted/filesystem shares continue
to behave as ordinary audio file paths; this is not streaming URL support.

A playlist is limited to 4 MiB and 50,000 entries. Import reports retain at most
200 detailed notices while showing the full count. The Rust picker supplies the
playlist path; only successfully resolved audio files receive the existing
station-save grants. Renderer-supplied arbitrary playlist paths are not accepted.
Parsing and filesystem checks run off the UI thread. No extra audio workers or
expanded playback queues are created.

Validation covers real temporary playlist files, Windows paths and file URLs,
VLC-style references, encodings, titles/order, duplicates, mixed bad entries,
stream/HLS rejection, size bounds, and station-save grants. Browser integration
covers the editor's import/cancel/error/report/save flows; desktop compile checks
include the new main-window capability. The native audio engine is unchanged.

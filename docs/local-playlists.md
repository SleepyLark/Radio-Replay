# Playlist imports

Playlist import is available in the normal app and uses its established library.
No feature worktree or alternate application identifier is needed.

## Try it

Developers can launch from the repository root:

```powershell
npm run desktop
```

Create or edit a station and choose **Import playlist** beside Add files / Add
folder. Select a local `.m3u` or `.m3u8` file. The valid audio entries are appended
in playlist order; an empty station name is filled from the playlist filename.
Review the files and import report, then **Save station**. Cancelling the editor
discards those changes. Importing does not start playback or overwrite the source
playlist, and does not copy/move the audio files.

For local files, **Music station** shuffles and **Radio broadcast** plays the saved
file order. A network import into an empty station selects **Network station**.
Local files and network links use separate stations; mixed-source entries are
reported in the import notices. See the [network station guide](network-playlists.md).

## Compatibility

- Plain and extended M3U, including a UTF-8 BOM and Windows line endings.
- Relative file references resolved against the playlist's folder, absolute
  filesystem paths, and `file:` URLs appropriate to the current platform.
- Imported documents cannot authorize UNC, mapped network drives, or device
  paths. Those entries are skipped before media access; select NAS libraries
  explicitly with **Add folder** instead. Library JSON imports with such
  references are rejected without changing the existing library.
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

The report lists missing/inaccessible files, unsupported or mixed-source entries,
and duplicates by source line. Duplicate files retain their first occurrence,
matching the existing rule that each station contains a path once. Entries
already in the station are also omitted and counted. Empty/all-invalid playlists
add no files and remain reviewable; HLS manifests are rejected with a specific
explanation because their segments are not standalone songs.

Imports are snapshots. Editing the M3U later does not automatically update a
station. Nested playlists are reported rather than expanded. Import currently
uses the explicit playlist picker, not drag-and-drop or OS file associations.

## Scope and limits

The picker reads a local playlist file. Direct public HTTP(S) audio links can be
imported or entered manually in a network station, but fetching remote playlists,
HLS playback and playlist exporting are not supported. Network filesystem paths
inside imported documents are rejected; choose NAS files explicitly through the
file/folder picker instead.

A playlist is limited to 4 MiB and 50,000 entries. Import reports retain at most
200 detailed notices while showing the full count. The Rust picker supplies the
playlist path; only successfully resolved audio files receive the existing
station-save grants. Renderer-supplied arbitrary playlist paths are not accepted.
Parsing and filesystem checks run off the UI thread. No extra audio workers or
expanded playback queues are created.

Validation covers real temporary playlist files, Windows paths and file URLs,
VLC-style references, encodings, titles/order, duplicates, mixed bad entries,
mixed-source handling, HLS rejection, size bounds, and station-save grants. Browser integration
covers the editor's import/cancel/error/report/save flows; desktop compile checks
include the main-window capability and prepared-source authorization.

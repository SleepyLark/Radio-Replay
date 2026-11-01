# Radio Replay

A local desktop radio player inspired by GTA's station tuning. Bring your own audio: the project contains no music, station artwork, or game assets. Windows is the primary development target; Electron also supports macOS and Linux, though those builds have not been verified.

## Run locally

Requires Node.js and npm.

```sh
npm install
npm start
```

The app opens on a searchable station list. Use **Create station** to select audio files or a folder. Importing a folder fills the station name from that folder when the name field is blank. Choose **One station with all files** or **One station per file** independently of the playback style. Per-file stations use their filenames; add artwork afterwards in Edit station. Dragging audio files into the window opens the creator, and dropping more files into the open editor adds them to that station.

Use the **Music station** preset for shuffled tracks, or **Radio broadcast** for recordings that play in order and loop, including multiple long recordings in one station. The editor groups files into collapsible parent folders in a scrollable box. Numbered rows show the broadcast order; move files up or down to change that order. Use the folder **↑ / ↓** buttons to reorder sibling folders, including their subfolders. Use **⇈ First** on a folder to move all its files and subfolders to the start, keeping their relative order. Drop additional audio files anywhere in the station dialog to add them; duplicate paths are ignored. For folders, use **Add folder**. The creator checks a small sample for timestamped sidecars or recordings over 20 minutes and offers to use broadcast playback.

To make a portable Windows EXE:

```sh
npm run package:win
```

The output appears in `release/`. Builds are not committed to Git.

## Stations and groups

- **Manage Stations** lets you edit any station or select several for bulk removal. Deleting a station never deletes its source audio.
- **Edit station** has Essentials, Appearance, and Advanced sections, a live artwork preview, and Save/Cancel buttons that stay visible. Customize its name, files, logo, optional 3D logo rotation, display background image or color, accent override, playback style, timestamp meaning, and crossfade choice. In Settings → Appearance, choose a global station banner layout: column (logo above the name) or row (logo to the left). This applies to every station and its editor preview. Logos and backgrounds accept animated GIFs.
- Groups can contain whole stations or selected tracks from individual-track stations. Use the group selector above the sidebar to show only that group.
- A station starts at a random point on first tune in a session. Its timeline advances while you listen elsewhere without playing background audio. The timeline resets when the app restarts.

The app remembers the path to a library scanned by an earlier version and continues to load those stations. New stations are created through the UI.

## Settings and metadata

The **Settings** page has General, Appearance, Playback, and Advanced categories and saves changes automatically. Appearance includes station list spacing, scanlines, reduced motion, recording progress, title bounce speed, and the default accent. Enable **Match player accent to each station banner** to softly change the player and tray icon accent when tuning. The app samples an image's color when it is selected, or uses the station's background color. A station's explicit accent override takes priority. The included `assets/audio/static.mp3` is the default tuning sound; you can choose another file or disable the sound. In Edit station → Advanced, **Default crossfade** is checked by default; uncheck it to set that shuffled station's crossfade in milliseconds (0–30000 ms). Existing saved crossfade values remain compatible; the settings file continues to store seconds internally. A value of `0` disables crossfade for that station. Crossfade is capped at half the shortest track. Broadcast playback has no crossfade.

The segmented **VOL** meter controls volume; click or drag it, or scroll over it. The speaker button mutes, **+ / −** adjust volume, and **M** toggles mute. **↑ / ↓** and the keyboard media Next/Previous keys tune stations; **Space** or the keyboard media Play/Pause key plays or pauses. System media Next/Previous actions also tune stations while Radio Replay is the active media session. The thin progress line is read-only and shows the current recording position.

Enable **Settings → Playback → Choose a random station when pressing Next** to randomize forward station tuning, including the Next button, media Next key, down arrow, and downward tuning scroll. It chooses another station from the current group when possible. While random tuning is enabled, Previous retraces up to two successfully tuned stations, and Next retraces forward history before making another random choice. At the oldest retained station, Previous stays put. Directly choosing another station starts a new forward branch. History is session-only and stores station keys; failed or cancelled tunes do not add entries. Changing groups or toggling random tuning resets history, and unavailable or deleted history entries are skipped. With random tuning off, Next and Previous follow the normal list order. The option defaults off and saves automatically.

In **Settings → Playback → Tuning sound**, enable **Play a random static transition when tuning** for a brief radio-style transition even when a station loads immediately. Set minimum and maximum transition lengths from **0–1500 ms**. Each tune chooses a duration within that range and starts at a random point in your selected static file. Static keeps looping until the station is ready, including while building the duration cache, even if loading takes longer than the selected transition. This option defaults off, preserving loading-only static; existing one-second transitions default to 1000–1000 ms. Disabling the static sound bypasses the transition, and switching stations again cancels the previous transition.

The radio display can use a station image or color behind the scanlines and station name. The text has a shadow for readability. The Windows app icon and tray icon use `assets/icons/radio.svg`; run `npm run icons` after changing that source SVG to regenerate the icon assets.

Station definitions, groups, logos, and settings live in `stations.json` in Electron's user data directory. On Windows this is normally `%APPDATA%\radio-replay\stations.json`. Use **Settings → Open stations.json** to find the active file. Edit it while the app is closed; audio and artwork remain at their original paths. The app keeps the previous version as `stations.json.bak` and refuses to overwrite a config file it could not read. Stations imported by older versions under the “Custom” heading are displayed without that label.

Any audio file can have an `.lrc`, `.txt`, or `.json` sidecar in the same folder with the same basename. For example, `Song.mp3` pairs with `Song.lrc`. LRC lines use `[mm:ss.xx]Text`; JSON uses a `timed` array of `{ "at": seconds, "text": "Title or lyric" }` entries with nonnegative numeric timestamps. Plain text and malformed timestamps are ignored, and the display's height is bounded. Shuffled tracks show `Artist - Song Title` from embedded tags when available, with the album below it and timed lyrics underneath. Broadcasts use timed lines as the main song title for every recording in the station. Until the first timestamp, the embedded title or filename is shown. Override this under **Edit station → Advanced → Use timestamped text as** with Track titles, Lyrics, or Ignore sidecars. Existing station names, keys, files, and playback styles are preserved.

## Project layout

| Path              | Purpose                                                        |
| ----------------- | -------------------------------------------------------------- |
| `src/main.cjs`    | Electron window, file dialogs, library scanning, settings file |
| `src/preload.cjs` | Narrow bridge between the window and Electron                  |
| `src/renderer/`   | Player UI, styling, station clocks, and playback               |
| `package.json`    | Development and packaging commands                             |

The first tune still reads each uncached audio file's duration. Later tunes, including after restarting, reuse durations from a separate `duration-cache.sqlite` file beside `stations.json`. Before tuning, the app checks file size, modification time, and filesystem change time; changed files are measured again and missing files are skipped. Unchanged station timelines keep advancing. The cache contains hashed path keys and numeric metadata, with no audio or artwork. It uses indexed lookups, a 2 MiB SQLite page-cache budget, and batches of at most 256 pending writes. There is no application-imposed database size or entry-count cap; the database grows with the cached library. Temporary journal and cleanup files can use additional disk space. Access stamps are refreshed at most once a day, unused entries expire after 90 days, and editing the saved library removes unreferenced entries. Cache failures fall back to normal audio probing; a corrupt cache is removed and rebuilt on a later launch.

Session memory retains at most 32 station/group clocks with 20,000 track slots in total, plus the active station if it alone exceeds that budget. Evicted station clocks get a new timeline on their next tune. Embedded tag results are limited to 512 files. These budgets cover the caches; audio buffers and the loaded station library still use memory separately.

Use **Settings → Advanced → Clear duration cache** to discard saved and pending duration entries and reclaim database space. Your audio, stations, settings, and current playback are kept. Durations are measured again as needed, so the next tune may take longer. Cache preparation already in progress can populate fresh entries after clearing; measurements started before clearing cannot restore stale entries.

The development build has no auto reload; restart `npm start` after changing source files.

Run `npm run format` to format the source, `npm run format:check` to verify formatting, `npm run check` for JavaScript syntax checks, `npm run test:cache` for duration-cache persistence, invalidation, cleanup, and large-library checks, and `npm run smoke` for hidden Electron integration checks. The renderer code documents the session clock, crossfade handoff, and settings migration where those behaviors are implemented.

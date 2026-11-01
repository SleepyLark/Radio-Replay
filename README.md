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

The **Settings** page has General, Appearance, Playback, and Advanced categories and saves changes automatically. Appearance includes station list spacing, scanlines, reduced motion, recording progress, title bounce speed, and the default accent. Enable **Match player accent to each station banner** to softly change the player and tray icon accent when tuning. The app samples an image's color when it is selected, or uses the station's background color. A station's explicit accent override takes priority. The included `assets/audio/static.mp3` is the default tuning sound; you can choose another file or disable the sound. In Edit station → Advanced, **Default crossfade** is checked by default; uncheck it to set that shuffled station's crossfade in seconds. A value of `0` disables crossfade for that station. Crossfade is capped at half the shortest track. Broadcast playback has no crossfade.

The segmented **VOL** meter controls volume; click or drag it, or scroll over it. The speaker button mutes, **+ / −** adjust volume, and **M** toggles mute. **↑ / ↓** and the keyboard media Next/Previous keys tune stations; **Space** or the keyboard media Play/Pause key plays or pauses. System media Next/Previous actions also tune stations while Radio Replay is the active media session. The thin progress line is read-only and shows the current recording position.

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

The app reads file metadata when first tuning a station. Large individual-track stations may take a moment to prepare. The development build has no auto reload; restart `npm start` after changing source files.

Run `npm run format` to format the source, `npm run format:check` to verify formatting, `npm run check` for JavaScript syntax checks, and `npm run smoke` for a hidden Electron startup and parsing check. The renderer code documents the session clock, crossfade handoff, and settings migration where those behaviors are implemented.

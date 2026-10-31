# Radio Replay

A local desktop radio player inspired by GTA's station tuning. Bring your own audio: the project contains no music, station artwork, or game assets. Windows is the primary development target; Electron also supports macOS and Linux, though those builds have not been verified.

## Run locally

Requires Node.js and npm.

```sh
npm install
npm start
```

The app opens on a station list. Use **Create station** to select audio files or a folder. Importing a folder fills the station name from that folder when the name field is blank. Choose **Individual tracks** for one shuffled station, or **Continuous longset** to create a station for each file. Dragging audio files into the window also opens the station creator.

To make a portable Windows EXE:

```sh
npm run package:win
```

The output appears in `release/`. Builds are not committed to Git.

## Stations and groups

- **Manage Stations** lets you edit any station or select several for bulk removal. Deleting a station never deletes its source audio.
- **Edit station** changes its name, audio files, logo, optional 3D logo rotation, radio display background image or color, playback mode, and crossfade choice. Logos and backgrounds accept animated GIFs.
- Groups can contain whole stations or selected tracks from individual-track stations. Use the group selector above the sidebar to show only that group.
- A station starts at a random point on first tune in a session. Its timeline advances while you listen elsewhere without playing background audio. The timeline resets when the app restarts.

The app remembers the path to a library scanned by an earlier version and continues to load those stations. New stations are created through the UI.

## Settings and metadata

The **Settings** page groups appearance, playback, tuning sound, and file options. Enable **Match player accent to each station banner** to softly change the player and tray icon accent when tuning. The app samples an image's color when it is selected, or uses the station's background color. The included `assets/audio/static.mp3` is the default; you can choose another file or disable the sound. In Edit station, **Use global crossfade** is checked by default; uncheck it to set that station's crossfade in seconds. A value of `0` disables crossfade for that station. Crossfade is capped at half the shortest track in that station.

The radio display can use a station image or color behind the scanlines and station name. The text has a shadow for readability. The Windows app icon and tray icon use `assets/icons/radio.svg`; run `npm run icons` after changing that source SVG to regenerate the icon assets.

Station definitions, groups, logos, and settings live in `stations.json` in Electron's user data directory. On Windows this is normally `%APPDATA%\radio-replay\stations.json`. Use **Settings → Open stations.json** to find the active file. Edit it while the app is closed; audio and artwork remain at their original paths. The app keeps the previous version as `stations.json.bak` and refuses to overwrite a config file it could not read. Stations imported by older versions under the “Custom” heading are displayed without that label.

Any audio file can have an `.lrc`, `.txt`, or `.json` sidecar in the same folder with the same basename. Timestamped LRC lines update during playback, including when individual tracks change or crossfade. For example, `Song.mp3` pairs with `Song.lrc`. Individual tracks show `Artist - Song Title` from embedded tags when available, and the album tag below it. For a single-file continuous station, each timed LRC line becomes the main song title instead of reading embedded tags.

## Project layout

| Path              | Purpose                                                        |
| ----------------- | -------------------------------------------------------------- |
| `src/main.cjs`    | Electron window, file dialogs, library scanning, settings file |
| `src/preload.cjs` | Narrow bridge between the window and Electron                  |
| `src/renderer/`   | Player UI, styling, station clocks, and playback               |
| `package.json`    | Development and packaging commands                             |

The app reads file metadata when first tuning a station. Large individual-track stations may take a moment to prepare. The development build has no auto reload; restart `npm start` after changing source files.

Run `npm run format` to format the source, `npm run format:check` to verify formatting, `npm run check` for JavaScript syntax checks, and `npm run smoke` for a hidden Electron startup and parsing check. The renderer code documents the session clock, crossfade handoff, and settings migration where those behaviors are implemented.

# Bumper preview

Development branch: `codex/station-bumpers`. This worktree uses the separate
`com.local.radioreplay.bumpers-preview` application identifier, so testing does
not overwrite the main app's library, preferences, or metadata cache. Restore
the main identifier before merging or publishing a normal release.

From this worktree, run `npm run desktop`. In Manage Stations, Import library
can copy your existing `stations.json` into the preview. The source library
and audio files remain in place.

## Configure a station

Edit a **Music station**, expand **Bumpers**, enable clips, and choose a folder.
Supported audio files in that folder and its subfolders form the bumper pool;
symlinks/junctions and folders named `extra` or `extras` are skipped, matching
the existing music-folder scanner. The pool is limited to 1,000 clips.

- **Song count:** choose an inclusive minimum and maximum from 1–1,000 songs.
- **Listening time:** choose a minimum and maximum from 0.1–360 minutes.
  A target is sampled within that range, then the closest song ending is used.
  For 10 minutes and three-minute songs, the break follows song three.
  Actual break times can fall outside the requested range because songs finish
  naturally. Music crossfade overlap is subtracted from elapsed listening time.

Equal minimum and maximum values give a fixed target. A new target is chosen
after each bumper, and the clip's own duration is excluded. A bumper is chosen
at random, avoiding the previous clip when another healthy clip is available.
The current partial song counts as one song; time mode counts only its remaining
playing time. Bumpers play fully without crossfading at either end. Normal
music-to-music crossfades remain enabled.

Missing/unsupported clips produce the existing file-warning toast. A missing,
empty, or invalid bumper folder falls back to ordinary music playback. Continuous
**Radio broadcast** stations do not expose or use bumper settings.

## Implementation and limits

Rust owns a rolling programme cursor over the existing music order and a separate
clip catalogue. It does not expand a full schedule, decode hidden stations, or
create additional audio contexts. Clip durations use the same SQLite cache and
native preparation/authorization path as music.

Pause freezes the active programme. Retuning advances the inactive station's
virtual timeline and can land inside a bumper. Up to 32 station/group contexts
are remembered during the app session. Interval state is not persisted across
app restarts. File/configuration changes start a new schedule; inactive catch-up
is bounded at 200,000 transitions, after which a fresh schedule starts.

The optional legacy Web Audio comparison engine does not implement bumpers;
use the default native libmpv build for this preview.

## Verification

The unit suite covers song-count wraparound, random ranges, avoiding repeats,
nearest-boundary timing, crossfade accounting, partial first songs, and retuning
inside a clip. The real libmpv tests cover pause, volume/mute, clip handoff,
music order, retune, and the existing static/preload/crossfade regression.
Browser integration covers folder selection, saved ranges, equal targets,
validation, disabling clips, and hiding controls for continuous broadcasts.
The opt-in native smoke harness also exercises bumper preparation, cached
durations, and playback through actual Tauri IPC; see [development](development.md).

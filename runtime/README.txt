Radio Replay native audio runtime

Radio Replay's own code is MIT licensed. The dynamically loaded libmpv and its
dependencies have their own licenses.

The installer and portable setup script download the unmodified official
Windows x64 LGPL libmpv 0.42.0 archive directly from mpv-player's GitHub release.
The application package does not redistribute that DLL. Both the archive and
extracted DLL are checked against pinned SHA-256 hashes before installation.
The upstream LICENSE.LGPL is extracted alongside audio/libmpv-2.dll.

Runtime release and upstream source:
https://github.com/mpv-player/mpv/releases/tag/v0.42.0
https://github.com/mpv-player/mpv/tree/v0.42.0
Upstream build recipe (including dependency build instructions):
https://github.com/mpv-player/mpv/blob/v0.42.0/ci/build-mingw64.sh

Portable installation: extract the entire ZIP, run Install-AudioRuntime.cmd
once, then start Radio Replay. An internet connection is required for setup;
local playback works offline afterward. Rerunning setup verifies the installed
runtime and does not download it again when it matches.

Offline setup: obtain the exact archive named in runtime/mpv-windows-x64.json
on a connected machine, transfer it, then run:
powershell.exe -NoProfile -ExecutionPolicy Bypass -File runtime/Install-AudioRuntime.ps1 -ArchivePath C:\path\to\libmpv-v0.42.0-x86_64-w64-mingw32-lgpl.zip

The DLL may be replaced with a compatible libmpv client API 2 build. The app
does not enforce the setup checksum during playback. RADIO_REPLAY_MPV_LIBRARY
can point to an alternate runtime; use an absolute path on Windows. If you
redistribute a replacement DLL, follow its licenses and source requirements.

Radio Replay native audio runtime

Radio Replay's own code is MIT licensed. The dynamically loaded libmpv and its
dependencies have their own licenses.

The installer and portable ZIP include the unmodified official Windows x64 LGPL
libmpv 0.42.0 DLL at audio/libmpv-2.dll. No extra audio download or installation
command is needed. The archive and DLL are verified against pinned SHA-256
hashes during the developer build. The upstream LICENSE.LGPL is bundled beside
the DLL. See SOURCES.txt for upstream source/build provenance and redistribution
source requirements, and LICENSE.Radio-Replay for the application's MIT license.

Runtime release and upstream source:
https://github.com/mpv-player/mpv/releases/tag/v0.42.0
https://github.com/mpv-player/mpv/tree/v0.42.0
Upstream build recipe (including dependency build instructions):
https://github.com/mpv-player/mpv/blob/v0.42.0/ci/build-mingw64-full.sh

Portable installation: extract the entire ZIP and start Radio Replay. Keep the
audio and runtime folders beside the executable. Audio playback works offline.
WebView2 is still required for the interface; the Windows installer can download
it if missing. It is not used as a fallback audio engine.

The download helper remaining in the source repository is a developer build
tool only. It is not shipped or executed by release installers/portable apps.

The DLL may be replaced with a compatible libmpv client API 2 build. The app
does not enforce the setup checksum during playback. RADIO_REPLAY_MPV_LIBRARY
can point to an alternate runtime; use an absolute path on Windows. If you
redistribute a replacement DLL, follow its licenses and source requirements.

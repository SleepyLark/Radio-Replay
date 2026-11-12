Radio Replay is now built with Rust, Tauri 2 and Svelte 5.
Audio playback uses native libmpv; WebView2 renders the interface.

### Windows x64 downloads

- **`*-setup.exe`**: recommended installer.
- **`*-Windows-x64-portable.zip`**: extract everything and run `Install-AudioRuntime.cmd` once, then start the included executable.
- **`SHA256SUMS.txt`**: SHA-256 checksums for both files.

These builds are unsigned. Windows may display a SmartScreen warning. The
installer downloads the checksum-pinned libmpv runtime from its official upstream
release and can download WebView2 if missing. Initial setup needs internet;
local playback then works offline. No account or music service is required.

Existing Rust libraries continue to use `com.local.radioreplay.preview`; stations,
settings and the metadata cache stay in the same location. To migrate from
Electron, use **Manage Stations → Import library** and select its `stations.json`.

Windows is the packaged platform for this release. Linux and macOS have source
checks, but their native playback and media controls still need testing.

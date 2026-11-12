Radio Replay is now built with Rust, Tauri 2 and Svelte 5.

### Windows x64 downloads

- **`*-setup.exe`**: recommended installer.
- **`*-Windows-x64.exe`**: standalone executable; requires Microsoft Edge WebView2.
- **`SHA256SUMS.txt`**: SHA-256 checksums for both files.

These builds are unsigned. Windows may display a SmartScreen warning. The
installer can download WebView2 if it is missing, so initial setup may need an
internet connection. Local playback does not require an account or music service.

Existing Rust libraries continue to use `com.local.radioreplay.preview`; stations,
settings and the metadata cache stay in the same location. To migrate from
Electron, use **Manage Stations → Import library** and select its `stations.json`.

Windows is the packaged platform for this release. Linux and macOS have source
checks, but their native playback and media controls still need testing.

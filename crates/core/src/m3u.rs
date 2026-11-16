//! Local M3U import. Playlist directives are data, never player options.
use crate::{library, Track};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ENTRIES: usize = 50_000;
const MAX_ISSUES: usize = 200;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    pub name: String,
    pub tracks: Vec<Track>,
    pub issues: Vec<Issue>,
    pub issue_count: usize,
}
#[derive(Serialize)]
pub struct Issue {
    pub line: usize,
    pub entry: String,
    pub reason: String,
}
impl Import {
    fn issue(&mut self, line: usize, entry: &str, reason: impl Into<String>) {
        self.issue_count += 1;
        if self.issues.len() < MAX_ISSUES {
            self.issues.push(Issue {
                line,
                entry: entry.chars().take(500).collect(),
                reason: reason.into(),
            });
        }
    }
}
pub fn playlist_file(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("m3u") || s.eq_ignore_ascii_case("m3u8"))
}
pub fn import(path: &Path) -> Result<Import, String> {
    if !playlist_file(path) {
        return Err("Choose an .m3u or .m3u8 playlist".into());
    }
    let path = std::fs::canonicalize(path).map_err(|e| format!("Cannot open playlist: {e}"))?;
    let file = File::open(&path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Choose a playlist file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Playlist exceeds the 4 MiB limit".into());
    }
    let strict = path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("m3u8"));
    let mut report = Import {
        name: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .take(200)
            .collect(),
        tracks: Vec::new(),
        issues: Vec::new(),
        issue_count: 0,
    };
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        return Err("UTF-16 playlists are unsupported. Export as UTF-8 M3U8 instead.".into());
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => std::borrow::Cow::Borrowed(text.trim_start_matches('\u{feff}')),
        Err(_) if !strict => {
            let (text, _, errors) = encoding_rs::WINDOWS_1252.decode(&bytes);
            if errors {
                return Err("Unsupported playlist encoding. Export as UTF-8 M3U8 instead.".into());
            }
            report.issue(
                0,
                "Encoding",
                "Read legacy M3U as Windows-1252. If names look wrong, export as UTF-8 M3U8.",
            );
            text
        }
        Err(_) => {
            return Err("M3U8 playlists must use UTF-8. Export the playlist as UTF-8 M3U8.".into())
        }
    };
    // HLS is also named M3U8, but its local segment files are not standalone songs.
    if text.lines().any(|line| line.trim().starts_with("#EXT-X-")) {
        return Err("This is an HLS stream playlist. This importer currently supports local audio playlists only.".into());
    }
    let base = path.parent().ok_or("Playlist has no parent folder")?;
    let mut title: Option<String> = None;
    let mut entries = 0;
    let mut seen = HashSet::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        if entry.len() > 32768 {
            report.issue(line, entry, "Playlist line is too long");
            title = None;
            continue;
        }
        if let Some(info) = entry.strip_prefix("#EXTINF:") {
            title = info
                .split_once(',')
                .map(|(_, name)| name.trim().chars().take(200).collect())
                .filter(|s: &String| !s.is_empty());
            continue;
        }
        if entry.starts_with('#') {
            continue;
        }
        entries += 1;
        if entries > MAX_ENTRIES {
            return Err(
                "Playlist exceeds the 50,000-entry limit. Split it into smaller playlists.".into(),
            );
        }
        let label = title.take();
        let result = resolve(base, entry).and_then(|path| {
            if playlist_file(&path) {
                return Err(
                    "Nested playlists are not expanded; import their local audio separately".into(),
                );
            }
            if !library::audio_file(&path) {
                return Err("Not a supported audio file (MP3, M4A, AAC, OGG, WAV or FLAC)".into());
            }
            let metadata =
                std::fs::metadata(&path).map_err(|e| format!("Cannot access audio file: {e}"))?;
            if !metadata.is_file() {
                return Err("Entry is not an audio file".into());
            }
            library::track(path)
        });
        match result {
            Ok(mut track) => {
                // Canonical paths also collapse literal/URI aliases of the same file.
                #[cfg(windows)]
                let identity = track.path.to_lowercase();
                #[cfg(not(windows))]
                let identity = track.path.clone();
                if !seen.insert(identity) {
                    report.issue(
                        line,
                        entry,
                        "Duplicate audio file; kept its first occurrence",
                    );
                    continue;
                }
                if let Some(label) = label {
                    track.name = label;
                }
                report.tracks.push(track);
            }
            Err(reason) => report.issue(line, entry, reason),
        }
    }
    Ok(report)
}
fn resolve(base: &Path, entry: &str) -> Result<PathBuf, String> {
    if entry.chars().any(char::is_control) {
        return Err("Invalid control character in path".into());
    }
    let drive = entry.as_bytes().get(1) == Some(&b':') && entry.as_bytes()[0].is_ascii_alphabetic();
    if drive {
        if !matches!(entry.as_bytes().get(2), Some(b'/') | Some(b'\\')) {
            return Err("Drive-relative paths are ambiguous; use an absolute path".into());
        }
        #[cfg(not(windows))]
        return Err("This Windows drive path is unavailable on this platform".into());
    }
    if !drive {
        if let Some((scheme, _)) = entry.split_once(':').filter(|(s, _)| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        }) {
            if !scheme.eq_ignore_ascii_case("file") {
                return Err("Network streams and other URL schemes are not supported in local playlist imports".into());
            }
            let url = url::Url::parse(entry).map_err(|_| "Invalid file URL")?;
            if url.query().is_some() || url.fragment().is_some() {
                return Err("File URLs with queries or fragments are unsupported; escape special filename characters".into());
            }
            let local = url
                .to_file_path()
                .map_err(|_| "Invalid local file URL for this platform")?;
            if local.to_string_lossy().chars().any(char::is_control) {
                return Err("Invalid file path".into());
            }
            return Ok(local);
        }
    }
    // VLC writes escaped relative URLs; ordinary M3U producers write literal paths.
    // Prefer an existing literal path, so a real filename containing '%20' works.
    #[cfg(not(windows))]
    let entry = &entry.replace('\\', "/");
    let path = base.join(entry);
    if path.is_file() || Path::new(entry).is_absolute() {
        return Ok(path);
    }
    let decoded = percent_encoding::percent_decode_str(entry)
        .decode_utf8()
        .map_err(|_| "Invalid UTF-8 in escaped path")?;
    if decoded.chars().any(char::is_control) {
        return Err("Invalid control character in escaped path".into());
    }
    // Decode only relative file references, never turn an escaped URL into a stream.
    if decoded.contains(':') && !drive {
        return Err("Invalid escaped local reference".into());
    }
    Ok(base.join(decoded.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("songs")).unwrap();
        for name in [
            "first song.mp3",
            "second.flac",
            "café.mp3",
            "literal%20name.mp3",
        ] {
            std::fs::write(dir.path().join("songs").join(name), b"fixture").unwrap();
        }
        dir
    }
    fn read(dir: &Path, name: &str, text: impl AsRef<[u8]>) -> Result<Import, String> {
        let file = dir.join(name);
        std::fs::write(&file, text).unwrap();
        import(&file)
    }
    #[test]
    fn vlc_relative_references_titles_and_order_with_bom_crlf() {
        let dir = fixture();
        let result = read(dir.path(), "Local.M3U8", "\u{feff}#EXTM3U\r\n#EXTINF:999,Example, with comma\r\n#EXTVLCOPT:input-repeat=100\r\nsongs/second.flac\r\n\r\n#EXTINF:-1,First song\r\nsongs/first%20song.mp3\r\n").unwrap();
        assert_eq!(result.tracks.len(), 2);
        assert_eq!(result.tracks[0].name, "Example, with comma");
        assert!(result.tracks[0].path.ends_with("second.flac"));
        assert_eq!(result.tracks[1].name, "First song");
        assert!(
            result.tracks[1].extra.is_empty(),
            "Never trust playlist duration or player options"
        );
        assert_eq!(result.issue_count, 0);
        assert_eq!(result.name, "Local");
    }
    #[test]
    fn absolute_file_uri_relative_aliases_and_literal_percent_names() {
        let dir = fixture();
        let absolute = dir.path().join("songs/first song.mp3");
        let uri = url::Url::from_file_path(&absolute).unwrap();
        let result = read(
            dir.path(),
            "local.m3u",
            format!(
                "{}\n{}\nsongs/first song.mp3\nsongs/literal%20name.mp3\nsongs/caf%C3%A9.mp3",
                absolute.display(),
                uri
            ),
        )
        .unwrap();
        assert_eq!(result.tracks.len(), 3);
        assert_eq!(result.issue_count, 2);
        assert!(result.tracks[1].path.ends_with("literal%20name.mp3"));
        assert!(result.tracks[2].path.ends_with("café.mp3"));
    }
    #[test]
    fn valid_files_survive_bad_entries_and_titles_do_not_leak() {
        let dir = fixture();
        std::fs::write(dir.path().join("nested.m3u"), "songs/second.flac").unwrap();
        let result = read(dir.path(), "mixed.m3u8", "#EXTINF:0,Must not leak\nhttps://example.invalid/song.mp3\nmissing.mp3\nsongs\nnested.m3u\nsongs/first song.mp3\n").unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert_eq!(result.tracks[0].name, "first song");
        assert_eq!(result.issue_count, 4);
        assert_eq!(result.issues[0].line, 2);
        assert!(result.issues[0].reason.contains("Network"));
    }
    #[test]
    fn streams_schemes_and_hls_are_never_imported() {
        let dir = fixture();
        let result = read(dir.path(), "urls.m3u", "https://example.invalid/a.mp3\nHTTP://example.invalid/live\nrtsp://example.invalid/live\ndata:audio/wav;base64,AA\nconcat:one.mp3|two.mp3\n").unwrap();
        assert!(result.tracks.is_empty());
        assert_eq!(result.issue_count, 5);
        assert!(read(
            dir.path(),
            "live.m3u8",
            "#EXTM3U\n#EXT-X-TARGETDURATION:6\nsongs/second.flac"
        )
        .err()
        .unwrap()
        .contains("HLS"));
    }
    #[test]
    fn legacy_m3u_can_use_windows_1252_but_m3u8_is_strict() {
        let dir = fixture();
        let bytes = b"songs/caf\xe9.mp3\n";
        let result = read(dir.path(), "old.m3u", bytes).unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert_eq!(result.issues[0].line, 0);
        assert!(read(dir.path(), "bad.m3u8", bytes)
            .err()
            .unwrap()
            .contains("UTF-8"));
        assert!(read(dir.path(), "wide.m3u", [0xff, 0xfe, 0, 0]).is_err());
    }
    #[test]
    fn bounded_input_and_diagnostics() {
        let dir = fixture();
        let result = read(dir.path(), "empty.m3u8", "#EXTM3U\n# comment\n").unwrap();
        assert!(result.tracks.is_empty());
        assert_eq!(result.issue_count, 0);
        let result = read(dir.path(), "bad.m3u", "missing.mp3\n".repeat(300)).unwrap();
        assert_eq!(result.issue_count, 300);
        assert_eq!(result.issues.len(), MAX_ISSUES);
        assert!(read(dir.path(), "big.m3u", vec![b' '; MAX_BYTES as usize + 1]).is_err());
        assert!(read(
            dir.path(),
            "many.m3u",
            "missing.mp3\n".repeat(MAX_ENTRIES + 1)
        )
        .is_err());
        assert!(read(dir.path(), "wrong.pls", "songs/second.flac").is_err());
    }
    #[test]
    fn importing_only_grants_returned_tracks_for_station_save() {
        let dir = fixture();
        let result = read(
            dir.path(),
            "grants.m3u8",
            "songs/second.flac\nmissing.mp3\n",
        )
        .unwrap();
        let grants = result.tracks.iter().map(|t| t.path.clone()).collect();
        let mut config = crate::Config::default();
        let request = || library::StationEdit {
            key: "playlist".into(),
            name: "Playlist".into(),
            mode: "longset".into(),
            tracks: result.tracks.clone(),
            logo_path: None,
            options: Default::default(),
        };
        assert!(library::upsert(&mut config, request(), &HashSet::new(), &HashSet::new()).is_err());
        library::upsert(&mut config, request(), &grants, &HashSet::new()).unwrap();
        assert_eq!(config.custom[0].tracks[0].path, result.tracks[0].path);
    }
    #[test]
    fn file_urls_unescape_special_characters_and_bad_paths_are_reported() {
        let dir = fixture();
        let file = dir.path().join("songs/with # hash.mp3");
        std::fs::write(&file, b"fixture").unwrap();
        let uri = url::Url::from_file_path(&file).unwrap();
        let result = read(
            dir.path(),
            "special.m3u8",
            format!(
                "{uri}\nfile:///tmp/song.mp3?token=bad\nsongs/invalid%00.mp3\nC:relative.mp3\n"
            ),
        )
        .unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert_eq!(result.issue_count, 3);
    }
    #[cfg(windows)]
    #[test]
    fn windows_separators_drive_paths_and_file_urls() {
        let dir = fixture();
        let file = dir.path().join("songs/second.flac");
        let result = read(
            dir.path(),
            "windows.m3u",
            format!(
                "songs\\second.flac\n{}\n{}\n",
                file.display(),
                url::Url::from_file_path(&file).unwrap()
            ),
        )
        .unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert_eq!(result.issue_count, 2);
    }
}

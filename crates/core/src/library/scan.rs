use super::{audio_file, track};
use crate::Track;
use serde::Serialize;
use std::{fs, io, path::Path};

pub const MAX_SCAN_ISSUES: usize = 50;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanNotices {
    pub skipped_folders: usize,
    pub skipped_entries: usize,
    pub issues: Vec<ScanIssue>,
}
#[derive(Clone, Serialize)]
pub struct ScanIssue {
    pub path: String,
    pub reason: String,
}
#[derive(Default, Serialize)]
pub struct ScanReport {
    pub tracks: Vec<Track>,
    pub notices: ScanNotices,
}
impl ScanNotices {
    pub fn count(&self) -> usize {
        self.skipped_folders + self.skipped_entries
    }
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.skipped_folders > 0 {
            parts.push(format!(
                "{} folder{}",
                self.skipped_folders,
                if self.skipped_folders == 1 { "" } else { "s" }
            ));
        }
        if self.skipped_entries > 0 {
            parts.push(format!(
                "{} entr{}",
                self.skipped_entries,
                if self.skipped_entries == 1 {
                    "y"
                } else {
                    "ies"
                }
            ));
        }
        format!("{} could not be read", parts.join(" and "))
    }
    fn skip(&mut self, path: &Path, reason: impl ToString, folder: bool) {
        if folder {
            self.skipped_folders += 1;
        } else {
            self.skipped_entries += 1;
        }
        if self.issues.len() < MAX_SCAN_ISSUES {
            self.issues.push(ScanIssue {
                path: path.to_string_lossy().into_owned(),
                reason: reason.to_string(),
            });
        }
    }
    fn append(&mut self, other: Self) {
        self.skipped_folders += other.skipped_folders;
        self.skipped_entries += other.skipped_entries;
        self.issues.extend(
            other
                .issues
                .into_iter()
                .take(MAX_SCAN_ISSUES - self.issues.len()),
        );
    }
}

type Entries = io::Result<Vec<io::Result<fs::DirEntry>>>;

/// Deterministic depth-first scan. The selected root and hard limits stay fatal;
/// unreadable descendants are reported without discarding healthy siblings.
pub fn scan_folder(root: &Path) -> Result<ScanReport, String> {
    scan_with(root, &mut |path| {
        fs::read_dir(path).map(|entries| entries.collect())
    })
}
fn scan_with(root: &Path, read: &mut impl FnMut(&Path) -> Entries) -> Result<ScanReport, String> {
    fn visit(
        root: &Path,
        depth: usize,
        report: &mut ScanReport,
        read: &mut impl FnMut(&Path) -> Entries,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("Folder nesting exceeds 64 levels".into());
        }
        let entries = match read(root) {
            Ok(entries) => entries,
            Err(error) if depth == 0 => return Err(format!("{}: {error}", root.display())),
            Err(error) => {
                report.notices.skip(root, error, true);
                return Ok(());
            }
        };
        let mut readable = Vec::with_capacity(entries.len());
        for entry in entries {
            match entry {
                Ok(entry) => readable.push(entry),
                Err(error) => report.notices.skip(root, error, false),
            }
        }
        readable.sort_by_key(|entry| entry.file_name());
        for entry in readable {
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    report.notices.skip(&path, error, false);
                    continue;
                }
            };
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    continue;
                }
            }
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if matches!(
                    entry
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .as_str(),
                    "extra" | "extras"
                ) {
                    continue;
                }
                visit(&path, depth + 1, report, read)?;
            } else if metadata.is_file() && audio_file(&path) {
                if report.tracks.len() >= 50_000 {
                    return Err("Choose a smaller folder (maximum 50,000 tracks)".into());
                }
                match track(path.clone()) {
                    Ok(track) => report.tracks.push(track),
                    Err(error) => report.notices.skip(&path, error, false),
                }
            }
        }
        Ok(())
    }
    let mut report = ScanReport::default();
    visit(root, 0, &mut report, read)?;
    Ok(report)
}

pub fn collect_audio(paths: &[std::path::PathBuf]) -> Result<ScanReport, String> {
    let mut report = ScanReport::default();
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        let selected = if path.is_dir() {
            scan_folder(path)?
        } else if audio_file(path) {
            let mut selected = ScanReport::default();
            match track(path.clone()) {
                Ok(track) => selected.tracks.push(track),
                Err(error) => selected.notices.skip(path, error, false),
            }
            selected
        } else {
            continue;
        };
        report.notices.append(selected.notices);
        for track in selected.tracks {
            if seen.insert(track.path.clone()) {
                report.tracks.push(track);
            }
        }
        if report.tracks.len() > 100_000 {
            return Err("Too many audio files in this drop".into());
        }
    }
    if report.tracks.is_empty() && report.notices.count() == 0 {
        return Err("No supported audio files in this drop".into());
    }
    Ok(report)
}

#[cfg(test)]
mod tests;

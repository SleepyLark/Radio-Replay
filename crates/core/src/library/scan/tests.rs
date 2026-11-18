use super::*;

#[test]
fn changed_subfolder_and_missing_entry_keep_healthy_siblings_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("b-folder");
    fs::create_dir(&gone).unwrap();
    fs::create_dir(dir.path().join("c-folder")).unwrap();
    for name in ["a.mp3", "c-folder/music.flac", "z.mp3"] {
        fs::write(dir.path().join(name), b"fixture").unwrap();
    }
    let report = scan_with(dir.path(), &mut |path| {
        if path == gone {
            // Reproduce a filesystem race after directory metadata was read.
            fs::remove_dir(&gone)?;
            fs::write(&gone, b"now a file")?;
            fs::remove_file(dir.path().join("z.mp3"))?;
        }
        fs::read_dir(path).map(|entries| entries.collect())
    })
    .unwrap();
    assert_eq!(
        report
            .tracks
            .iter()
            .map(|track| track.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "music"]
    );
    assert_eq!(report.notices.skipped_folders, 1);
    assert_eq!(report.notices.skipped_entries, 1);
    assert_eq!(report.notices.issues.len(), 2);
    assert_eq!(report.notices.issues[0].path, gone.to_string_lossy());
    assert!(report.notices.issues[1].path.ends_with("z.mp3"));
}

#[test]
fn directory_entry_errors_are_counted_and_details_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("healthy.wav"), b"fixture").unwrap();
    let report = scan_with(dir.path(), &mut |path| {
        let mut entries: Vec<_> = fs::read_dir(path)?.collect();
        entries.extend((0..250).map(|_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unreadable directory entry",
            ))
        }));
        Ok(entries)
    })
    .unwrap();
    assert_eq!(report.tracks.len(), 1);
    assert_eq!(report.notices.skipped_entries, 250);
    assert_eq!(report.notices.issues.len(), MAX_SCAN_ISSUES);
}

#[test]
fn unreadable_selected_root_and_excessive_depth_still_fail() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-a-folder");
    fs::write(&file, b"fixture").unwrap();
    assert!(scan_folder(&file).is_err());
    assert!(scan_folder(&dir.path().join("missing")).is_err());
    let mut nested = dir.path().to_path_buf();
    for _ in 0..65 {
        nested.push("d");
        fs::create_dir(&nested).unwrap();
    }
    assert!(scan_folder(dir.path()).err().unwrap().contains("64 levels"));
}

#[test]
fn intentional_exclusions_are_not_reported_as_failures() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("Extras")).unwrap();
    fs::write(dir.path().join("Extras/ignored.mp3"), b"fixture").unwrap();
    fs::write(dir.path().join("notes.txt"), b"fixture").unwrap();
    let report = scan_folder(dir.path()).unwrap();
    assert!(report.tracks.is_empty());
    assert_eq!(report.notices.count(), 0);
}

#[test]
fn explicit_file_failures_keep_valid_selected_files_and_surface_empty_results() {
    let dir = tempfile::tempdir().unwrap();
    let healthy = dir.path().join("healthy.wav");
    let missing = dir.path().join("missing.mp3");
    fs::write(&healthy, b"fixture").unwrap();
    let report = collect_audio(&[missing.clone(), healthy.clone(), healthy]).unwrap();
    assert_eq!(report.tracks.len(), 1);
    assert_eq!(report.notices.skipped_entries, 1);
    let empty = collect_audio(&[missing]).unwrap();
    assert!(empty.tracks.is_empty());
    assert_eq!(empty.notices.skipped_entries, 1);
}

#[test]
fn merging_notices_retains_full_counts_with_a_shared_detail_limit() {
    let mut notices = ScanNotices::default();
    for _ in 0..2 {
        let mut other = ScanNotices::default();
        for _ in 0..60 {
            other.skip(Path::new("folder"), "unreadable", true);
        }
        notices.append(other);
    }
    assert_eq!(notices.skipped_folders, 120);
    assert_eq!(notices.issues.len(), MAX_SCAN_ISSUES);
    assert_eq!(notices.summary(), "120 folders could not be read");
}

#[test]
fn partial_bumper_scan_keeps_clips_and_reports_a_non_removable_warning() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("clip.wav"), b"fixture").unwrap();
    fs::create_dir(dir.path().join("unreadable")).unwrap();
    let mut config = crate::Config::default();
    config.custom.push(crate::Station {
        key: "music".into(), station: "Music".into(), tracks: vec![],
        extra: serde_json::from_value(serde_json::json!({"mode":"tracks","bumpers":{"enabled":true,"folder":dir.path(),"method":"songs","min":1,"max":1}})).unwrap(),
    });
    let report = super::super::bumper_report_with(&config, "music", |root| {
        scan_with(root, &mut |path| {
            if path.file_name().is_some_and(|name| name == "unreadable") {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Access denied",
                ));
            }
            fs::read_dir(path).map(|entries| entries.collect())
        })
    });
    assert_eq!(report.tracks.len(), 1);
    assert_eq!(report.tracks[0].track.name, "clip");
    assert_eq!(report.problems.len(), 1);
    assert_eq!(report.problems[0].kind, "scan");
    assert_eq!(report.problems[0].reason, "1 folder could not be read");

    let empty = super::super::bumper_report_with(&config, "music", |_| {
        let mut report = ScanReport::default();
        report.notices.skip(dir.path(), "Access denied", true);
        Ok(report)
    });
    assert!(empty.tracks.is_empty());
    assert_eq!(empty.problems.len(), 1);
    assert_eq!(empty.problems[0].kind, "scan");
    assert!(empty.problems[0]
        .reason
        .contains("1 folder could not be read"));
}

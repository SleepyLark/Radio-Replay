use super::*;
fn track(path: &str) -> PlayableTrack {
    PlayableTrack {
        track: crate::Track {
            path: path.into(),
            name: path.into(),
            extra: Default::default(),
        },
        duration: Some(10.0),
        size: 10,
        modified: 1,
        cache_signature: Some("identity".into()),
    }
}
fn lineup() -> IndexedLineup {
    let music = vec![track("song-a.wav"), track("song-b.wav")];
    let bumpers = vec![track("clip.wav")];
    let mut config = Config::default();
    config.custom.push(crate::Station {
        key: "a".into(),
        station: "Station".into(),
        tracks: music.iter().map(|t| t.track.clone()).collect(),
        extra: Default::default(),
    });
    IndexedLineup::new(
        7,
        "a".into(),
        Arc::new(config),
        "all".into(),
        music,
        bumpers,
    )
}
#[test]
fn compact_descriptors_and_index_selection_keep_file_authority_in_rust() {
    let lineup = lineup();
    let described = lineup.describe(vec![]);
    let json = serde_json::to_value(&described).unwrap();
    assert!(json["tracks"][0].get("path").is_none());
    assert!(json["tracks"][0].get("size").is_none());
    assert_eq!(described.tracks[0].slot, 0);
    assert_eq!(described.bumpers[0].slot, 2);
    assert_eq!(described.tracks[0].id, lineup.describe(vec![]).tracks[0].id);
    let selected = lineup.select(7, "a", &[1, 0, 2], 1).unwrap();
    assert_eq!(selected[0].track.path, "song-b.wav");
    assert_eq!(selected[2].track.path, "clip.wav");
    for (generation, key, slots, bumpers) in [
        (8, "a", vec![0, 1, 2], 1),
        (7, "b", vec![0, 1, 2], 1),
        (7, "a", vec![0, 1], 0),
        (7, "a", vec![0, 1, 99], 1),
        (7, "a", vec![0, 0, 2], 1),
        (7, "a", vec![2, 1, 0], 1),
        (7, "a", vec![0, 1, 2], 0),
    ] {
        assert!(lineup.select(generation, key, &slots, bumpers).is_err());
    }
}
#[test]
fn lineup_revision_tracks_order_file_identity_roles_and_bumper_intervals() {
    let mut initial = lineup();
    let revision = initial.describe(vec![]).revision;
    assert!(revision.len() < 64);
    let mut renamed = (*initial.config).clone();
    renamed.custom[0].station = "New display name".into();
    initial.config = Arc::new(renamed);
    assert_eq!(initial.describe(vec![]).revision, revision);
    let previous = initial.config.clone();
    let mut mode = (*previous).clone();
    mode.custom[0].extra.insert("mode".into(), "tracks".into());
    initial.config = Arc::new(mode);
    assert_ne!(initial.describe(vec![]).revision, revision);
    initial.config = previous;
    initial.tracks.swap(0, 1);
    assert_ne!(initial.describe(vec![]).revision, revision);
    initial.tracks.swap(0, 1);
    Arc::make_mut(&mut initial.tracks[0]).cache_signature = Some("changed".into());
    assert_ne!(initial.describe(vec![]).revision, revision);
    let mut role = lineup();
    role.music_count = 3;
    assert_ne!(role.describe(vec![]).revision, revision);
    let mut intervals = lineup();
    let mut config = (*intervals.config).clone();
    config.custom[0].extra.insert(
        "bumpers".into(),
        serde_json::json!({"method":"songs","min":2,"max":3}),
    );
    intervals.config = Arc::new(config);
    let first = intervals.describe(vec![]).revision;
    let mut config = (*intervals.config).clone();
    config.custom[0].extra["bumpers"]["min"] = 3.into();
    intervals.config = Arc::new(config);
    assert_ne!(intervals.describe(vec![]).revision, first);
}
#[test]
fn prepared_lineups_survive_appearance_edits_and_reject_changed_music_groups_or_bumpers() {
    let mut lineup = lineup();
    assert!(lineup.validate_config(&lineup.config).is_ok());
    assert!(validate_preparation(&lineup.config, &lineup.config, "missing", "all").is_err());
    let mut changed = (*lineup.config).clone();
    changed.settings.insert("accent".into(), "#123456".into());
    changed.custom[0]
        .extra
        .insert("bannerColor".into(), "#123456".into());
    assert!(lineup.validate_config(&Arc::new(changed)).is_ok());
    for change in [
        |c: &mut Config| {
            c.custom[0].tracks.pop();
        },
        |c: &mut Config| {
            c.custom[0].extra.insert("mode".into(), "network".into());
        },
        |c: &mut Config| {
            c.custom[0]
                .extra
                .insert("bumpers".into(), serde_json::json!({"enabled":false}));
        },
        |c: &mut Config| {
            c.custom.clear();
        },
    ] {
        let mut changed = (*lineup.config).clone();
        change(&mut changed);
        assert!(lineup.validate_config(&Arc::new(changed)).is_err());
    }
    lineup.group = "group".into();
    let mut changed = (*lineup.config).clone();
    changed
        .groups
        .push(serde_json::json!({"id":"group","name":"Group","members":["a"]}));
    assert!(lineup.validate_config(&Arc::new(changed)).is_err());
}

#[test]
fn file_identity_verification_rejects_changes_missing_files_and_superseded_tunes() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let directory = tempfile::tempdir().unwrap();
    let paths = [
        directory.path().join("a.wav"),
        directory.path().join("b.wav"),
    ];
    let tracks: Vec<_> = paths
        .iter()
        .map(|path| {
            std::fs::write(path, b"fixture").unwrap();
            let mut value = track(&path.to_string_lossy());
            value.cache_signature =
                crate::metadata::Cache::signature_from_metadata(&std::fs::metadata(path).unwrap());
            value
        })
        .collect();
    let selected: Vec<_> = tracks.iter().collect();
    assert_eq!(
        IndexedLineup::verify_files(&selected, || true).unwrap(),
        paths
    );
    let checks = AtomicUsize::new(0);
    assert!(
        IndexedLineup::verify_files(&selected, || checks.fetch_add(1, Ordering::SeqCst) < 2)
            .unwrap_err()
            .contains("Tuning changed")
    );
    std::fs::write(&paths[0], b"changed file length").unwrap();
    assert!(IndexedLineup::verify_files(&selected, || true)
        .unwrap_err()
        .contains("changed after station preparation"));
    std::fs::remove_file(&paths[1]).unwrap();
    assert!(IndexedLineup::verify_files(&[&tracks[1]], || true).is_err());
}

#[test]
fn bounded_parallel_file_verification_preserves_the_selected_order() {
    let directory = tempfile::tempdir().unwrap();
    let tracks: Vec<_> = (0..1024)
        .map(|index| {
            let path = directory.path().join(format!("{index}.wav"));
            std::fs::write(&path, b"fixture").unwrap();
            let mut value = track(&path.to_string_lossy());
            value.cache_signature =
                crate::metadata::Cache::signature_from_metadata(&std::fs::metadata(path).unwrap());
            value
        })
        .collect();
    let selected: Vec<_> = tracks.iter().rev().collect();
    let actual = IndexedLineup::verify_files(&selected, || true).unwrap();
    assert_eq!(
        actual,
        selected
            .iter()
            .map(|track| std::path::PathBuf::from(&track.track.path))
            .collect::<Vec<_>>()
    );
}
#[test]
fn the_same_file_can_be_music_and_a_bumper_but_not_duplicate_music() {
    let initial = lineup();
    let duplicate = IndexedLineup::new(
        7,
        "a".into(),
        initial.config.clone(),
        "all".into(),
        vec![track("same.wav"), track("same.wav")],
        vec![],
    );
    assert!(duplicate.select(7, "a", &[0, 1], 0).is_err());
    let separate = IndexedLineup::new(
        7,
        "a".into(),
        initial.config.clone(),
        "all".into(),
        vec![track("same.wav")],
        vec![track("same.wav")],
    );
    assert!(separate.select(7, "a", &[0, 1], 1).is_ok());
}

use crate::{library, Config, ConfigStore, Station, Track};
use std::{
    fs,
    time::{Duration, Instant},
};

/// Deliberately coarse regression budgets, not tuning-latency targets. Fixture
/// creation/cleanup are outside the timer; no personal library is inspected.
#[test]
#[ignore = "Creates 50,000 local fixtures; run explicitly for large-library regression budgets"]
fn large_library_budget() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let mut tracks = Vec::with_capacity(50_000);
    for folder in 0..50 {
        let folder = root.join(folder.to_string());
        fs::create_dir(&folder).unwrap();
        for index in 0..1000 {
            let path = folder.join(format!("{index}.wav"));
            fs::write(&path, b"fixture").unwrap();
            tracks.push(Track {
                path: path.to_string_lossy().into_owned(),
                name: format!("Track {index}"),
                extra: Default::default(),
            });
        }
    }
    let members: Vec<_> = tracks
        .iter()
        .step_by(2)
        .map(|track| format!("large|{}", track.path))
        .collect();
    let mut config = Config::default();
    config.custom.push(Station {
        key: "large".into(),
        station: "Large library".into(),
        tracks,
        extra: Default::default(),
    });
    config
        .groups
        .push(serde_json::json!({"id":"subset","name":"Subset","members":members}));
    let store = ConfigStore::new(root.join("stations.json"));
    let started = Instant::now();
    store.save(&config).unwrap();
    let save = started.elapsed();
    let started = Instant::now();
    let report = library::playback_report_for_group(config, "large", Some("subset")).unwrap();
    let report_time = started.elapsed();
    assert_eq!(report.tracks.len(), 25_000);
    assert!(report.problems.is_empty());
    println!(
        "LARGE_LIBRARY_BUDGET tracks=50000 selected=25000 save_ms={} grouped_report_ms={}",
        save.as_millis(),
        report_time.as_millis()
    );
    assert!(
        save < Duration::from_secs(5),
        "50,000-track save exceeded 5 seconds: {save:?}"
    );
    assert!(
        report_time < Duration::from_secs(10),
        "Grouped inspection exceeded 10 seconds: {report_time:?}"
    );
}

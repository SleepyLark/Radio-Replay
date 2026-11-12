//! Synthetic membership benchmark; excludes filesystem I/O, IPC and serialization.
use radio_replay_core::{
    library::{playback_report_for_group, upsert, StationEdit},
    Config, Station, Track,
};
use std::{collections::HashSet, time::Instant};

fn main() {
    let count: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "3000".into())
        .parse()
        .unwrap();
    let tracks: Vec<_> = (0..count)
        .map(|i| Track {
            path: format!("track-{i}.mp3"),
            name: format!("Track {i}"),
            extra: Default::default(),
        })
        .collect();
    let mut config = Config::default();
    config.custom.push(Station {
        key: "station".into(),
        station: "Benchmark".into(),
        tracks: tracks.clone(),
        extra: Default::default(),
    });
    // Nonmatching members exercise the complete filtering path without opening media.
    config.groups.push(serde_json::json!({"id":"group","members":(0..count).map(|i|format!("other|track-{i}.mp3")).collect::<Vec<_>>()}));
    let start = Instant::now();
    let report = playback_report_for_group(config.clone(), "station", Some("group")).unwrap();
    assert!(report.tracks.is_empty() && report.problems.is_empty());
    println!(
        "{count} tracks / {count} members: group filter {:?}",
        start.elapsed()
    );
    let start = Instant::now();
    upsert(
        &mut config,
        StationEdit {
            key: "station".into(),
            name: "Updated".into(),
            mode: "tracks".into(),
            tracks,
            logo_path: None,
            options: Default::default(),
        },
        &HashSet::new(),
        &HashSet::new(),
    )
    .unwrap();
    println!(
        "{count} retained tracks: station update {:?}",
        start.elapsed()
    );
}

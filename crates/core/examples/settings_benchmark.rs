//! Settings persistence and response size; not a renderer frame-time measurement.
use radio_replay_core::{preferences, Config, ConfigStore, Station, Track};
use std::time::Instant;
fn main() {
    for count in std::env::args()
        .skip(1)
        .map(|s| s.parse::<usize>().unwrap())
    {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("stations.json"));
        let mut config = Config::default();
        config.custom.push(Station {
            key: "bench".into(),
            station: "Bench".into(),
            extra: Default::default(),
            tracks: (0..count)
                .map(|i| Track {
                    path: format!("C:/Music/Album {}/song-{i}.flac", i / 20),
                    name: format!("Song {i}"),
                    extra: Default::default(),
                })
                .collect(),
        });
        store.save(&config).unwrap();
        store.load_shared().unwrap();
        for (phase, color) in [
            ("changed", "#123456"),
            ("unchanged", "#123456"),
            ("changed_again", "#654321"),
        ] {
            let start = Instant::now();
            let baseline = store.load_shared().unwrap();
            let mut settings = Config {
                settings: baseline.settings.clone(),
                ..Default::default()
            };
            preferences::save_settings(
                &mut settings,
                serde_json::from_value(serde_json::json!({"accent":color})).unwrap(),
            )
            .unwrap();
            let current = if settings.settings == baseline.settings {
                store.confirm_unchanged(&baseline).unwrap()
            } else {
                let mut current = (*baseline).clone();
                current.settings = settings.settings;
                store.save_owned(current).unwrap()
            };
            let elapsed = start.elapsed().as_micros();
            let response = serde_json::to_vec(
                &radio_replay_core::changes::LibraryUpdate::settings(&current),
            )
            .unwrap();
            println!("SETTINGS_BENCHMARK tracks={count} phase={phase} save_us={elapsed} response_bytes={}",response.len());
        }
    }
}

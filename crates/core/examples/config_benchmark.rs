//! Parsed-library reuse only; excludes filesystem/audio preparation and IPC.
use radio_replay_core::{Config, ConfigStore, Station, Track};
use std::time::Instant;
fn main() {
    for count in std::env::args()
        .skip(1)
        .map(|s| s.parse::<usize>().unwrap())
    {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("stations.json"));
        let mut config = Config::default();
        config.custom.push(Station {
            key: "benchmark".into(),
            station: "Benchmark".into(),
            extra: Default::default(),
            tracks: (0..count)
                .map(|i| Track {
                    path: format!("C:/Music/Benchmark/Album {}/Recording {i:06}.flac", i / 20),
                    name: format!("Recording {i}"),
                    extra: Default::default(),
                })
                .collect(),
        });
        store.save(&config).unwrap();
        // Initial validated read is separate from the three native tune stages.
        std::hint::black_box(store.load_shared().unwrap());
        let began = Instant::now();
        for _ in 0..3 {
            std::hint::black_box(store.load_shared().unwrap());
        }
        println!(
            "CONFIG_BENCHMARK tracks={count} three_loads_us={}",
            began.elapsed().as_micros()
        );
    }
}

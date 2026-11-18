//! Synthetic file preparation, not click-to-audio or a disk/library prediction.
use radio_replay_core::{library, metadata, Config, Station, Track};
use std::{sync::Mutex, time::Instant};
fn main() {
    let sizes: Vec<usize> = std::env::args()
        .skip(1)
        .map(|n| n.parse().unwrap())
        .collect();
    for count in if sizes.is_empty() { vec![1000] } else { sizes } {
        let dir = tempfile::tempdir().unwrap();
        let mut wav = b"RIFF".to_vec();
        wav.extend(52u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(8000u32.to_le_bytes());
        wav.extend(16000u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(16u32.to_le_bytes());
        wav.extend([0; 16]);
        let mut tracks = Vec::with_capacity(count);
        for i in 0..count {
            let path = dir.path().join(format!("song-{i}.wav"));
            std::fs::write(&path, &wav).unwrap();
            tracks.push(Track {
                path: path.to_string_lossy().into_owned(),
                name: format!("Song {i}"),
                extra: Default::default(),
            });
        }
        let mut config = Config::default();
        config.custom.push(Station {
            key: "bench".into(),
            station: "Bench".into(),
            tracks,
            extra: Default::default(),
        });
        let cache = Mutex::new(metadata::Cache::open(&dir.path().join("cache.sqlite")));
        for phase in ["cold", "warm"] {
            let start = Instant::now();
            let mut tracks = library::playback_files(&config, "bench").unwrap();
            let filesystem_ms = start.elapsed().as_millis();
            let measured =
                metadata::measure_durations(&cache, &mut tracks, || true, |_, _| {}).unwrap();
            assert!(measured.cache_error.is_none(), "{:?}", measured.cache_error);
            let misses = measured.parsed;
            assert!(tracks.iter().all(|t| t.duration.is_some_and(|n| n > 0.0)));
            println!("METADATA_BENCHMARK count={count} phase={phase} filesystem_ms={filesystem_ms} total_ms={} misses={misses}",start.elapsed().as_millis());
        }
    }
}

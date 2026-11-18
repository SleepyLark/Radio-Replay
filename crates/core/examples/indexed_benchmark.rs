//! Wire-size comparison for the same synthetic prepared station; no IO or audio.
use radio_replay_core::{
    library::{IndexedLineup, PlayableTrack, PreparedStation},
    Config, Track,
};
use std::sync::Arc;
fn main() {
    for count in std::env::args()
        .skip(1)
        .map(|n| n.parse::<usize>().unwrap())
    {
        let tracks:Vec<_>=(0..count).map(|i|PlayableTrack {track:Track {path:format!("C:/Music Library/Game Archive/Original Soundtracks/Album {}/Recording {i:06}.flac",i/20),name:format!("Recording {i}"),extra:Default::default()},size:12345678,modified:1700000000000,duration:Some(180.0),cache_signature:Some(format!("12345678:{}",1700000000000u64+i as u64))}).collect();
        let legacy_request = serde_json::to_vec(
            &serde_json::json!({"paths":tracks.iter().map(|t|&t.track.path).collect::<Vec<_>>()}),
        )
        .unwrap()
        .len();
        let report = PreparedStation {
            tracks,
            bumpers: vec![],
            problems: vec![],
        };
        let legacy_response = serde_json::to_vec(&report).unwrap().len();
        let lineup = IndexedLineup::new(
            1,
            "benchmark".into(),
            Arc::new(Config::default()),
            "all".into(),
            report.tracks,
            vec![],
        );
        let indexed_response = serde_json::to_vec(&lineup.describe(vec![])).unwrap().len();
        let indexed_request =
            serde_json::to_vec(&serde_json::json!({"slots":(0..count).collect::<Vec<_>>()}))
                .unwrap()
                .len();
        println!("INDEXED_BENCHMARK tracks={count} old_response_bytes={legacy_response} new_response_bytes={indexed_response} old_request_bytes={legacy_request} new_request_bytes={indexed_request}");
    }
}

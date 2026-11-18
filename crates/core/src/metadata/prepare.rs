use super::*;
use crate::library::PlayableTrack;
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

const BATCH: usize = 256;
const MAX_WORKERS: usize = 4;

#[derive(Default, Debug)]
pub struct DurationPreparation {
    pub cache_hits: usize,
    pub parsed: usize,
    pub cache_error: Option<String>,
    pub cache_epoch: u64,
}

fn parse_duration(path: &Path, mut value: Metadata) -> Metadata {
    if let Ok(file) = lofty::probe::Probe::open(path).and_then(|probe| {
        probe
            .options(
                lofty::config::ParseOptions::new()
                    .read_tags(false)
                    .read_cover_art(false),
            )
            .read()
    }) {
        let seconds = file.properties().duration().as_secs_f64();
        if seconds.is_finite() && seconds > 0.0 && seconds <= 172800.0 {
            value.duration = Some(seconds);
        }
    }
    value.tags_read = false;
    value
}

/// Bounded parser concurrency and transaction batches. The decoder fallback stays
/// on its own worker. Neither parsing nor filesystem reads hold the cache mutex.
pub fn measure_durations(
    cache: &Mutex<Cache>,
    tracks: &mut [PlayableTrack],
    current: impl Fn() -> bool + Sync,
    progress: impl Fn(usize, usize),
) -> Result<DurationPreparation, String> {
    measure_with(cache, tracks, current, progress, parse_duration)
}

fn measure_with(
    cache: &Mutex<Cache>,
    tracks: &mut [PlayableTrack],
    current: impl Fn() -> bool + Sync,
    progress: impl Fn(usize, usize),
    parse: impl Fn(&Path, Metadata) -> Metadata + Sync,
) -> Result<DurationPreparation, String> {
    if !current() {
        return Err("Tuning changed".into());
    }
    let epoch = {
        let cache = lock(cache);
        cache.populate_durations(tracks);
        cache.epoch
    };
    let jobs: Vec<_> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.duration.is_none())
        .map(|(i, t)| (i, PathBuf::from(&t.track.path), t.cache_signature.clone()))
        .collect();
    let mut stats = DurationPreparation {
        cache_hits: tracks.len() - jobs.len(),
        cache_epoch: epoch,
        ..Default::default()
    };
    let total = tracks.len();
    let mut done = stats.cache_hits;
    progress(done, total);
    if !jobs.is_empty() {
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(MAX_WORKERS)
            .min(jobs.len());
        let (sender, receiver) = mpsc::sync_channel(BATCH);
        std::thread::scope(|scope| {
            for chunk in jobs.chunks(jobs.len().div_ceil(workers)) {
                let sender = sender.clone();
                let current = &current;
                let parse = &parse;
                scope.spawn(move || {
                    for (index, path, signature) in chunk {
                        if !current() {
                            break;
                        }
                        let mut value = parse_safely(path, Metadata::default(), parse);
                        // Duration-only reads, including a caught panic, never claim tags.
                        value.tags_read = false;
                        let stable = signature.as_ref().is_some_and(|signature| {
                            Cache::signature(path).as_ref() == Some(signature)
                        });
                        if sender.send((*index, value, stable)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(sender);
            let mut pending = Vec::with_capacity(BATCH);
            let mut notified = Instant::now();
            for (index, value, stable) in receiver {
                if !current() {
                    continue;
                } // Drain before joining bounded senders.
                stats.parsed += 1;
                done += 1;
                if stable {
                    tracks[index].duration = value.duration;
                    if value.duration.is_some() {
                        pending.push(Write {
                            path: PathBuf::from(&tracks[index].track.path),
                            signature: tracks[index].cache_signature.clone().unwrap(),
                            value,
                        });
                    }
                }
                if pending.len() >= BATCH {
                    persist(cache, epoch, &pending, &mut stats.cache_error);
                    pending.clear();
                }
                if notified.elapsed() >= Duration::from_millis(100) || done == total {
                    progress(done, total);
                    notified = Instant::now();
                }
            }
            if current() {
                persist(cache, epoch, &pending, &mut stats.cache_error);
            }
        });
    }
    if !current() {
        return Err("Tuning changed".into());
    }
    Ok(stats)
}

#[cfg(test)]
mod tests;

fn persist(cache: &Mutex<Cache>, epoch: u64, entries: &[Write], error: &mut Option<String>) {
    // Do not pay SQLite's busy timeout again for every batch after one failure.
    if entries.is_empty() || error.is_some() {
        return;
    }
    let cache = lock(cache);
    // Clear and poison recovery invalidate in-flight writes, including a result
    // that finished parsing immediately before the clear acquired the mutex.
    if cache.epoch != epoch {
        return;
    }
    if let Err(reason) = cache.put_batch(entries) {
        if error.is_none() {
            *error = Some(reason);
        }
    }
}

/// Store decoder fallbacks in the same bounded batches without erasing lazy tags.
pub fn remember_prepared_durations(
    cache: &Mutex<Cache>,
    epoch: u64,
    tracks: &[PlayableTrack],
) -> Result<(), String> {
    for chunk in tracks.chunks(BATCH) {
        let entries: Vec<_> = chunk
            .iter()
            .filter_map(|track| {
                let seconds = track
                    .duration
                    .filter(|n| n.is_finite() && *n > 0.0 && *n <= 172800.0)?;
                let path = PathBuf::from(&track.track.path);
                let signature = track.cache_signature.as_ref()?;
                (Cache::signature(&path).as_ref() == Some(signature)).then(|| Write {
                    path,
                    signature: signature.clone(),
                    value: Metadata {
                        duration: Some(seconds),
                        ..Default::default()
                    },
                })
            })
            .collect();
        let mut error = None;
        persist(cache, epoch, &entries, &mut error);
        if let Some(error) = error {
            return Err(error);
        }
    }
    Ok(())
}

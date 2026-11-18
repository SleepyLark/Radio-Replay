use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn tracks(dir: &Path, count: usize) -> Vec<PlayableTrack> {
    (0..count)
        .map(|i| {
            let path = dir.join(format!("song-{i}.wav"));
            fs::write(&path, b"fixture").unwrap();
            PlayableTrack {
                track: crate::Track {
                    path: path.to_string_lossy().into_owned(),
                    name: format!("Song {i}"),
                    extra: Default::default(),
                },
                size: 7,
                modified: 0,
                duration: None,
                cache_signature: Cache::signature(&path),
            }
        })
        .collect()
}

#[test]
fn bounded_parallel_reads_leave_the_mutex_free_and_warm_tunes_do_not_parse() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Mutex::new(Cache::open(&dir.path().join("cache.sqlite")));
    let mut tracks = tracks(dir.path(), 24);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let updates = std::cell::RefCell::new(Vec::new());
    let stats = measure_with(
        &cache,
        &mut tracks,
        || true,
        |done, total| updates.borrow_mut().push((done, total)),
        |_, mut value| {
            // Other workers may briefly commit while this parser starts. Its
            // own call must not retain the mutex across parsing.
            let deadline = Instant::now() + Duration::from_secs(2);
            while cache.try_lock().is_err() {
                assert!(
                    Instant::now() < deadline,
                    "Parser retained the metadata mutex"
                );
                std::thread::yield_now();
            }
            let readers = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(readers, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            active.fetch_sub(1, Ordering::SeqCst);
            value.duration = Some(42.0);
            value
        },
    )
    .unwrap();
    assert_eq!(stats.parsed, 24);
    assert!(stats.cache_error.is_none());
    assert!(peak.load(Ordering::SeqCst) <= MAX_WORKERS);
    if std::thread::available_parallelism().unwrap().get() > 1 {
        assert!(peak.load(Ordering::SeqCst) > 1);
    }
    assert_eq!(updates.borrow().first(), Some(&(0, 24)));
    assert_eq!(updates.borrow().last(), Some(&(24, 24)));
    assert!(updates.borrow().windows(2).all(|w| w[0].0 <= w[1].0));
    for track in &mut tracks {
        track.duration = None;
    }
    let stats = measure_with(
        &cache,
        &mut tracks,
        || true,
        |_, _| {},
        |_, _| panic!("Warm tune parsed media"),
    )
    .unwrap();
    assert_eq!(stats.cache_hits, 24);
    assert_eq!(stats.parsed, 0);
    assert!(tracks.iter().all(|t| t.duration == Some(42.0)));
    assert!(
        !lock(&cache)
            .peek(Path::new(&tracks[0].track.path))
            .unwrap()
            .tags_read
    );
}

#[test]
fn clearing_during_a_read_does_not_repopulate_the_new_database() {
    let dir = tempfile::tempdir().unwrap();
    let mut tracks = tracks(dir.path(), 1);
    let cache = Mutex::new(Cache::open(&dir.path().join("cache.sqlite")));
    let stats = measure_with(
        &cache,
        &mut tracks,
        || true,
        |_, _| {},
        |_, mut value| {
            lock(&cache).clear().unwrap();
            value.duration = Some(9.0);
            value
        },
    )
    .unwrap();
    assert_eq!(tracks[0].duration, Some(9.0));
    assert!(lock(&cache)
        .peek(Path::new(&tracks[0].track.path))
        .is_none());
    remember_prepared_durations(&cache, stats.cache_epoch, &tracks).unwrap();
    assert!(lock(&cache)
        .peek(Path::new(&tracks[0].track.path))
        .is_none());
}

#[test]
fn cancelled_jobs_drain_without_writing_or_hanging_and_changed_files_are_not_cached() {
    let dir = tempfile::tempdir().unwrap();
    let mut items = tracks(dir.path(), 32);
    let cache = Mutex::new(Cache::open(&dir.path().join("cache.sqlite")));
    let current = AtomicBool::new(true);
    let result = measure_with(
        &cache,
        &mut items,
        || current.load(Ordering::SeqCst),
        |_, _| {},
        |_, mut value| {
            current.store(false, Ordering::SeqCst);
            value.duration = Some(9.0);
            value
        },
    );
    assert_eq!(result.unwrap_err(), "Tuning changed");
    assert!(lock(&cache).peek(Path::new(&items[0].track.path)).is_none());
    measure_with(
        &cache,
        &mut items[..1],
        || true,
        |_, _| {},
        |path, mut value| {
            fs::write(path, b"changed fixture").unwrap();
            value.duration = Some(9.0);
            value
        },
    )
    .unwrap();
    assert!(items[0].duration.is_none());
    assert!(lock(&cache).peek(Path::new(&items[0].track.path)).is_none());
}

#[test]
fn parser_panics_fall_back_and_cache_failures_preserve_measured_durations() {
    let dir = tempfile::tempdir().unwrap();
    let mut tracks = tracks(dir.path(), 1);
    let path = dir.path().join("cache.sqlite");
    fs::create_dir(&path).unwrap();
    let cache = Mutex::new(Cache::open(&path));
    let stats = measure_with(
        &cache,
        &mut tracks,
        || true,
        |_, _| {},
        |_, _| panic!("Injected decoder fallback"),
    )
    .unwrap();
    assert_eq!(stats.parsed, 1);
    assert_eq!(tracks[0].duration, None);
    assert!(!cache.is_poisoned());
    let stats = measure_with(
        &cache,
        &mut tracks,
        || true,
        |_, _| {},
        |_, mut value| {
            value.duration = Some(9.0);
            value
        },
    )
    .unwrap();
    assert!(stats.cache_error.is_some());
    assert_eq!(tracks[0].duration, Some(9.0));
}

#[test]
fn duration_writes_merge_with_tags_read_concurrently() {
    let dir = tempfile::tempdir().unwrap();
    let mut tracks = tracks(dir.path(), 1);
    let cache = Mutex::new(Cache::open(&dir.path().join("cache.sqlite")));
    measure_with(
        &cache,
        &mut tracks,
        || true,
        |_, _| {},
        |path, mut value| {
            let tags = Metadata {
                title: "Lazy title".into(),
                tags_read: true,
                ..Default::default()
            };
            lock(&cache)
                .put(path, &Cache::signature(path).unwrap(), &tags)
                .unwrap();
            value.duration = Some(9.0);
            value
        },
    )
    .unwrap();
    let value = lock(&cache).peek(Path::new(&tracks[0].track.path)).unwrap();
    assert_eq!(value.title, "Lazy title");
    assert!(value.tags_read);
    assert_eq!(value.duration, Some(9.0));
}

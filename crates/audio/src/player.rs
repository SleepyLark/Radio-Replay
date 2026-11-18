//! Bounded controls for a native worker. Decoder handles never leave its thread.
use crate::runtime::Runtime;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender, SyncSender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub generation: u64,
    pub sequence: u64,
    pub index: usize,
    pub path: String,
    pub seconds: f64,
    pub duration: f64,
    pub playing: bool,
    pub loading: bool,
    pub volume: f64,
    pub muted: bool,
    pub error: String,
    pub error_path: String,
    pub issue_revision: u64,
    pub tuning: bool,
    pub tuning_playing: bool,
    pub next_path: String,
    pub transitioning: bool,
    pub transition_progress: f64,
    pub bumper: bool,
}
// Decoder polling and fade gains retain their existing cadence. Only transport
// of the continuously moving clock/fade progress is limited to four per second.
fn publish_due(previous: &Snapshot, next: &Snapshot, elapsed: Duration) -> bool {
    let Snapshot {
        generation,
        sequence: _,
        index,
        path,
        seconds,
        duration,
        playing,
        loading,
        volume,
        muted,
        error,
        error_path,
        issue_revision,
        tuning,
        tuning_playing,
        next_path,
        transitioning,
        transition_progress,
        bumper,
    } = next;
    *generation != previous.generation
        || *index != previous.index
        || path != &previous.path
        || *duration != previous.duration
        || *playing != previous.playing
        || *loading != previous.loading
        || *volume != previous.volume
        || *muted != previous.muted
        || error != &previous.error
        || error_path != &previous.error_path
        || *issue_revision != previous.issue_revision
        || *tuning != previous.tuning
        || *tuning_playing != previous.tuning_playing
        || next_path != &previous.next_path
        || *transitioning != previous.transitioning
        || *bumper != previous.bumper
        || (elapsed >= Duration::from_millis(250)
            && (*seconds != previous.seconds
                || *transition_progress != previous.transition_progress))
}
#[derive(Clone)]
pub struct Plan {
    pub paths: Vec<PathBuf>,
    pub index: usize,
    pub seconds: f64,
    pub fade: f64,
    pub bumpers: Option<crate::programme::Bumpers>,
    pub network: bool,
}
impl Plan {
    pub(crate) fn valid_size(&self) -> bool {
        let music = self
            .bumpers
            .as_ref()
            .map_or(self.paths.len(), |b| b.music_count);
        !self.paths.is_empty()
            && music > 0
            && music <= crate::MAX_MUSIC_TRACKS
            && self.paths.len() >= music
            && self.paths.len() - music <= crate::MAX_BUMPER_TRACKS
            && (!self.network || self.paths.len() <= crate::MAX_NETWORK_STREAMS)
    }
}
#[derive(Clone, Default)]
pub struct Tuning {
    pub sound: Option<PathBuf>,
    pub minimum: Duration,
    pub problem: Option<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_failure_is_reported_before_any_tune() {
        let player = Player::start(PathBuf::from("missing-startup-runtime.dll"), |_| {});
        assert!(player.control(Control::Status).is_err());
        player.shutdown();
    }
    #[test]
    #[ignore = "Needs RADIO_REPLAY_MPV_LIBRARY; verifies restored levels before any tune"]
    fn real_initial_levels_are_ready_before_any_tune() {
        let library = std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").unwrap();
        let player = Player::start_with_preferences(library.into(), 0.37, true, |_| {});
        let state = player.control(Control::Status).unwrap();
        assert_eq!(state.volume, 0.37);
        assert!(state.muted);
        assert!(!state.playing);
        assert_eq!(state.generation, 0);
        let state = player.control(Control::Volume(0.6)).unwrap();
        assert_eq!(state.volume, 0.6);
        assert!(!state.muted);
        player.shutdown();
    }
    #[test]
    fn clock_and_fade_progress_are_limited_without_delaying_state_changes() {
        let previous = Snapshot {
            playing: true,
            ..Default::default()
        };
        let mut next = previous.clone();
        next.seconds = 0.1;
        next.transition_progress = 0.5;
        assert!(!publish_due(&previous, &next, Duration::from_millis(249)));
        assert!(publish_due(&previous, &next, Duration::from_millis(250)));
        next = previous.clone();
        next.sequence += 1;
        assert!(!publish_due(&previous, &next, Duration::from_secs(5)));
        for change in [
            |s: &mut Snapshot| s.playing = false,
            |s: &mut Snapshot| s.loading = true,
            |s: &mut Snapshot| s.generation += 1,
            |s: &mut Snapshot| s.path = "next.wav".into(),
            |s: &mut Snapshot| s.next_path = "preloaded.wav".into(),
            |s: &mut Snapshot| s.transitioning = true,
            |s: &mut Snapshot| s.bumper = true,
            |s: &mut Snapshot| s.issue_revision += 1,
            |s: &mut Snapshot| s.error = "Decoder failed".into(),
            |s: &mut Snapshot| s.volume = 0.5,
            |s: &mut Snapshot| s.muted = true,
            |s: &mut Snapshot| s.tuning = true,
        ] {
            let mut next = previous.clone();
            change(&mut next);
            assert!(publish_due(&previous, &next, Duration::ZERO));
        }
        assert!(!publish_due(&previous, &previous, Duration::from_secs(5)));
    }
    #[test]
    #[ignore = "Needs RADIO_REPLAY_MPV_LIBRARY; measures real worker publication frequency"]
    fn real_publication_frequency() {
        use std::{sync::atomic::AtomicUsize, time::Instant};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("silent.wav");
        let data = 44100u32 * 2 * 30;
        let mut wav = Vec::from(&b"RIFF"[..]);
        wav.extend((data + 36).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(44100u32.to_le_bytes());
        wav.extend(88200u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(data.to_le_bytes());
        wav.resize(44 + data as usize, 0);
        std::fs::write(&path, wav).unwrap();
        let events = Arc::new(AtomicUsize::new(0));
        let counter = events.clone();
        let player = Player::start(
            std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").unwrap().into(),
            move |_| {
                counter.fetch_add(1, Ordering::Relaxed);
            },
        );
        let generation = player.begin().unwrap().generation;
        player
            .load(
                generation,
                Plan {
                    paths: vec![path],
                    index: 0,
                    seconds: 0.0,
                    fade: 0.0,
                    bumpers: None,
                    network: false,
                },
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !player.snapshot().unwrap().playing {
            assert!(Instant::now() < deadline, "Playback did not start");
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(300));
        events.store(0, Ordering::Relaxed);
        let start = Instant::now();
        std::thread::sleep(Duration::from_secs(3));
        let playing = events.swap(0, Ordering::Relaxed);
        let elapsed = start.elapsed().as_secs_f64();
        assert!(
            playing <= 13,
            "Clock publication exceeded four per second: {playing}"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut previous = player.snapshot().unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(20));
            let next = player.snapshot().unwrap();
            if next.sequence == previous.sequence && next.seconds > previous.seconds {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Suppressed clock polls did not refresh the shared snapshot"
            );
            previous = next;
        }
        player.control(Control::Pause).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        events.store(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_secs(1));
        let paused = events.load(Ordering::Relaxed);
        player.shutdown();
        println!("PUBLICATION_BENCHMARK playing_events={playing} playing_seconds={elapsed:.3} playing_hz={:.2} paused_events={paused} paused_seconds=1", playing as f64 / elapsed);
    }
    #[test]
    fn full_size_music_stations_leave_room_for_the_bumper_catalogue() {
        let mut plan = Plan {
            paths: vec![PathBuf::from("song.wav"); crate::MAX_PLAN_PATHS],
            index: 0,
            seconds: 0.0,
            fade: 0.0,
            network: false,
            bumpers: Some(crate::programme::Bumpers {
                music_count: crate::MAX_MUSIC_TRACKS,
                lengths: vec![10.0; crate::MAX_PLAN_PATHS],
                interval: crate::programme::Interval::Songs { min: 1, max: 1 },
                seed: 1,
                context: "s".into(),
                revision: 0,
            }),
        };
        assert!(plan.valid_size());
        assert!(plan.bumpers.as_ref().unwrap().valid(plan.paths.len()));
        plan.paths.push(PathBuf::from("extra.wav"));
        assert!(!plan.valid_size());
        plan.paths.pop();
        plan.bumpers.as_mut().unwrap().music_count += 1;
        assert!(!plan.valid_size());
        plan.bumpers.as_mut().unwrap().music_count -= 2;
        assert!(!plan.valid_size());
        plan.bumpers = None;
        plan.paths.truncate(crate::MAX_MUSIC_TRACKS);
        assert!(plan.valid_size());
        plan.paths.push(PathBuf::from("extra.wav"));
        assert!(!plan.valid_size());
        plan.network = true;
        plan.paths.truncate(crate::MAX_NETWORK_STREAMS);
        assert!(plan.valid_size());
        plan.paths.push(PathBuf::from("extra.wav"));
        assert!(!plan.valid_size());
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Control {
    Status,
    Play,
    Pause,
    Mute,
    Volume(f64),
    Stop,
}
type Reply = Sender<Result<Snapshot, String>>;
enum Message {
    Begin(u64, Tuning, Reply),
    Load(u64, Plan, Reply),
    Control(Control, Option<u64>, Reply),
    Probe(u64, PathBuf, Sender<Result<f64, crate::Failure>>),
    Shutdown(Sender<()>),
}
#[derive(Clone)]
pub struct Player {
    sender: SyncSender<Message>,
    snapshot: Arc<Mutex<Snapshot>>,
    generation: Arc<AtomicU64>,
}
impl Player {
    pub fn start(library: PathBuf, publish: impl Fn(Snapshot) + Send + 'static) -> Self {
        Self::start_with_preferences(library, 0.8, false, publish)
    }
    pub fn start_with_preferences(
        library: PathBuf,
        volume: f64,
        muted: bool,
        publish: impl Fn(Snapshot) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::sync_channel(32);
        let snapshot = Arc::new(Mutex::new(Snapshot {
            volume,
            muted,
            ..Default::default()
        }));
        let generation = Arc::new(AtomicU64::new(0));
        let shared = snapshot.clone();
        let current = generation.clone();
        std::thread::spawn(move || {
            let mut runtime = Runtime::open(library).and_then(|mut runtime| {
                if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
                    return Err("Invalid initial audio volume".into());
                }
                runtime.control(Control::Volume(volume))?;
                if muted {
                    runtime.control(Control::Mute)?;
                }
                Ok(runtime)
            });
            let mut last_emitted = Snapshot::default();
            let mut emitted_at = Instant::now();
            loop {
                let interval = if runtime
                    .as_ref()
                    .is_ok_and(|r| r.state.loading || r.state.playing)
                {
                    20
                } else {
                    100
                };
                let message = match receiver.recv_timeout(Duration::from_millis(interval)) {
                    Ok(message) => Some(message),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(_) => break,
                };
                if let Some(Message::Shutdown(reply)) = message {
                    let _ = reply.send(());
                    break;
                }
                let mut pending = None;
                match runtime.as_mut() {
                    Err(error) => {
                        if let Some(message) = message {
                            match message {
                                Message::Begin(_, _, reply)
                                | Message::Load(_, _, reply)
                                | Message::Control(_, _, reply) => {
                                    let _ = reply.send(Err(error.clone()));
                                }
                                Message::Probe(_, _, reply) => {
                                    let _ = reply.send(Err(error.clone().into()));
                                }
                                Message::Shutdown(_) => unreachable!(),
                            }
                        }
                    }
                    Ok(runtime) => {
                        if let Some(message) = message {
                            match message {
                                Message::Begin(id, tuning, reply) => {
                                    if id != current.load(Ordering::SeqCst) {
                                        let _ = reply.send(Err("Tuning changed".into()));
                                        continue;
                                    }
                                    pending = Some((reply, runtime.begin(id, tuning)));
                                }
                                Message::Load(id, plan, reply) => {
                                    let result = if id != current.load(Ordering::SeqCst) {
                                        Err("Tuning changed".into())
                                    } else {
                                        runtime.load_plan(plan)
                                    };
                                    pending = Some((reply, result));
                                }
                                Message::Control(control, expected, reply) => {
                                    if expected
                                        .is_some_and(|id| id != current.load(Ordering::SeqCst))
                                    {
                                        let _ = reply.send(Ok(runtime.state.clone()));
                                        continue;
                                    }
                                    let result = runtime.control(control);
                                    if runtime.idle() {
                                        runtime.state.generation = current.load(Ordering::SeqCst);
                                    }
                                    pending = Some((reply, result));
                                }
                                Message::Probe(id, path, reply) => {
                                    runtime.start_probe(
                                        id,
                                        current.load(Ordering::SeqCst),
                                        path,
                                        reply,
                                    );
                                }
                                Message::Shutdown(_) => unreachable!(),
                            }
                        }
                        runtime.poll(current.load(Ordering::SeqCst));
                        // Commands acknowledge a fresh, ordered snapshot immediately.
                        // Suppressed clock polls do not consume sequence numbers.
                        let emit = pending.is_some()
                            || publish_due(&last_emitted, &runtime.state, emitted_at.elapsed());
                        if emit {
                            runtime.state.sequence += 1;
                            last_emitted.clone_from(&runtime.state);
                            emitted_at = Instant::now();
                        }
                        if let Ok(mut value) = shared.lock() {
                            if *value != runtime.state {
                                value.clone_from(&runtime.state);
                            }
                        }
                        if emit {
                            publish(runtime.state.clone());
                        }
                        if let Some((reply, result)) = pending {
                            let _ = reply.send(result.map(|_| runtime.state.clone()));
                        }
                    }
                }
            }
        });
        Self {
            sender,
            snapshot,
            generation,
        }
    }
    fn send(&self, message: Message) -> Result<(), String> {
        self.sender
            .try_send(message)
            .map_err(|e| format!("Native audio worker unavailable or busy: {e}"))
    }
    pub fn current(&self, id: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == id
    }
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.snapshot
            .lock()
            .map(|v| v.clone())
            .map_err(|e| e.to_string())
    }
    pub fn begin(&self) -> Result<Snapshot, String> {
        self.begin_tuning(Tuning::default())
    }
    pub fn begin_tuning(&self, tuning: Tuning) -> Result<Snapshot, String> {
        if tuning.minimum > Duration::from_millis(1500) {
            return Err("Invalid minimum tuning delay".into());
        }
        let id = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (reply, rx) = mpsc::channel();
        self.send(Message::Begin(id, tuning, reply))?;
        rx.recv_timeout(Duration::from_secs(12))
            .map_err(|e| e.to_string())?
    }
    pub fn load(&self, id: u64, plan: Plan) -> Result<Snapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.send(Message::Load(id, plan, reply))?;
        rx.recv_timeout(Duration::from_secs(12))
            .map_err(|e| e.to_string())?
    }
    pub fn control(&self, control: Control) -> Result<Snapshot, String> {
        if let Control::Volume(value) = control {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err("Invalid audio volume".into());
            }
        }
        let expected = if matches!(control, Control::Stop) {
            Some(self.generation.fetch_add(1, Ordering::SeqCst) + 1)
        } else {
            None
        };
        let (reply, rx) = mpsc::channel();
        self.send(Message::Control(control, expected, reply))?;
        rx.recv_timeout(Duration::from_secs(12))
            .map_err(|e| e.to_string())?
    }
    pub fn stop_if_current(&self, id: u64) -> Result<Snapshot, String> {
        if self
            .generation
            .compare_exchange(id, id + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return self.snapshot();
        }
        let (reply, rx) = mpsc::channel();
        self.send(Message::Control(Control::Stop, Some(id + 1), reply))?;
        rx.recv_timeout(Duration::from_secs(12))
            .map_err(|e| e.to_string())?
    }
    pub fn probe(&self, id: u64, path: PathBuf) -> Result<f64, crate::Failure> {
        let (reply, rx) = mpsc::channel();
        self.send(Message::Probe(id, path, reply))?;
        rx.recv_timeout(Duration::from_secs(12))
            .map_err(|e| crate::Failure::from(e.to_string()))?
    }
    pub fn shutdown(&self) {
        let (reply, rx) = mpsc::channel();
        if self.send(Message::Shutdown(reply)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(2));
        }
    }
}

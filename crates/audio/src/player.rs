//! One libmpv context, owned by one worker. No decoder work runs in the WebView.
use crate::{Event, Mpv};
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
}
#[derive(Clone)]
pub struct Plan {
    pub paths: Vec<PathBuf>,
    pub index: usize,
    pub seconds: f64,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Control {
    Play,
    Pause,
    Mute,
    Volume(f64),
    Stop,
}
type Reply = Sender<Result<Snapshot, String>>;
enum Message {
    Begin(u64, Reply),
    Load(u64, Plan, Reply),
    Control(Control, Option<u64>, Reply),
    Probe(u64, PathBuf, Sender<Result<f64, String>>),
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
        let (sender, receiver) = mpsc::sync_channel(32);
        let snapshot = Arc::new(Mutex::new(Snapshot {
            volume: 0.8,
            ..Default::default()
        }));
        let generation = Arc::new(AtomicU64::new(0));
        let shared = snapshot.clone();
        let current = generation.clone();
        std::thread::spawn(move || {
            let mut runtime = Mpv::open(&library).map(|mpv| Runtime {
                mpv,
                state: Snapshot {
                    volume: 0.8,
                    ..Default::default()
                },
                plan: None,
                want_play: false,
                offset: 0.0,
                failed: 0,
            });
            let mut previous = Snapshot::default();
            loop {
                let message = match receiver.recv_timeout(Duration::from_millis(100)) {
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
                                Message::Begin(_, reply)
                                | Message::Load(_, _, reply)
                                | Message::Control(_, _, reply) => {
                                    let _ = reply.send(Err(error.clone()));
                                }
                                Message::Probe(_, _, reply) => {
                                    let _ = reply.send(Err(error.clone()));
                                }
                                Message::Shutdown(_) => unreachable!(),
                            }
                        }
                    }
                    Ok(runtime) => {
                        if let Some(message) = message {
                            match message {
                                Message::Begin(id, reply) => {
                                    if id != current.load(Ordering::SeqCst) {
                                        let _ = reply.send(Err("Tuning changed".into()));
                                        continue;
                                    }
                                    let _ = runtime.mpv.command(&["stop"]);
                                    runtime.drain();
                                    runtime.plan = None;
                                    runtime.want_play = true;
                                    runtime.failed = 0;
                                    runtime.state = Snapshot {
                                        generation: id,
                                        sequence: runtime.state.sequence,
                                        volume: runtime.state.volume,
                                        muted: runtime.state.muted,
                                        loading: true,
                                        ..Default::default()
                                    };
                                    pending = Some((reply, Ok(())));
                                }
                                Message::Load(id, plan, reply) => {
                                    let result = if id != current.load(Ordering::SeqCst) {
                                        Err("Tuning changed".into())
                                    } else if plan.paths.is_empty()
                                        || plan.index >= plan.paths.len()
                                        || !plan.seconds.is_finite()
                                        || !(0.0..=172800.0).contains(&plan.seconds)
                                    {
                                        Err("Invalid native playback plan".into())
                                    } else {
                                        runtime.state.index = plan.index;
                                        runtime.offset = plan.seconds;
                                        runtime.plan = Some(plan);
                                        runtime.load()
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
                                    if runtime.plan.is_none() && !runtime.state.loading {
                                        runtime.state.generation = current.load(Ordering::SeqCst);
                                    }
                                    pending = Some((reply, result));
                                }
                                Message::Probe(id, path, reply) => {
                                    let result = runtime.probe(id, &current, path);
                                    let _ = reply.send(result);
                                }
                                Message::Shutdown(_) => unreachable!(),
                            }
                        }
                        runtime.poll();
                        if runtime.state != previous {
                            runtime.state.sequence += 1;
                            previous = runtime.state.clone();
                            if let Ok(mut value) = shared.lock() {
                                *value = runtime.state.clone();
                            }
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
        let id = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (reply, rx) = mpsc::channel();
        self.send(Message::Begin(id, reply))?;
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
    pub fn probe(&self, id: u64, path: PathBuf) -> Result<f64, String> {
        let (reply, rx) = mpsc::channel();
        self.send(Message::Probe(id, path, reply))?;
        rx.recv_timeout(Duration::from_secs(12))
            .map_err(|e| e.to_string())?
    }
    pub fn shutdown(&self) {
        let (reply, rx) = mpsc::channel();
        if self.send(Message::Shutdown(reply)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(2));
        }
    }
}
struct Runtime {
    mpv: Mpv,
    state: Snapshot,
    plan: Option<Plan>,
    want_play: bool,
    offset: f64,
    failed: usize,
}
impl Runtime {
    fn drain(&mut self) {
        for _ in 0..128 {
            if self.mpv.event(0.0) == Event::None {
                break;
            }
        }
    }
    fn volume(&mut self) -> Result<(), String> {
        self.mpv.set(
            "volume",
            &(if self.state.muted {
                0.0
            } else {
                self.state.volume * 100.0
            })
            .to_string(),
        )
    }
    fn load(&mut self) -> Result<(), String> {
        let path =
            self.plan.as_ref().ok_or("No native playback plan")?.paths[self.state.index].clone();
        self.mpv.set("pause", "yes")?;
        self.volume()?;
        self.state.path = path.to_string_lossy().into_owned();
        self.state.seconds = 0.0;
        self.state.duration = 0.0;
        self.state.loading = true;
        self.state.playing = false;
        self.mpv.load_local(&path)
    }
    fn control(&mut self, control: Control) -> Result<(), String> {
        match control {
            Control::Play => {
                self.want_play = true;
                if self.plan.is_some() && !self.state.loading {
                    self.mpv.set("pause", "no")?;
                }
            }
            Control::Pause => {
                self.want_play = false;
                self.mpv.set("pause", "yes")?;
                self.state.playing = false;
            }
            Control::Mute => {
                self.state.muted = !self.state.muted;
                self.volume()?;
            }
            Control::Volume(value) => {
                self.state.volume = value;
                self.state.muted = false;
                self.volume()?;
            }
            Control::Stop => {
                self.mpv.command(&["stop"])?;
                self.drain();
                self.plan = None;
                self.want_play = false;
                self.state.loading = false;
                self.state.playing = false;
                self.state.path.clear();
                self.state.seconds = 0.0;
                self.state.duration = 0.0;
            }
        }
        Ok(())
    }
    fn fail(&mut self, mut error: String) {
        loop {
            self.state.error_path = self.state.path.clone();
            self.state.error = error;
            self.state.issue_revision += 1;
            self.failed += 1;
            let count = self.plan.as_ref().map(|p| p.paths.len()).unwrap_or(0);
            if count == 0 || self.failed >= count {
                self.want_play = false;
                self.state.loading = false;
                self.state.playing = false;
                return;
            }
            self.state.index = (self.state.index + 1) % count;
            self.offset = 0.0;
            match self.load() {
                Ok(()) => return,
                Err(next) => error = next,
            }
        }
    }
    fn poll(&mut self) {
        for _ in 0..64 {
            match self.mpv.event(0.0) {
                Event::None => break,
                Event::Loaded if self.plan.is_some() => {
                    let result = (|| {
                        if self.offset > 0.0 {
                            self.mpv.command(&[
                                "seek",
                                &self.offset.to_string(),
                                "absolute+exact",
                            ])?;
                        }
                        self.mpv
                            .set("pause", if self.want_play { "no" } else { "yes" })
                    })();
                    if let Err(error) = result {
                        self.fail(error);
                        continue;
                    }
                    self.offset = 0.0;
                    self.state.loading = false;
                    self.failed = 0;
                }
                Event::Ended if self.plan.is_some() => {
                    let count = self.plan.as_ref().unwrap().paths.len();
                    self.state.index = (self.state.index + 1) % count;
                    self.offset = 0.0;
                    if let Err(error) = self.load() {
                        self.fail(error);
                    }
                }
                Event::Failed(error) if self.plan.is_some() => self.fail(error),
                Event::Shutdown => {
                    self.want_play = false;
                    self.state.playing = false;
                    self.state.error = "libmpv stopped".into();
                    self.state.issue_revision += 1;
                }
                _ => {}
            }
        }
        if self.plan.is_some() && !self.state.loading {
            self.state.seconds = self
                .mpv
                .number("time-pos")
                .unwrap_or(self.state.seconds)
                .max(0.0);
            self.state.duration = self
                .mpv
                .number("duration")
                .unwrap_or(self.state.duration)
                .max(0.0);
            self.state.playing = self.want_play
                && !self.mpv.flag("pause").unwrap_or(true)
                && !self.mpv.flag("idle-active").unwrap_or(true);
        }
    }
    fn probe(&mut self, id: u64, current: &AtomicU64, path: PathBuf) -> Result<f64, String> {
        if id != current.load(Ordering::SeqCst) {
            return Err("Tuning changed".into());
        }
        if self.plan.is_some() {
            return Err("Begin a new tune before probing audio durations".into());
        }
        self.mpv.set("pause", "yes")?;
        self.mpv.set("volume", "0")?;
        self.drain();
        self.mpv.load_local(&path)?;
        let until = Instant::now() + Duration::from_secs(8);
        let result = loop {
            if id != current.load(Ordering::SeqCst) {
                break Err("Tuning changed".into());
            }
            if Instant::now() > until {
                break Err("Native duration probe timed out".into());
            }
            match self.mpv.event(0.02) {
                Event::Loaded => {
                    break self
                        .mpv
                        .number("duration")
                        .filter(|n| *n > 0.0 && *n <= 172800.0)
                        .ok_or("No finite audio duration".into())
                }
                Event::Failed(error) => break Err(error),
                _ => {}
            }
        };
        let _ = self.mpv.command(&["stop"]);
        self.drain();
        let _ = self.volume();
        result
    }
}

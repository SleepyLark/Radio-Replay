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
    time::Duration,
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
}
#[derive(Clone)]
pub struct Plan {
    pub paths: Vec<PathBuf>,
    pub index: usize,
    pub seconds: f64,
    pub fade: f64,
}
#[derive(Clone, Default)]
pub struct Tuning {
    pub sound: Option<PathBuf>,
    pub minimum: Duration,
    pub problem: Option<(String, String)>,
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
    Begin(u64, Tuning, Reply),
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
            let mut runtime = Runtime::open(library);
            let mut previous = Snapshot::default();
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
                                    let _ = reply.send(Err(error.clone()));
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

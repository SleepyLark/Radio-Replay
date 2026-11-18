//! Worker-only state machine. At most two contexts: current/probe and static/next.
use crate::{
    player::{Control, Plan, Snapshot, Tuning},
    programme::Cursor,
    Event, Failure, Mpv,
};
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

// Preparation margin beyond the fade window. Keep one silent auxiliary context;
// increase this source constant when profiling unusually slow local/NAS storage.
const PRELOAD_MARGIN: Duration = Duration::from_secs(10);

enum Role {
    Idle,
    Static {
        ready: bool,
    },
    Next {
        index: usize,
        ready: bool,
        started: bool,
    },
}
fn auxiliary_load_timeout(role: &Role) -> Duration {
    if matches!(role, Role::Next { .. }) {
        PRELOAD_MARGIN + Duration::from_secs(5)
    } else {
        Duration::from_secs(8)
    }
}
struct Auxiliary {
    mpv: Mpv,
    role: Role,
    path: String,
    started: Instant,
}
struct Probe {
    generation: u64,
    deadline: Instant,
    reply: Sender<Result<f64, Failure>>,
}
struct Fade {
    start: f64,
    duration: f64,
}
struct SavedProgramme {
    context: String,
    signature: u64,
    cursor: Cursor,
    seconds: f64,
    anchor: Instant,
}
enum SeekState {
    Requested,
    Started,
}
pub(crate) struct Runtime {
    library: PathBuf,
    mpv: Mpv,
    auxiliary: Option<Auxiliary>,
    pub(crate) state: Snapshot,
    plan: Option<Plan>,
    probe: Option<Probe>,
    blocked: HashSet<usize>,
    want_play: bool,
    ready: bool,
    retry_load: bool,
    offset: f64,
    seek_pending: Option<SeekState>,
    seek_position: Option<f64>,
    minimum_until: Instant,
    load_started: Instant,
    fade: Option<Fade>,
    // Do not repeatedly try to create a context if its initialization fails.
    auxiliary_failed: bool,
    programme: Option<Cursor>,
    programmes: VecDeque<SavedProgramme>,
}
fn drain(mpv: &mut Mpv) {
    for _ in 0..128 {
        if mpv.event(0.0) == Event::None {
            break;
        }
    }
}
fn stop(mpv: &mut Mpv) {
    let _ = mpv.set("pause", "yes");
    let _ = mpv.command(&["stop"]);
    drain(mpv);
}
fn gains(progress: f64) -> (f64, f64) {
    let angle = progress.clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}
impl Runtime {
    pub(crate) fn open(library: PathBuf) -> Result<Self, String> {
        Ok(Self {
            mpv: Mpv::open(&library)?,
            library,
            auxiliary: None,
            state: Snapshot {
                volume: 0.8,
                ..Default::default()
            },
            plan: None,
            probe: None,
            blocked: HashSet::new(),
            want_play: false,
            ready: false,
            retry_load: false,
            offset: 0.0,
            seek_pending: None,
            seek_position: None,
            minimum_until: Instant::now(),
            load_started: Instant::now(),
            fade: None,
            auxiliary_failed: false,
            programme: None,
            programmes: VecDeque::new(),
        })
    }
    pub(crate) fn idle(&self) -> bool {
        self.plan.is_none() && !self.state.loading
    }
    fn fade_between(&self, a: usize, b: usize) -> f64 {
        let Some(plan) = &self.plan else {
            return 0.0;
        };
        plan.bumpers
            .as_ref()
            .map(|spec| spec.fade_between(a, b, plan.fade))
            .unwrap_or(plan.fade)
    }
    fn next_programme(&self, skip: bool) -> Option<Cursor> {
        let plan = self.plan.as_ref()?;
        let spec = plan.bumpers.as_ref()?;
        let current = self.programme.as_ref()?;
        let mut next = if skip {
            current.skip(spec, plan.fade)
        } else {
            current.following(spec, plan.fade)
        };
        for _ in 0..=plan.paths.len() {
            if !self.blocked.contains(&next.current) {
                return Some(next);
            }
            if next.current >= spec.music_count {
                let last = current
                    .previous_bumper()
                    .map(|index| index + spec.music_count);
                let alternate = (spec.music_count..plan.paths.len())
                    .find(|index| !self.blocked.contains(index) && Some(*index) != last)
                    .or_else(|| {
                        (spec.music_count..plan.paths.len())
                            .find(|index| !self.blocked.contains(index))
                    });
                if let Some(index) = alternate {
                    next.replace_bumper(spec, index);
                    return Some(next);
                }
            }
            next = next.skip(spec, plan.fade);
        }
        None
    }
    fn save_programme(&mut self) {
        if !self.ready || self.state.loading || self.programme.is_none() {
            return;
        }
        self.read_position();
        let (Some(plan), Some(cursor)) = (&self.plan, &self.programme) else {
            return;
        };
        let Some(spec) = &plan.bumpers else {
            return;
        };
        self.programmes.retain(|old| old.context != spec.context);
        self.programmes.push_back(SavedProgramme {
            context: spec.context.clone(),
            signature: spec.signature(&plan.paths, plan.fade),
            cursor: cursor.clone(),
            seconds: self.state.seconds,
            anchor: Instant::now(),
        });
        while self.programmes.len() > 32 {
            self.programmes.pop_front();
        }
    }
    fn issue(&mut self, path: String, error: String) {
        self.state.error_path = path;
        self.state.error = error;
        self.state.issue_revision += 1;
    }
    fn finish_probe(&mut self, result: Result<f64, Failure>) {
        if let Some(probe) = self.probe.take() {
            stop(&mut self.mpv);
            let _ = probe.reply.send(result);
        }
    }
    fn clear_auxiliary(&mut self) {
        if let Some(auxiliary) = &mut self.auxiliary {
            stop(&mut auxiliary.mpv);
            auxiliary.role = Role::Idle;
            auxiliary.path.clear();
        }
        self.state.tuning_playing = false;
        self.state.next_path.clear();
        self.state.transitioning = false;
        self.state.transition_progress = 0.0;
        self.fade = None;
    }
    fn stop_all(&mut self) {
        self.finish_probe(Err("Tuning changed".into()));
        stop(&mut self.mpv);
        self.clear_auxiliary();
        // Idle players retain neither a second decoder nor its demuxer buffers.
        self.auxiliary = None;
        self.plan = None;
        self.programme = None;
        self.state.bumper = false;
        self.ready = false;
        self.retry_load = false;
        self.seek_pending = None;
        self.seek_position = None;
        self.want_play = false;
        self.state.loading = false;
        self.state.tuning = false;
        self.state.playing = false;
    }
    fn ensure_auxiliary(&mut self) -> Result<&mut Auxiliary, String> {
        if self.auxiliary.is_none() {
            if self.auxiliary_failed {
                return Err("Secondary audio context is unavailable".into());
            }
            match Mpv::open(&self.library) {
                Ok(mpv) => {
                    self.auxiliary = Some(Auxiliary {
                        mpv,
                        role: Role::Idle,
                        path: String::new(),
                        started: Instant::now(),
                    })
                }
                Err(error) => {
                    self.auxiliary_failed = true;
                    return Err(error);
                }
            }
        }
        Ok(self.auxiliary.as_mut().unwrap())
    }
    pub(crate) fn begin(&mut self, id: u64, tuning: Tuning) -> Result<(), String> {
        self.save_programme();
        self.stop_all();
        self.blocked.clear();
        self.auxiliary_failed = false;
        self.want_play = true;
        self.minimum_until = Instant::now() + tuning.minimum;
        self.state = Snapshot {
            generation: id,
            sequence: self.state.sequence,
            volume: self.state.volume,
            muted: self.state.muted,
            loading: true,
            tuning: true,
            ..Default::default()
        };
        if let Some((path, error)) = tuning.problem {
            self.issue(path, error);
        }
        if let Some(path) = tuning.sound {
            let result = (|| {
                let aux = self.ensure_auxiliary()?;
                aux.mpv.set("pause", "yes")?;
                aux.mpv.set("loop-file", "inf")?;
                aux.role = Role::Static { ready: false };
                aux.path = path.to_string_lossy().into_owned();
                aux.started = Instant::now();
                aux.mpv.load_local(&path)
            })();
            if let Err(error) = result {
                let failed_path = if self.auxiliary_failed {
                    String::new()
                } else {
                    path.to_string_lossy().into_owned()
                };
                self.clear_auxiliary();
                self.issue(failed_path, format!("Tuning sound unavailable: {error}"));
            }
            self.volume()?;
        }
        Ok(())
    }
    pub(crate) fn load_plan(&mut self, plan: Plan) -> Result<(), String> {
        if self.probe.is_some()
            || !plan.valid_size()
            || plan.index >= plan.paths.len()
            || plan.network
                && (plan.paths.len() > crate::MAX_NETWORK_STREAMS
                    || plan.fade != 0.0
                    || plan.seconds != 0.0
                    || plan.bumpers.is_some()
                    || plan
                        .paths
                        .iter()
                        .any(|p| radio_replay_network::validate(&p.to_string_lossy()).is_err()))
            || !plan.seconds.is_finite()
            || !(0.0..=172800.0).contains(&plan.seconds)
            || !plan.fade.is_finite()
            || !(0.0..=30.0).contains(&plan.fade)
            || plan
                .bumpers
                .as_ref()
                .is_some_and(|b| !b.valid(plan.paths.len()) || plan.index >= b.music_count)
        {
            return Err("Invalid native playback plan".into());
        }
        let mut index = plan.index;
        let mut seconds = plan.seconds;
        self.programme = plan.bumpers.as_ref().map(|spec| {
            let signature = spec.signature(&plan.paths, plan.fade);
            let restored = self
                .programmes
                .iter()
                .find(|old| old.context == spec.context && old.signature == signature)
                .and_then(|old| {
                    old.cursor.clone().seek(
                        spec,
                        plan.fade,
                        old.seconds + old.anchor.elapsed().as_secs_f64(),
                    )
                });
            if let Some((cursor, position)) = restored {
                index = cursor.current;
                seconds = position;
                cursor
            } else {
                Cursor::new(spec, index, seconds)
            }
        });
        self.state.index = index;
        self.offset = seconds;
        self.plan = Some(plan);
        self.load()
    }
    fn load(&mut self) -> Result<(), String> {
        self.retry_load = false;
        self.seek_pending = None;
        self.seek_position = None;
        self.state.bumper = self
            .plan
            .as_ref()
            .and_then(|p| p.bumpers.as_ref())
            .is_some_and(|b| self.state.index >= b.music_count);
        let path =
            self.plan.as_ref().ok_or("No native playback plan")?.paths[self.state.index].clone();
        self.mpv.set("pause", "yes")?;
        self.mpv.set("loop-file", "no")?;
        self.state.path = path.to_string_lossy().into_owned();
        self.state.seconds = 0.0;
        self.state.duration = 0.0;
        self.state.loading = true;
        self.state.playing = false;
        self.ready = false;
        self.load_started = Instant::now();
        self.volume()?;
        if self.plan.as_ref().is_some_and(|p| p.network) {
            self.mpv.load_network(&path.to_string_lossy())
        } else {
            self.mpv.load_local(&path)
        }
    }
    fn volume(&mut self) -> Result<(), String> {
        let level = if self.state.muted {
            0.0
        } else {
            self.state.volume * 100.0
        };
        let (outgoing, incoming) = if self.fade.is_some() {
            gains(self.state.transition_progress)
        } else {
            (1.0, 0.0)
        };
        self.mpv.set(
            "volume",
            &(if self.probe.is_some() {
                0.0
            } else {
                level * outgoing
            })
            .to_string(),
        )?;
        if let Some(aux) = &mut self.auxiliary {
            let gain = match aux.role {
                Role::Static { .. } => 1.0,
                Role::Next { started: true, .. } => incoming,
                _ => 0.0,
            };
            aux.mpv.set("volume", &(level * gain).to_string())?;
        }
        Ok(())
    }
    fn paused(&mut self, paused: bool) -> Result<(), String> {
        self.mpv.set(
            "pause",
            if paused || !self.ready || self.state.loading {
                "yes"
            } else {
                "no"
            },
        )?;
        if let Some(aux) = &mut self.auxiliary {
            let runnable = matches!(
                aux.role,
                Role::Static { ready: true } | Role::Next { started: true, .. }
            );
            aux.mpv
                .set("pause", if paused || !runnable { "yes" } else { "no" })?;
        }
        Ok(())
    }
    pub(crate) fn control(&mut self, control: Control) -> Result<(), String> {
        match control {
            Control::Status => {}
            Control::Play => {
                self.want_play = true;
                if self.state.error_path.is_empty() && !self.state.error.is_empty() {
                    self.issue(String::new(), String::new());
                }
                if self.retry_load {
                    if let Err(error) = self.load() {
                        self.fail(error);
                    }
                } else {
                    self.paused(false)?;
                }
            }
            Control::Pause => {
                self.want_play = false;
                self.paused(true)?;
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
                self.save_programme();
                self.stop_all();
                self.state.path.clear();
                self.state.seconds = 0.0;
                self.state.duration = 0.0;
            }
        }
        Ok(())
    }
    pub(crate) fn start_probe(
        &mut self,
        id: u64,
        current: u64,
        path: PathBuf,
        reply: Sender<Result<f64, Failure>>,
    ) {
        let error = if id != current {
            Some("Tuning changed")
        } else if self.plan.is_some() {
            Some("Begin a new tune before probing audio durations")
        } else if self.probe.is_some() {
            Some("A duration probe is already running")
        } else {
            None
        };
        if let Some(error) = error {
            let _ = reply.send(Err(error.into()));
            return;
        }
        self.probe = Some(Probe {
            generation: id,
            deadline: Instant::now() + Duration::from_secs(8),
            reply,
        });
        stop(&mut self.mpv);
        let result = (|| {
            self.mpv.set("volume", "0")?;
            self.mpv.load_local(&path)
        })();
        if let Err(error) = result {
            self.finish_probe(Err(error.into()));
        }
    }
    fn poll_probe(&mut self, current: u64) {
        let Some(probe) = &self.probe else {
            return;
        };
        if probe.generation != current {
            self.finish_probe(Err("Tuning changed".into()));
            return;
        }
        if Instant::now() >= probe.deadline {
            self.finish_probe(Err("Native duration probe timed out".into()));
            return;
        }
        for _ in 0..64 {
            let result = match self.mpv.event(0.0) {
                Event::None => break,
                Event::Loaded => Some(
                    self.mpv
                        .number("duration")
                        .filter(|n| *n > 0.0 && *n <= 172800.0)
                        .ok_or("No finite audio duration".into()),
                ),
                Event::Failed(error) => Some(Err(error)),
                Event::Shutdown => Some(Err("libmpv stopped during duration probe".into())),
                _ => None,
            };
            if let Some(result) = result {
                self.finish_probe(result);
                break;
            }
        }
    }
    fn next_index(&self) -> Option<usize> {
        if self.programme.is_some() {
            return self.next_programme(false).map(|c| c.current);
        }
        if self.plan.as_ref()?.network && self.blocked.len() >= 3 {
            return None;
        }
        let count = self.plan.as_ref()?.paths.len();
        (1..=count)
            .map(|offset| (self.state.index + offset) % count)
            .find(|index| !self.blocked.contains(index))
    }
    fn fail(&mut self, mut error: String) {
        if self.plan.as_ref().is_some_and(|p| p.network) {
            let reason = self.mpv.network_error();
            if !reason.is_empty() {
                error = reason;
            }
        }
        self.clear_auxiliary();
        loop {
            self.issue(self.state.path.clone(), error);
            self.blocked.insert(self.state.index);
            let next = self.next_programme(true);
            let index = if self.programme.is_some() {
                next.as_ref().map(|c| c.current)
            } else {
                self.next_index()
            };
            let Some(index) = index else {
                self.stop_all();
                return;
            };
            self.programme = next;
            self.state.index = index;
            self.offset = 0.0;
            match self.load() {
                Ok(()) => return,
                Err(next) => error = next,
            }
        }
    }
    fn failure(&mut self, failure: Failure) {
        if let Some(message) = failure.recovery_message() {
            self.finish_probe(Err(failure.clone()));
            if self.ready && !self.state.loading {
                self.offset = self.state.seconds;
            }
            stop(&mut self.mpv);
            self.clear_auxiliary();
            self.ready = false;
            self.seek_pending = None;
            self.retry_load = self.plan.is_some();
            self.want_play = false;
            self.state.playing = false;
            self.state.loading = false;
            self.state.tuning = false;
            self.issue(String::new(), message.into());
        } else {
            self.fail(failure.message);
        }
    }
    fn poll_auxiliary(&mut self) {
        let mut failure = None;
        let mut loaded = false;
        if let Some(aux) = &mut self.auxiliary {
            for _ in 0..32 {
                match aux.mpv.event(0.0) {
                    Event::None => break,
                    Event::Loaded => {
                        loaded = true;
                        match &mut aux.role {
                            Role::Static { ready } | Role::Next { ready, .. } => *ready = true,
                            _ => {}
                        }
                    }
                    Event::Failed(error) => {
                        failure = Some(error);
                        break;
                    }
                    Event::Shutdown => {
                        failure = Some(Failure::from("Secondary audio context stopped"));
                        break;
                    }
                    Event::Ended if matches!(aux.role, Role::Next { started: false, .. }) => {
                        failure = Some("Preloaded recording ended before handoff".into());
                        break;
                    }
                    _ => {}
                }
            }
            if matches!(
                aux.role,
                Role::Static { ready: false } | Role::Next { ready: false, .. }
            ) && aux.started.elapsed() > auxiliary_load_timeout(&aux.role)
            {
                failure = Some("Secondary audio load timed out".into());
            }
            if loaded && matches!(aux.role, Role::Static { .. }) {
                if let Err(error) = aux
                    .mpv
                    .set("pause", if self.want_play { "no" } else { "yes" })
                {
                    failure = Some(error.into());
                }
            }
            self.state.tuning_playing = matches!(aux.role, Role::Static { ready: true })
                && !aux.mpv.flag("pause").unwrap_or(true)
                && !aux.mpv.flag("idle-active").unwrap_or(true);
        }
        if let Some(error) = failure {
            if error.recovery_message().is_some() {
                self.failure(error);
                return;
            }
            let aux = self.auxiliary.as_ref().unwrap();
            let path = aux.path.clone();
            if let Role::Next { index, .. } = aux.role {
                self.blocked.insert(index);
            }
            self.clear_auxiliary();
            self.issue(path, error.message);
            let _ = self.volume();
        }
    }
    fn finish_loading(&mut self) -> Result<(), String> {
        if !self.ready || !self.state.loading || Instant::now() < self.minimum_until {
            return Ok(());
        }
        if matches!(
            self.auxiliary.as_ref().map(|a| &a.role),
            Some(Role::Static { .. })
        ) {
            self.clear_auxiliary();
        }
        self.state.tuning = false;
        self.state.loading = false;
        self.volume()?;
        self.mpv
            .set("pause", if self.want_play { "no" } else { "yes" })
    }
    fn preload(&mut self) -> Result<(), String> {
        if self.plan.as_ref().is_some_and(|p| p.network)
            || self.state.loading
            || !self.ready
            || self.auxiliary_failed
            || self.state.duration <= 0.0
        {
            return Ok(());
        }
        if self
            .auxiliary
            .as_ref()
            .is_some_and(|a| !matches!(a.role, Role::Idle))
        {
            return Ok(());
        }
        let Some(index) = self.next_index().filter(|i| *i != self.state.index) else {
            return Ok(());
        };
        let plan = self.plan.as_ref().unwrap();
        let lead = self.fade_between(self.state.index, index) + PRELOAD_MARGIN.as_secs_f64();
        if self.state.duration - self.state.seconds > lead {
            return Ok(());
        }
        let path = plan.paths[index].clone();
        let result = (|| {
            let aux = self.ensure_auxiliary()?;
            stop(&mut aux.mpv);
            aux.mpv.set("loop-file", "no")?;
            aux.mpv.set("volume", "0")?;
            aux.role = Role::Next {
                index,
                ready: false,
                started: false,
            };
            aux.path = path.to_string_lossy().into_owned();
            aux.started = Instant::now();
            aux.mpv.load_local(&path)
        })();
        if let Err(error) = result {
            // Context initialization failure falls back to current-only playback.
            let failed_path = if self.auxiliary_failed {
                String::new()
            } else {
                self.blocked.insert(index);
                path.to_string_lossy().into_owned()
            };
            self.clear_auxiliary();
            self.issue(failed_path, format!("Could not preload audio: {error}"));
        } else {
            self.state.next_path = path.to_string_lossy().into_owned();
        }
        Ok(())
    }
    fn transition(&mut self) -> Result<(), String> {
        if self.state.loading || !self.ready || !self.want_play {
            return Ok(());
        }
        if self.fade.is_none() {
            let requested = self
                .auxiliary
                .as_ref()
                .and_then(|a| match a.role {
                    Role::Next { index, .. } => Some(self.fade_between(self.state.index, index)),
                    _ => None,
                })
                .unwrap_or(0.0);
            let Some(aux) = &mut self.auxiliary else {
                return Ok(());
            };
            if !matches!(
                aux.role,
                Role::Next {
                    ready: true,
                    started: false,
                    ..
                }
            ) {
                return Ok(());
            }
            let next_duration = aux.mpv.number("duration").unwrap_or(0.0);
            let duration = requested
                .min(self.state.duration / 2.0)
                .min(next_duration / 2.0);
            let remaining = self.state.duration - self.state.seconds;
            if duration <= 0.0 || remaining > duration || remaining <= 0.0 {
                return Ok(());
            }
            self.fade = Some(Fade {
                start: self.state.seconds,
                duration: duration.min(remaining),
            });
            if let Role::Next { started, .. } = &mut aux.role {
                *started = true;
            }
            aux.mpv.set("pause", "no")?;
            self.state.transitioning = true;
        }
        let fade = self.fade.as_ref().unwrap();
        self.state.transition_progress =
            ((self.state.seconds - fade.start) / fade.duration).clamp(0.0, 1.0);
        self.volume()?;
        if self.state.transition_progress >= 1.0 {
            self.handoff()?;
        }
        Ok(())
    }
    fn handoff(&mut self) -> Result<bool, String> {
        let next = self.next_programme(false);
        let Some(aux) = &mut self.auxiliary else {
            return Ok(false);
        };
        let Role::Next {
            index, ready: true, ..
        } = aux.role
        else {
            return Ok(false);
        };
        std::mem::swap(&mut self.mpv, &mut aux.mpv);
        stop(&mut aux.mpv);
        aux.role = Role::Idle;
        aux.path.clear();
        self.fade = None;
        self.state.index = index;
        self.programme = next;
        self.state.bumper = self
            .plan
            .as_ref()
            .and_then(|p| p.bumpers.as_ref())
            .is_some_and(|b| index >= b.music_count);
        self.state.path = self.plan.as_ref().unwrap().paths[index]
            .to_string_lossy()
            .into_owned();
        self.state.next_path.clear();
        self.state.transitioning = false;
        self.state.transition_progress = 0.0;
        self.state.loading = false;
        self.ready = true;
        self.volume()?;
        self.mpv
            .set("pause", if self.want_play { "no" } else { "yes" })?;
        self.read_position();
        Ok(true)
    }
    fn advance(&mut self) -> Result<(), String> {
        if self.handoff()? {
            return Ok(());
        }
        self.clear_auxiliary();
        let Some(index) = self.next_index() else {
            self.stop_all();
            return Ok(());
        };
        self.programme = self.next_programme(false);
        self.state.index = index;
        self.offset = 0.0;
        self.load()
    }
    fn read_position(&mut self) {
        let observed = self.mpv.number("time-pos");
        self.state.seconds = if let Some(target) = self.seek_position {
            if observed.is_some_and(|seconds| seconds >= target - 0.005 && seconds <= target + 0.25)
            {
                self.seek_position = None;
                observed.unwrap().max(0.0)
            } else {
                // The audio output clock can retain the previous file's position,
                // especially when the new recording starts paused. The completed
                // seek target is authoritative until the output clock catches up.
                target
            }
        } else {
            observed.unwrap_or(0.0).max(0.0)
        };
        self.state.duration = self.mpv.number("duration").unwrap_or(0.0).max(0.0);
        self.state.playing = self.want_play
            && !self.state.loading
            && !self.mpv.flag("pause").unwrap_or(true)
            && !self.mpv.flag("idle-active").unwrap_or(true);
    }
    pub(crate) fn poll(&mut self, current: u64) {
        self.poll_auxiliary();
        if self.retry_load {
            return;
        }
        if self.probe.is_some() {
            self.poll_probe(current);
            return;
        }
        for _ in 0..64 {
            match self.mpv.event(0.0) {
                Event::None => break,
                Event::Loaded if self.plan.is_some() => {
                    let seek = self.offset > 0.0;
                    let result = if seek {
                        self.seek_pending = Some(SeekState::Requested);
                        self.mpv
                            .command(&["seek", &self.offset.to_string(), "absolute+exact"])
                    } else {
                        Ok(())
                    };
                    if let Err(error) = result {
                        self.fail(error);
                        break;
                    }
                    self.ready = !seek;
                }
                Event::Seeking if self.seek_pending.is_some() => {
                    self.seek_pending = Some(SeekState::Started);
                }
                Event::Restarted if matches!(self.seek_pending, Some(SeekState::Started)) => {
                    // mpv_command("seek") returns before the new position is ready.
                    // Ignore the initial load's restart until our seek actually starts.
                    self.seek_pending = None;
                    self.seek_position = Some(self.offset);
                    self.offset = 0.0;
                    self.ready = true;
                }
                Event::Ended if self.plan.is_some() => {
                    if let Err(error) = self.advance() {
                        self.fail(error);
                    }
                    break;
                }
                Event::Failed(error) if self.plan.is_some() => {
                    self.failure(error);
                    break;
                }
                Event::Shutdown => {
                    self.stop_all();
                    self.issue(self.state.path.clone(), "libmpv stopped".into());
                    break;
                }
                _ => {}
            }
        }
        if self.plan.is_none() || self.retry_load {
            return;
        }
        if !self.ready && self.load_started.elapsed() > Duration::from_secs(15) {
            self.fail("Native audio load timed out".into());
            return;
        }
        if let Err(error) = self.finish_loading() {
            self.fail(error);
            return;
        }
        if self.ready {
            self.read_position();
            let result = self.preload().and_then(|_| self.transition());
            if let Err(error) = result {
                self.fail(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn next_load_deadline_leaves_time_for_the_full_preload_margin() {
        let next = Role::Next {
            index: 1,
            ready: false,
            started: false,
        };
        assert!(auxiliary_load_timeout(&next) > PRELOAD_MARGIN);
        assert_eq!(
            auxiliary_load_timeout(&Role::Static { ready: false }),
            Duration::from_secs(8)
        );
    }
    #[test]
    #[ignore = "Needs RADIO_REPLAY_MPV_LIBRARY; verifies real seek completion before publishing playback"]
    fn real_resume_waits_for_the_requested_position_before_reporting_playback() {
        let library = std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let audio = directory.path().join("resume.wav");
        fixture(&audio, 10.0);
        let mut runtime = Runtime::open(library.into()).unwrap();
        for id in 1..=12 {
            let target = 1.0 + id as f64 * 0.1;
            runtime.begin(id, Tuning::default()).unwrap();
            runtime
                .load_plan(Plan {
                    paths: vec![audio.clone()],
                    index: 0,
                    seconds: target,
                    fade: 0.0,
                    bumpers: None,
                    network: false,
                })
                .unwrap();
            if id % 2 == 0 {
                runtime.control(Control::Pause).unwrap();
            }
            until(&mut runtime, id, |r| !r.state.loading);
            assert!(runtime.seek_pending.is_none());
            assert!(
                runtime.state.seconds >= target - 0.05,
                "Resume exposed the old position: {:?}, target {target}",
                runtime.state
            );
            if id % 2 == 0 {
                assert!(!runtime.state.playing);
                tick_for(&mut runtime, id, Duration::from_millis(100));
                assert!(
                    runtime.state.seconds >= target - 0.05,
                    "Paused resume clock moved backwards: {:?}",
                    runtime.state
                );
                runtime.control(Control::Play).unwrap();
                until(&mut runtime, id, |r| r.state.playing);
            }
        }
        runtime.stop_all();
    }
    #[test]
    #[ignore = "Needs RADIO_REPLAY_MPV_LIBRARY; injects device and memory failures into the real runtime"]
    fn environmental_failures_keep_the_station_and_retry_the_same_recording() {
        let library = std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.wav");
        let second = directory.path().join("second.wav");
        fixture(&first, 10.0);
        fixture(&second, 10.0);
        let mut runtime = Runtime::open(library.into()).unwrap();
        runtime.begin(1, Tuning::default()).unwrap();
        runtime
            .load_plan(Plan {
                paths: vec![first.clone(), second],
                index: 0,
                seconds: 0.0,
                fade: 0.0,
                bumpers: None,
                network: false,
            })
            .unwrap();
        until(&mut runtime, 1, |r| r.state.playing);
        for code in [crate::MPV_ERROR_AO_INIT_FAILED, crate::MPV_ERROR_NOMEM] {
            runtime.failure(Failure {
                code,
                message: "Injected environmental failure".into(),
            });
            assert!(runtime.blocked.is_empty());
            assert!(runtime.plan.is_some());
            assert!(!runtime.want_play);
            assert!(!runtime.state.playing && !runtime.state.loading);
            assert!(runtime.state.error_path.is_empty());
            assert!(runtime.state.error.contains("press Play"));
            tick_for(&mut runtime, 1, Duration::from_millis(50));
            assert_eq!(runtime.state.index, 0);
            runtime.control(Control::Play).unwrap();
            until(&mut runtime, 1, |r| r.state.playing);
            assert_eq!(runtime.state.path, first.to_string_lossy());
            assert!(runtime.state.error.is_empty());
        }
        runtime.stop_all();
    }
    #[test]
    #[ignore = "Needs RADIO_REPLAY_MPV_LIBRARY; secondary context failure must not mark a file bad"]
    fn real_auxiliary_initialization_failure_preserves_files_and_current_only_playback() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.wav");
        let second = directory.path().join("second.wav");
        fixture(&first, 3.0);
        fixture(&second, 3.0);
        let mut runtime =
            Runtime::open(std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").unwrap().into()).unwrap();
        // The current decoder remains valid; only opening another context fails.
        runtime.library = directory.path().join("unavailable-runtime.dll");
        runtime.begin(1, Tuning::default()).unwrap();
        runtime
            .load_plan(Plan {
                paths: vec![first, second],
                index: 0,
                seconds: 0.0,
                fade: 0.0,
                bumpers: None,
                network: false,
            })
            .unwrap();
        until(&mut runtime, 1, |runtime| runtime.auxiliary_failed);
        assert!(runtime.state.playing);
        assert!(runtime.state.error_path.is_empty());
        assert!(!runtime.state.error.is_empty());
        assert!(runtime.blocked.is_empty());
        until(&mut runtime, 1, |runtime| {
            runtime.state.index == 1 && runtime.state.playing
        });
        runtime.stop_all();
        runtime
            .begin(
                2,
                Tuning {
                    sound: Some(directory.path().join("first.wav")),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(runtime.auxiliary_failed);
        assert!(runtime.state.error_path.is_empty());
        assert!(!runtime.state.error.is_empty());
        runtime.stop_all();
    }
    fn fixture(path: &std::path::Path, seconds: f64) {
        let samples = (seconds * 8000.0) as u32;
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((36 + samples * 2).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(8000u32.to_le_bytes());
        wav.extend(16000u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend((samples * 2).to_le_bytes());
        wav.resize(44 + samples as usize * 2, 0);
        std::fs::write(path, wav).unwrap();
    }
    fn until(runtime: &mut Runtime, id: u64, predicate: impl Fn(&Runtime) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            runtime.poll(id);
            if predicate(runtime) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Native state timed out: {:?}",
                runtime.state
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn tick_for(runtime: &mut Runtime, id: u64, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            runtime.poll(id);
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "Needs RADIO_REPLAY_MPV_LIBRARY; verifies silent zero-fade preloading before the old three-second window"]
    fn real_zero_fade_preloads_with_a_storage_margin() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.wav");
        let second = directory.path().join("second.wav");
        fixture(&first, 20.0);
        fixture(&second, 20.0);
        let mut runtime =
            Runtime::open(std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").unwrap().into()).unwrap();
        runtime.begin(1, Tuning::default()).unwrap();
        runtime
            .load_plan(Plan {
                paths: vec![first, second.clone()],
                index: 0,
                seconds: 11.0,
                fade: 0.0,
                bumpers: None,
                network: false,
            })
            .unwrap();
        until(&mut runtime, 1, |runtime| {
            matches!(
                runtime.auxiliary.as_ref().map(|aux| &aux.role),
                Some(Role::Next {
                    index: 1,
                    ready: true,
                    started: false
                })
            )
        });
        assert!(runtime.state.playing && !runtime.state.transitioning);
        assert!(
            runtime.state.duration - runtime.state.seconds > 5.0,
            "Preload waited until too close to EOF"
        );
        assert_eq!(runtime.state.next_path, second.to_string_lossy());
        assert_eq!(
            runtime.auxiliary.as_ref().unwrap().mpv.number("volume"),
            Some(0.0)
        );
        runtime.stop_all();
    }
    #[test]
    #[ignore = "Needs RADIO_REPLAY_MPV_LIBRARY; exercises the real decoder/output with silent fixtures"]
    fn real_native_static_preload_crossfade_and_cancellation() {
        let library =
            std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").expect("Set the libmpv runtime path");
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.wav");
        let second = directory.path().join("second.wav");
        let noise = directory.path().join("short-static.wav");
        let bad = directory.path().join("disguised.mp3");
        fixture(&first, 4.0);
        fixture(&second, 4.0);
        fixture(&noise, 0.2);
        std::fs::write(&bad, b"#EXTM3U\nhttps://example.invalid/not-allowed.mp3\n").unwrap();
        let mut runtime = Runtime::open(library.into()).unwrap();
        runtime.control(Control::Volume(0.0)).unwrap();
        let started = Instant::now();
        runtime
            .begin(
                1,
                Tuning {
                    sound: Some(noise.clone()),
                    minimum: Duration::from_millis(1500),
                    ..Default::default()
                },
            )
            .unwrap();
        until(&mut runtime, 1, |r| r.state.tuning_playing);
        tick_for(&mut runtime, 1, Duration::from_millis(700));
        assert!(
            runtime.state.tuning_playing,
            "Short tuning audio did not loop"
        );

        // Probes are event-driven: controls can run before a result is available.
        let (reply, result) = std::sync::mpsc::channel();
        runtime.start_probe(1, 1, first.clone(), reply);
        assert!(runtime.probe.is_some());
        runtime.control(Control::Pause).unwrap();
        assert!(runtime
            .auxiliary
            .as_ref()
            .unwrap()
            .mpv
            .flag("pause")
            .unwrap());
        until(&mut runtime, 1, |r| r.probe.is_none());
        assert!((result.recv().unwrap().unwrap() - 4.0).abs() < 0.01);
        assert!(!runtime.state.tuning_playing);
        runtime.control(Control::Play).unwrap();

        // A malformed next file is skipped while the outgoing file keeps playing.
        runtime
            .load_plan(Plan {
                paths: vec![first.clone(), bad.clone(), second.clone()],
                index: 0,
                seconds: 2.6,
                fade: 1.0,
                bumpers: None,
                network: false,
            })
            .unwrap();
        until(&mut runtime, 1, |r| r.state.playing);
        assert!(
            started.elapsed() >= Duration::from_millis(1500),
            "Minimum tuning delay was bypassed"
        );
        assert!(!runtime.state.tuning && !runtime.state.tuning_playing);
        until(&mut runtime, 1, |r| {
            matches!(
                r.auxiliary.as_ref().map(|a| &a.role),
                Some(Role::Next {
                    index: 2,
                    ready: true,
                    started: false
                })
            )
        });
        assert!(runtime.blocked.contains(&1));
        assert_eq!(runtime.state.error_path, bad.to_string_lossy());
        assert!(runtime.state.playing && runtime.state.index == 0);
        assert_eq!(
            runtime.auxiliary.as_ref().unwrap().mpv.number("volume"),
            Some(0.0)
        );
        assert_eq!(
            runtime.auxiliary.as_ref().unwrap().mpv.flag("pause"),
            Some(true)
        );

        until(&mut runtime, 1, |r| {
            r.state.transitioning && r.state.transition_progress >= 0.2
        });
        runtime.control(Control::Mute).unwrap();
        assert_eq!(runtime.mpv.number("volume"), Some(0.0));
        assert_eq!(
            runtime.auxiliary.as_ref().unwrap().mpv.number("volume"),
            Some(0.0)
        );
        // These fixtures contain only zero samples; checking positive gains is silent.
        runtime.control(Control::Volume(0.6)).unwrap();
        let outgoing = runtime.mpv.number("volume").unwrap() / 60.0;
        let incoming = runtime
            .auxiliary
            .as_ref()
            .unwrap()
            .mpv
            .number("volume")
            .unwrap()
            / 60.0;
        assert!((outgoing * outgoing + incoming * incoming - 1.0).abs() < 0.001);
        assert!(outgoing > 0.0 && incoming > 0.0 && !runtime.state.muted);
        runtime.control(Control::Pause).unwrap();
        assert_eq!(runtime.mpv.flag("pause"), Some(true));
        assert_eq!(
            runtime.auxiliary.as_ref().unwrap().mpv.flag("pause"),
            Some(true)
        );
        tick_for(&mut runtime, 1, Duration::from_millis(200));
        let frozen = runtime.state.transition_progress;
        let incoming_position = runtime
            .auxiliary
            .as_ref()
            .unwrap()
            .mpv
            .number("time-pos")
            .unwrap();
        tick_for(&mut runtime, 1, Duration::from_millis(200));
        assert!((runtime.state.transition_progress - frozen).abs() < 0.05);
        assert!(
            (runtime
                .auxiliary
                .as_ref()
                .unwrap()
                .mpv
                .number("time-pos")
                .unwrap()
                - incoming_position)
                .abs()
                < 0.05
        );
        runtime.control(Control::Play).unwrap();
        until(&mut runtime, 1, |r| {
            r.state.index == 2 && r.state.playing && !r.state.transitioning
        });
        assert!(
            runtime.state.seconds > 0.3 && runtime.state.seconds < 1.6,
            "Incoming track restarted after crossfade: {:?}",
            runtime.state
        );

        // No-overlap stations use the already loaded decoder at EOF.
        runtime.begin(2, Tuning::default()).unwrap();
        runtime
            .load_plan(Plan {
                paths: vec![first.clone(), second.clone()],
                index: 0,
                seconds: 3.2,
                fade: 0.0,
                bumpers: None,
                network: false,
            })
            .unwrap();
        until(&mut runtime, 2, |r| {
            matches!(
                r.auxiliary.as_ref().map(|a| &a.role),
                Some(Role::Next {
                    ready: true,
                    started: false,
                    ..
                })
            )
        });
        assert!(!runtime.state.transitioning);
        until(&mut runtime, 2, |r| r.state.index == 1 && r.state.playing);
        assert!(runtime.state.seconds < 0.4);

        // Cancel a running overlap; neither deck may survive a new tune/stop.
        runtime.begin(3, Tuning::default()).unwrap();
        runtime
            .load_plan(Plan {
                paths: vec![first.clone(), second],
                index: 0,
                seconds: 2.8,
                fade: 1.0,
                bumpers: None,
                network: false,
            })
            .unwrap();
        until(&mut runtime, 3, |r| r.state.transitioning);
        runtime
            .begin(
                4,
                Tuning {
                    sound: Some(noise),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(runtime.plan.is_none() && runtime.fade.is_none());
        assert!(!runtime.state.transitioning && runtime.state.next_path.is_empty());
        until(&mut runtime, 4, |r| r.state.tuning_playing);
        runtime.control(Control::Stop).unwrap();
        assert!(runtime.auxiliary.is_none() && runtime.probe.is_none());
        assert!(!runtime.state.playing && !runtime.state.loading && !runtime.state.tuning_playing);

        // A cancelled in-flight probe must resolve with an error, not hang its caller.
        runtime.begin(5, Tuning::default()).unwrap();
        let (reply, result) = std::sync::mpsc::channel();
        runtime.start_probe(5, 5, first, reply);
        runtime.begin(6, Tuning::default()).unwrap();
        assert!(result
            .recv_timeout(Duration::from_millis(100))
            .unwrap()
            .is_err());
        runtime
            .load_plan(Plan {
                paths: vec![bad],
                index: 0,
                seconds: 0.0,
                fade: 0.0,
                bumpers: None,
                network: false,
            })
            .unwrap();
        until(&mut runtime, 6, |r| !r.state.loading);
        assert!(
            !runtime.state.playing && runtime.plan.is_none() && !runtime.state.error.is_empty()
        );
    }
    #[test]
    fn equal_power_envelope_preserves_energy_and_has_exact_endpoints() {
        for progress in [-1.0, 0.0, 0.1, 0.5, 0.9, 1.0, 2.0] {
            let (outgoing, incoming) = gains(progress);
            assert!((outgoing * outgoing + incoming * incoming - 1.0).abs() < 1e-12);
        }
        assert_eq!(gains(0.0), (1.0, 0.0));
        assert!(gains(1.0).0.abs() < 1e-12);
        assert_eq!(gains(1.0).1, 1.0);
    }
    #[test]
    #[ignore = "Needs RADIO_REPLAY_MPV_LIBRARY; exercises real music/bumper playback"]
    fn real_bumpers_preserve_music_order_pause_volume_and_station_clock() {
        use crate::programme::{Bumpers, Interval};
        let library =
            std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").expect("Set the libmpv runtime path");
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.wav");
        let second = directory.path().join("second.wav");
        let clip = directory.path().join("bumper.wav");
        fixture(&first, 4.0);
        fixture(&second, 4.0);
        fixture(&clip, 2.0);
        let plan = |context: &str| Plan {
            paths: vec![first.clone(), second.clone(), clip.clone()],
            index: 0,
            seconds: 3.5,
            fade: 1.0,
            network: false,
            bumpers: Some(Bumpers {
                music_count: 2,
                lengths: vec![4.0, 4.0, 2.0],
                interval: Interval::Songs { min: 1, max: 1 },
                seed: 77,
                context: context.into(),
                revision: 0,
            }),
        };
        let mut runtime = Runtime::open(library.into()).unwrap();
        runtime.control(Control::Volume(0.0)).unwrap();
        runtime.begin(1, Tuning::default()).unwrap();
        runtime.load_plan(plan("station-a/all")).unwrap();
        until(&mut runtime, 1, |r| r.state.bumper && r.state.playing);
        assert_eq!(runtime.state.index, 2);
        assert!(!runtime.state.transitioning, "Music overlapped the bumper");
        assert_eq!(runtime.state.path, clip.to_string_lossy());
        runtime.control(Control::Pause).unwrap();
        tick_for(&mut runtime, 1, Duration::from_millis(200));
        let position = runtime.state.seconds;
        tick_for(&mut runtime, 1, Duration::from_millis(200));
        assert!((runtime.state.seconds - position).abs() < 0.05);
        runtime.control(Control::Mute).unwrap();
        assert_eq!(runtime.mpv.number("volume"), Some(0.0));
        runtime.control(Control::Volume(0.3)).unwrap();
        assert!(!runtime.state.muted);
        assert!((runtime.mpv.number("volume").unwrap() - 30.0).abs() < 0.01);
        runtime.control(Control::Stop).unwrap();
        assert_eq!(runtime.programmes.len(), 1);
        tick_for(&mut runtime, 2, Duration::from_millis(250));
        runtime.begin(2, Tuning::default()).unwrap();
        runtime.load_plan(plan("station-a/all")).unwrap();
        until(&mut runtime, 2, |r| r.state.playing && !r.state.loading);
        assert!(runtime.state.bumper, "Retuning lost the scheduled bumper");
        assert!(
            runtime.state.seconds >= position + 0.15,
            "Retuning restarted the bumper"
        );
        until(&mut runtime, 2, |r| {
            !r.state.bumper && r.state.index == 1 && r.state.playing
        });
        assert!(!runtime.state.transitioning);
        assert!(
            runtime.state.seconds < 0.5,
            "The next music track was interrupted or restarted incorrectly"
        );
        runtime.control(Control::Stop).unwrap();
        runtime.begin(3, Tuning::default()).unwrap();
        runtime
            .load_plan(plan("station-a/different-group"))
            .unwrap();
        until(&mut runtime, 3, |r| r.state.playing && !r.state.loading);
        assert!(
            !runtime.state.bumper && runtime.state.index == 0,
            "Group programme clocks were mixed"
        );
        runtime.control(Control::Stop).unwrap();
        assert!(runtime.auxiliary.is_none() && !runtime.state.bumper);
    }
    #[test]
    #[ignore = "Needs RADIO_REPLAY_MPV_LIBRARY; real decoder with guarded custom IO"]
    fn real_network_custom_io_queue_and_stop() {
        let library = std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("stream.wav");
        fixture(&file, 1.2);
        let mut runtime = Runtime::open(library.into()).unwrap();
        *runtime.mpv._stream.fixture.lock().unwrap() = Some(std::fs::read(file).unwrap());
        runtime.control(Control::Volume(0.0)).unwrap();
        runtime.begin(1, Tuning::default()).unwrap();
        runtime
            .load_plan(Plan {
                paths: vec![
                    "https://radio.example.org/first".into(),
                    "https://radio.example.org/second".into(),
                ],
                index: 0,
                seconds: 0.0,
                fade: 0.0,
                bumpers: None,
                network: true,
            })
            .unwrap();
        until(&mut runtime, 1, |r| r.state.playing);
        assert_eq!(runtime.state.index, 0);
        assert!(runtime.auxiliary.is_none());
        runtime.control(Control::Pause).unwrap();
        tick_for(&mut runtime, 1, Duration::from_millis(150));
        assert!(!runtime.state.playing);
        runtime.control(Control::Play).unwrap();
        until(&mut runtime, 1, |r| r.state.playing && r.state.index == 1);
        assert!(!runtime.state.transitioning);
        assert!(runtime.auxiliary.is_none());
        runtime.control(Control::Stop).unwrap();
        assert!(!runtime.state.playing);
        // A disguised playlist must never become a decoder/network instruction.
        *runtime.mpv._stream.fixture.lock().unwrap() =
            Some(b"#EXTM3U\nfile:///C:/private.wav\nhttp://127.0.0.1/private\n".to_vec());
        runtime.begin(2, Tuning::default()).unwrap();
        runtime
            .load_plan(Plan {
                paths: vec!["https://radio.example.org/disguised.mp3".into()],
                index: 0,
                seconds: 0.0,
                fade: 0.0,
                bumpers: None,
                network: true,
            })
            .unwrap();
        until(&mut runtime, 2, |r| {
            !r.state.error.is_empty() && !r.state.loading
        });
        assert!(!runtime.state.playing);
    }
    #[test]
    #[ignore = "Needs libmpv and an explicitly supplied public test stream URL"]
    fn real_public_http_stream() {
        let library = std::env::var_os("RADIO_REPLAY_MPV_LIBRARY").unwrap();
        let url = std::env::var("RADIO_REPLAY_NETWORK_TEST_URL").unwrap();
        let mut runtime = Runtime::open(library.into()).unwrap();
        runtime.control(Control::Volume(0.0)).unwrap();
        runtime.begin(1, Tuning::default()).unwrap();
        runtime
            .load_plan(Plan {
                paths: vec![url.into()],
                index: 0,
                seconds: 0.0,
                fade: 0.0,
                bumpers: None,
                network: true,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(25);
        while !runtime.state.playing && Instant::now() < deadline {
            runtime.poll(1);
            if !runtime.state.error.is_empty() {
                panic!("{}", runtime.state.error);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(runtime.state.playing, "Public stream did not start");
        tick_for(&mut runtime, 1, Duration::from_secs(1));
        assert!(runtime.state.playing);
        let before = runtime.state.seconds;
        runtime.control(Control::Pause).unwrap();
        tick_for(&mut runtime, 1, Duration::from_millis(200));
        assert!(!runtime.state.playing);
        assert!((runtime.state.seconds - before).abs() < 0.15);
        runtime.control(Control::Play).unwrap();
        tick_for(&mut runtime, 1, Duration::from_millis(200));
        assert!(runtime.state.playing);
        let stopped = Instant::now();
        runtime.control(Control::Stop).unwrap();
        assert!(stopped.elapsed() < Duration::from_secs(2));
        assert!(runtime.auxiliary.is_none());
    }
}

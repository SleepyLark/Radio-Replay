//! Worker-only state machine. At most two contexts: current/probe and static/next.
use crate::{
    player::{Control, Plan, Snapshot, Tuning},
    Event, Mpv,
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

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
struct Auxiliary {
    mpv: Mpv,
    role: Role,
    path: String,
    started: Instant,
}
struct Probe {
    generation: u64,
    deadline: Instant,
    reply: Sender<Result<f64, String>>,
}
struct Fade {
    start: f64,
    duration: f64,
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
    offset: f64,
    minimum_until: Instant,
    load_started: Instant,
    fade: Option<Fade>,
    // Do not repeatedly try to create a context if its initialization fails.
    auxiliary_failed: bool,
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
            offset: 0.0,
            minimum_until: Instant::now(),
            load_started: Instant::now(),
            fade: None,
            auxiliary_failed: false,
        })
    }
    pub(crate) fn idle(&self) -> bool {
        self.plan.is_none() && !self.state.loading
    }
    fn issue(&mut self, path: String, error: String) {
        self.state.error_path = path;
        self.state.error = error;
        self.state.issue_revision += 1;
    }
    fn finish_probe(&mut self, result: Result<f64, String>) {
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
        self.ready = false;
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
                self.clear_auxiliary();
                self.issue(
                    path.to_string_lossy().into_owned(),
                    format!("Tuning sound unavailable: {error}"),
                );
            }
            self.volume()?;
        }
        Ok(())
    }
    pub(crate) fn load_plan(&mut self, plan: Plan) -> Result<(), String> {
        if self.probe.is_some()
            || plan.paths.is_empty()
            || plan.paths.len() > 100000
            || plan.index >= plan.paths.len()
            || !plan.seconds.is_finite()
            || !(0.0..=172800.0).contains(&plan.seconds)
            || !plan.fade.is_finite()
            || !(0.0..=30.0).contains(&plan.fade)
        {
            return Err("Invalid native playback plan".into());
        }
        self.state.index = plan.index;
        self.offset = plan.seconds;
        self.plan = Some(plan);
        self.load()
    }
    fn load(&mut self) -> Result<(), String> {
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
        self.mpv.load_local(&path)
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
            Control::Play => {
                self.want_play = true;
                self.paused(false)?;
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
        reply: Sender<Result<f64, String>>,
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
            self.finish_probe(Err(error));
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
        let count = self.plan.as_ref()?.paths.len();
        (1..=count)
            .map(|offset| (self.state.index + offset) % count)
            .find(|index| !self.blocked.contains(index))
    }
    fn fail(&mut self, mut error: String) {
        self.clear_auxiliary();
        loop {
            self.issue(self.state.path.clone(), error);
            self.blocked.insert(self.state.index);
            let Some(index) = self.next_index() else {
                self.stop_all();
                return;
            };
            self.state.index = index;
            self.offset = 0.0;
            match self.load() {
                Ok(()) => return,
                Err(next) => error = next,
            }
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
                        failure = Some("Secondary audio context stopped".into());
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
            ) && aux.started.elapsed() > Duration::from_secs(8)
            {
                failure = Some("Secondary audio load timed out".into());
            }
            if loaded && matches!(aux.role, Role::Static { .. }) {
                if let Err(error) = aux
                    .mpv
                    .set("pause", if self.want_play { "no" } else { "yes" })
                {
                    failure = Some(error);
                }
            }
            self.state.tuning_playing = matches!(aux.role, Role::Static { ready: true })
                && !aux.mpv.flag("pause").unwrap_or(true)
                && !aux.mpv.flag("idle-active").unwrap_or(true);
        }
        if let Some(error) = failure {
            let aux = self.auxiliary.as_ref().unwrap();
            let path = aux.path.clone();
            if let Role::Next { index, .. } = aux.role {
                self.blocked.insert(index);
            }
            self.clear_auxiliary();
            self.issue(path, error);
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
        if self.state.loading || !self.ready || self.auxiliary_failed || self.state.duration <= 0.0
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
        let lead = plan.fade + 3.0;
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
            if !self.auxiliary_failed {
                self.blocked.insert(index);
            }
            self.clear_auxiliary();
            self.issue(
                path.to_string_lossy().into_owned(),
                format!("Could not preload audio: {error}"),
            );
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
            let requested = self.plan.as_ref().unwrap().fade;
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
        self.state.index = index;
        self.offset = 0.0;
        self.load()
    }
    fn read_position(&mut self) {
        self.state.seconds = self.mpv.number("time-pos").unwrap_or(0.0).max(0.0);
        self.state.duration = self.mpv.number("duration").unwrap_or(0.0).max(0.0);
        self.state.playing = self.want_play
            && !self.state.loading
            && !self.mpv.flag("pause").unwrap_or(true)
            && !self.mpv.flag("idle-active").unwrap_or(true);
    }
    pub(crate) fn poll(&mut self, current: u64) {
        self.poll_auxiliary();
        if self.probe.is_some() {
            self.poll_probe(current);
            return;
        }
        for _ in 0..64 {
            match self.mpv.event(0.0) {
                Event::None => break,
                Event::Loaded if self.plan.is_some() => {
                    let result = if self.offset > 0.0 {
                        self.mpv
                            .command(&["seek", &self.offset.to_string(), "absolute+exact"])
                    } else {
                        Ok(())
                    };
                    if let Err(error) = result {
                        self.fail(error);
                        break;
                    }
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
                    self.fail(error);
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
        if self.plan.is_none() {
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
}

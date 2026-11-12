//! Real libmpv integration check. Generates silent media; never edits a user library.
use radio_replay_audio::{Event, Mpv};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn wait_for(mpv: &mut Mpv, expected: Event) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until {
        let event = mpv.event(0.02);
        if event == expected {
            return Ok(());
        }
        if let Event::Failed(error) = event {
            return Err(error);
        }
    }
    Err(format!("Timed out waiting for {expected:?}"))
}
fn main() -> Result<(), String> {
    let library = std::env::args()
        .nth(1)
        .ok_or("Pass the absolute libmpv library path")?;
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let file = directory.path().join("silent.wav");
    let samples = 3 * 8000u32;
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
    std::fs::write(&file, wav).map_err(|e| e.to_string())?;
    let mut mpv = Mpv::open(Path::new(&library))?;
    println!("Runtime: {:?}", mpv.text("mpv-version"));
    mpv.set("volume", "0")?;
    mpv.load_local(&file)?;
    wait_for(&mut mpv, Event::Loaded)?;
    let duration = mpv.number("duration").ok_or("No duration")?;
    if (duration - 3.0).abs() > 0.1 {
        return Err("Wrong WAV duration".into());
    }
    mpv.set("pause", "no")?;
    std::thread::sleep(Duration::from_millis(250));
    mpv.set("pause", "yes")?;
    if mpv.flag("pause") != Some(true) {
        return Err("Pause property was not set".into());
    }
    // mpv's reported output position can settle after the pause command returns.
    std::thread::sleep(Duration::from_millis(150));
    let position = mpv.number("time-pos").ok_or("No playback position")?;
    std::thread::sleep(Duration::from_millis(150));
    if (mpv.number("time-pos").unwrap() - position).abs() > 0.05 {
        return Err("Pause did not freeze playback".into());
    }
    mpv.command(&["seek", "2.5", "absolute+exact"])?;
    mpv.set("pause", "no")?;
    wait_for(&mut mpv, Event::Ended)?;
    let disguised = directory.path().join("not-audio.mp3");
    std::fs::write(
        &disguised,
        b"#EXTM3U\nhttps://example.invalid/not-allowed.mp3\n",
    )
    .map_err(|e| e.to_string())?;
    mpv.set("pause", "yes")?;
    mpv.load_local(&disguised)?;
    let until = Instant::now() + Duration::from_secs(5);
    let mut rejected = false;
    while Instant::now() < until {
        if let Event::Failed(_) = mpv.event(0.02) {
            rejected = true;
            break;
        }
    }
    if !rejected {
        return Err("Disguised playlist was not rejected".into());
    }
    println!("Real libmpv passed: duration, muted output, pause, seek, EOF and disguised-playlist rejection.");
    if let Some(media) = std::env::args().nth(2) {
        mpv.load_local(Path::new(&media))?;
        wait_for(&mut mpv, Event::Loaded)?;
        mpv.set("pause", "no")?;
        std::thread::sleep(Duration::from_millis(300));
        mpv.set("pause", "yes")?;
        if mpv.number("time-pos").unwrap_or(0.0) <= 0.0 {
            return Err("Supplied media did not advance".into());
        }
        println!(
            "Supplied media passed (muted): codec {:?}, duration {:?}",
            mpv.text("audio-codec-name"),
            mpv.number("duration")
        );
    }
    drop(mpv);
    use radio_replay_audio::player::{Control, Plan, Player};
    let second = directory.path().join("second.wav");
    std::fs::copy(&file, &second).map_err(|e| e.to_string())?;
    let player = Player::start(library.into(), |_| {});
    player.control(Control::Volume(0.0))?;
    let first = player.begin()?;
    let measured = player.probe(first.generation, file.clone())?;
    if (measured - 3.0).abs() > 0.1 {
        return Err("Worker probe duration mismatch".into());
    }
    player.load(
        first.generation,
        Plan {
            paths: vec![file.clone(), second],
            index: 0,
            seconds: 2.5,
            fade: 0.0,
        },
    )?;
    let until = Instant::now() + Duration::from_secs(6);
    loop {
        let state = player.snapshot()?;
        if state.index == 1 && state.playing {
            break;
        }
        if Instant::now() > until {
            return Err(format!(
                "Worker did not advance to second recording: {state:?}"
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if player.probe(first.generation, file.clone()).is_ok() || !player.snapshot()?.playing {
        return Err("A duration probe interrupted active playback".into());
    }
    player.control(Control::Pause)?;
    std::thread::sleep(Duration::from_millis(200));
    let paused = player.snapshot()?;
    std::thread::sleep(Duration::from_millis(150));
    if player.snapshot()?.playing || (player.snapshot()?.seconds - paused.seconds).abs() > 0.05 {
        return Err("Worker pause did not freeze playback".into());
    }
    let current = player.begin()?;
    player.stop_if_current(first.generation)?;
    if !player.current(current.generation) {
        return Err("Stale stop cancelled a newer tune".into());
    }
    player.control(Control::Pause)?;
    player.load(
        current.generation,
        Plan {
            paths: vec![file],
            index: 0,
            seconds: 0.0,
            fade: 0.0,
        },
    )?;
    std::thread::sleep(Duration::from_millis(250));
    if player.snapshot()?.playing || player.snapshot()?.loading {
        return Err("Pause during preparation was lost".into());
    }
    player.control(Control::Play)?;
    player.control(Control::Mute)?;
    let volume = player.control(Control::Volume(0.25))?;
    if volume.muted || (volume.volume - 0.25).abs() > 0.001 {
        return Err("Worker volume did not unmute".into());
    }
    player.control(Control::Stop)?;
    player.shutdown();
    println!("Native worker passed: probing, EOF advancement, pause, stale-stop cancellation and volume unmute.");
    Ok(())
}

use prospekt_dj::{
    audio,
    engine::{Command, DeckControl, Snapshot},
    media,
    recording::{self, Format},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[test]
fn cpal_callback_updates_state_and_records_audio() {
    if std::env::var_os("PROSPEKT_TEST_AUDIO").is_none() {
        return;
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    let (errors, _) = crossbeam_channel::bounded(8);
    let view = Arc::new(Mutex::new(Snapshot::default()));
    let runtime = audio::start(rx, view.clone(), errors, "", "", 12).expect("test output device");
    tx.send(Command::Deck(0, DeckControl::Load(media::demo(0))))
        .unwrap();
    tx.send(Command::Deck(0, DeckControl::Loop(4.))).unwrap();
    tx.send(Command::Deck(0, DeckControl::Play)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while view.lock().unwrap().decks[0].position < 0.01 {
        assert!(Instant::now() < deadline, "callback did not advance");
        std::thread::sleep(Duration::from_millis(2));
    }
    let output = std::env::temp_dir().join(format!("prospekt-cpal-{}.wav", std::process::id()));
    let (sink, result) =
        recording::start(output.clone(), Format::Wav, runtime.sample_rate).unwrap();
    tx.send(Command::Recording(Some(sink))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while view.lock().unwrap().recorded_seconds < 0.05 {
        assert!(Instant::now() < deadline, "callback did not capture audio");
        std::thread::sleep(Duration::from_millis(2));
    }
    tx.send(Command::Recording(None)).unwrap();
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        output
    );
    let decoded = media::load(&output, false).unwrap();
    assert!(!decoded.frames.is_empty());
    assert!(decoded.frames.iter().any(|f| f[0].abs() > 0.001));
    drop(runtime);
    let _ = std::fs::remove_file(output);
}

#[test]
fn missing_saved_master_and_unavailable_pfl_do_not_disable_working_default_output() {
    if std::env::var_os("PROSPEKT_TEST_AUDIO").is_none() {
        return;
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    let (errors, _) = crossbeam_channel::bounded(8);
    let view = Arc::new(Mutex::new(Snapshot::default()));
    let options = audio::Options {
        master_name: "Prospekt deliberately missing saved device".into(),
        cue_name: "Prospekt deliberately missing PFL device".into(),
        ..Default::default()
    };
    let runtime = audio::start_with_options(rx, view.clone(), errors, &options)
        .expect("default device must recover from stale preferences");
    assert!(runtime.is_running());
    assert!(runtime.warnings.iter().any(|w| w.contains("Saved MASTER")));
    assert!(runtime.warnings.iter().any(|w| w.contains("PFL disabled")));
    tx.send(Command::Deck(0, DeckControl::Load(media::demo(0))))
        .unwrap();
    tx.send(Command::Deck(0, DeckControl::Play)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while view.lock().unwrap().decks[0].position < 0.01 {
        assert!(
            Instant::now() < deadline,
            "recovered default stream did not render"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!runtime.device_name.is_empty());
    drop(runtime);
}
#[test]
fn linux_fallback_uses_bridges_and_never_silent_null_or_unrelated_hardware() {
    assert_eq!(
        prospekt_dj::audio::linux_bridge_candidates(vec![
            "hw:0".into(),
            "null".into(),
            "default".into(),
            "pipewire".into(),
            "pulse".into(),
            "pulse".into()
        ]),
        vec!["pulse", "pipewire", "default"]
    );
}

#[test]
fn rate_negotiation_keeps_alternate_rates_and_rejects_invalid_ranges() {
    assert_eq!(
        audio::sample_rate_attempts(8000, 96000),
        vec![48000, 44100, 8000, 96000]
    );
    assert_eq!(
        audio::sample_rate_attempts(44100, 48000),
        vec![48000, 44100]
    );
    assert_eq!(audio::sample_rate_attempts(16000, 16000), vec![16000]);
    assert!(audio::sample_rate_attempts(0, 48000).is_empty());
    assert!(audio::sample_rate_attempts(48000, 44100).is_empty());
}

#[test]
fn broken_alsa_default_recovers_to_bridge_and_renders() {
    if std::env::var_os("PROSPEKT_TEST_BROKEN_ALSA").is_none() {
        return;
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    let (errors, _) = crossbeam_channel::bounded(8);
    let view = Arc::new(Mutex::new(Snapshot::default()));
    let options = audio::Options {
        buffer_frames: Some(64),
        ..Default::default()
    };
    let runtime =
        audio::start_with_options(rx, view.clone(), errors, &options).expect("bridge recovery");
    assert_eq!(runtime.device_name, "pulse");
    assert_eq!(runtime.requested_buffer, None);
    assert!(runtime
        .warnings
        .iter()
        .any(|w| w.contains("Original audio failure")));
    tx.send(Command::Deck(0, DeckControl::Load(media::demo(0))))
        .unwrap();
    tx.send(Command::Deck(0, DeckControl::Play)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while view.lock().unwrap().decks[0].position < 0.01 {
        assert!(Instant::now() < deadline, "bridge callback stalled");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(runtime.is_running());
}

#[test]
fn legacy_alsa_description_still_selects_the_registered_pcm() {
    if std::env::var_os("PROSPEKT_TEST_BROKEN_ALSA").is_none() {
        return;
    }
    let (_, rx) = crossbeam_channel::unbounded();
    let (errors, _) = crossbeam_channel::bounded(8);
    let view = Arc::new(Mutex::new(Snapshot::default()));
    let options = audio::Options {
        master_name: "Prospekt simulated container bridge".into(),
        ..Default::default()
    };
    let runtime = audio::start_with_options(rx, view, errors, &options).unwrap();
    assert_eq!(runtime.device_name, "pulse");
    assert!(!runtime
        .warnings
        .iter()
        .any(|w| w.contains("Original audio failure")));
}

#[test]
fn microphone_uses_container_bridge_when_default_hardware_is_missing() {
    if std::env::var_os("PROSPEKT_TEST_BROKEN_ALSA").is_none() {
        return;
    }
    let (microphone, frames) = prospekt_dj::microphone::start("", 48000).unwrap();
    assert_eq!(microphone.device_name, "pulse");
    assert!(microphone.is_running());
    frames
        .recv_timeout(Duration::from_secs(3))
        .expect("capture callback");
}

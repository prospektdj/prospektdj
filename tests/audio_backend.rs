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
    std::thread::sleep(Duration::from_millis(20));
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

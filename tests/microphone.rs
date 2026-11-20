use crossbeam_channel::bounded;
use prospekt_dj::{
    engine::{Command, Engine},
    microphone::Resampler,
};
#[test]
fn microphone_gain_master_mute_disconnect_and_pfl_isolation() {
    let mut engine = Engine::new(48000);
    let (tx, rx) = bounded(8);
    engine.apply(Command::Microphone(Some(rx)));
    engine.apply(Command::MicVolume(0.5));
    engine.apply(Command::Master(1.));
    tx.send([0.4, 0.4]).unwrap();
    let (master, pfl) = engine.render();
    assert!((master[0] - 0.2).abs() < 0.00001);
    assert_eq!(pfl, [0., 0.]);
    engine.apply(Command::MicVolume(0.));
    tx.send([0.4, 0.4]).unwrap();
    assert_eq!(engine.render().0, [0., 0.]);
    engine.apply(Command::MicVolume(1.));
    engine.apply(Command::Master(0.));
    tx.send([0.4, 0.4]).unwrap();
    assert_eq!(engine.render().0, [0., 0.]);
    engine.apply(Command::Microphone(None));
    engine.apply(Command::Master(1.));
    assert_eq!(engine.render().0, [0., 0.]);
}
#[test]
fn resampling_produces_expected_duration_and_sanitizes_input() {
    for (input, output) in [(44100, 48000), (48000, 44100), (48000, 48000)] {
        let mut resampler = Resampler::new(input, output);
        let mut count = 0_i64;
        for n in 0..input {
            resampler.push(if n % 17 == 0 { f32::NAN } else { 0.5 }, |frame| {
                assert!(frame.iter().all(|x| x.is_finite() && x.abs() <= 1.));
                assert_eq!(frame[0], frame[1]);
                count += 1;
            });
        }
        assert!((count - i64::from(output)).abs() <= 1);
    }
}
#[test]
fn microphone_is_off_by_default() {
    let settings: prospekt_dj::settings::Settings = serde_json::from_str("{}").unwrap();
    assert!(!settings.mic_enabled);
}

#[test]
fn microphone_is_included_in_recorded_master_blocks() {
    use std::sync::{atomic::AtomicBool, Arc};
    let mut engine = Engine::new(48000);
    engine.apply(Command::Master(1.));
    engine.apply(Command::MicVolume(0.5));
    let (input, rx) = bounded(8);
    engine.apply(Command::Microphone(Some(rx)));
    let (sender, blocks) = bounded(1);
    engine.apply(Command::Recording(Some(
        prospekt_dj::recording::CaptureSink {
            sender,
            overflow: Arc::new(AtomicBool::new(false)),
        },
    )));
    input.send([0.4; 2]).unwrap();
    engine.render();
    engine.apply(Command::Recording(None));
    let block = blocks.try_recv().unwrap();
    assert_eq!(block.len, 1);
    assert!((block.frames[0][0] - 0.2).abs() < 0.00001);
}

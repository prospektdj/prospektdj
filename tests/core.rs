use prospekt_dj::{
    dsp::{cross_gains, Biquad},
    engine::{Command, DeckControl, Engine, PadMode},
    media::{self, Track},
    midi::{self, Encoding},
    recording::{self, Block, Format},
};
use std::{
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

fn track(seconds: f64, sr: u32) -> Arc<Track> {
    let frames = (0..(seconds * sr as f64) as usize)
        .map(|i| {
            let x = (i as f32 / sr as f32 * std::f32::consts::TAU * 220.).sin() * 0.7;
            [x, x * 0.5]
        })
        .collect();
    Arc::new(Track::from_frames("Test".into(), frames, sr))
}
#[test]
fn rgb_waveform_cache_distinguishes_bass_and_treble_at_fixed_time_resolution() {
    let tone = |hz: f32| {
        let frames = (0..48000)
            .map(|n| {
                let x = (n as f32 / 48000. * hz * std::f32::consts::TAU).sin();
                [x, x]
            })
            .collect();
        Track::from_frames("Tone".into(), frames, 48000)
    };
    let low = tone(80.);
    let high = tone(8000.);
    assert_eq!(low.detail.len(), 200);
    assert_eq!(low.detail_rate, 200.);
    let energy = |t: &Track, band: usize| t.detail.iter().map(|b| b.bands[band]).sum::<f32>();
    assert!(energy(&low, 0) > energy(&low, 2) * 5.);
    assert!(energy(&high, 2) > energy(&high, 0) * 5.);
    assert!(low.detail.iter().all(|b| b.level <= 1. && b.level >= 0.));
}
fn loaded() -> Engine {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(track(8., 48000))));
    e
}
fn run(engine: &mut Engine, frames: usize) {
    for _ in 0..frames {
        engine.render();
    }
}
fn temp(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("prospekt-rust-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    root.join(name)
}

#[test]
fn transport_rate_pause_and_resampling() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(track(8., 44100))));
    e.apply(Command::Deck(0, DeckControl::Play));
    run(&mut e, 48000);
    assert!((e.snapshot().decks[0].position - 1.).abs() < 1e-7);
    e.apply(Command::Deck(0, DeckControl::Rate(1.5)));
    run(&mut e, 48000);
    assert!((e.snapshot().decks[0].position - 2.5).abs() < 1e-7);
    e.apply(Command::Deck(0, DeckControl::Pause));
    run(&mut e, 1000);
    assert!((e.snapshot().decks[0].position - 2.5).abs() < 1e-7);
    e.apply(Command::Deck(0, DeckControl::Seek(100.)));
    e.apply(Command::Deck(0, DeckControl::Play));
    run(&mut e, 480);
    assert!(e.snapshot().decks[0].position < 0.1);
}
#[test]
fn cue_hot_cue_manual_and_beat_loops() {
    let mut e = loaded();
    e.apply(Command::Deck(0, DeckControl::Seek(2.)));
    e.apply(Command::Deck(0, DeckControl::SetCue));
    e.apply(Command::Deck(0, DeckControl::Seek(4.)));
    e.apply(Command::Deck(0, DeckControl::Cue));
    run(&mut e, 1);
    assert_eq!(e.snapshot().decks[0].position, 2.);
    e.apply(Command::Deck(0, DeckControl::Pad(0, false)));
    e.apply(Command::Deck(0, DeckControl::Seek(3.)));
    e.apply(Command::Deck(0, DeckControl::Pad(0, false)));
    run(&mut e, 1);
    assert_eq!(e.snapshot().decks[0].position, 2.);
    e.apply(Command::Deck(0, DeckControl::Loop(2.)));
    e.apply(Command::Deck(0, DeckControl::Play));
    run(&mut e, 72000);
    assert!((e.snapshot().decks[0].position - 2.5).abs() < 1e-7);
    e.apply(Command::Deck(0, DeckControl::Pause));
    e.apply(Command::Deck(0, DeckControl::LoopOff));
    e.apply(Command::Deck(0, DeckControl::PadMode(PadMode::ManualLoop)));
    e.apply(Command::Deck(0, DeckControl::Pad(0, false)));
    e.apply(Command::Deck(0, DeckControl::Seek(4.)));
    e.apply(Command::Deck(0, DeckControl::Pad(1, false)));
    assert_eq!(e.snapshot().decks[0].loop_range, Some((2.5, 4.)));
    e.apply(Command::Deck(0, DeckControl::Pad(2, false)));
    assert_eq!(e.snapshot().decks[0].loop_range, None);
}
#[test]
fn reverse_scratch_and_bend_are_temporary() {
    let mut e = loaded();
    e.apply(Command::Deck(0, DeckControl::Seek(3.)));
    e.apply(Command::Deck(0, DeckControl::Play));
    e.apply(Command::Deck(0, DeckControl::Jog(-1., 0.5)));
    run(&mut e, 24000);
    assert!((e.snapshot().decks[0].position - 2.5).abs() < 1e-6);
    run(&mut e, 24000);
    assert!((e.snapshot().decks[0].position - 3.).abs() < 1e-6);
    e.apply(Command::Deck(0, DeckControl::Bend(0.04)));
    run(&mut e, 48000);
    assert!((e.snapshot().decks[0].position - 4.04).abs() < 1e-6);
    assert_eq!(e.snapshot().decks[0].rate, 1.);
}
#[test]
fn sampler_works_without_decks_and_can_be_erased() {
    let mut e = Engine::new(48000);
    e.apply(Command::SampleLoad(0, track(1., 48000)));
    e.apply(Command::SampleTrigger(0));
    let peak = (0..1000)
        .map(|_| e.render().0[0].abs())
        .fold(0f32, f32::max);
    assert!(peak > 0.1);
    e.apply(Command::SampleClear(0));
    assert!(e.snapshot().samples[0].is_none());
    assert_eq!(e.render().0, [0.; 2]);
    e.apply(Command::Deck(0, DeckControl::Load(track(8., 48000))));
    e.apply(Command::Deck(0, DeckControl::Seek(1.)));
    e.apply(Command::Deck(0, DeckControl::PadMode(PadMode::Sampler)));
    e.apply(Command::Deck(0, DeckControl::Pad(1, false)));
    let sample = e.snapshot().samples[1].clone().unwrap();
    assert_eq!(sample.1, 48000.);
    assert_eq!(sample.2, 144000.);
}
#[test]
fn crossfader_limiter_pfl_and_sample_accurate_fade() {
    let center = cross_gains(0.5, false);
    assert!((center[0] * center[0] + center[1] * center[1] - 1.).abs() < 1e-6);
    let mut e = loaded();
    e.apply(Command::Deck(1, DeckControl::Load(track(8., 48000))));
    e.apply(Command::Deck(0, DeckControl::Play));
    e.apply(Command::Deck(0, DeckControl::Pfl));
    e.apply(Command::Cross(0.));
    let mut pfl = 0f32;
    for _ in 0..1000 {
        let (out, cue) = e.render();
        pfl = pfl.max(cue[0].abs());
        assert!(out[0].abs() <= 0.98);
    }
    assert!(pfl > 0.1);
    e.apply(Command::Fade {
        to: 1,
        seconds: 0.5,
        linear: false,
    });
    run(&mut e, 24001);
    assert!(!e.snapshot().decks[0].playing);
    assert!(e.snapshot().decks[1].playing);
    assert_eq!(e.snapshot().cross, 1.);
    for linear in [false, true] {
        e.apply(Command::Cross(1.));
        e.apply(Command::Deck(1, DeckControl::Play));
        e.apply(Command::Fade {
            to: 0,
            seconds: 0.1,
            linear,
        });
        run(&mut e, 5000);
        assert_eq!(e.snapshot().cross, 0.);
    }
}
#[test]
fn shelf_eq_attenuates_high_frequencies() {
    let mut filter = Biquad::default();
    filter.configure(0, -24., 48000);
    let mut in_energy = 0.;
    let mut out_energy = 0.;
    for i in 0..4800 {
        let x = (i as f32 / 48000. * 8000. * std::f32::consts::TAU).sin();
        let y = filter.process([x, x]);
        if i > 100 {
            in_energy += x * x;
            out_energy += y[0] * y[0];
        }
    }
    assert!(out_energy / in_energy < 0.1);
    filter.configure(0, 12., 48000);
    for _ in 0..1000 {
        assert!(filter.process([0., 0.])[0].is_finite());
    }
}
#[test]
fn bpm_detects_demo_and_rejects_silence() {
    assert!((media::estimate_bpm(&media::demo(0)).unwrap() - 120.).abs() < 1.);
    let silence = Track::from_frames("Silent".into(), vec![[0.; 2]; 48000 * 5], 48000);
    assert_eq!(media::estimate_bpm(&silence), None);
}
#[test]
fn midi_relative_encodings_and_legacy_map() {
    assert_eq!(midi::relative(127, Encoding::TwosComplement), -1);
    assert_eq!(midi::relative(63, Encoding::Offset64), -1);
    assert_eq!(midi::relative(65, Encoding::SignMagnitude), -1);
    assert_eq!(midi::key(&[128, 7, 127]), Some((144, 7, 0)));
    let maps = midi::import_json(
        br#"[{"key":"176:7","action":"cross","encoding":"twos"}]"#,
        &["cross".into()],
    )
    .unwrap();
    assert_eq!(maps[0].number, 7);
    assert_eq!(maps[0].status, 176);
    assert!(midi::import_json(
        br#"[{"key":"176:7","action":"unknown","encoding":"twos"}]"#,
        &["cross".into()]
    )
    .is_err());
}
#[test]
fn streamed_recording_wav_roundtrip_and_aiff() {
    let wav = temp("record.wav");
    let (sink, result) = recording::start(wav.clone(), Format::Wav, 48000).unwrap();
    let mut block = Block {
        len: 512,
        ..Default::default()
    };
    for (i, f) in block.frames.iter_mut().enumerate() {
        *f = [(i as f32 * 0.2).sin() * 0.5, 0.25];
    }
    sink.sender.send(block).unwrap();
    drop(sink);
    assert_eq!(
        result
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap(),
        wav
    );
    let decoded = media::load(&wav, false).unwrap();
    assert_eq!(decoded.frames.len(), 512);
    assert!((decoded.frames[100][1] - 0.25).abs() < 0.001);
    let aiff = temp("record.aiff");
    recording::export_wav(&wav, &aiff, Format::Aiff).unwrap();
    let decoded_aiff = media::load(&aiff, false).unwrap();
    assert_eq!(decoded_aiff.sample_rate, 48000);
    assert_eq!(decoded_aiff.frames.len(), 512);
}
#[test]
fn recording_failure_preserves_wav() {
    let wav = temp("overflow.wav");
    let (sink, result) = recording::start(wav.clone(), Format::Wav, 48000).unwrap();
    sink.overflow.store(true, Ordering::Relaxed);
    drop(sink);
    assert!(result
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
        .is_err());
    assert!(wav.is_file());
}
#[test]
fn restore_keeps_hotcues_eq_sampler_and_positions() {
    let mut e = loaded();
    e.apply(Command::Deck(0, DeckControl::Seek(2.)));
    e.apply(Command::Deck(0, DeckControl::Pad(0, false)));
    e.apply(Command::Deck(0, DeckControl::Eq(2, -12.)));
    e.apply(Command::SampleLoad(0, track(0.1, 48000)));
    run(&mut e, 1);
    let s = e.snapshot();
    let mut second = Engine::new(44100);
    second.apply(Command::Restore(Box::new(s)));
    assert_eq!(second.snapshot().decks[0].hot[0], Some(2.));
    assert_eq!(second.snapshot().decks[0].eq[2], -12.);
    assert!(second.snapshot().samples[0].is_some());
}

#[test]
fn ffmpeg_exports_all_optional_formats() {
    if std::env::var_os("PROSPEKT_TEST_FFMPEG").is_none() {
        return;
    }
    let wav = temp("codecs.wav");
    let mut writer = hound::WavWriter::create(
        &wav,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for i in 0..24000 {
        let sample = ((i as f32 * 0.05).sin() * 10000.) as i16;
        writer.write_sample(sample).unwrap();
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    for format in [Format::Mp3, Format::Flac, Format::Ogg, Format::Opus] {
        let output = temp(&format!("codecs.{}", format.extension()));
        recording::export_wav(&wav, &output, format).unwrap();
        let t = media::load(&output, false).unwrap();
        assert!(t.duration() > 0.4);
        assert!(t.frames.iter().any(|f| f[0].abs() > 0.1));
    }
}

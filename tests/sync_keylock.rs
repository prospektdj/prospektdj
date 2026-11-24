//! Phase SYNC, key lock and input sanitising regressions.
use prospekt_dj::{
    dsp::{Biquad, PitchShifter},
    engine::{self, Command, DeckControl, Engine, Snapshot},
    media::{self, Track},
};
use std::sync::Arc;

/// Clicks every beat starting at `offset` seconds.
fn clicks(bpm: f64, offset: f64, seconds: f64, sr: u32) -> Track {
    let period = 60. / bpm;
    let frames = (0..(seconds * sr as f64) as usize)
        .map(|n| {
            let t = n as f64 / sr as f64 - offset;
            let p = if t < 0. { 1e9 } else { t % period };
            let x = ((-p * 60.).exp() * (p * 900.).sin()) as f32 * 0.8;
            [x, x]
        })
        .collect();
    Track::from_frames("Click".into(), frames, sr)
}

fn with_grid(mut t: Track, bpm: f64) -> Arc<Track> {
    t.bpm = bpm;
    t.beat_offset = media::estimate_beat_offset(&t, bpm);
    Arc::new(t)
}

fn beat_phase(engine: &Engine, deck: usize, t: &Track) -> f64 {
    let s = engine.snapshot();
    ((s.decks[deck].position - t.beat_offset) * s.decks[deck].bpm / 60.).rem_euclid(1.)
}

#[test]
fn beat_offset_is_detected_within_one_hop() {
    let t = clicks(120., 0.137, 20., 48000);
    let offset = media::estimate_beat_offset(&t, 120.);
    assert!((offset - 0.137).abs() < 0.012, "offset {offset}");
}

#[test]
fn sync_matches_tempo_and_beat_phase() {
    let master = with_grid(clicks(128., 0.05, 30., 48000), 128.);
    let slave = with_grid(clicks(120., 0.31, 30., 48000), 120.);
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(master.clone())));
    e.apply(Command::Deck(1, DeckControl::Load(slave.clone())));
    e.apply(Command::Deck(0, DeckControl::Seek(5.2)));
    e.apply(Command::Deck(1, DeckControl::Seek(9.9)));
    e.apply(Command::Deck(0, DeckControl::Play));
    e.apply(Command::Sync { deck: 1, master: 0 });
    let s = e.snapshot();
    assert!((s.decks[1].rate * 120. - 128.).abs() < 1e-6);
    let diff = (beat_phase(&e, 0, &master) - beat_phase(&e, 1, &slave)).rem_euclid(1.);
    assert!(diff.min(1. - diff) < 0.02, "phase diff {diff}");
}

#[test]
fn sync_prefers_half_or_double_tempo_when_closer() {
    let master = with_grid(clicks(140., 0., 10., 48000), 140.);
    let slave = with_grid(clicks(72., 0., 10., 48000), 72.);
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(master)));
    e.apply(Command::Deck(1, DeckControl::Load(slave)));
    e.apply(Command::Sync { deck: 1, master: 0 });
    let rate = e.snapshot().decks[1].rate;
    assert!((rate * 72. - 70.).abs() < 1e-6, "rate {rate}");
}

#[test]
fn sync_never_follows_an_empty_deck() {
    let mut s = Snapshot::default();
    assert_eq!(engine::sync_master(&s, 0), None);
    s.decks[0].track = Some(Arc::new(clicks(120., 0., 2., 8000)));
    assert_eq!(engine::sync_master(&s, 0), None);
    s.deck_count = 4;
    s.decks[3].track = Some(Arc::new(clicks(120., 0., 2., 8000)));
    assert_eq!(engine::sync_master(&s, 0), Some(3));
    s.decks[1].track = Some(Arc::new(clicks(120., 0., 2., 8000)));
    assert_eq!(engine::sync_master(&s, 0), Some(1));
    s.decks[3].playing = true;
    assert_eq!(engine::sync_master(&s, 0), Some(3));
}

/// Dominant frequency by zero-crossing count.
fn frequency(samples: &[f32], sr: f64) -> f64 {
    let crossings = samples
        .windows(2)
        .filter(|w| w[0] <= 0. && w[1] > 0.)
        .count();
    crossings as f64 * sr / samples.len() as f64
}

#[test]
fn key_lock_keeps_pitch_while_tempo_changes() {
    let sr = 48000;
    let tone: Vec<[f32; 2]> = (0..sr * 6)
        .map(|n| {
            let x = (n as f32 / sr as f32 * 440. * std::f32::consts::TAU).sin() * 0.5;
            [x, x]
        })
        .collect();
    let track = Arc::new(Track::from_frames("Tone".into(), tone, sr));
    let measure = |lock: bool| {
        let mut e = Engine::new(sr);
        e.apply(Command::Cross(0.));
        e.apply(Command::Deck(0, DeckControl::Load(track.clone())));
        e.apply(Command::Deck(0, DeckControl::Rate(1.25)));
        if lock {
            e.apply(Command::Deck(0, DeckControl::KeyLock));
        }
        e.apply(Command::Deck(0, DeckControl::Play));
        let out: Vec<f32> = (0..sr as usize * 2).map(|_| e.render().0[0]).collect();
        (
            frequency(&out[sr as usize..], sr as f64),
            e.snapshot().decks[0].position,
        )
    };
    let (shifted, pos_free) = measure(false);
    let (locked, pos_locked) = measure(true);
    assert!((shifted - 550.).abs() < 15., "free {shifted}");
    assert!((locked - 440.).abs() < 15., "locked {locked}");
    assert!(
        (pos_free - pos_locked).abs() < 1e-9,
        "tempo must not change"
    );
}

#[test]
fn pitch_shifter_is_transparent_at_unity() {
    let mut p = PitchShifter::new(48000);
    for n in 0..1000 {
        let x = [n as f32, -(n as f32)];
        assert_eq!(p.process(x, 1.), x);
    }
}

#[test]
fn non_finite_controls_are_ignored() {
    let t = Arc::new(clicks(120., 0., 4., 48000));
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(t)));
    e.apply(Command::Deck(0, DeckControl::Seek(1.)));
    for c in [
        DeckControl::Seek(f64::NAN),
        DeckControl::Rate(f64::NAN),
        DeckControl::Bpm(f64::INFINITY),
        DeckControl::Volume(f32::NAN),
        DeckControl::Eq(0, f32::NAN),
        DeckControl::Bend(f64::NAN),
        DeckControl::Jog(f64::NAN, 1.),
    ] {
        e.apply(Command::Deck(0, c));
    }
    e.apply(Command::Cross(f32::NAN));
    e.apply(Command::Master(f32::NAN));
    let s = e.snapshot();
    let d = &s.decks[0];
    assert!((d.position - 1.).abs() < 1e-9);
    assert!(d.rate.is_finite() && d.bpm.is_finite() && d.volume.is_finite());
    assert!(d.eq[0].is_finite() && s.cross.is_finite() && s.master.is_finite());
    e.apply(Command::Deck(0, DeckControl::Play));
    for _ in 0..1000 {
        let (out, _) = e.render();
        assert!(out.iter().all(|x| x.is_finite()));
    }
}

#[test]
fn missing_bpm_falls_back_for_loops() {
    let mut t = clicks(120., 0., 8., 48000);
    t.bpm = 0.;
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(Arc::new(t))));
    e.apply(Command::Deck(0, DeckControl::Loop(4.)));
    let (a, b) = e.snapshot().decks[0].loop_range.unwrap();
    assert!((b - a - 2.).abs() < 1e-9, "4 beats at fallback 120 BPM");
}

#[test]
fn biquad_recovers_after_non_finite_input() {
    let mut f = Biquad::default();
    f.configure(0, 6., 48000);
    f.process([f32::NAN, f32::INFINITY]);
    let out = f.process([0.25, 0.25]);
    assert!(out.iter().all(|x| x.is_finite() && x.abs() > 0.));
}

#[test]
fn toggle_restart_counts_as_new_performance() {
    let t = Arc::new(clicks(120., 0., 0.5, 8000));
    let mut e = Engine::new(8000);
    e.apply(Command::Deck(0, DeckControl::Load(t)));
    e.apply(Command::Deck(0, DeckControl::Seek(10.)));
    e.apply(Command::Deck(0, DeckControl::Toggle));
    let s = e.snapshot();
    assert!(s.decks[0].playing);
    assert_eq!(s.decks[0].position, 0.);
}

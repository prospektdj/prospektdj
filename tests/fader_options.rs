//! Crossfader curves and switch, beat jump and tempo fader ranges.
use prospekt_dj::{
    dsp::{cross_curve_gains, cross_gains, CrossCurve},
    engine::{tempo_range, Command, DeckControl, Engine, TEMPO_RANGES},
    media::Track,
};
use std::sync::Arc;

fn tone(seconds: f64, sr: u32, bpm: f64) -> Arc<Track> {
    let frames = (0..(seconds * sr as f64) as usize)
        .map(|n| {
            let x = (n as f32 / sr as f32 * 220. * std::f32::consts::TAU).sin() * 0.5;
            [x, x]
        })
        .collect();
    let mut t = Track::from_frames("Tone".into(), frames, sr);
    t.bpm = bpm;
    Arc::new(t)
}

fn position(e: &Engine, deck: usize) -> f64 {
    e.snapshot().decks[deck].position
}

#[test]
fn smooth_curve_is_the_original_equal_power_curve() {
    for x in [0., 0.25, 0.5, 0.75, 1.] {
        assert_eq!(
            cross_curve_gains(x, CrossCurve::Smooth),
            cross_gains(x, false)
        );
    }
    let g = cross_curve_gains(0.5, CrossCurve::Smooth);
    assert!((g[0] * g[0] + g[1] * g[1] - 1.).abs() < 1e-6);
}

#[test]
fn const_curve_is_linear() {
    assert_eq!(cross_curve_gains(0.25, CrossCurve::Const), [0.75, 0.25]);
}

#[test]
fn sharp_curve_keeps_both_sides_open_in_the_middle() {
    assert_eq!(cross_curve_gains(0., CrossCurve::Sharp), [1., 0.]);
    assert_eq!(cross_curve_gains(1., CrossCurve::Sharp), [0., 1.]);
    assert_eq!(cross_curve_gains(0.5, CrossCurve::Sharp), [1., 1.]);
    assert_eq!(cross_curve_gains(0.75, CrossCurve::Sharp), [0.5, 1.]);
}

fn peak(e: &mut Engine, frames: usize) -> f32 {
    (0..frames)
        .map(|_| e.render().0[0].abs())
        .fold(0f32, f32::max)
}

#[test]
fn switching_the_crossfader_off_opens_both_sides() {
    let mut e = Engine::new(48000);
    e.apply(Command::Cross(0.));
    e.apply(Command::Deck(1, DeckControl::Load(tone(6., 48000, 120.))));
    e.apply(Command::Deck(1, DeckControl::Play));
    peak(&mut e, 24_000);
    let closed = peak(&mut e, 4_800);
    e.apply(Command::CrossEnabled(false));
    peak(&mut e, 24_000);
    let open = peak(&mut e, 4_800);
    assert!(
        closed < 0.01,
        "B side should be silent at fader 0: {closed}"
    );
    assert!(
        open > 0.05,
        "B side should be heard with the switch off: {open}"
    );
}

#[test]
fn beat_jump_moves_by_whole_beats_and_clamps() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(tone(20., 48000, 120.))));
    e.apply(Command::Deck(0, DeckControl::Seek(1.)));
    e.apply(Command::Deck(0, DeckControl::BeatJump(4.)));
    assert!(
        (position(&e, 0) - 3.).abs() < 1e-6,
        "4 beats at 120 BPM = 2 s"
    );
    e.apply(Command::Deck(0, DeckControl::BeatJump(-1.)));
    assert!((position(&e, 0) - 2.5).abs() < 1e-6);
    e.apply(Command::Deck(0, DeckControl::BeatJump(1e6)));
    assert!((position(&e, 0) - 20.).abs() < 1e-6, "clamped to track end");
    e.apply(Command::Deck(0, DeckControl::BeatJump(f64::NAN)));
    assert!((position(&e, 0) - 20.).abs() < 1e-6, "NaN ignored");
}

#[test]
fn tempo_fader_ranges_match_openfader_steps() {
    assert_eq!(TEMPO_RANGES, [0.06, 0.10, 0.16, 0.50]);
    assert_eq!(tempo_range(0), 0.06);
    assert_eq!(tempo_range(3), 0.50);
    assert_eq!(tempo_range(200), 0.50, "out-of-range index clamps");
}

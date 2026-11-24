use prospekt_dj::{
    effects::{Effects, FxSettings},
    engine::{Command, DeckControl, Engine},
    media::Track,
};
use std::sync::Arc;

#[test]
fn consecutive_controller_commands_preserve_other_effect_parameters() {
    use prospekt_dj::effects::{FxParam, FxSwitch};
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(
        0,
        DeckControl::FxParam(FxParam::EchoMix, 0.72),
    ));
    e.apply(Command::Deck(0, DeckControl::FxToggle(FxSwitch::Echo)));
    e.apply(Command::Deck(
        0,
        DeckControl::FxParam(FxParam::ReverbRoom, 0.9),
    ));
    let fx = e.snapshot().decks[0].fx;
    assert!(fx.echo_enabled);
    assert_eq!(fx.echo_mix, 0.72);
    assert_eq!(fx.reverb_room, 0.9);
    let mut processor = Effects::new(48000);
    let on = FxSettings {
        echo_enabled: true,
        echo_mix: 1.,
        echo_feedback: 0.,
        ..Default::default()
    };
    processor.configure(on, 120.);
    processor.process([1.; 2]);
    for _ in 0..200 {
        processor.process([0.; 2]);
    }
    processor.configure(FxSettings::default(), 120.);
    processor.configure(on, 120.);
    for _ in 0..15000 {
        assert_eq!(processor.process([0.; 2]), [0.; 2]);
    }
}

#[test]
fn neutral_and_bypass_are_exact_passthrough() {
    let mut fx = Effects::new(48000);
    for n in 0..1000 {
        let input = [(n as f32 * 0.17).sin() * 0.5, 0.25];
        assert_eq!(fx.process(input), input);
    }
    fx.configure(
        FxSettings {
            bypass: true,
            filter: 1.,
            echo_enabled: true,
            reverb_enabled: true,
            flanger_enabled: true,
            ..Default::default()
        },
        120.,
    );
    for _ in 0..100 {
        assert_eq!(fx.process([0.3, -0.2]), [0.3, -0.2]);
    }
}
#[test]
fn echo_hits_the_beat_and_follows_effective_bpm() {
    let mut fx = Effects::new(48000);
    fx.configure(
        FxSettings {
            echo_enabled: true,
            echo_mix: 1.,
            echo_feedback: 0.,
            echo_beats: 0.5,
            ..Default::default()
        },
        120.,
    );
    assert!((fx.echo_seconds() - 0.25).abs() < 1e-6);
    assert_eq!(fx.process([1., 0.5]), [0.; 2]);
    for _ in 1..12000 {
        assert_eq!(fx.process([0.; 2]), [0.; 2]);
    }
    let echo = fx.process([0.; 2]);
    assert!((echo[0] - 1.).abs() < 0.001);
    fx.configure(
        FxSettings {
            echo_enabled: true,
            ..Default::default()
        },
        60.,
    );
    assert!((fx.echo_seconds() - 0.5).abs() < 1e-6);
}
#[test]
fn reverb_produces_a_decaying_tail() {
    let mut fx = Effects::new(48000);
    fx.configure(
        FxSettings {
            reverb_enabled: true,
            reverb_mix: 1.,
            ..Default::default()
        },
        120.,
    );
    fx.process([1., 1.]);
    let mut early = 0.;
    let mut late = 0.;
    for i in 0..48000 * 3 {
        let y = fx.process([0.; 2]);
        assert!(y[0].is_finite());
        if i < 24000 {
            early += y[0] * y[0];
        }
        if i > 48000 * 2 {
            late += y[0] * y[0];
        }
    }
    assert!(early > 0.001);
    assert!(late < early * 0.1);
}
#[test]
fn filter_rejects_high_frequencies_and_flanger_changes_audio() {
    let mut filter = Effects::new(48000);
    filter.configure(
        FxSettings {
            filter: -0.8,
            ..Default::default()
        },
        120.,
    );
    let mut dry = 0.;
    let mut wet = 0.;
    for i in 0..4800 {
        let x = (i as f32 * 8000. / 48000. * std::f32::consts::TAU).sin();
        let y = filter.process([x, x]);
        if i > 100 {
            dry += x * x;
            wet += y[0] * y[0];
        }
    }
    assert!(wet < dry * 0.02);
    let mut flange = Effects::new(48000);
    flange.configure(
        FxSettings {
            flanger_enabled: true,
            ..Default::default()
        },
        120.,
    );
    let mut difference = 0.;
    for i in 0..10000 {
        let x = (i as f32 * 0.12).sin();
        difference += (flange.process([x, x])[0] - x).abs();
    }
    assert!(difference > 100.);
}
#[test]
fn hostile_params_are_sanitized_and_effect_chain_stays_finite() {
    let mut fx = Effects::new(44100);
    fx.configure(
        FxSettings {
            filter: f32::NAN,
            echo_enabled: true,
            echo_feedback: 100.,
            echo_ms: -1.,
            echo_mix: f32::INFINITY,
            reverb_enabled: true,
            reverb_room: 100.,
            flanger_enabled: true,
            flanger_rate: f32::NAN,
            ..Default::default()
        },
        f64::NAN,
    );
    for n in 0..100000 {
        let x = if n < 1000 { 1. } else { 0. };
        let out = fx.process([x, -x]);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 8.));
    }
}
#[test]
fn effects_reach_engine_output_and_survive_reload_and_restore() {
    let mut engine = Engine::new(48000);
    let settings = FxSettings {
        echo_enabled: true,
        echo_mix: 1.,
        echo_feedback: 0.,
        ..Default::default()
    };
    engine.apply(Command::Deck(0, DeckControl::Fx(settings)));
    let mut frames = vec![[0.; 2]; 20000];
    frames[0] = [1.; 2];
    engine.apply(Command::Deck(
        0,
        DeckControl::Load(Arc::new(Track::from_frames(
            "Impulse".into(),
            frames,
            48000,
        ))),
    ));
    assert_eq!(engine.snapshot().decks[0].fx, settings);
    engine.apply(Command::Cross(0.));
    engine.apply(Command::Deck(0, DeckControl::Play));
    assert_eq!(engine.render().0, [0.; 2]);
    for _ in 1..12000 {
        engine.render();
    }
    assert!(engine.render().0[0] > 0.01);
    let mut restored = Engine::new(44100);
    restored.apply(Command::Restore(Box::new(engine.snapshot())));
    assert_eq!(restored.snapshot().decks[0].fx, settings);
    let mut empty = Engine::new(48000);
    empty.apply(Command::Deck(1, DeckControl::Fx(settings)));
    restored.apply(Command::Restore(Box::new(empty.snapshot())));
    assert_eq!(restored.snapshot().decks[1].fx, settings);
}

#[test]
fn effects_mode_preserves_playback_cues_and_other_decks() {
    use prospekt_dj::engine::PadMode;
    let mut engine = Engine::new(48000);
    engine.apply(Command::Deck(
        0,
        DeckControl::Load(Arc::new(Track::from_frames(
            "Effects mode".into(),
            vec![[0.1; 2]; 48000],
            48000,
        ))),
    ));
    engine.apply(Command::Deck(0, DeckControl::HotCue(0, false)));
    engine.apply(Command::Deck(0, DeckControl::Play));
    let fx = FxSettings {
        echo_enabled: true,
        echo_mix: 0.7,
        ..Default::default()
    };
    engine.apply(Command::Deck(0, DeckControl::Fx(fx)));
    let before = engine.snapshot();
    engine.apply(Command::Deck(0, DeckControl::PadMode(PadMode::Effects)));
    engine.apply(Command::Deck(0, DeckControl::Pad(0, false)));
    let after = engine.snapshot();
    assert_eq!(after.decks[0].mode, PadMode::Effects);
    assert_eq!(after.decks[0].fx, fx);
    assert_eq!(after.decks[0].hot, before.decks[0].hot);
    assert_eq!(after.decks[0].position, before.decks[0].position);
    assert!(after.decks[0].playing);
    assert_eq!(after.decks[1].mode, before.decks[1].mode);
    assert_eq!(after.decks[1].fx, before.decks[1].fx);
    engine.apply(Command::Deck(0, DeckControl::PadMode(PadMode::HotCue)));
    assert_eq!(engine.snapshot().decks[0].fx, fx);
}

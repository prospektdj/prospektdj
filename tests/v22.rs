use prospekt_dj::{
    effects::{Effects, FxSettings},
    engine::{Command, DeckControl, Engine, PadMode},
    i18n, media, midi,
    settings::Settings,
};

#[test]
fn four_decks_mix_and_all_eight_cues_work_independently() {
    let mut engine = Engine::new(48000);
    engine.apply(Command::DeckCount(4));
    for i in 0..4 {
        let source = media::demo(i % 2);
        let frames = source
            .frames
            .iter()
            .map(|f| [f[0] * (1. - i as f32 * 0.1), f[1] * (1. - i as f32 * 0.1)])
            .collect();
        let distinct = std::sync::Arc::new(media::Track::from_frames(
            format!("Deck {i}"),
            frames,
            source.sample_rate,
        ));
        engine.apply(Command::Deck(i, DeckControl::Load(distinct)));
        engine.apply(Command::Deck(i, DeckControl::PadMode(PadMode::HotCue)));
        for p in 0..8 {
            engine.apply(Command::Deck(
                i,
                DeckControl::Seek((i * 8 + p) as f64 * 0.05),
            ));
            engine.apply(Command::Deck(i, DeckControl::Pad(p, false)));
        }
    }
    let state = engine.snapshot();
    for (i, deck) in state.decks.iter().enumerate() {
        for (p, cue) in deck.hot.iter().enumerate() {
            assert!((cue.unwrap() - (i * 8 + p) as f64 * 0.05).abs() < 1e-9);
        }
    }
    // Only C/D play: this must still produce master and PFL output.
    for i in 2..4 {
        engine.apply(Command::Deck(i, DeckControl::Play));
        engine.apply(Command::Deck(i, DeckControl::Pfl));
    }
    let mut master = 0.;
    let mut cue = 0.;
    for _ in 0..4800 {
        let (a, b) = engine.render();
        master += a[0].abs();
        cue += b[0].abs();
    }
    assert!(master > 1. && cue > 1.);
    for (i, deck) in state.decks.iter().enumerate().skip(2) {
        assert!(engine.snapshot().decks[i].position > deck.position);
    }
    let snapshot = engine.snapshot();
    let mut restored = Engine::new(44100);
    restored.apply(Command::Restore(Box::new(snapshot.clone())));
    for (i, deck) in snapshot.decks.iter().enumerate() {
        assert_eq!(restored.snapshot().decks[i].hot, deck.hot);
    }
    engine.apply(Command::Deck(3, DeckControl::Pad(7, true)));
    assert!(engine.snapshot().decks[3].hot[7].is_none());
    assert!(engine.snapshot().decks[2].hot[7].is_some());
}
#[test]
fn non_hotcue_modes_ignore_upper_four_pads() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(3, DeckControl::Load(media::demo(0))));
    for mode in [PadMode::Sampler, PadMode::AutoLoop, PadMode::ManualLoop] {
        e.apply(Command::Deck(3, DeckControl::PadMode(mode)));
        for p in 4..8 {
            e.apply(Command::Deck(3, DeckControl::Pad(p, false)));
        }
    }
    assert!(e.snapshot().samples.iter().all(Option::is_none));
    assert!(e.snapshot().decks[3].loop_range.is_none());
}
#[test]
fn each_new_effect_changes_audio_and_bypass_is_exact() {
    for unit in 0..4 {
        let mut s = FxSettings::default();
        match unit {
            0 => s.chorus_enabled = true,
            1 => s.tremolo_enabled = true,
            2 => s.distortion_enabled = true,
            _ => s.crusher_enabled = true,
        }
        let mut fx = Effects::new(48000);
        fx.configure(s, 120.);
        let mut difference = 0.;
        for n in 0..48000 {
            let x = ((n as f32 * 0.131).sin() * 0.21) + 0.0037;
            let y = fx.process([x, x]);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() <= 8.));
            difference += (y[0] - x).abs();
        }
        assert!(difference > 0.1, "unit {unit} did not alter signal");
        s.bypass = true;
        fx.configure(s, 120.);
        assert_eq!(fx.process([0.17, -0.2]), [0.17, -0.2]);
    }
}
#[test]
fn profiles_import_metadata_and_legacy_maps_without_guessed_assignments() {
    let actions = vec!["3.pad7".to_owned(), "2.play".to_owned()];
    let data=br#"{"manufacturer":"Test vendor","model":"Four deck","name":"My mapping","mappings":[{"status":144,"number":4,"action":"3.pad7","encoding":"TwosComplement"}]}"#;
    let p = midi::ControllerProfile::import(data, "file", &actions).unwrap();
    assert_eq!(p.model, "Four deck");
    assert_eq!(p.mappings[0].action, "3.pad7");
    let legacy = br#"[{"key":"144:9","action":"2.play","encoding":"twos"}]"#;
    assert_eq!(
        midi::ControllerProfile::import(legacy, "Legacy controller", &actions)
            .unwrap()
            .name,
        "Legacy controller"
    );
    assert!(midi::ControllerProfile::import(data, "file", &[]).is_err());
}
#[test]
fn language_catalogs_are_complete_for_every_declared_interface_key() {
    let catalog: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(include_str!("../assets/translations.json")).unwrap();
    assert_eq!(i18n::LANGUAGES.len(), 10);
    for (key, row) in catalog {
        assert_eq!(row.len(), 10, "{key}");
        assert!(row.iter().all(|s| !s.trim().is_empty()), "{key}");
        for (index, expected) in row.iter().enumerate() {
            assert_eq!(i18n::translate(&key, index), expected);
        }
    }
    assert_eq!(i18n::translate("Ustawienia", 1), "Settings");
    assert_eq!(i18n::translate("Ustawienia", 2), "设置");
}
#[test]
fn old_settings_and_new_effect_defaults_remain_compatible() {
    let s: Settings =
        serde_json::from_str(r#"{"effects":[{"echo_enabled":true},{}],"light":false}"#).unwrap();
    assert_eq!(s.effects.len(), 2);
    assert!(s.effects[0].echo_enabled);
    assert!(!s.effects[0].chorus_enabled);
    assert_eq!(s.deck_count, 2);
    assert_eq!(s.language, "pl");
    let defaults = Settings::default();
    assert_eq!(defaults.effects.len(), 4);
    assert!(defaults.controller_profiles.is_empty());
    let s = FxSettings {
        chorus_rate: f32::NAN,
        tremolo_rate: f32::INFINITY,
        drive: f32::NEG_INFINITY,
        crusher_bits: 900.,
        ..Default::default()
    }
    .sanitized();
    assert!(s.chorus_rate.is_finite() && s.tremolo_rate.is_finite() && s.drive.is_finite());
    assert_eq!(s.crusher_bits, 16.);
}

#[test]
fn hidden_channels_are_silent_in_two_deck_mode_including_effect_tails() {
    let mut e = Engine::new(48000);
    e.apply(Command::DeckCount(4));
    e.apply(Command::Deck(2, DeckControl::Load(media::demo(0))));
    e.apply(Command::Deck(2, DeckControl::Play));
    e.apply(Command::Deck(
        2,
        DeckControl::Fx(FxSettings {
            echo_enabled: true,
            ..Default::default()
        }),
    ));
    for _ in 0..12000 {
        e.render();
    }
    e.apply(Command::DeckCount(2));
    for _ in 0..24000 {
        let (master, cue) = e.render();
        assert_eq!(master, [0., 0.]);
        assert_eq!(cue, [0., 0.]);
    }
    assert!(!e.snapshot().decks[2].playing);
}
#[test]
fn arabic_joining_and_direction_are_prepared_for_the_renderer() {
    let shaped = i18n::shape("الإعدادات");
    assert_ne!(shaped, "الإعدادات");
    assert!(!shaped.is_empty());
    assert_eq!(i18n::shape("Polski"), "Polski");
}

use cpal::{BufferSize, SupportedBufferSize};
use prospekt_dj::{
    audio::{self, Backend, Routing},
    engine::Engine,
    theme,
};
#[test]
fn realtek_default_uses_driver_buffer_and_manual_buffer_has_safe_fallback() {
    let range = SupportedBufferSize::Range {
        min: 128,
        max: 1024,
    };
    assert_eq!(
        audio::buffer_attempts(None, range),
        vec![BufferSize::Default]
    );
    assert_eq!(
        audio::buffer_attempts(Some(576), range),
        vec![BufferSize::Fixed(576), BufferSize::Default]
    );
    assert_eq!(
        audio::buffer_attempts(Some(64), range),
        vec![BufferSize::Default]
    );
    assert_eq!(
        audio::buffer_attempts(Some(0), SupportedBufferSize::Unknown),
        vec![BufferSize::Default]
    );
}
#[test]
fn asio_channel_pairs_route_master_and_pfl_once_per_frame() {
    let routing = Routing {
        master: 2,
        cue: Some(4),
    };
    routing.validate(6).unwrap();
    let mut data = [9_f32; 18];
    let mut count = 0;
    audio::fill_output(&mut data, 6, routing, &mut || {
        count += 1;
        ([0.1, -0.2], [0.3, -0.4])
    });
    assert_eq!(count, 3);
    for frame in data.as_chunks::<6>().0 {
        assert_eq!(*frame, [0., 0., 0.1, -0.2, 0.3, -0.4]);
    }
    assert!(Routing {
        master: 0,
        cue: Some(0)
    }
    .validate(4)
    .is_err());
    assert!(Routing {
        master: 0,
        cue: Some(2)
    }
    .validate(2)
    .is_err());
    assert!(Routing {
        master: 1,
        cue: None
    }
    .validate(4)
    .is_err());
    assert!(Routing::default().validate(0).is_err());
}
#[test]
fn integer_32_float_64_unsigned_and_mono_output_are_valid() {
    let mut i32_data = [0_i32; 4];
    audio::fill_output(&mut i32_data, 2, Routing::default(), &mut || {
        ([1., -1.], [0.; 2])
    });
    assert_eq!(i32_data, [i32::MAX, i32::MIN, i32::MAX, i32::MIN]);
    let mut double = [0_f64; 2];
    audio::fill_output(&mut double, 2, Routing::default(), &mut || {
        ([0.25, -0.5], [0.; 2])
    });
    assert_eq!(double, [0.25, -0.5]);
    let mut silence = [0_u16; 4];
    audio::fill_output(&mut silence, 4, Routing::default(), &mut || {
        ([0.; 2], [0.; 2])
    });
    assert_eq!(silence, [32768; 4]);
    let mut mono = [0_f32; 2];
    audio::fill_output(&mut mono, 1, Routing::default(), &mut || {
        ([0.75, 0.25], [0.; 2])
    });
    assert_eq!(mono, [0.5; 2]);
    let mut nan = [1_f32; 2];
    audio::fill_output(&mut nan, 2, Routing::default(), &mut || {
        ([f32::NAN, f32::INFINITY], [0.; 2])
    });
    assert_eq!(nan, [0.; 2]);
}
#[test]
fn database_failure_does_not_disable_audio_engine_or_overwrite_future_database() {
    let path = std::env::temp_dir().join(format!(
        "prospekt-audio-recovery-{}.sqlite",
        std::process::id()
    ));
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("PRAGMA user_version=999").unwrap();
    let (mut engine, warning) = Engine::with_store_or_memory(48000, &path);
    assert!(warning.unwrap().contains("session-only"));
    assert!(engine.snapshot().database_failed);
    assert_eq!(engine.render().0, [0.; 2]);
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        999
    );
    drop(engine);
    drop(c);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn windows_light_os_event_cannot_replace_the_saved_dark_app_theme() {
    let ctx = egui::Context::default();
    theme::apply(&ctx, false);
    let _ = ctx.run(
        egui::RawInput {
            system_theme: Some(egui::Theme::Light),
            ..Default::default()
        },
        |ctx| {
            let visuals = &ctx.style().visuals;
            assert!(visuals.dark_mode);
            assert_eq!(visuals.panel_fill, egui::Color32::from_gray(8));
            assert_eq!(
                ctx.options(|o| o.theme_preference),
                egui::ThemePreference::Dark
            );
        },
    );
    theme::apply(&ctx, true);
    let _ = ctx.run(
        egui::RawInput {
            system_theme: Some(egui::Theme::Dark),
            ..Default::default()
        },
        |ctx| assert!(!ctx.style().visuals.dark_mode),
    );
}
#[test]
fn backend_and_buffer_defaults_migrate_old_settings() {
    let s: prospekt_dj::settings::Settings =
        serde_json::from_str(r#"{"latency_ms":12,"light":false}"#).unwrap();
    assert_eq!(s.audio_backend, Backend::System);
    assert_eq!(s.audio_buffer_frames, None);
    assert_eq!(s.audio_master_pair, 0);
    assert_eq!(s.audio_cue_pair, None);
    assert!(audio::backends().contains(&Backend::System));
    assert_eq!(
        audio::backends().contains(&Backend::Asio),
        cfg!(target_os = "windows")
    );
}

#[test]
fn device_failure_drop_finalizes_the_partial_recording_block() {
    use prospekt_dj::{
        engine::{Command, DeckControl},
        media, recording,
    };
    let path =
        std::env::temp_dir().join(format!("prospekt-device-drop-{}.wav", std::process::id()));
    let (sink, result) = recording::start(path.clone(), recording::Format::Wav, 48000).unwrap();
    let mut engine = Engine::new(48000);
    engine.apply(Command::Deck(0, DeckControl::Load(media::demo(0))));
    engine.apply(Command::Deck(0, DeckControl::Play));
    engine.apply(Command::Recording(Some(sink)));
    for _ in 0..101 {
        engine.render();
    }
    drop(engine);
    result
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap()
        .unwrap();
    let reader = hound::WavReader::open(&path).unwrap();
    assert_eq!(reader.duration(), 101);
    drop(reader);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn reopening_output_does_not_restore_stale_snapshot_over_newer_saved_cues() {
    use prospekt_dj::{
        engine::{Command, DeckControl},
        media::Track,
    };
    let path = std::env::temp_dir().join(format!(
        "prospekt-restart-cue-{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let track = std::sync::Arc::new(Track::from_frames(
        "Restart".into(),
        vec![[0.1; 2]; 48000],
        48000,
    ));
    let stale = {
        let mut engine = Engine::with_store(48000, &path).unwrap();
        engine.apply(Command::Deck(0, DeckControl::Load(track)));
        engine.apply(Command::Deck(0, DeckControl::HotCue(0, false)));
        let stale = engine.snapshot();
        engine.apply(Command::Deck(0, DeckControl::HotCue(0, true)));
        engine.flush_store().unwrap();
        stale
    };
    let mut engine = Engine::with_store(44100, &path).unwrap();
    engine.apply(Command::Restore(Box::new(stale)));
    assert_eq!(engine.snapshot().decks[0].hot[0], None);
    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn unavailable_asio_pfl_pair_preserves_valid_master_without_duplicating_outputs() {
    let (routing, warning) = audio::resolve_routing(
        Routing {
            master: 0,
            cue: Some(2),
        },
        2,
    )
    .unwrap();
    assert_eq!(routing, Routing::default());
    assert!(warning.unwrap().contains("PFL disabled"));
    assert!(audio::resolve_routing(
        Routing {
            master: 2,
            cue: None
        },
        2
    )
    .is_err());
    let (routing, warning) = audio::resolve_routing(
        Routing {
            master: 0,
            cue: Some(2),
        },
        4,
    )
    .unwrap();
    assert_eq!(routing.cue, Some(2));
    assert!(warning.is_none());
}
#[test]
fn emoji_comment_insertion_replaces_selection_with_unicode_caret_positions() {
    use prospekt_dj::music_view::insert_unicode;
    let mut comment = "Zażółć A\nB".to_owned();
    let cursor = insert_unicode(&mut comment, 7..8, "👩‍💻");
    assert_eq!(comment, "Zażółć 👩‍💻\nB");
    assert_eq!(cursor, 10);
    let cursor = insert_unicode(&mut comment, cursor..cursor, "❤️");
    assert_eq!(comment, "Zażółć 👩‍💻❤️\nB");
    assert_eq!(cursor, 12);
    let user = prospekt_dj::library_data::UserData {
        comment: comment.clone(),
        ..Default::default()
    };
    let saved = serde_json::to_string(&user).unwrap();
    let restored: prospekt_dj::library_data::UserData = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored.comment, comment);
}

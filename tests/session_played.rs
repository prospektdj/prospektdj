use prospekt_dj::{
    engine::{Command, DeckControl, Engine},
    media::Track,
};
use std::{path::PathBuf, sync::Arc};
#[test]
fn session_marker_counts_master_on_deck_c_survives_audio_restore_and_resets_on_new_session() {
    let path = PathBuf::from("session-fixture.wav");
    let mut track = Track::from_frames("Session test".into(), vec![[0.25, 0.25]; 1000], 1000);
    track.path = Some(path.clone());
    let mut engine = Engine::new(1000);
    engine.apply(Command::DeckCount(4));
    engine.apply(Command::Deck(2, DeckControl::Load(Arc::new(track))));
    assert!(engine.snapshot().session_played.is_empty());
    engine.apply(Command::Deck(2, DeckControl::Volume(0.)));
    engine.apply(Command::Deck(2, DeckControl::Pfl));
    engine.apply(Command::Deck(2, DeckControl::Play));
    for _ in 0..400 {
        engine.render();
    }
    assert!(engine.snapshot().session_played.is_empty());
    engine.apply(Command::Deck(2, DeckControl::Volume(0.8)));
    for _ in 0..501 {
        engine.render();
    }
    let snapshot = engine.snapshot();
    assert!(snapshot.session_played.contains(&path));
    let mut restored = Engine::new(1000);
    restored.apply(Command::Restore(Box::new(snapshot)));
    assert!(restored.snapshot().session_played.contains(&path));
    assert!(Engine::new(1000).snapshot().session_played.is_empty());
}

#[test]
fn emoji_visibility_defaults_on_and_remembers_disabled_preference() {
    let mut settings: prospekt_dj::settings::Settings = serde_json::from_str("{}").unwrap();
    assert!(settings.show_emojis);
    settings.show_emojis = false;
    let saved = serde_json::to_string(&settings).unwrap();
    let restored: prospekt_dj::settings::Settings = serde_json::from_str(&saved).unwrap();
    assert!(!restored.show_emojis);
}

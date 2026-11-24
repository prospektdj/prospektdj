use prospekt_dj::{
    engine::{Command, DeckControl, Engine},
    history,
    media::Track,
};
use std::sync::Arc;
#[test]
fn qualifying_master_plays_are_logged_once_per_play_and_repeated_plays_are_retained() {
    let root =
        std::env::temp_dir().join(format!("prospekt-session-journal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut track = Track::from_frames("Zażółć 🎶".into(), vec![[0.2; 2]; 1000], 1000);
    track.path = Some(root.join("test.wav"));
    track.artist = "Wykonawca".into();
    let track = Arc::new(track);
    let mut engine = Engine::with_store(1000, &root.join("tracks.sqlite")).unwrap();
    engine.apply(Command::DeckCount(4));
    engine.apply(Command::Deck(2, DeckControl::Load(track.clone())));
    engine.apply(Command::Deck(2, DeckControl::Volume(0.)));
    engine.apply(Command::Deck(2, DeckControl::Play));
    for _ in 0..300 {
        engine.render();
    }
    engine.flush_store().unwrap();
    assert!(history::sessions(&root.join("history")).unwrap().is_empty());
    engine.apply(Command::Deck(2, DeckControl::Volume(0.8)));
    for _ in 0..600 {
        engine.render();
    }
    engine.flush_store().unwrap();
    let sessions = history::sessions(&root.join("history")).unwrap();
    assert_eq!(sessions.len(), 1);
    let first = history::read(&root.join("history"), &sessions[0].id).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].deck, 2);
    assert_eq!(first[0].title, "Zażółć 🎶");
    assert!(!sessions[0].id.contains(':'));
    engine.apply(Command::Deck(2, DeckControl::Pause));
    engine.apply(Command::Deck(2, DeckControl::Load(track)));
    engine.apply(Command::Deck(2, DeckControl::Play));
    for _ in 0..600 {
        engine.render();
    }
    engine.flush_store().unwrap();
    let second = history::read(&root.join("history"), &sessions[0].id).unwrap();
    assert_eq!(second.len(), 2);
    assert_ne!(second[0].event_id, second[1].event_id);
    drop(engine);
    assert_eq!(
        history::view(&root.join("history"), Some(&sessions[0].id))
            .unwrap()
            .events
            .len(),
        2
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn separate_sessions_and_invalid_session_paths_are_handled() {
    let root = std::env::temp_dir().join(format!("prospekt-history-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let event = history::Play {
        event_id: "one".into(),
        track_id: "audio".into(),
        path: None,
        title: "Demo".into(),
        artist: "".into(),
        deck: 0,
        played_at: "2026-11-20T20:15:00+01:00".into(),
    };
    for (id, label) in [
        ("2026-11-20-20-15-00-1", "2026-11-20-20:15"),
        ("2026-11-21-20-15-00-2", "2026-11-21-20:15"),
    ] {
        let session = history::Session {
            id: id.into(),
            label: label.into(),
            created_at: label.into(),
        };
        history::append(&root, &session, &event).unwrap();
    }
    let sessions = history::sessions(&root).unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(sessions[0].id.starts_with("2026-11-21"));
    assert!(history::read(&root, "../outside").is_err());
    std::fs::remove_dir_all(root).unwrap();
}

use prospekt_dj::{
    engine::{Command, DeckControl, Engine},
    media::Track,
};
use std::sync::Arc;
#[test]
fn eject_clears_playback_preserves_mixer_saved_cues_and_other_decks() {
    let track = Arc::new(Track::from_frames(
        "Eject test".into(),
        vec![[0.25; 2]; 48000],
        48000,
    ));
    let mut engine = Engine::new(48000);
    engine.apply(Command::Deck(0, DeckControl::Load(track.clone())));
    engine.apply(Command::Deck(1, DeckControl::Load(track.clone())));
    engine.apply(Command::Deck(0, DeckControl::Volume(0.6)));
    engine.apply(Command::Deck(0, DeckControl::Gain(3.)));
    engine.apply(Command::Deck(0, DeckControl::Seek(0.2)));
    engine.apply(Command::Deck(0, DeckControl::HotCue(3, false)));
    engine.apply(Command::Deck(0, DeckControl::Play));
    engine.apply(Command::Deck(0, DeckControl::Unload));
    let snapshot = engine.snapshot();
    let deck = &snapshot.decks[0];
    assert!(deck.track.is_none());
    assert!(!deck.playing);
    assert_eq!(deck.hot, [None; 8]);
    assert_eq!(deck.volume, 0.6);
    assert_eq!(deck.gain_db, 3.);
    assert!(snapshot.decks[1].track.is_some());
    assert_eq!(engine.render().0, [0.; 2]);
    engine.apply(Command::Deck(0, DeckControl::Load(track)));
    assert_eq!(engine.snapshot().decks[0].hot[3], Some(0.2));
}

use prospekt_dj::{
    engine::{Command, DeckControl, Engine},
    media::Track,
};
use std::sync::Arc;
fn engine(position: f64) -> Engine {
    let mut engine = Engine::new(1000);
    engine.apply(Command::DeckCount(4));
    engine.apply(Command::Deck(
        3,
        DeckControl::Load(Arc::new(Track::from_frames(
            "Reverse".into(),
            vec![[0.2; 2]; 10000],
            1000,
        ))),
    ));
    engine.apply(Command::Deck(3, DeckControl::Seek(position)));
    engine.apply(Command::Deck(3, DeckControl::Play));
    engine
}
#[test]
fn held_reverse_runs_backwards_and_release_catches_up_to_forward_timeline() {
    let mut reversed = engine(3.);
    let mut forward = engine(3.);
    for engine in [&mut reversed, &mut forward] {
        engine.apply(Command::Deck(3, DeckControl::Rate(1.25)));
    }
    reversed.apply(Command::Deck(3, DeckControl::Reverse(true)));
    for _ in 0..800 {
        reversed.render();
        forward.render();
    }
    assert!((reversed.snapshot().decks[3].position - 2.).abs() < 1e-8);
    reversed.apply(Command::Deck(3, DeckControl::Reverse(false)));
    assert!(
        (reversed.snapshot().decks[3].position - forward.snapshot().decks[3].position).abs() < 1e-8
    );
    assert!(!reversed.snapshot().decks[3].reverse);
    assert!(reversed.snapshot().decks[3].playing);
}
#[test]
fn reverse_at_beginning_still_advances_slip_and_seek_cancels_reverse() {
    let mut engine = engine(0.1);
    engine.apply(Command::Deck(3, DeckControl::Reverse(true)));
    for _ in 0..1000 {
        engine.render();
    }
    assert!(engine.snapshot().decks[3].reverse);
    assert!(engine.snapshot().decks[3].playing);
    engine.apply(Command::Deck(3, DeckControl::Reverse(false)));
    assert!((engine.snapshot().decks[3].position - 1.1).abs() < 1e-8);
    engine.apply(Command::Deck(3, DeckControl::Reverse(true)));
    engine.apply(Command::Deck(3, DeckControl::Seek(5.)));
    assert!(!engine.snapshot().decks[3].reverse);
    engine.apply(Command::Deck(3, DeckControl::Reverse(false)));
    assert_eq!(engine.snapshot().decks[3].position, 5.);
}
#[test]
fn reverse_slip_respects_loop_timeline_and_unload_cancels_it() {
    let mut reversed = engine(0.5);
    let mut forward = engine(0.5);
    for engine in [&mut reversed, &mut forward] {
        engine.apply(Command::Deck(3, DeckControl::Loop(1.)));
    }
    reversed.apply(Command::Deck(3, DeckControl::Reverse(true)));
    for _ in 0..1300 {
        reversed.render();
        forward.render();
    }
    reversed.apply(Command::Deck(3, DeckControl::Reverse(false)));
    assert!(
        (reversed.snapshot().decks[3].position - forward.snapshot().decks[3].position).abs() < 1e-8
    );
    reversed.apply(Command::Deck(3, DeckControl::Reverse(true)));
    reversed.apply(Command::Deck(3, DeckControl::Unload));
    assert!(!reversed.snapshot().decks[3].reverse);
    assert!(reversed.snapshot().decks[3].track.is_none());
}

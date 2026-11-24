use prospekt_dj::{
    engine::{Command, Engine},
    media::Track,
};
use std::sync::Arc;
#[test]
fn preview_uses_phones_only_resamples_seeks_and_finishes_without_decks() {
    let mut engine = Engine::new(1000);
    let track = Arc::new(Track::from_frames(
        "Preview".into(),
        vec![[0.5, 0.25]; 2000],
        2000,
    ));
    engine.apply(Command::Preview(Some(track)));
    for _ in 0..100 {
        let (master, phones) = engine.render();
        assert_eq!(master, [0., 0.]);
        assert!(phones[0] > 0. && phones[1] > 0.);
    }
    assert!((engine.snapshot().preview_position - 0.1).abs() < 1e-8);
    assert!(engine.snapshot().decks.iter().all(|d| d.track.is_none()));
    assert!(!engine.snapshot().recording);
    engine.apply(Command::PreviewSeek(0.9));
    engine.apply(Command::PreviewSeek(f64::NAN));
    for _ in 0..101 {
        engine.render();
    }
    assert!(!engine.snapshot().preview_playing);
    assert_eq!(engine.render().1, [0., 0.]);
}
#[test]
fn stop_preview_immediately_silences_phones() {
    let mut engine = Engine::new(1000);
    engine.apply(Command::Preview(Some(Arc::new(Track::from_frames(
        "Preview".into(),
        vec![[0.4; 2]; 1000],
        1000,
    )))));
    assert!(engine.render().1[0] > 0.);
    engine.apply(Command::Preview(None));
    assert_eq!(engine.render().1, [0., 0.]);
}

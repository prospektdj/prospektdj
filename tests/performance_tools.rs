//! SLIP, BRAKE, QUANTIZE and loop slots.
use prospekt_dj::{
    engine::{Command, DeckControl, Engine},
    media::Track,
};
use std::sync::Arc;

fn track(seconds: f64, sr: u32, bpm: f64, offset: f64) -> Arc<Track> {
    let frames = (0..(seconds * sr as f64) as usize)
        .map(|n| {
            let x = (n as f32 / sr as f32 * 220. * std::f32::consts::TAU).sin() * 0.5;
            [x, x]
        })
        .collect();
    let mut t = Track::from_frames("Tone".into(), frames, sr);
    t.bpm = bpm;
    t.beat_offset = offset;
    Arc::new(t)
}

fn pos(e: &Engine) -> f64 {
    e.snapshot().decks[0].position
}

fn run(e: &mut Engine, seconds: f64) {
    for _ in 0..(seconds * 48_000.) as usize {
        e.render();
    }
}

#[test]
fn slip_keeps_the_hidden_playhead_moving_through_a_loop() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(
        0,
        DeckControl::Load(track(20., 48000, 120., 0.)),
    ));
    e.apply(Command::Deck(0, DeckControl::Seek(1.)));
    e.apply(Command::Deck(0, DeckControl::Slip));
    e.apply(Command::Deck(0, DeckControl::Play));
    e.apply(Command::Deck(0, DeckControl::Loop(1.)));
    run(&mut e, 1.);
    // Audible playback stays inside the one-beat loop [1.0, 1.5) ...
    assert!(pos(&e) >= 1.0 && pos(&e) < 1.5, "loop position {}", pos(&e));
    // ... while the hidden playhead has moved on by about one second.
    e.apply(Command::Deck(0, DeckControl::Slip));
    assert!((pos(&e) - 2.0).abs() < 0.02, "slip release at {}", pos(&e));
}

#[test]
fn brake_slows_to_a_stop_and_pauses() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(
        0,
        DeckControl::Load(track(20., 48000, 120., 0.)),
    ));
    e.apply(Command::Deck(0, DeckControl::Play));
    run(&mut e, 0.5);
    let before = pos(&e);
    e.apply(Command::Deck(0, DeckControl::Brake));
    assert!(e.snapshot().decks[0].braking);
    run(&mut e, 2.5);
    let s = e.snapshot();
    assert!(!s.decks[0].playing, "deck should pause once braked");
    assert!(!s.decks[0].braking);
    let travelled = pos(&e) - before;
    // Without braking 2.5 s would move the playhead 2.5 s; braking covers about 0.75 s.
    assert!(travelled > 0.5 && travelled < 1.0, "travelled {travelled}");
}

#[test]
fn quantize_snaps_hot_cues_to_the_beat_grid() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(
        0,
        DeckControl::Load(track(20., 48000, 120., 0.13)),
    ));
    e.apply(Command::Deck(0, DeckControl::Quantize));
    e.apply(Command::Deck(0, DeckControl::Seek(1.05)));
    e.apply(Command::Deck(0, DeckControl::HotCue(0, false)));
    let cue = e.snapshot().decks[0].hot[0].expect("cue set");
    assert!((cue - 1.13).abs() < 1e-6, "snapped to {cue}");
    e.apply(Command::Deck(0, DeckControl::Quantize));
    e.apply(Command::Deck(0, DeckControl::Seek(3.05)));
    e.apply(Command::Deck(0, DeckControl::HotCue(1, false)));
    let free = e.snapshot().decks[0].hot[1].expect("cue set");
    assert!((free - 3.05).abs() < 1e-6, "free cue {free}");
}

#[test]
fn loop_slots_capture_and_recall() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(
        0,
        DeckControl::Load(track(20., 48000, 120., 0.)),
    ));
    e.apply(Command::Deck(0, DeckControl::Seek(2.)));
    e.apply(Command::Deck(0, DeckControl::CaptureLoop(0)));
    let slot = e.snapshot().decks[0].loop_slots[0].expect("captured");
    assert!(
        (slot.0 - 2.).abs() < 1e-6 && (slot.1 - 4.).abs() < 1e-6,
        "{slot:?}"
    );
    e.apply(Command::Deck(0, DeckControl::Seek(10.)));
    e.apply(Command::Deck(0, DeckControl::RecallLoop(0)));
    let s = e.snapshot();
    assert_eq!(s.decks[0].loop_range, Some(slot));
    assert!(
        (s.decks[0].position - 2.).abs() < 1e-6,
        "jumps into the slot"
    );
    e.apply(Command::Deck(0, DeckControl::RecallLoop(3)));
    assert_eq!(
        e.snapshot().decks[0].loop_range,
        Some(slot),
        "empty slot ignored"
    );
}

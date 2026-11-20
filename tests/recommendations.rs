use prospekt_dj::{library::Entry, recommendations::rank};
use std::collections::BTreeSet;
fn entry(name: &str, bpm: Option<f64>, key: &str) -> Entry {
    Entry {
        path: name.into(),
        bpm,
        key: key.into(),
        genre: "House".into(),
        ..Entry::default()
    }
}
#[test]
fn matches_tempo_harmonic_wheel_and_ignores_loaded_or_missing() {
    let reference = entry("playing", Some(128.), "8A");
    let near = entry("near", Some(129.), "9A");
    let far = entry("far", Some(160.), "3A");
    let mut missing = entry("missing", Some(128.), "8A");
    missing.missing = true;
    let entries = vec![reference.clone(), far, missing, near];
    let loaded = BTreeSet::from([reference.path.clone()]);
    let result = rank(&entries, &[reference], &loaded);
    assert_eq!(result[0].path.to_str(), Some("near"));
    assert!(!result
        .iter()
        .any(|s| loaded.contains(&s.path) || s.path.to_str() == Some("missing")));
}
#[test]
fn live_reference_changes_ranking_and_handles_half_time() {
    let entries = vec![
        entry("slow", Some(64.), "Am"),
        entry("fast", Some(150.), "F#m"),
    ];
    let a = rank(&entries, &[entry("a", Some(128.), "8A")], &BTreeSet::new());
    let b = rank(&entries, &[entry("b", Some(150.), "11A")], &BTreeSet::new());
    assert_eq!(a[0].path.to_str(), Some("slow"));
    assert_eq!(b[0].path.to_str(), Some("fast"));
}
#[test]
fn no_playback_or_no_signals_does_not_invent_recommendations() {
    let mut unknown = entry("unknown", Some(f64::NAN), "invalid");
    unknown.genre.clear();
    assert!(rank(&[unknown.clone()], &[], &BTreeSet::new()).is_empty());
    assert!(rank(&[unknown], &[entry("a", None, "")], &BTreeSet::new()).is_empty());
}
#[test]
fn considers_all_playing_decks_and_is_deterministic() {
    let entries = vec![entry("z", Some(128.), "8A"), entry("a", Some(128.), "8A")];
    let references = vec![entry("r1", Some(128.), "8A"), entry("r2", Some(130.), "9A")];
    let results = rank(&entries, &references, &BTreeSet::new());
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].path.to_str(), Some("a"));
    assert!(results.iter().all(|s| s.score.is_finite()));
}

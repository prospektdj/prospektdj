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

#[test]
fn session_limit_counts_presentations_not_frames_and_allows_only_two() {
    use prospekt_dj::recommendations::{SessionMemory, Suggestion};
    let mut memory = SessionMemory::default();
    let candidates = vec![
        Suggestion {
            path: "a".into(),
            score: 1.,
            reason: String::new(),
        },
        Suggestion {
            path: "b".into(),
            score: 1.,
            reason: String::new(),
        },
    ];
    memory.remember("a".into());
    for _ in 0..120 {
        memory.remember("a".into());
    }
    assert_eq!(memory.count(std::path::Path::new("a")), 1);
    assert_eq!(memory.next_available(&candidates, false), Some(1));
    memory.clear_current();
    memory.remember("b".into());
    assert_eq!(memory.next_available(&candidates, false), Some(0));
    memory.clear_current();
    memory.remember("a".into());
    assert_eq!(memory.count(std::path::Path::new("a")), 2);
    let mut refreshed = candidates.clone();
    memory.filter(&mut refreshed);
    assert_eq!(refreshed.len(), 2); // current stays on screen until rotation
    memory.clear_current();
    memory.filter(&mut refreshed);
    assert_eq!(refreshed.len(), 1);
    assert_eq!(refreshed[0].path.to_str(), Some("b"));
    memory.remember("b".into());
    memory.clear_current();
    assert_eq!(memory.next_available(&candidates, false), None);
    assert_eq!(memory.exclusions().len(), 2);
    assert_eq!(SessionMemory::default().count(std::path::Path::new("a")), 0);
}
#[test]
fn context_switch_and_late_worker_results_cannot_reintroduce_exhausted_tracks() {
    use prospekt_dj::recommendations::{SessionMemory, Suggestion};
    let mut memory = SessionMemory::default();
    for _ in 0..2 {
        memory.remember("a".into());
        memory.clear_current();
    }
    let mut stale = vec![
        Suggestion {
            path: "a".into(),
            score: 100.,
            reason: String::new(),
        },
        Suggestion {
            path: "new".into(),
            score: 1.,
            reason: String::new(),
        },
    ];
    memory.filter(&mut stale);
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].path.to_str(), Some("new"));
}

#[test]
fn regeneration_notice_only_when_limits_exhaust_real_matches() {
    use prospekt_dj::recommendations::rank_session;
    let candidates = vec![entry("a", Some(128.), "8A")];
    let playing = vec![entry("playing", Some(128.), "8A")];
    let excluded = BTreeSet::from(["a".into()]);
    let batch = rank_session(&candidates, &playing, &BTreeSet::new(), &excluded);
    assert!(batch.restart);
    assert!(batch.results.is_empty());
    let no_play = rank_session(&candidates, &[], &BTreeSet::new(), &excluded);
    assert!(!no_play.restart);
    let available = rank_session(&candidates, &playing, &BTreeSet::new(), &BTreeSet::new());
    assert!(!available.restart);
    assert_eq!(available.results.len(), 1);
}

#[test]
fn recent_history_keeps_ten_actual_presentations_and_statistics_span_cycles() {
    use prospekt_dj::recommendations::{Presentation, SessionMemory, Statistics};
    let mut memory = SessionMemory::default();
    let mut stats = Statistics::default();
    for i in 0..12 {
        let path = std::path::PathBuf::from(format!("track-{i}"));
        for _ in 0..30 {
            if memory.remember(path.clone()) {
                stats.presented(Presentation {
                    at: i.to_string(),
                    path: path.clone(),
                    title: i.to_string(),
                    artist: "Artist".into(),
                    reason: "BPM".into(),
                    score: 100.,
                });
            }
        }
    }
    assert_eq!(stats.presentations, 12);
    assert_eq!(stats.unique_count(), 12);
    assert_eq!(stats.recent.len(), 10);
    assert_eq!(stats.recent.front().unwrap().title, "11");
    assert_eq!(stats.recent.back().unwrap().title, "2");
    memory.clear_current();
    assert!(memory.remember("track-11".into()));
    stats.presented(Presentation {
        at: "again".into(),
        path: "track-11".into(),
        title: "11".into(),
        artist: String::new(),
        reason: String::new(),
        score: 99.,
    });
    assert_eq!(stats.presentations, 13);
    assert_eq!(stats.unique_count(), 12);
    assert_eq!(stats.recent.len(), 10);
}
#[test]
fn recommendation_interval_defaults_to_sixty_and_is_serialized() {
    let mut settings: prospekt_dj::settings::Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(settings.recommendation_interval_seconds, 60);
    settings.recommendation_interval_seconds = 15;
    let restored: prospekt_dj::settings::Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(restored.recommendation_interval_seconds, 15);
}

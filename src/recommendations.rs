//! Local, deterministic recommendations over the entire indexed library.
use crate::{library::Entry, music_view::MusicalKey};
use std::{collections::BTreeSet, path::PathBuf};
#[derive(Clone, Debug)]
pub struct Suggestion {
    pub path: PathBuf,
    pub score: f64,
    pub reason: String,
}
pub fn rank(entries: &[Entry], playing: &[Entry], loaded: &BTreeSet<PathBuf>) -> Vec<Suggestion> {
    if playing.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    for candidate in entries
        .iter()
        .filter(|e| !e.missing && !loaded.contains(&e.path))
    {
        let mut total = 0.;
        let mut reasons = Vec::new();
        let mut signals = 0;
        for reference in playing {
            let mut score = 0.;
            if let (Some(a), Some(b)) = (candidate.bpm, reference.bpm) {
                if a.is_finite() && b.is_finite() && a > 0. && b > 0. {
                    let delta = [a / b, a * 2. / b, a / (2. * b)]
                        .into_iter()
                        .map(|r| r.ln().abs())
                        .fold(f64::INFINITY, f64::min);
                    score += 60. * (1. - delta / 0.15).max(0.);
                    signals += 1;
                    if delta < 0.06 {
                        reasons.push("zbliżone BPM");
                    }
                }
            }
            if let (Some(a), Some(b)) = (
                MusicalKey::parse(&candidate.key),
                MusicalKey::parse(&reference.key),
            ) {
                let distance = a.number.abs_diff(b.number);
                let compatible = a.number == b.number
                    || (a.minor == b.minor && (distance == 1 || distance == 11));
                if compatible {
                    score += 30.;
                    reasons.push("zgodna tonacja");
                }
                signals += 1;
            }
            if !candidate.genre.trim().is_empty()
                && candidate
                    .genre
                    .trim()
                    .eq_ignore_ascii_case(reference.genre.trim())
            {
                score += 10.;
                reasons.push("ten sam gatunek");
                signals += 1;
            }
            total += score;
        }
        if signals == 0 || total <= 0. {
            continue;
        }
        reasons.sort_unstable();
        reasons.dedup();
        results.push(Suggestion {
            path: candidate.path.clone(),
            score: total / playing.len() as f64,
            reason: reasons.join(" · "),
        });
    }
    results.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
    });
    results.truncate(50);
    results
}

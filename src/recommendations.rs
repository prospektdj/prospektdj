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

/// Counts distinct presentations, not UI frames or background ranking refreshes.
#[derive(Default)]
pub struct SessionMemory {
    shown: std::collections::BTreeMap<PathBuf, u8>,
    current: Option<PathBuf>,
}
impl SessionMemory {
    pub const LIMIT: u8 = 2;
    pub fn remember(&mut self, path: PathBuf) -> bool {
        let changed = self.current.as_ref() != Some(&path);
        if changed {
            let count = self.shown.entry(path.clone()).or_default();
            *count = count.saturating_add(1).min(Self::LIMIT);
        }
        self.current = Some(path);
        changed
    }
    pub fn clear_current(&mut self) {
        self.current = None;
    }
    pub fn has_seen(&self) -> bool {
        !self.shown.is_empty()
    }
    pub fn count(&self, path: &std::path::Path) -> u8 {
        self.shown.get(path).copied().unwrap_or(0)
    }
    pub fn exclusions(&self) -> BTreeSet<PathBuf> {
        self.shown
            .iter()
            .filter(|(path, count)| **count >= Self::LIMIT && self.current.as_ref() != Some(*path))
            .map(|(path, _)| path.clone())
            .collect()
    }
    pub fn filter(&self, results: &mut Vec<Suggestion>) {
        results.retain(|item| {
            self.count(&item.path) < Self::LIMIT || self.current.as_ref() == Some(&item.path)
        });
    }
    pub fn next_available(&self, results: &[Suggestion], backwards: bool) -> Option<usize> {
        // Prefer a different, less frequently shown candidate before repeating.
        let least = results
            .iter()
            .filter(|item| self.current.as_ref() != Some(&item.path))
            .map(|item| self.count(&item.path))
            .filter(|count| *count < Self::LIMIT)
            .min();
        if let Some(least) = least {
            let mut candidates = results.iter().enumerate().filter(|(_, item)| {
                self.current.as_ref() != Some(&item.path) && self.count(&item.path) == least
            });
            return if backwards {
                candidates.next_back()
            } else {
                candidates.next()
            }
            .map(|(index, _)| index);
        }
        results
            .iter()
            .position(|item| self.count(&item.path) < Self::LIMIT)
    }
}

pub struct Batch {
    pub elapsed_ms: f64,
    pub scanned: usize,
    pub results: Vec<Suggestion>,
    pub restart: bool,
}
pub fn rank_session(
    entries: &[Entry],
    playing: &[Entry],
    loaded: &BTreeSet<PathBuf>,
    excluded: &BTreeSet<PathBuf>,
) -> Batch {
    let started = std::time::Instant::now();
    let mut unavailable = loaded.clone();
    unavailable.extend(excluded.iter().cloned());
    let results = rank(entries, playing, &unavailable);
    // Restart only when the cycle limit, rather than missing matches, exhausted the pool.
    let restart =
        results.is_empty() && !excluded.is_empty() && !rank(entries, playing, loaded).is_empty();
    Batch {
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
        scanned: entries.len(),
        results,
        restart,
    }
}

#[derive(Clone, Debug)]
pub struct Presentation {
    pub at: String,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub reason: String,
    pub score: f64,
}
#[derive(Default)]
pub struct Statistics {
    pub recent: std::collections::VecDeque<Presentation>,
    pub batches: u64,
    pub discarded: u64,
    pub scanned: u64,
    pub last_candidates: usize,
    pub last_ms: f64,
    pub total_ms: f64,
    pub presentations: u64,
    pub restarts: u64,
    unique: BTreeSet<PathBuf>,
}
impl Statistics {
    pub fn batch(&mut self, batch: &Batch, discarded: bool) {
        self.batches += 1;
        self.discarded += u64::from(discarded);
        self.scanned = self.scanned.saturating_add(batch.scanned as u64);
        self.last_candidates = batch.results.len();
        self.last_ms = batch.elapsed_ms;
        self.total_ms += batch.elapsed_ms;
    }
    pub fn presented(&mut self, item: Presentation) {
        self.presentations += 1;
        self.unique.insert(item.path.clone());
        self.recent.push_front(item);
        self.recent.truncate(10);
    }
    pub fn unique_count(&self) -> usize {
        self.unique.len()
    }
    pub fn average_ms(&self) -> f64 {
        if self.batches == 0 {
            0.
        } else {
            self.total_ms / self.batches as f64
        }
    }
}

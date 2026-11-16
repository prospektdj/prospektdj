//! Persistent, metadata-only indexing: scanning never decodes entire tracks.
use anyhow::{Context, Result};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    tag::{Accessor, ItemKey},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub key: String,
    pub bpm: Option<f64>,
    pub duration: Option<f64>,
    pub bytes: u64,
    pub bitrate: Option<u32>,
    pub modified: u64,
    pub missing: bool,
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            genre: String::new(),
            key: String::new(),
            bpm: None,
            duration: None,
            bytes: 0,
            bitrate: None,
            modified: 0,
            missing: false,
        }
    }
}
impl Entry {
    pub fn placeholder(path: PathBuf) -> Self {
        Self {
            title: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path,
            ..Default::default()
        }
    }
    pub fn scan(path: PathBuf) -> Self {
        let mut e = Self::placeholder(path);
        let Ok(stat) = std::fs::metadata(&e.path) else {
            e.missing = true;
            return e;
        };
        e.bytes = stat.len();
        e.modified = stat
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(file) = lofty::probe::Probe::open(&e.path).and_then(|p| {
            p.options(lofty::config::ParseOptions::new().read_cover_art(false))
                .read()
        }) {
            let duration = file.properties().duration().as_secs_f64();
            e.bitrate = file.properties().audio_bitrate();
            if duration > 0. {
                e.duration = Some(duration);
            }
            if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
                if let Some(s) = tag.title() {
                    e.title = s.into_owned();
                }
                if let Some(s) = tag.artist() {
                    e.artist = s.into_owned();
                }
                if let Some(s) = tag.album() {
                    e.album = s.into_owned();
                }
                if let Some(s) = tag.genre() {
                    e.genre = s.into_owned();
                }
                e.key = tag
                    .get_string(&ItemKey::InitialKey)
                    .unwrap_or("")
                    .to_owned();
                e.bpm = tag
                    .get_string(&ItemKey::Bpm)
                    .or_else(|| tag.get_string(&ItemKey::IntegerBpm))
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|x| x.is_finite() && (20.0..=400.).contains(x));
            }
        }
        e
    }
    fn matches(&self, query: &str) -> bool {
        let text = format!(
            "{} {} {} {} {} {}",
            self.title,
            self.artist,
            self.album,
            self.genre,
            self.key,
            self.path.display()
        )
        .to_lowercase();
        query.split_whitespace().all(|word| text.contains(word))
    }
}
pub fn index(
    paths: Vec<PathBuf>,
    existing: Vec<PathBuf>,
    cache: Catalog,
    force: bool,
    mut publish: impl FnMut(Vec<Entry>),
) -> Vec<PathBuf> {
    let force = force || cache.schema < 1;
    let identity = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let mut known: BTreeMap<PathBuf, PathBuf> =
        existing.into_iter().map(|p| (identity(&p), p)).collect();
    let excluded: BTreeSet<_> = cache.excluded.iter().map(|p| identity(p)).collect();
    let mut seen = BTreeSet::new();
    let mut batch = Vec::new();
    let mut roots = Vec::new();
    let mut visit = |path: PathBuf| {
        if !crate::media::is_audio(&path) {
            return;
        }
        let key = identity(&path);
        if excluded.contains(&key) || !seen.insert(key.clone()) || seen.len() > 50000 {
            return;
        }
        let path = known.entry(key).or_insert(path).clone();
        let old = cache.entries.get(&path);
        let unchanged = std::fs::metadata(&path).ok().is_some_and(|s| {
            old.is_some_and(|e| {
                !e.missing
                    && e.bytes == s.len()
                    && e.modified
                        == s.modified()
                            .ok()
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0)
            })
        });
        let mut entry = if unchanged && !force {
            old.unwrap().clone()
        } else {
            Entry::scan(path)
        };
        if entry.missing {
            if let Some(old) = old {
                entry = old.clone();
                entry.missing = true;
            }
        }
        if unchanged && entry.bpm.is_none() {
            entry.bpm = old.and_then(|e| e.bpm);
        }
        batch.push(entry);
        if batch.len() >= 64 {
            publish(std::mem::take(&mut batch));
        }
    };
    for path in paths {
        if path.is_dir() {
            roots.push(path.clone());
            for entry in walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(Result::ok)
            {
                if entry.file_type().is_file() {
                    visit(entry.into_path());
                }
            }
        } else {
            visit(path);
        }
    }
    if !batch.is_empty() {
        publish(batch);
    }
    roots
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Crate {
    pub id: u64,
    pub name: String,
    pub tracks: BTreeSet<PathBuf>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub schema: u32,
    pub entries: BTreeMap<PathBuf, Entry>,
    pub crates: Vec<Crate>,
    pub roots: BTreeSet<PathBuf>,
    pub excluded: BTreeSet<PathBuf>,
}
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Favorites,
    Loaded,
    Missing,
    Folder(PathBuf),
    Crate(u64),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sort {
    #[default]
    Title,
    Artist,
    Album,
    Bpm,
    Key,
    Duration,
    Size,
    Bitrate,
}
pub struct Query<'a> {
    pub text: &'a str,
    pub scope: &'a Scope,
    pub sort: Sort,
    pub descending: bool,
    pub favorites: &'a BTreeSet<PathBuf>,
    pub loaded: [Option<&'a Path>; 2],
}
impl Catalog {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&tmp)?
            .sync_all()?;
        std::fs::rename(tmp, path).context("Nie można zapisać katalogu muzyki")
    }
    pub fn merge(&mut self, entry: Entry) {
        self.schema = 1;
        self.entries.insert(entry.path.clone(), entry);
    }
    pub fn add_crate(&mut self, name: &str) -> Option<u64> {
        let name = name.trim();
        if name.is_empty()
            || self
                .crates
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(name))
        {
            return None;
        }
        let id = self.crates.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        self.crates.push(Crate {
            id,
            name: name.to_owned(),
            tracks: BTreeSet::new(),
        });
        Some(id)
    }
    pub fn remove(&mut self, path: &Path) {
        self.excluded.insert(path.to_path_buf());
        self.entries.remove(path);
        for c in &mut self.crates {
            c.tracks.remove(path);
        }
    }
    pub fn visible(&self, paths: &[PathBuf], q: &Query<'_>) -> Vec<usize> {
        let search = q.text.to_lowercase();
        let mut rows: Vec<_> = paths
            .iter()
            .enumerate()
            .filter(|(_, path)| {
                let scope = match q.scope {
                    Scope::All => true,
                    Scope::Favorites => q.favorites.contains(*path),
                    Scope::Loaded => q.loaded.contains(&Some(path.as_path())),
                    Scope::Missing => self.entries.get(*path).is_some_and(|e| e.missing),
                    Scope::Folder(root) => path.starts_with(root),
                    Scope::Crate(id) => self
                        .crates
                        .iter()
                        .find(|c| c.id == *id)
                        .is_some_and(|c| c.tracks.contains(*path)),
                };
                scope
                    && (search.is_empty()
                        || self
                            .entries
                            .get(*path)
                            .map(|e| e.matches(&search))
                            .unwrap_or_else(|| {
                                path.to_string_lossy().to_lowercase().contains(&search)
                            }))
            })
            .map(|(i, _)| i)
            .collect();
        rows.sort_by(|&a, &b| {
            let x = self.entries.get(&paths[a]);
            let y = self.entries.get(&paths[b]);
            let order = match (x, y) {
                (Some(x), Some(y)) => match q.sort {
                    Sort::Title => x.title.to_lowercase().cmp(&y.title.to_lowercase()),
                    Sort::Artist => x.artist.to_lowercase().cmp(&y.artist.to_lowercase()),
                    Sort::Album => x.album.to_lowercase().cmp(&y.album.to_lowercase()),
                    Sort::Bpm => x.bpm.unwrap_or(0.).total_cmp(&y.bpm.unwrap_or(0.)),
                    Sort::Key => x.key.cmp(&y.key),
                    Sort::Duration => x
                        .duration
                        .unwrap_or(0.)
                        .total_cmp(&y.duration.unwrap_or(0.)),
                    Sort::Size => x.bytes.cmp(&y.bytes),
                    Sort::Bitrate => x.bitrate.cmp(&y.bitrate),
                },
                _ => paths[a].cmp(&paths[b]),
            };
            let order = order.then_with(|| paths[a].cmp(&paths[b]));
            if q.descending {
                order.reverse()
            } else {
                order
            }
        });
        rows
    }
}

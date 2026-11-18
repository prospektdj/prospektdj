//! Bounded background analysis and user-data cache, independent of loaded decks.
use crate::{media, track_store::CueBank};
use crossbeam_channel::{bounded, Receiver, Sender};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    thread::JoinHandle,
};
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UserData {
    pub rating: u8,
    pub rating_edited: bool,
    pub comment_edited: bool,
    pub comment: String,
    pub first_seen: i64,
    pub created: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Analysis {
    pub id: String,
    pub duration: f64,
    pub peaks: Vec<f32>,
    pub bands: Vec<[f32; 3]>,
}
#[derive(Clone, Debug)]
pub struct Record {
    pub analysis: Analysis,
    pub hot: CueBank,
    pub user: UserData,
    pub play_count: u64,
    pub first_play: Option<i64>,
    pub last_play: Option<i64>,
    pub previous_play: Option<i64>,
    pub sources: Vec<(String, crate::dj_import::ImportTrack)>,
}
enum Job {
    Read(PathBuf, i64),
    Edit(String, UserData),
}
pub struct LibraryData {
    tx: Option<Sender<Job>>,
    rx: Receiver<(PathBuf, Result<Record, String>)>,
    worker: Option<JoinHandle<()>>,
    pending: HashSet<PathBuf>,
    requested: HashMap<PathBuf, std::time::Instant>,
    overrides: HashMap<String, UserData>,
    pub records: HashMap<PathBuf, Record>,
    pub error: Option<String>,
}
fn timestamp(t: std::time::SystemTime) -> i64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_secs() as i64)
        .unwrap_or(0)
}
pub(crate) fn schema(c: &Connection) -> rusqlite::Result<()> {
    let version: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > 1 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    c.execute_batch("CREATE TABLE IF NOT EXISTS cue_banks(track_id TEXT PRIMARY KEY,version INTEGER NOT NULL,bank TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS waveform_cache(path TEXT PRIMARY KEY,size INTEGER NOT NULL,mtime INTEGER NOT NULL,analysis TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS user_tracks(track_id TEXT PRIMARY KEY,data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS plays(event_id TEXT PRIMARY KEY,track_id TEXT NOT NULL,played_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS imported_tracks(source TEXT NOT NULL,source_id TEXT NOT NULL,track_id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source,source_id));")
}
pub(crate) fn read(c: &Connection, path: &Path, first_seen: i64) -> anyhow::Result<Record> {
    let meta = std::fs::metadata(path).ok();
    let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|t| t.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0);
    let name = path.to_string_lossy();
    let cached: Option<String> = if meta.is_some() {
        c.query_row(
            "SELECT analysis FROM waveform_cache WHERE path=?1 AND size=?2 AND mtime=?3",
            params![name, size, modified],
            |r| r.get(0),
        )
        .optional()?
    } else {
        c.query_row(
            "SELECT analysis FROM waveform_cache WHERE path=?1",
            [&name],
            |r| r.get(0),
        )
        .optional()?
    };
    let analysis: Analysis = match cached {
        Some(text) => serde_json::from_str(&text)?,
        None => {
            let t = media::load(path, false)?;
            let analysis = Analysis {
                id: t.id.clone(),
                duration: t.duration(),
                peaks: t.peaks.clone(),
                bands: t.bands.clone(),
            };
            c.execute("INSERT INTO waveform_cache VALUES(?1,?2,?3,?4) ON CONFLICT(path) DO UPDATE SET size=excluded.size,mtime=excluded.mtime,analysis=excluded.analysis",params![name,size,modified,serde_json::to_string(&analysis)?])?;
            analysis
        }
    };
    let data: Option<String> = c
        .query_row(
            "SELECT data FROM user_tracks WHERE track_id=?1",
            [&analysis.id],
            |r| r.get(0),
        )
        .optional()?;
    let user = match data {
        Some(text) => serde_json::from_str(&text)?,
        None => {
            let user = UserData {
                first_seen,
                created: meta.as_ref().and_then(|m| m.created().ok()).map(timestamp),
                ..Default::default()
            };
            c.execute(
                "INSERT OR IGNORE INTO user_tracks VALUES(?1,?2)",
                params![analysis.id, serde_json::to_string(&user)?],
            )?;
            user
        }
    };
    let hot: Option<String> = c
        .query_row(
            "SELECT bank FROM cue_banks WHERE track_id=?1",
            [&analysis.id],
            |r| r.get(0),
        )
        .optional()?;
    let (play_count,first_play,last_play,previous_play)=c.query_row("SELECT count(*),min(played_at),max(played_at),max(CASE WHEN played_at < ?2 THEN played_at END) FROM plays WHERE track_id=?1",params![analysis.id,crate::track_store::session_start()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
    let mut statement = c.prepare(
        "SELECT source,payload FROM imported_tracks WHERE track_id=?1 ORDER BY source,source_id",
    )?;
    let mut sources = Vec::new();
    for row in statement.query_map([&analysis.id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (source, payload) = row?;
        sources.push((source, serde_json::from_str(&payload)?));
    }
    Ok(Record {
        sources,
        play_count,
        first_play,
        last_play,
        previous_play,
        analysis,
        hot: hot
            .map(|v| serde_json::from_str(&v))
            .transpose()?
            .unwrap_or([None; 8]),
        user,
    })
}
impl LibraryData {
    pub fn new(path: PathBuf) -> Self {
        let (tx, jobs) = bounded(16);
        let (events, rx) = bounded(32);
        let worker = std::thread::spawn(move || {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let connection = Connection::open(path)
                .map_err(|e| e.to_string())
                .and_then(|c| {
                    c.busy_timeout(std::time::Duration::from_secs(3))
                        .map_err(|e| e.to_string())?;
                    schema(&c).map_err(|e| e.to_string())?;
                    Ok(c)
                });
            for job in jobs {
                match job {
                    Job::Read(path, first_seen) => {
                        let result = match &connection {
                            Ok(c) => read(c, &path, first_seen).map_err(|e| e.to_string()),
                            Err(e) => Err(e.clone()),
                        };
                        if events.send((path, result)).is_err() {
                            break;
                        }
                    }
                    Job::Edit(id, user) => {
                        if let Ok(c) = &connection {
                            let result=serde_json::to_string(&user).map_err(|e|e.to_string()).and_then(|data|c.execute("INSERT INTO user_tracks VALUES(?1,?2) ON CONFLICT(track_id) DO UPDATE SET data=excluded.data",params![id,data]).map(|_|()).map_err(|e|e.to_string()));
                            if let Err(e) = result {
                                let _ = events.send((PathBuf::new(), Err(e)));
                            }
                        }
                    }
                }
            }
        });
        Self {
            tx: Some(tx),
            rx,
            worker: Some(worker),
            pending: HashSet::new(),
            requested: HashMap::new(),
            overrides: HashMap::new(),
            records: HashMap::new(),
            error: None,
        }
    }
    pub fn request(&mut self, path: &Path, first_seen: i64) {
        if self
            .requested
            .get(path)
            .is_some_and(|t| t.elapsed().as_secs() < 2)
        {
            return;
        }
        if self.pending.contains(path) {
            return;
        }
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(Job::Read(path.to_owned(), first_seen)).is_ok())
        {
            self.pending.insert(path.to_owned());
            if self.requested.len() > 4096 {
                self.requested.clear();
            }
            self.requested
                .insert(path.to_owned(), std::time::Instant::now());
        }
    }
    pub fn edit(&mut self, path: &Path, mut user: UserData) -> bool {
        let Some(record) = self.records.get(path) else {
            return false;
        };
        if user.rating != record.user.rating {
            user.rating_edited = true;
        }
        if user.comment != record.user.comment {
            user.comment_edited = true;
        }
        let id = record.analysis.id.clone();
        if !self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(Job::Edit(id.clone(), user.clone())).is_ok())
        {
            return false;
        }
        self.overrides.insert(id.clone(), user.clone());
        for record in self.records.values_mut() {
            if record.analysis.id == id {
                record.user = user.clone();
            }
        }
        true
    }
    pub fn poll(&mut self) {
        for (path, result) in self.rx.try_iter() {
            self.pending.remove(&path);
            match result {
                Ok(mut record) => {
                    if let Some(user) = self.overrides.get(&record.analysis.id) {
                        record.user = user.clone();
                    }
                    if self.records.len() > 512 {
                        self.records.clear();
                    }
                    self.records.insert(path, record);
                }
                Err(e) => self.error = Some(e),
            }
        }
    }
}
impl Drop for LibraryData {
    fn drop(&mut self) {
        self.tx.take(); // Drain responses while joining to avoid a full response queue deadlock.
        if let Some(worker) = self.worker.take() {
            while !worker.is_finished() {
                while self.rx.try_recv().is_ok() {}
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let _ = worker.join();
        }
    }
}

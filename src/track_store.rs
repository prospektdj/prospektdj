//! Versioned, transactional user data. SQLite is exclusively owned by a worker.
//! Track identity hashes decoded source audio, so tags and paths are not identity.
use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use rusqlite::{params, Connection};
use std::{collections::HashMap, path::Path, thread::JoinHandle};

pub type CueBank = [Option<f64>; 8];
enum Write {
    Cues(String, CueBank),
    Play(String, String),
    Flush(Sender<Result<(), String>>),
}
pub struct Store {
    tx: Option<Sender<Write>>,
    worker: Option<JoinHandle<()>>,
    pub errors: Receiver<String>,
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Store {
    /// Called before starting the audio stream; never called in its callback.
    pub fn open(path: &Path) -> Result<(Self, HashMap<String, CueBank>)> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path).context("Cannot open track database")?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        anyhow::ensure!(
            version <= 1,
            "Unsupported future track database version: {version}"
        );
        connection.busy_timeout(std::time::Duration::from_secs(3))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS cue_banks(track_id TEXT PRIMARY KEY, version INTEGER NOT NULL, bank TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS plays(event_id TEXT PRIMARY KEY,track_id TEXT NOT NULL,played_at INTEGER NOT NULL);
            PRAGMA user_version=1;")?;
        let mut banks = HashMap::new();
        {
            let mut statement =
                connection.prepare("SELECT track_id, bank, version FROM cue_banks")?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (id, bank, version) = row?;
                anyhow::ensure!(version == 1, "Unsupported Cue bank version: {version}");
                let bank: CueBank =
                    serde_json::from_str(&bank).context("Invalid stored cue bank")?;
                anyhow::ensure!(
                    bank.iter().flatten().all(|t| t.is_finite() && *t >= 0.),
                    "Invalid cue position"
                );
                banks.insert(id, bank);
            }
        }
        let (tx, rx) = unbounded();
        let (error_tx, errors) = unbounded();
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_failed = failed.clone();
        let worker = std::thread::spawn(move || {
            let mut failure = None;
            for write in rx {
                match write {
                    Write::Cues(id, bank) => {
                        let result = serde_json::to_string(&bank).map_err(|e| e.to_string()).and_then(|bank| {
                            connection.execute("INSERT INTO cue_banks(track_id,version,bank) VALUES(?1,1,?2)
                                ON CONFLICT(track_id) DO UPDATE SET version=1, bank=excluded.bank", params![id,bank]).map(|_|()).map_err(|e|e.to_string())
                        });
                        if let Err(error) = result {
                            worker_failed.store(true, std::sync::atomic::Ordering::Relaxed);
                            let _ = error_tx.send(error.clone());
                            failure = Some(error);
                        }
                    }
                    Write::Play(event, id) => {
                        if let Err(error) = connection.execute(
                            "INSERT OR IGNORE INTO plays VALUES(?1,?2,unixepoch())",
                            params![event, id],
                        ) {
                            worker_failed.store(true, std::sync::atomic::Ordering::Relaxed);
                            let error = error.to_string();
                            let _ = error_tx.send(error.clone());
                            failure = Some(error);
                        }
                    }
                    Write::Flush(reply) => {
                        let _ = reply.send(failure.clone().map_or(Ok(()), Err));
                    }
                }
            }
        });
        Ok((
            Self {
                tx: Some(tx),
                worker: Some(worker),
                errors,
                failed,
            },
            banks,
        ))
    }
    pub fn failed(&self) -> bool {
        self.failed.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn play(&self, event: String, id: String) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Write::Play(event, id));
        }
    }
    pub fn cues(&self, id: &str, bank: CueBank) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Write::Cues(id.to_owned(), bank));
        }
    }
    /// UI/shutdown/test only. FIFO barrier after all preceding writes.
    pub fn flush(&self) -> Result<(), String> {
        let (tx, rx) = unbounded();
        self.tx
            .as_ref()
            .ok_or("Database closed")?
            .send(Write::Flush(tx))
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn session_start() -> i64 {
    static START: std::sync::LazyLock<i64> =
        std::sync::LazyLock::new(|| chrono::Utc::now().timestamp());
    *START
}

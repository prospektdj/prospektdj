//! Append-only recommendation history, written off the UI/audio threads.
use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate};
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use std::{
    io::Write,
    path::{Path, PathBuf},
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub struct Logger {
    tx: Option<Sender<(DateTime<Local>, String)>>,
    worker: Option<JoinHandle<()>>,
    pub errors: Receiver<String>,
}
impl Logger {
    pub fn new(root: PathBuf) -> Self {
        let (tx, rx) = unbounded::<(DateTime<Local>, String)>();
        let (errors, error_rx) = unbounded();
        let worker = std::thread::spawn(move || {
            let report = |result: Result<()>| {
                if let Err(e) = result {
                    let _ = errors.send(format!("Historia rekomendacji: {e}"));
                }
            };
            report(cleanup(&root, Local::now().date_naive()));
            let mut last_cleanup = Instant::now();
            loop {
                match rx.recv_timeout(Duration::from_secs(3600)) {
                    Ok((at, line)) => report(append(&root, at, &line)),
                    Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }
                if last_cleanup.elapsed() >= Duration::from_secs(3600) {
                    report(cleanup(&root, Local::now().date_naive()));
                    last_cleanup = Instant::now();
                }
            }
        });
        Self {
            tx: Some(tx),
            worker: Some(worker),
            errors: error_rx,
        }
    }
    pub fn record(&self, line: String) {
        if let Some(tx) = &self.tx {
            let _ = tx.send((Local::now(), line));
        }
    }
}
impl Drop for Logger {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub fn append(root: &Path, at: DateTime<Local>, line: &str) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let path = root.join(format!("recommendations-{}.txt", at.format("%Y-%m-%d")));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(
        file,
        "[{}] {}",
        at.to_rfc3339(),
        line.replace(['\r', '\n'], " ")
    )?;
    file.sync_data()?;
    Ok(())
}
/// Only our dated TXT files are eligible; user files and symlinks are untouched.
pub fn cleanup(root: &Path, today: NaiveDate) -> Result<()> {
    std::fs::create_dir_all(root)?;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(date) = name
            .strip_prefix("recommendations-")
            .and_then(|s| s.strip_suffix(".txt"))
        else {
            continue;
        };
        let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
            continue;
        };
        if today.signed_duration_since(date).num_days() > 30 {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

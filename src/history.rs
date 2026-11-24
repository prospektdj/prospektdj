//! Per-application-session JSONL playback journals; all file I/O runs off the audio thread.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Play {
    pub event_id: String,
    pub track_id: String,
    pub path: Option<PathBuf>,
    pub title: String,
    pub artist: String,
    pub deck: usize,
    pub played_at: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub label: String,
    pub created_at: String,
}
pub fn session() -> &'static Session {
    static SESSION: OnceLock<Session> = OnceLock::new();
    SESSION.get_or_init(|| {
        let now = chrono::Local::now();
        Session {
            id: format!("{}-{}", now.format("%Y-%m-%d-%H-%M-%S"), std::process::id()),
            label: now.format("%Y-%m-%d-%H:%M").to_string(),
            created_at: now.to_rfc3339(),
        }
    })
}
pub fn append(root: &Path, session: &Session, event: &Play) -> Result<()> {
    let folder = root.join(&session.id);
    anyhow::ensure!(
        Path::new(&session.id).components().count() == 1 && session.id != "." && session.id != "..",
        "Invalid history session ID"
    );
    fs::create_dir_all(&folder)?;
    let metadata = folder.join("session.json");
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(metadata)
    {
        Ok(mut file) => {
            serde_json::to_writer(&mut file, session)?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let mut bytes = serde_json::to_vec(event)?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(folder.join("history.jsonl"))?;
    file.write_all(&bytes)?;
    file.sync_data()?;
    Ok(())
}
pub fn sessions(root: &Path) -> Result<Vec<Session>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(root)?.flatten() {
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let Some(session) = fs::read(path.join("session.json"))
            .ok()
            .and_then(|data| serde_json::from_slice::<Session>(&data).ok())
        else {
            continue;
        };
        if session.id == entry.file_name().to_string_lossy() && path.join("history.jsonl").is_file()
        {
            result.push(session);
        }
    }
    result.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(result)
}
pub fn read(root: &Path, id: &str) -> Result<Vec<Play>> {
    anyhow::ensure!(
        Path::new(id).components().count() == 1 && id != "." && id != "..",
        "Invalid history session ID"
    );
    let file = fs::File::open(root.join(id).join("history.jsonl"))?;
    let mut events = Vec::new();
    for (n, line) in BufReader::new(file).lines().enumerate() {
        anyhow::ensure!(
            n < 100_000,
            "Historia przekracza 100 000 wpisów; podziel plik sesji."
        );
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        events.push(
            serde_json::from_str(&line)
                .with_context(|| format!("Uszkodzony wpis historii: wiersz {}", n + 1))?,
        );
    }
    Ok(events)
}

#[derive(Clone, Debug, Default)]
pub struct View {
    pub sessions: Vec<Session>,
    pub events: Vec<Play>,
}
pub fn view(root: &Path, selected: Option<&str>) -> Result<View> {
    Ok(View {
        sessions: sessions(root)?,
        events: selected
            .map(|id| read(root, id))
            .transpose()?
            .unwrap_or_default(),
    })
}

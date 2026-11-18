//! Read-only import adapters. Recognized exports are deliberately explicit.
use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    SeratoCsv,
    VirtualDjXml,
    RekordboxXml,
    MixxxSqlite,
}
impl Source {
    pub const ALL: [(Self, &'static str); 4] = [
        (Self::SeratoCsv, "Serato history CSV"),
        (Self::VirtualDjXml, "VirtualDJ database.xml"),
        (Self::RekordboxXml, "rekordbox collection XML"),
        (Self::MixxxSqlite, "Mixxx mixxxdb.sqlite"),
    ];
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ImportTrack {
    pub source_id: String,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: Option<u32>,
    pub bpm: Option<f64>,
    pub duration: Option<f64>,
    pub rating: Option<u8>,
    pub comment: String,
    /// Original dates are preserved verbatim, including day-only precision.
    pub added: Option<String>,
    pub first_play: Option<String>,
    pub last_play: Option<String>,
    pub play_count: Option<u64>,
    pub hot: [Option<f64>; 8],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub source: Source,
    pub file: PathBuf,
    pub format: String,
    pub missing_paths: usize,
    pub tracks: Vec<ImportTrack>,
    pub playlists: Vec<(String, Vec<String>)>,
    pub limitations: Vec<String>,
}
fn path(value: &str, base: &Path) -> Result<PathBuf> {
    let p = if value.starts_with("file:") {
        let url = url::Url::parse(value)?;
        url.to_file_path()
            .map_err(|_| anyhow::anyhow!("Unsupported file URI: {value}"))?
    } else {
        PathBuf::from(value)
    };
    // Never silently map a foreign drive onto a similarly named local file.
    if p.is_absolute() || value.as_bytes().get(1) == Some(&b':') {
        Ok(p)
    } else {
        Ok(base.join(p))
    }
}
fn positive(value: Option<&str>) -> Option<f64> {
    value
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.)
}
fn text(node: roxmltree::Node<'_, '_>, key: &str) -> String {
    node.attribute(key).unwrap_or("").to_owned()
}
fn xml(file: &Path, source: Source) -> Result<Report> {
    anyhow::ensure!(
        std::fs::metadata(file)?.len() < 64 * 1024 * 1024,
        "XML exceeds 64 MiB"
    );
    let data = std::fs::read_to_string(file)?;
    let doc = roxmltree::Document::parse(&data)?;
    let root = doc.root_element();
    let base = file.parent().unwrap_or(Path::new("."));
    let mut report = Report {
        source,
        missing_paths: 0,
        file: file.to_owned(),
        format: root.tag_name().name().into(),
        tracks: vec![],
        playlists: vec![],
        limitations: vec![],
    };
    match source {
        Source::RekordboxXml => {
            anyhow::ensure!(
                root.has_tag_name("DJ_PLAYLISTS"),
                "Expected DJ_PLAYLISTS export"
            );
            for node in root
                .descendants()
                .filter(|n| n.has_tag_name("TRACK") && n.attribute("Location").is_some())
            {
                let mut t = ImportTrack {
                    source_id: text(node, "TrackID"),
                    path: path(node.attribute("Location").unwrap_or(""), base)?,
                    title: text(node, "Name"),
                    artist: text(node, "Artist"),
                    album: text(node, "Album"),
                    genre: text(node, "Genre"),
                    comment: text(node, "Comments"),
                    year: node.attribute("Year").and_then(|s| s.parse().ok()),
                    bpm: positive(node.attribute("AverageBpm")),
                    duration: positive(node.attribute("TotalTime")),
                    rating: node
                        .attribute("Rating")
                        .and_then(|s| s.parse::<u16>().ok())
                        .filter(|v| *v <= 255 && *v % 51 == 0)
                        .map(|v| (v / 51) as u8),
                    added: node.attribute("DateAdded").map(str::to_owned),
                    last_play: node.attribute("LastPlayed").map(str::to_owned),
                    play_count: node.attribute("PlayCount").and_then(|s| s.parse().ok()),
                    ..Default::default()
                };
                for cue in node.children().filter(|n| n.has_tag_name("POSITION_MARK")) {
                    if cue.attribute("Type") == Some("0") {
                        if let Some(slot) = cue
                            .attribute("Num")
                            .and_then(|v| v.parse::<usize>().ok())
                            .filter(|n| *n < 8)
                        {
                            t.hot[slot] = positive(cue.attribute("Start"));
                        }
                    }
                }
                report.tracks.push(t);
            }
            for node in root
                .descendants()
                .filter(|n| n.has_tag_name("NODE") && n.attribute("Type") == Some("1"))
            {
                let ids = node
                    .children()
                    .filter(|n| n.has_tag_name("TRACK"))
                    .filter_map(|n| n.attribute("Key").map(str::to_owned))
                    .collect();
                report.playlists.push((text(node, "Name"), ids));
            }
            report.limitations.push("Collection XML is not a complete session-history export. Memory Cue and loop markers are not imported as Hot Cue.".into());
        }
        Source::VirtualDjXml => {
            anyhow::ensure!(
                root.has_tag_name("VirtualDJ_Database"),
                "Expected VirtualDJ_Database export"
            );
            report.format = format!(
                "VirtualDJ database XML {}",
                root.attribute("Version").unwrap_or("unspecified")
            );
            for song in root.children().filter(|n| n.has_tag_name("Song")) {
                let tags = song.children().find(|n| n.has_tag_name("Tags"));
                let infos = song.children().find(|n| n.has_tag_name("Infos"));
                let scan = song.children().find(|n| n.has_tag_name("Scan"));
                let attr = |node: Option<roxmltree::Node<'_, '_>>, key: &str| {
                    node.and_then(|n| n.attribute(key)).map(str::to_owned)
                };
                let raw = song
                    .attribute("FilePath")
                    .context("Song without FilePath")?;
                let beat = attr(tags, "Bpm")
                    .or_else(|| attr(scan, "Bpm"))
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v > 0.);
                let mut t = ImportTrack {
                    source_id: raw.into(),
                    path: path(raw, base)?,
                    title: attr(tags, "Title").unwrap_or_default(),
                    artist: attr(tags, "Author").unwrap_or_default(),
                    album: attr(tags, "Album").unwrap_or_default(),
                    genre: attr(tags, "Genre").unwrap_or_default(),
                    comment: song
                        .children()
                        .find(|n| n.has_tag_name("Comment"))
                        .and_then(|n| n.text())
                        .unwrap_or("")
                        .into(),
                    bpm: beat.map(|b| 60. / b),
                    duration: attr(infos, "SongLength").and_then(|v| v.parse().ok()),
                    rating: attr(tags, "Stars")
                        .and_then(|v| v.parse::<u8>().ok())
                        .filter(|v| *v <= 5),
                    added: attr(infos, "FirstSeen"),
                    first_play: attr(infos, "FirstPlay"),
                    last_play: attr(infos, "LastPlay"),
                    play_count: attr(infos, "PlayCount").and_then(|v| v.parse().ok()),
                    ..Default::default()
                };
                for cue in song.children().filter(|n| {
                    n.has_tag_name("Poi")
                        && (n.attribute("Type") == Some("cue") || n.attribute("Type").is_none())
                }) {
                    if let Some(slot) = cue
                        .attribute("Num")
                        .and_then(|v| v.parse::<usize>().ok())
                        .filter(|n| (1..=8).contains(n))
                    {
                        t.hot[slot - 1] = positive(cue.attribute("Pos"));
                    }
                }
                report.tracks.push(t);
            }
            report.limitations.push("Only fields present in database.xml are available. Playlists and separate history files are not in this XML; foreign-drive paths require explicit repair.".into());
        }
        _ => bail!("Wrong XML adapter"),
    }
    Ok(report)
}
fn mixxx(file: &Path) -> Result<Report> {
    let c = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    c.busy_timeout(std::time::Duration::from_secs(3))?;
    c.execute_batch("BEGIN DEFERRED TRANSACTION")?; // Consistent SQLite snapshot includes committed WAL data.
    let fields: Vec<String> = c
        .prepare("PRAGMA table_info(library)")?
        .query_map([], |r| r.get(1))?
        .collect::<rusqlite::Result<_>>()?;
    for field in ["id", "location", "title", "artist"] {
        anyhow::ensure!(
            fields.iter().any(|f| f == field),
            "Unsupported Mixxx schema: missing {field}"
        );
    }
    let select = |name: &str| {
        if fields.iter().any(|f| f == name) {
            format!("l.\"{name}\"")
        } else {
            "NULL".into()
        }
    };
    let sql = format!(
        "SELECT l.id,tl.location,{} FROM library l JOIN track_locations tl ON tl.id=l.location",
        [
            "title",
            "artist",
            "album",
            "genre",
            "comment",
            "rating",
            "bpm",
            "duration",
            "datetime_added",
            "timesplayed",
            "last_played_at"
        ]
        .map(select)
        .join(",")
    );
    let mut statement = c.prepare(&sql)?;
    let tracks = statement
        .query_map([], |r| {
            let id: i64 = r.get(0)?;
            let rating: Option<i64> = r.get(7)?;
            Ok(ImportTrack {
                source_id: id.to_string(),
                path: PathBuf::from(r.get::<_, String>(1)?),
                title: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                artist: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                album: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                genre: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                comment: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                rating: rating.filter(|v| (0..=5).contains(v)).map(|v| v as u8),
                bpm: r.get(8)?,
                duration: r.get(9)?,
                added: r.get(10)?,
                play_count: r
                    .get::<_, Option<i64>>(11)?
                    .filter(|v| *v >= 0)
                    .map(|v| v as u64),
                last_play: r.get(12)?,
                ..Default::default()
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut playlists = vec![];
    for (list_sql, track_sql) in [
        (
            "SELECT id,name FROM crates",
            "SELECT track_id FROM crate_tracks WHERE crate_id=?1",
        ),
        (
            "SELECT id,name FROM Playlists",
            "SELECT track_id FROM PlaylistTracks WHERE playlist_id=?1 ORDER BY position",
        ),
    ] {
        if let Ok(mut stmt) = c.prepare(list_sql) {
            let lists = stmt
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (id, name) in lists {
                let ids = c
                    .prepare(track_sql)?
                    .query_map([id], |r| Ok(r.get::<_, i64>(0)?.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                playlists.push((name, ids));
            }
        }
    }
    Ok(Report{missing_paths:0,source:Source::MixxxSqlite,file:file.to_owned(),format:"Mixxx library joined through track_locations.id (schema feature detection)".into(),tracks,playlists,limitations:vec!["Native Cue positions and history playlists need additional schema-specific verification; only aggregate history fields are imported by this adapter.".into()]})
}
fn serato(file: &Path) -> Result<Report> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_path(file)?;
    let headers = reader.headers()?.clone();
    let columns: HashMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.trim().to_lowercase(), i))
        .collect();
    let path_column=["location","file path","filename","file"].iter().find_map(|name|columns.get(*name)).copied().context("Serato CSV needs an explicit Location/File Path column; title-only history cannot be matched safely")?;
    let mut tracks = vec![];
    for (row, record) in reader.records().enumerate() {
        let record = record?;
        let get = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| columns.get(*n).and_then(|i| record.get(*i)))
                .unwrap_or("")
                .to_owned()
        };
        let raw = record.get(path_column).unwrap_or("");
        if raw.is_empty() {
            continue;
        }
        tracks.push(ImportTrack {
            source_id: format!("{row}:{raw}"),
            path: path(raw, file.parent().unwrap_or(Path::new(".")))?,
            title: get(&["title", "song"]),
            artist: get(&["artist"]),
            album: get(&["album"]),
            genre: get(&["genre"]),
            comment: get(&["comment", "comments"]),
            last_play: {
                let date = get(&["start time", "date", "played at"]);
                if date.is_empty() {
                    None
                } else {
                    Some(date)
                }
            },
            ..Default::default()
        });
    }
    Ok(Report{missing_paths:0,source:Source::SeratoCsv,file:file.to_owned(),format:"UTF-8 Serato history CSV with explicit file paths".into(),tracks,playlists:vec![],limitations:vec!["Title-only exports require manual matching and are rejected. Native Serato database/crate/marker variants are not read by this adapter. A session CSV does not establish lifetime play counts.".into()]})
}
pub fn inspect(file: &Path, source: Source) -> Result<Report> {
    if source != Source::MixxxSqlite {
        anyhow::ensure!(
            std::fs::metadata(file)?.len() < 64 * 1024 * 1024,
            "Import export exceeds 64 MiB"
        );
    }
    let mut report = match source {
        Source::SeratoCsv => serato(file),
        Source::VirtualDjXml | Source::RekordboxXml => xml(file, source),
        Source::MixxxSqlite => mixxx(file),
    }?;
    report.missing_paths = report.tracks.iter().filter(|t| !t.path.is_file()).count();
    Ok(report)
}

#[derive(Clone, Debug)]
pub struct Applied {
    pub entries: Vec<crate::library::Entry>,
    pub lists: Vec<(String, Vec<PathBuf>)>,
    pub banks: Vec<(String, crate::track_store::CueBank)>,
    pub messages: Vec<String>,
}
/// Writes only Prospekt's database. Each completed track is an atomic transaction.
pub fn apply(
    report: &Report,
    database: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Applied> {
    let mut c = Connection::open(database)?;
    c.busy_timeout(std::time::Duration::from_secs(3))?;
    crate::library_data::schema(&c)?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS imported_tracks(source TEXT NOT NULL,source_id TEXT NOT NULL,track_id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source,source_id));")?;
    let source = std::fs::canonicalize(&report.file)
        .unwrap_or_else(|_| report.file.clone())
        .to_string_lossy()
        .into_owned();
    let mut out = Applied {
        entries: vec![],
        lists: vec![],
        banks: vec![],
        messages: report.limitations.clone(),
    };
    let mut paths = HashMap::new();
    for imported in &report.tracks {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            out.messages.push("Import cancelled. Completed track transactions were retained; repeating the import is safe.".into());
            break;
        }
        if !imported.path.is_file() {
            out.messages
                .push(format!("Missing path: {}", imported.path.display()));
            continue;
        }
        let record = match crate::library_data::read(&c, &imported.path, 0) {
            Ok(r) => r,
            Err(e) => {
                out.messages
                    .push(format!("{}: {e}", imported.path.display()));
                continue;
            }
        };
        let mut user = record.user;
        if user.rating == 0 && !user.rating_edited {
            user.rating = imported.rating.unwrap_or(0);
        } else if imported.rating.is_some_and(|r| r != user.rating) {
            out.messages
                .push(format!("Kept local rating: {}", imported.path.display()));
        }
        if user.comment.is_empty() && !user.comment_edited {
            user.comment = imported.comment.clone();
        } else if !imported.comment.is_empty() && user.comment != imported.comment {
            out.messages
                .push(format!("Kept local comment: {}", imported.path.display()));
        }
        let mut bank = record.hot;
        for (slot, value) in imported.hot.iter().enumerate() {
            if bank[slot].is_none() {
                bank[slot] = *value;
            } else if value.is_some() && bank[slot] != *value {
                out.messages.push(format!(
                    "Kept local Cue {}: {}",
                    slot + 1,
                    imported.path.display()
                ));
            }
        }
        let tx = c.transaction()?;
        tx.execute("INSERT INTO user_tracks VALUES(?1,?2) ON CONFLICT(track_id) DO UPDATE SET data=excluded.data",rusqlite::params![record.analysis.id,serde_json::to_string(&user)?])?;
        tx.execute("INSERT INTO cue_banks VALUES(?1,1,?2) ON CONFLICT(track_id) DO UPDATE SET bank=excluded.bank",rusqlite::params![record.analysis.id,serde_json::to_string(&bank)?])?;
        tx.execute("INSERT INTO imported_tracks VALUES(?1,?2,?3,?4) ON CONFLICT(source,source_id) DO UPDATE SET track_id=excluded.track_id,payload=excluded.payload",rusqlite::params![source,imported.source_id,record.analysis.id,serde_json::to_string(imported)?])?;
        tx.commit()?;
        let mut entry = crate::library::Entry::scan(imported.path.clone());
        for (current, value) in [
            (&mut entry.title, &imported.title),
            (&mut entry.artist, &imported.artist),
            (&mut entry.album, &imported.album),
            (&mut entry.genre, &imported.genre),
        ] {
            if !value.is_empty() {
                *current = value.clone();
            }
        }
        entry.bpm = imported.bpm.or(entry.bpm);
        entry.duration = imported.duration.or(entry.duration);
        entry.year = imported.year.or(entry.year);
        entry.rating = user.rating;
        paths.insert(imported.source_id.clone(), imported.path.clone());
        out.banks.push((record.analysis.id, bank));
        out.entries.push(entry);
    }
    for (name, ids) in &report.playlists {
        out.lists.push((
            format!("{:?} / {name}", report.source),
            ids.iter().filter_map(|id| paths.get(id).cloned()).collect(),
        ));
    }
    let log = database.with_file_name("last-import-report.json");
    let summary = serde_json::json!({"source":report.source,"file":source,"format":report.format,"imported":out.entries.len(),"messages":out.messages,"date":chrono::Utc::now().to_rfc3339()});
    std::fs::write(log, serde_json::to_vec_pretty(&summary)?)?;
    Ok(out)
}

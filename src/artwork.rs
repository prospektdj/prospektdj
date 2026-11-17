//! Bounded asynchronous artwork lookup. Audio files are read only; metadata never leaves
//! this module except artist/album/title and public MusicBrainz IDs sent to the services.
use crate::library::Entry;
use crossbeam_channel::{bounded, Receiver, Sender};
use lofty::file::TaggedFileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use unicode_normalization::UnicodeNormalization;
const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub fn identity(e: &Entry) -> String {
    hash(&format!(
        "{:?}|{}|{}|{}|{}|{}|{}|{}|{}",
        e.path,
        e.bytes,
        e.modified,
        e.artist,
        e.album,
        e.title,
        e.musicbrainz_release,
        e.musicbrainz_recording,
        e.version
    ))
}
fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn normalized(s: &str) -> String {
    s.nfkd()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Record {
    image: Option<String>,
    expires: u64,
    rejected: bool,
    source: String,
}
#[derive(Clone)]
pub struct Thumbnail {
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
}
pub struct Event {
    pub key: String,
    pub image: Option<Thumbnail>,
    pub status: String,
    pub retry_after: Duration,
    epoch: u64,
    token: u64,
}
struct Job {
    entry: Entry,
    key: String,
    token: u64,
    epoch: u64,
    force: bool,
}
enum Task {
    Lookup(Job),
    Manual(Job, PathBuf),
    Reject(Job),
    Clear,
}
struct NetResult {
    job: Job,
    result: Result<Option<Vec<u8>>, String>,
    query: String,
}
pub struct Artwork {
    tx: Sender<Task>,
    rx: Receiver<Event>,
    enabled: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
    tokens: HashMap<String, u64>,
    waiting: HashMap<String, Instant>,
}
impl Artwork {
    pub fn new(root: PathBuf, enabled: bool) -> Self {
        let (tx, rx) = bounded(32);
        let (out, events) = bounded(64);
        let (net, net_rx) = bounded::<Job>(16);
        let (done, done_rx) = bounded(16);
        let enabled = Arc::new(AtomicBool::new(enabled));
        let epoch = Arc::new(AtomicU64::new(0));
        let active = enabled.clone();
        let http_root = root.join("http");
        let generation = epoch.clone();
        std::thread::spawn(move || {
            let mut client = Http::new(http_root);
            while let Ok(job) = net_rx.recv() {
                if job.epoch != generation.load(Ordering::Relaxed) {
                    continue;
                }
                client.bypass = job.force;
                client.epoch = job.epoch;
                let query = query_key(&job.entry);
                let result = if active.load(Ordering::Relaxed) {
                    lookup(&mut client, &job.entry, &active)
                } else {
                    Err("Online artwork disabled".into())
                };
                if done.send(NetResult { job, result, query }).is_err() {
                    break;
                }
            }
        });
        let generation = epoch.clone();
        let active = enabled.clone();
        std::thread::spawn(move || {
            let _ = std::fs::create_dir_all(&root);
            let mut worker = LocalWorker {
                root,
                epoch: generation,
                active,
                out,
                net,
                versions: HashMap::new(),
            };
            loop {
                crossbeam_channel::select! {
                    recv(rx)->task=>match task{Ok(task)=>worker.task(task),Err(_)=>break},
                    recv(done_rx)->result=>match result{Ok(result)=>worker.complete(result),Err(_)=>break},
                }
            }
        });
        Self {
            tx,
            rx: events,
            enabled,
            epoch,
            tokens: HashMap::new(),
            waiting: HashMap::new(),
        }
    }
    pub fn enabled(&self, value: bool) {
        self.enabled.store(value, Ordering::Relaxed);
    }
    fn job(&mut self, e: &Entry, force: bool) -> Job {
        let key = identity(e);
        let token = self.tokens.entry(key.clone()).or_default();
        *token += 1;
        Job {
            entry: e.clone(),
            key,
            token: *token,
            epoch: self.epoch.load(Ordering::Relaxed),
            force,
        }
    }
    pub fn request(&mut self, e: &Entry) {
        let key = identity(e);
        if self
            .waiting
            .get(&key)
            .is_some_and(|until| *until > Instant::now())
        {
            return;
        }
        let job = self.job(e, false);
        if self.tx.try_send(Task::Lookup(job)).is_ok() {
            self.waiting
                .insert(key, Instant::now() + Duration::from_secs(90));
        }
    }
    pub fn force(&mut self, e: &Entry) -> bool {
        let job = self.job(e, true);
        self.submit(Task::Lookup(job), identity(e))
    }
    pub fn manual(&mut self, e: &Entry, path: PathBuf) -> bool {
        let job = self.job(e, true);
        self.submit(Task::Manual(job, path), identity(e))
    }
    pub fn reject(&mut self, e: &Entry) -> bool {
        let job = self.job(e, true);
        self.submit(Task::Reject(job), identity(e))
    }
    fn submit(&mut self, task: Task, key: String) -> bool {
        if self.tx.try_send(task).is_ok() {
            self.waiting
                .insert(key, Instant::now() + Duration::from_secs(90));
            true
        } else {
            self.waiting.remove(&key);
            false
        }
    }
    pub fn clear(&mut self) -> bool {
        if self.tx.is_full() {
            return false;
        }
        self.epoch.fetch_add(1, Ordering::Relaxed);
        self.waiting.clear();
        self.tx.try_send(Task::Clear).is_ok()
    }
    pub fn forget(&mut self, key: &str) {
        self.waiting.remove(key);
    }
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(e) = self.rx.try_recv() {
            if e.epoch == self.epoch.load(Ordering::Relaxed)
                && self.tokens.get(&e.key) == Some(&e.token)
            {
                self.waiting
                    .insert(e.key.clone(), Instant::now() + e.retry_after);
                events.push(e);
            }
        }
        if self.waiting.len() > 4096 {
            self.waiting.retain(|_, v| *v > Instant::now());
            if self.waiting.len() > 4096 {
                self.waiting.clear();
            }
            self.tokens.retain(|k, _| self.waiting.contains_key(k));
        }
        events
    }
}
struct LocalWorker {
    root: PathBuf,
    epoch: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    out: Sender<Event>,
    net: Sender<Job>,
    versions: HashMap<String, u64>,
}
impl LocalWorker {
    fn task(&mut self, task: Task) {
        match task {
            Task::Clear => {
                clear_downloads(&self.root);
                self.versions.clear();
            }
            Task::Lookup(job) => self.lookup(job),
            Task::Reject(job) => {
                self.versions.remove(&job.key);
                let rec = Record {
                    rejected: true,
                    expires: u64::MAX,
                    source: "Rejected".into(),
                    ..Default::default()
                };
                save_record(&self.root, &format!("t-{}", job.key), &rec);
                let image = embedded(&job.entry.path);
                let source = if image.is_some() {
                    "Embedded"
                } else {
                    "Rejected"
                };
                publish(&self.out, &job, image, source, Duration::from_secs(86400));
            }
            Task::Manual(job, path) => {
                self.versions.remove(&job.key);
                match read_limited(&path).and_then(|b| decode(&b)) {
                    Some(image) => {
                        store_image(&self.root, &job, &image, "Manual");
                        let (image, source) = embedded(&job.entry.path)
                            .map(|img| (img, "Embedded"))
                            .unwrap_or((image, "Manual"));
                        publish(
                            &self.out,
                            &job,
                            Some(image),
                            source,
                            Duration::from_secs(86400),
                        );
                    }
                    None => publish(
                        &self.out,
                        &job,
                        None,
                        "Invalid image",
                        Duration::from_secs(30),
                    ),
                }
            }
        }
    }
    fn lookup(&mut self, job: Job) {
        if job.epoch != self.epoch.load(Ordering::Relaxed) {
            return;
        }
        self.versions.remove(&job.key);
        // Fresh tag read always precedes downloaded artwork; no audio decode.
        if let Some(image) = embedded(&job.entry.path) {
            publish(
                &self.out,
                &job,
                Some(image),
                "Embedded",
                Duration::from_secs(86400),
            );
            return;
        }
        if !job.force {
            if let Some((image, rec)) = cached(&self.root, &format!("t-{}", job.key))
                .or_else(|| cached(&self.root, &query_key(&job.entry)))
            {
                let retry = Duration::from_secs(rec.expires.saturating_sub(now()).min(86400));
                publish(&self.out, &job, image, &rec.source, retry);
                return;
            }
        }
        if self.active.load(Ordering::Relaxed) {
            self.versions.insert(job.key.clone(), job.token);
            if let Err(err) = self.net.try_send(job) {
                let job = err.into_inner();
                self.versions.remove(&job.key);
                publish(
                    &self.out,
                    &job,
                    None,
                    "Artwork queue busy",
                    Duration::from_secs(3),
                );
            }
        } else {
            publish(
                &self.out,
                &job,
                None,
                "No local artwork",
                Duration::from_secs(60),
            );
        }
    }
    fn complete(&mut self, result: NetResult) {
        let job = result.job;
        if job.epoch != self.epoch.load(Ordering::Relaxed)
            || self.versions.remove(&job.key) != Some(job.token)
        {
            return;
        }
        match result.result {
            Ok(Some(bytes)) => {
                if let Some(image) = decode(&bytes) {
                    let rec = store_image(&self.root, &job, &image, "Online");
                    save_record(&self.root, &result.query, &rec);
                    publish(
                        &self.out,
                        &job,
                        Some(image),
                        "Online",
                        Duration::from_secs(86400),
                    );
                } else {
                    publish(
                        &self.out,
                        &job,
                        None,
                        "Invalid server image",
                        Duration::from_secs(600),
                    );
                }
            }
            other => {
                let (status, ttl) = match other {
                    Ok(None) => ("No verified match".to_owned(), 86400),
                    Err(err) => (err, 600),
                    _ => unreachable!(),
                };
                let rec = Record {
                    expires: now() + ttl,
                    source: status.clone(),
                    ..Default::default()
                };
                save_record(&self.root, &format!("t-{}", job.key), &rec);
                save_record(&self.root, &result.query, &rec);
                publish(&self.out, &job, None, &status, Duration::from_secs(ttl));
            }
        }
    }
}
fn publish(
    out: &Sender<Event>,
    job: &Job,
    image: Option<Thumbnail>,
    status: &str,
    retry_after: Duration,
) {
    let _ = out.send(Event {
        key: job.key.clone(),
        image,
        status: status.into(),
        retry_after,
        epoch: job.epoch,
        token: job.token,
    });
}
fn query_key(e: &Entry) -> String {
    format!(
        "q-{}",
        hash(&format!(
            "{}|{}|{}|{}|{}|{}",
            normalized(&e.artist),
            normalized(&e.album),
            normalized(&e.title),
            e.musicbrainz_release,
            e.musicbrainz_recording,
            e.version
        ))
    )
}
fn read_limited(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        None
    } else {
        Some(bytes)
    }
}
pub fn decode(bytes: &[u8]) -> Option<Thumbnail> {
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().ok()?;
    let img = if img.width() > 160 || img.height() > 160 {
        img.thumbnail(160, 160)
    } else {
        img
    }
    .to_rgba8();
    Some(Thumbnail {
        size: [img.width() as usize, img.height() as usize],
        rgba: img.into_raw(),
    })
}
fn embedded(path: &Path) -> Option<Thumbnail> {
    let file = lofty::probe::Probe::open(path).ok()?.read().ok()?;
    file.primary_tag()
        .or_else(|| file.first_tag())?
        .pictures()
        .iter()
        .find_map(|pic| decode(pic.data()))
}
fn save_record(root: &Path, key: &str, rec: &Record) {
    if let Ok(bytes) = serde_json::to_vec(rec) {
        let path = root.join(format!("{key}.json"));
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}
fn cached(root: &Path, key: &str) -> Option<(Option<Thumbnail>, Record)> {
    let rec: Record =
        serde_json::from_slice(&read_limited(&root.join(format!("{key}.json")))?).ok()?;
    if rec.expires < now() {
        return None;
    }
    let image = if let Some(name) = &rec.image {
        if name.contains('/') || name.contains('\\') {
            return None;
        }
        Some(decode(&read_limited(&root.join(name))?)?)
    } else {
        None
    };
    Some((image, rec))
}
fn store_image(root: &Path, job: &Job, image: &Thumbnail, source: &str) -> Record {
    let name = format!(
        "{}.png",
        hash(&format!("{:x}", Sha256::digest(&image.rgba)))
    );
    if let Some(img) = image::RgbaImage::from_raw(
        image.size[0] as u32,
        image.size[1] as u32,
        image.rgba.clone(),
    ) {
        let _ = img.save(root.join(&name));
    }
    let rec = Record {
        image: Some(name),
        expires: now() + 365 * 86400,
        source: source.into(),
        rejected: false,
    };
    save_record(root, &format!("t-{}", job.key), &rec);
    rec
}
/// Transport is injectable so matching/throttling/error tests need no public API.
pub trait Transport {
    fn get(&mut self, url: &str) -> Result<Option<Vec<u8>>, String>;
}
struct Http {
    client: Result<reqwest::blocking::Client, reqwest::Error>,
    last: Option<Instant>,
    root: PathBuf,
    epoch: u64,
    bypass: bool,
}
impl Http {
    fn new(root: PathBuf) -> Self {
        Self {
            client: reqwest::blocking::Client::builder()
                .user_agent(concat!(
                    "ProspektDJ/",
                    env!("CARGO_PKG_VERSION"),
                    " (https://github.com/prospektdj/prospektdj)"
                ))
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build(),
            last: None,
            root,
            epoch: 0,
            bypass: false,
        }
    }
}
impl Transport for Http {
    fn get(&mut self, url: &str) -> Result<Option<Vec<u8>>, String> {
        let dir = self.root.join(self.epoch.to_string());
        let path = dir.join(hash(url));
        if !self.bypass {
            if let Ok(stat) = std::fs::metadata(&path) {
                if stat
                    .modified()
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|d| d < Duration::from_secs(86400))
                {
                    if let Some(bytes) = read_limited(&path) {
                        return if bytes.is_empty() {
                            Ok(None)
                        } else {
                            Ok(Some(bytes))
                        };
                    }
                }
            }
        }
        let client = self
            .client
            .as_ref()
            .map_err(|_| "HTTP initialization failed")?;
        for attempt in 0..3 {
            let delay = request_delay(self.last, Instant::now());
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            self.last = Some(Instant::now());
            let response = client
                .get(url)
                .send()
                .map_err(|_| "Artwork service offline")?;
            let status = response.status().as_u16();
            if status == 404 {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(&path, []);
                return Ok(None);
            }
            if let Some(wait) = retry_delay(
                status,
                attempt,
                response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok()),
            ) {
                if attempt == 2 {
                    break;
                }
                std::thread::sleep(wait);
                continue;
            }
            if !response.status().is_success() {
                return Err(format!("Artwork service HTTP {status}"));
            }
            if response.content_length().is_some_and(|n| n > MAX_BYTES) {
                return Err("Artwork response too large".into());
            }
            let mut bytes = Vec::new();
            response
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Artwork download interrupted")?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err("Artwork response too large".into());
            }
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&path, &bytes);
            return Ok(Some(bytes));
        }
        Err("Artwork service busy; retry later".into())
    }
}
fn json(t: &mut impl Transport, url: &str) -> Result<Option<Value>, String> {
    t.get(url)?
        .map(|b| serde_json::from_slice(&b).map_err(|_| "Invalid metadata response".into()))
        .transpose()
}
fn artist_matches(v: &Value, artist: &str) -> bool {
    let names = v["artist-credit"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|v| {
                    v["artist"]["name"]
                        .as_str()
                        .or(v["name"].as_str())
                        .unwrap_or("")
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    normalized(&names) == normalized(artist)
}
/// Return only verified, unambiguous releases; preserve remix/version words in comparison.
pub fn lookup(
    t: &mut impl Transport,
    e: &Entry,
    enabled: &AtomicBool,
) -> Result<Option<Vec<u8>>, String> {
    if !enabled.load(Ordering::Relaxed) || e.artist.trim().is_empty() || e.title.trim().is_empty() {
        return Ok(None);
    }
    let expected = expected_title(e);
    let release = if uuid(&e.musicbrainz_release) {
        e.musicbrainz_release.clone()
    } else if uuid(&e.musicbrainz_recording) {
        let Some(recording) = json(
            t,
            &format!(
                "https://musicbrainz.org/ws/2/recording/{}?inc=releases+artist-credits&fmt=json",
                e.musicbrainz_recording
            ),
        )?
        else {
            return Ok(None);
        };
        if !artist_matches(&recording, &e.artist)
            || normalized(recording["title"].as_str().unwrap_or("")) != normalized(&expected)
        {
            return Ok(None);
        }
        let releases: Vec<_> = recording["releases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| {
                e.album.is_empty()
                    || normalized(r["title"].as_str().unwrap_or("")) == normalized(&e.album)
            })
            .collect();
        if releases.len() != 1 {
            return Ok(None);
        }
        releases[0]["id"].as_str().unwrap_or("").to_owned()
    } else {
        let (kind, query) = if !e.album.trim().is_empty() {
            (
                "release",
                format!(
                    "artist:\"{}\" AND release:\"{}\"",
                    escape(&e.artist),
                    escape(&e.album)
                ),
            )
        } else {
            (
                "recording",
                format!(
                    "artist:\"{}\" AND recording:\"{}\"",
                    escape(&e.artist),
                    escape(&expected)
                ),
            )
        };
        let mut url =
            reqwest::Url::parse(&format!("https://musicbrainz.org/ws/2/{kind}/")).unwrap();
        url.query_pairs_mut()
            .append_pair("query", &query)
            .append_pair("limit", "5")
            .append_pair("fmt", "json");
        let Some(data) = json(t, url.as_str())? else {
            return Ok(None);
        };
        let candidates = data[if kind == "release" {
            "releases"
        } else {
            "recordings"
        }]
        .as_array()
        .cloned()
        .unwrap_or_default();
        let matches: Vec<_> = candidates
            .iter()
            .filter(|v| {
                v["score"]
                    .as_u64()
                    .or_else(|| v["score"].as_str().and_then(|s| s.parse().ok()))
                    .unwrap_or(0)
                    >= 95
                    && artist_matches(v, &e.artist)
                    && normalized(v["title"].as_str().unwrap_or(""))
                        == normalized(if kind == "release" {
                            &e.album
                        } else {
                            &expected
                        })
            })
            .collect();
        // Multiple editions can differ in artwork. Prefer no match to a guessed first result.
        if matches.len() != 1 {
            return Ok(None);
        }
        if kind == "release" {
            matches[0]["id"].as_str().unwrap_or("").to_owned()
        } else {
            let releases = matches[0]["releases"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if releases.len() != 1 {
                return Ok(None);
            }
            releases[0]["id"].as_str().unwrap_or("").to_owned()
        }
    };
    if !uuid(&release) || !enabled.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let Some(data) = json(
        t,
        &format!(
            "https://musicbrainz.org/ws/2/release/{release}?inc=artist-credits+recordings&fmt=json"
        ),
    )?
    else {
        return Ok(None);
    };
    if !artist_matches(&data, &e.artist)
        || (!e.album.is_empty()
            && normalized(data["title"].as_str().unwrap_or("")) != normalized(&e.album))
    {
        return Ok(None);
    }
    let title_ok = data["media"].as_array().is_some_and(|media| {
        media.iter().any(|m| {
            m["tracks"].as_array().is_some_and(|tracks| {
                tracks.iter().any(|track| {
                    let title = track["title"]
                        .as_str()
                        .or(track["recording"]["title"].as_str())
                        .unwrap_or("");
                    normalized(title) == normalized(&expected)
                        && (e.musicbrainz_recording.is_empty()
                            || track["recording"]["id"].as_str()
                                == Some(e.musicbrainz_recording.as_str()))
                })
            })
        })
    });
    if !title_ok || !enabled.load(Ordering::Relaxed) {
        return Ok(None);
    }
    t.get(&format!(
        "https://coverartarchive.org/release/{release}/front-250"
    ))
}
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn clear_downloads(root: &Path) {
    let mut keep = std::collections::HashSet::new();
    if let Ok(files) = std::fs::read_dir(root) {
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_some_and(|e| e == "json") {
                let rec =
                    read_limited(&path).and_then(|b| serde_json::from_slice::<Record>(&b).ok());
                if let Some(rec) = rec.filter(|r| r.source == "Manual" || r.rejected) {
                    if let Some(image) = rec.image {
                        keep.insert(image);
                    }
                } else {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }
    if let Ok(files) = std::fs::read_dir(root) {
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_some_and(|e| e == "png")
                && !keep.contains(&file.file_name().to_string_lossy().into_owned())
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let _ = std::fs::remove_dir_all(root.join("http"));
}

fn expected_title(e: &Entry) -> String {
    if e.version.is_empty() || normalized(&e.title).contains(&normalized(&e.version)) {
        e.title.clone()
    } else {
        format!("{} ({})", e.title, e.version)
    }
}

fn request_delay(last: Option<Instant>, now: Instant) -> Duration {
    last.map(|last| Duration::from_secs(1).saturating_sub(now.saturating_duration_since(last)))
        .unwrap_or_default()
}
fn retry_delay(status: u16, attempt: u32, retry_after: Option<u64>) -> Option<Duration> {
    if status == 429 || status == 503 {
        Some(Duration::from_secs(
            retry_after.unwrap_or(2u64.pow(attempt + 1)).min(30),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_and_rate_policy_handles_busy_service_without_ui_waits() {
        let start = Instant::now();
        assert_eq!(
            request_delay(Some(start), start + Duration::from_millis(250)),
            Duration::from_millis(750)
        );
        assert_eq!(
            request_delay(Some(start), start + Duration::from_secs(2)),
            Duration::ZERO
        );
        assert_eq!(
            retry_delay(429, 0, Some(120)),
            Some(Duration::from_secs(30))
        );
        assert_eq!(retry_delay(503, 1, None), Some(Duration::from_secs(4)));
        assert_eq!(retry_delay(404, 0, None), None);
    }
    #[test]
    fn late_network_result_cannot_overwrite_rejection() {
        let root = root("late");
        let (out, events) = bounded(8);
        let (net, network) = bounded(8);
        let mut worker = LocalWorker {
            root: root.clone(),
            epoch: Arc::new(AtomicU64::new(0)),
            active: Arc::new(AtomicBool::new(true)),
            out,
            net,
            versions: HashMap::new(),
        };
        worker.lookup(job());
        let pending = network.recv().unwrap();
        let mut reject = job();
        reject.token = 2;
        worker.task(Task::Reject(reject));
        worker.complete(NetResult {
            job: pending,
            result: Ok(Some(vec![1, 2, 3])),
            query: "query".into(),
        });
        assert_eq!(events.recv().unwrap().status, "Rejected");
        assert!(events.is_empty());
        assert!(
            cached(&root, &format!("t-{}", job().key))
                .unwrap()
                .1
                .rejected
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("prospekt-art-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    fn image() -> Thumbnail {
        Thumbnail {
            size: [2, 2],
            rgba: vec![140; 16],
        }
    }
    fn job() -> Job {
        let entry = Entry {
            title: "Fixture".into(),
            artist: "Author".into(),
            album: "Album".into(),
            ..Default::default()
        };
        Job {
            key: identity(&entry),
            entry,
            token: 1,
            epoch: 0,
            force: false,
        }
    }
    #[test]
    fn positive_negative_rejection_and_clear_cache_contract() {
        let root = root("records");
        let job = job();
        let rec = store_image(&root, &job, &image(), "Online");
        let key = format!("t-{}", job.key);
        let (img, _) = cached(&root, &key).unwrap();
        assert_eq!(img.unwrap().rgba, vec![140; 16]);
        let expired = Record {
            expires: now() - 1,
            ..rec.clone()
        };
        save_record(&root, "expired", &expired);
        assert!(cached(&root, "expired").is_none());
        let missing = Record {
            source: "404".into(),
            expires: now() + 86400,
            ..Default::default()
        };
        save_record(&root, "negative", &missing);
        assert!(cached(&root, "negative").unwrap().0.is_none());
        let reject = Record {
            source: "Rejected".into(),
            rejected: true,
            expires: u64::MAX,
            ..Default::default()
        };
        save_record(&root, "reject", &reject);
        store_image(&root, &job, &image(), "Manual");
        clear_downloads(&root);
        assert!(cached(&root, "negative").is_none());
        assert_eq!(cached(&root, &key).unwrap().1.source, "Manual");
        assert!(cached(&root, "reject").unwrap().1.rejected);
        std::fs::remove_dir_all(root).unwrap();
    }
    fn wait(service: &mut Artwork) -> Event {
        let start = Instant::now();
        loop {
            if let Some(event) = service.poll().pop() {
                return event;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn manual_cover_and_rejection_survive_service_restart_offline() {
        let root = root("worker");
        let file = root.join("local.png");
        image::RgbaImage::from_raw(2, 2, vec![140; 16])
            .unwrap()
            .save(&file)
            .unwrap();
        let entry = job().entry;
        {
            let mut service = Artwork::new(root.join("cache"), false);
            assert!(service.manual(&entry, file));
            assert_eq!(wait(&mut service).status, "Manual");
        }
        {
            let mut service = Artwork::new(root.join("cache"), false);
            service.request(&entry);
            let event = wait(&mut service);
            assert_eq!(event.image.unwrap().rgba, vec![140; 16]);
            assert!(service.reject(&entry));
            assert_eq!(wait(&mut service).status, "Rejected");
        }
        {
            let mut service = Artwork::new(root.join("cache"), false);
            service.request(&entry);
            let event = wait(&mut service);
            assert_eq!(event.status, "Rejected");
            assert!(event.image.is_none());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn embedded_picture_precedes_cached_image_without_audio_decode() {
        use lofty::{
            picture::{MimeType, Picture, PictureType},
            tag::{Tag, TagExt, TagType},
        };
        let root = root("embedded");
        let path = root.join("test.wav");
        let mut wav = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..100 {
            wav.write_sample(0i16).unwrap();
        }
        wav.finalize().unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_raw(2, 2, vec![200; 16])
            .unwrap()
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut tag = Tag::new(TagType::Id3v2);
        tag.push_picture(Picture::new_unchecked(
            PictureType::CoverFront,
            Some(MimeType::Png),
            None,
            png.into_inner(),
        ));
        tag.save_to_path(&path, lofty::config::WriteOptions::default())
            .unwrap();
        let entry = Entry::scan(path);
        let mut job = job();
        job.key = identity(&entry);
        job.entry = entry.clone();
        let cache = root.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        store_image(&cache, &job, &image(), "Online");
        let mut service = Artwork::new(cache, false);
        service.request(&entry);
        let event = wait(&mut service);
        assert_eq!(event.status, "Embedded");
        assert_eq!(event.image.unwrap().rgba, vec![200; 16]);
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }
}

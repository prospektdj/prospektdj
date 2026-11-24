//! Google Drive read-only integration. Credentials and tokens never enter settings or logs.
use anyhow::{bail, ensure, Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Deserialize)]
pub struct OAuthClient {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
}
impl OAuthClient {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() < 128 * 1024,
            "Plik konfiguracji OAuth jest zbyt duży"
        );
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let installed = value
            .get("installed")
            .context("Wymagany klient OAuth typu Aplikacja komputerowa (installed)")?;
        let client: Self = serde_json::from_value(installed.clone())?;
        ensure!(
            client.client_id.ends_with(".apps.googleusercontent.com"),
            "Nieprawidłowy identyfikator klienta Google"
        );
        Ok(client)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime: String,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default, rename = "modifiedTime")]
    pub modified: String,
}
impl DriveFile {
    pub fn folder(&self) -> bool {
        self.mime == "application/vnd.google-apps.folder"
    }
    pub fn audio(&self) -> bool {
        self.mime.starts_with("audio/")
            || Path::new(&self.name).extension().is_some_and(|e| {
                [
                    "mp3", "wav", "flac", "aac", "ogg", "m4a", "aiff", "aif", "opus",
                ]
                .iter()
                .any(|ext| e.eq_ignore_ascii_case(ext))
            })
    }
}
#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    files: Vec<DriveFile>,
    #[serde(rename = "nextPageToken")]
    next: Option<String>,
}
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default = "token_lifetime")]
    expires_in: u64,
}
fn token_lifetime() -> u64 {
    3600
}
struct Session {
    client: OAuthClient,
    tokens: Tokens,
    expires: Instant,
}
enum Job {
    Connect(OAuthClient),
    List(String, Option<String>),
    Download(DriveFile, Option<usize>),
    Disconnect,
}
pub enum Event {
    Authorize(String),
    Connected,
    Disconnected,
    Files {
        folder: String,
        files: Vec<DriveFile>,
        next: Option<String>,
    },
    Downloaded(PathBuf, Option<usize>),
    Error(String),
}
pub struct Drive {
    tx: Sender<Job>,
    pub events: Receiver<Event>,
    stop: Arc<AtomicBool>,
}
impl Drive {
    pub fn new(root: PathBuf) -> Self {
        let (tx, rx) = bounded(16);
        let (out, events) = crossbeam_channel::unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        std::thread::spawn(move || {
            let http = match Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(300))
                .build()
            {
                Ok(client) => client,
                Err(_) => {
                    let _ = out.send(Event::Error(
                        "Nie można uruchomić klienta HTTPS Google".into(),
                    ));
                    return;
                }
            };
            let mut session: Option<Session> = None;
            while let Ok(job) = rx.recv() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = (|| -> Result<()> {
                    match job {
                        Job::Connect(client) => {
                            session = Some(authorize(&http, client, &out, &cancel)?);
                            let _ = out.send(Event::Connected);
                        }
                        Job::Disconnect => {
                            session = None;
                            let _ = out.send(Event::Disconnected);
                        }
                        Job::List(folder, page) => {
                            let s = session.as_mut().context("Najpierw połącz Dysk Google")?;
                            refresh(&http, s)?;
                            validate_id(&folder)?;
                            let query = format!("'{}' in parents and trashed = false", folder);
                            let mut request = http
                                .get("https://www.googleapis.com/drive/v3/files")
                                .bearer_auth(&s.tokens.access_token)
                                .query(&[
                                    ("q", query.as_str()),
                                    ("pageSize", "100"),
                                    ("orderBy", "folder,name"),
                                    (
                                        "fields",
                                        "nextPageToken,files(id,name,mimeType,size,modifiedTime)",
                                    ),
                                ]);
                            if let Some(page) = page {
                                request = request.query(&[("pageToken", page)]);
                            }
                            let response = request.send()?;
                            ensure!(
                                response.status().is_success(),
                                "Google Drive: błąd listy plików HTTP {}",
                                response.status()
                            );
                            let page: Page = response.json()?;
                            let files = page
                                .files
                                .into_iter()
                                .filter(|f| f.folder() || f.audio())
                                .collect();
                            let _ = out.send(Event::Files {
                                folder,
                                files,
                                next: page.next,
                            });
                        }
                        Job::Download(file, deck) => {
                            let s = session.as_mut().context("Najpierw połącz Dysk Google")?;
                            refresh(&http, s)?;
                            let path =
                                download(&http, &s.tokens.access_token, &root, &file, &cancel)?;
                            let _ = out.send(Event::Downloaded(path, deck));
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = out.send(Event::Error(format!("Dysk Google: {error}")));
                }
            }
        });
        Self { tx, events, stop }
    }
    fn submit(&self, job: Job) -> bool {
        self.tx.try_send(job).is_ok()
    }
    pub fn connect(&self, client: OAuthClient) -> bool {
        self.submit(Job::Connect(client))
    }
    pub fn list(&self, folder: String, page: Option<String>) -> bool {
        self.submit(Job::List(folder, page))
    }
    pub fn download(&self, file: DriveFile, deck: Option<usize>) -> bool {
        self.submit(Job::Download(file, deck))
    }
    pub fn disconnect(&self) -> bool {
        self.submit(Job::Disconnect)
    }
}
impl Drop for Drive {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::new();
    let mut bits: u32 = 0;
    let mut count = 0;
    for byte in bytes {
        bits = (bits << 8) | u32::from(*byte);
        count += 8;
        while count >= 6 {
            count -= 6;
            result.push(ALPHABET[((bits >> count) & 63) as usize] as char);
        }
    }
    if count > 0 {
        result.push(ALPHABET[((bits << (6 - count)) & 63) as usize] as char);
    }
    result
}
fn authorize(
    http: &Client,
    client: OAuthClient,
    events: &Sender<Event>,
    cancel: &AtomicBool,
) -> Result<Session> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let redirect = format!(
        "http://127.0.0.1:{}/callback",
        listener.local_addr()?.port()
    );
    let verifier = base64url(&rand::random::<[u8; 32]>());
    let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
    let state = base64url(&rand::random::<[u8; 32]>());
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", client.client_id.as_str()),
        ("redirect_uri", &redirect),
        ("response_type", "code"),
        ("scope", "https://www.googleapis.com/auth/drive.readonly"),
        ("state", &state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("access_type", "offline"),
    ]);
    let _ = events.send(Event::Authorize(url.into()));
    let started = Instant::now();
    let code = loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Logowanie anulowane");
        ensure!(
            started.elapsed() < Duration::from_secs(180),
            "Upłynął czas logowania — spróbuj ponownie"
        );
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                while request.len() < 8192 && !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buffer[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let route = request
                    .lines()
                    .next()
                    .and_then(|l| l.strip_prefix("GET "))
                    .and_then(|l| l.split(' ').next());
                let parsed =
                    route.and_then(|r| url::Url::parse(&format!("http://localhost{r}")).ok());
                let valid = parsed.as_ref().is_some_and(|u| {
                    u.path() == "/callback"
                        && u.query_pairs().any(|(k, v)| k == "state" && v == state)
                });
                if !valid {
                    let _ = stream.write_all(
                        b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nInvalid OAuth state",
                    );
                    continue;
                }
                let code = parsed
                    .unwrap()
                    .query_pairs()
                    .find(|(k, _)| k == "code")
                    .map(|(_, v)| v.into_owned());
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nProspekt DJ: mozesz wrocic do programu.");
                break code.context("Nie udzielono dostępu do Dysku Google")?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => return Err(e.into()),
        }
    };
    let response = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client.client_id.as_str()),
            ("client_secret", &client.client_secret),
            ("code", &code),
            ("code_verifier", &verifier),
            ("redirect_uri", &redirect),
            ("grant_type", "authorization_code"),
        ])
        .send()?;
    ensure!(
        response.status().is_success(),
        "Google OAuth: błąd logowania HTTP {}",
        response.status()
    );
    let tokens: Tokens = response.json()?;
    let expires =
        Instant::now() + Duration::from_secs(tokens.expires_in.saturating_sub(60).min(86400));
    Ok(Session {
        client,
        tokens,
        expires,
    })
}
fn refresh(http: &Client, s: &mut Session) -> Result<()> {
    if Instant::now() < s.expires {
        return Ok(());
    }
    let refresh = s
        .tokens
        .refresh_token
        .as_deref()
        .context("Sesja wygasła — połącz ponownie")?;
    let response = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", s.client.client_id.as_str()),
            ("client_secret", &s.client.client_secret),
            ("refresh_token", refresh),
            ("grant_type", "refresh_token"),
        ])
        .send()?;
    ensure!(
        response.status().is_success(),
        "Sesja Google wygasła — połącz ponownie"
    );
    let mut tokens: Tokens = response.json()?;
    if tokens.refresh_token.is_none() {
        tokens.refresh_token = s.tokens.refresh_token.take();
    }
    s.expires =
        Instant::now() + Duration::from_secs(tokens.expires_in.saturating_sub(60).min(86400));
    s.tokens = tokens;
    Ok(())
}
fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 200
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "Nieprawidłowy identyfikator Google Drive"
    );
    Ok(())
}
pub fn local_path(root: &Path, file: &DriveFile) -> Result<PathBuf> {
    validate_id(&file.id)?;
    let mut name: String = file
        .name
        .chars()
        .map(|c| {
            if c.is_control() || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(180)
        .collect();
    name = name.trim().trim_end_matches('.').to_owned();
    ensure!(
        !name.is_empty() && name != "." && name != "..",
        "Nieprawidłowa nazwa pliku"
    );
    // Prefix also avoids Windows reserved device names (CON, AUX, COM1...).
    Ok(root.join(&file.id).join(format!("gdrive-{name}")))
}
fn download(
    http: &Client,
    token: &str,
    root: &Path,
    file: &DriveFile,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    ensure!(
        !file.folder() && file.audio(),
        "Można pobierać tylko pliki audio"
    );
    let target = local_path(root, file)?;
    let parent = target.parent().unwrap();
    std::fs::create_dir_all(parent)?;
    let manifest = parent.join("download.json");
    if target.is_file() {
        if let Ok(bytes) = std::fs::read(&manifest) {
            if let Ok(saved) = serde_json::from_slice::<DriveFile>(&bytes) {
                if saved.modified == file.modified
                    && saved.size == file.size
                    && std::fs::metadata(&target)?.len()
                        == file
                            .size
                            .as_deref()
                            .and_then(|n| n.parse().ok())
                            .unwrap_or(u64::MAX)
                {
                    return Ok(target);
                }
            }
        }
        bail!("Lokalny plik już istnieje i różni się od wersji w chmurze. Przenieś go przed ponownym pobraniem: {}", target.display());
    }
    let mut response = http
        .get(format!(
            "https://www.googleapis.com/drive/v3/files/{}",
            file.id
        ))
        .query(&[("alt", "media")])
        .bearer_auth(token)
        .send()?;
    ensure!(
        response.status().is_success(),
        "Google Drive: pobieranie HTTP {}",
        response.status()
    );
    let temporary = parent.join(format!("download-{}.part", rand::random::<u64>()));
    let result = (|| -> Result<()> {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut buffer = [0u8; 65536];
        let mut total = 0u64;
        loop {
            ensure!(!cancel.load(Ordering::Relaxed), "Pobieranie anulowane");
            let n = response.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            output.write_all(&buffer[..n])?;
            total += n as u64;
        }
        if let Some(expected) = file.size.as_deref().and_then(|n| n.parse::<u64>().ok()) {
            ensure!(total == expected, "Niepełny plik pobrany z chmury");
        }
        output.sync_all()?;
        drop(output);
        // Destination is reserved without overwriting a user's existing file.
        let mut destination = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let copied = std::io::copy(&mut std::fs::File::open(&temporary)?, &mut destination)
            .and_then(|_| destination.sync_all());
        drop(destination);
        if let Err(error) = copied {
            let _ = std::fs::remove_file(&target);
            return Err(error.into());
        }
        std::fs::write(&manifest, serde_json::to_vec_pretty(file)?)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(temporary);
    result?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    #[test]
    fn pkce_base64_matches_rfc7636() {
        use sha2::{Digest, Sha256};
        assert_eq!(
            super::base64url(&Sha256::digest(
                b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
            )),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}

//! Live MP3 source streaming via optional FFmpeg; audio callback only enqueues blocks.
use crate::{
    media::ffmpeg_command,
    recording::{Block, CaptureSink},
};
use crossbeam_channel::{bounded, Receiver};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub shoutcast: bool,
    pub host: String,
    pub port: u16,
    pub mount: String,
    pub username: String,
    #[serde(skip)]
    pub password: String,
    pub name: String,
    pub bitrate: u32,
    pub tls: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            shoutcast: false,
            host: String::new(),
            port: 8000,
            mount: "/live.mp3".into(),
            username: "source".into(),
            password: String::new(),
            name: "Prospekt DJ Live".into(),
            bitrate: 192,
            tls: false,
        }
    }
}
pub fn endpoint(config: &Config) -> anyhow::Result<url::Url> {
    anyhow::ensure!(!config.host.trim().is_empty(), "Podaj adres serwera");
    anyhow::ensure!(!config.password.is_empty(), "Podaj hasło źródła");
    anyhow::ensure!(
        [64, 96, 128, 160, 192, 256, 320].contains(&config.bitrate),
        "Nieprawidłowy bitrate"
    );
    anyhow::ensure!(config.port > 0, "Nieprawidłowy port");
    anyhow::ensure!(
        !config.password.contains(['\r', '\n']) && !config.name.contains(['\r', '\n']),
        "Hasło i nazwa nie mogą zawierać nowych linii"
    );
    let mut url = url::Url::parse("icecast://localhost")?;
    url.set_host(Some(config.host.trim()))
        .map_err(|_| anyhow::anyhow!("Nieprawidłowa nazwa serwera (bez http:// i ścieżki)"))?;
    url.set_port(Some(config.port))
        .map_err(|_| anyhow::anyhow!("Nieprawidłowy port"))?;
    url.set_username(if config.shoutcast {
        "source"
    } else {
        &config.username
    })
    .map_err(|_| anyhow::anyhow!("Nieprawidłowy użytkownik"))?;
    url.set_password(Some(&config.password))
        .map_err(|_| anyhow::anyhow!("Nieprawidłowe hasło"))?;
    url.set_path(if config.shoutcast { "/" } else { &config.mount });
    Ok(url)
}
pub struct Session {
    pub sink: CaptureSink,
    pub result: Receiver<Result<(), String>>,
    child: Arc<Mutex<Child>>,
    stopped: Arc<AtomicBool>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}
pub fn start(config: &Config, rate: u32) -> anyhow::Result<Session> {
    let url = endpoint(config)?;
    let mut command = ffmpeg_command();
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-f",
        "f32le",
        "-ar",
        &rate.to_string(),
        "-ac",
        "2",
        "-probesize",
        "32",
        "-analyzeduration",
        "0",
        "-i",
        "pipe:0",
        "-c:a",
        "libmp3lame",
        "-b:a",
        &format!("{}k", config.bitrate),
    ]);
    if config.shoutcast {
        anyhow::ensure!(
            !config.tls,
            "Shoutcast v1: wyłącz TLS lub użyj wejścia Icecast na serwerze"
        );
        command.args(["-f", "mp3", "pipe:1"]).stdout(Stdio::piped());
    } else {
        command
            .args([
                "-content_type",
                "audio/mpeg",
                "-ice_name",
                &config.name,
                "-tls",
                if config.tls { "1" } else { "0" },
                "-rw_timeout",
                "10000000",
                "-f",
                "mp3",
            ])
            .arg(url.as_str())
            .stdout(Stdio::null());
    }
    command.stdin(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| {
        anyhow::anyhow!(
            "Nie można uruchomić FFmpeg. Zainstaluj FFmpeg z koderem libmp3lame i obsługą Icecast."
        )
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("Brak wejścia kodera"))?;
    let child = Arc::new(Mutex::new(child));
    let monitor = child.clone();
    if config.shoutcast {
        let target = (config.host.clone(), config.port);
        let password = config.password.clone();
        let name = config.name.clone();
        let source = child.clone();
        let mut encoded = child.lock().unwrap().stdout.take().unwrap();
        std::thread::spawn(move || {
            let mut stream = || -> anyhow::Result<()> {
                use std::net::ToSocketAddrs;
                let address = target
                    .to_socket_addrs()?
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Brak adresu"))?;
                let mut socket =
                    std::net::TcpStream::connect_timeout(&address, Duration::from_secs(10))?;
                socket.set_read_timeout(Some(Duration::from_secs(10)))?;
                socket.set_write_timeout(Some(Duration::from_secs(10)))?;
                socket.write_all(format!("{password}\n").as_bytes())?;
                let mut reply = Vec::new();
                for _ in 0..1024 {
                    let mut byte = [0];
                    socket.read_exact(&mut byte)?;
                    reply.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                anyhow::ensure!(reply.starts_with(b"OK2"), "Odrzucono hasło Shoutcast");
                socket.write_all(
                    format!("icy-name:{name}\nicy-pub:0\ncontent-type:audio/mpeg\n\n").as_bytes(),
                )?;
                std::io::copy(&mut encoded, &mut socket)?;
                Ok(())
            };
            let _ = stream();
            if let Ok(mut process) = source.lock() {
                let _ = process.kill();
            }
        });
    }
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let overflow = Arc::new(AtomicBool::new(false));
    let fault = overflow.clone();
    let (tx, rx) = bounded::<Block>(128);
    let (done, result) = bounded(1);
    std::thread::spawn(move || {
        let mut run = || -> Result<(), String> {
            loop {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                if fault.load(Ordering::Relaxed) {
                    return Err("Transmisja przerwana: sieć lub koder nie nadąża za audio.".into());
                }
                if let Ok(mut process) = monitor.lock() {
                    if let Ok(Some(_)) = process.try_wait() {
                        return Err("Koder zakończył transmisję. Sprawdź serwer, port, hasło, punkt montowania oraz dostępność kodera MP3.".into());
                    }
                }
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(block) => {
                        let mut bytes = Vec::with_capacity(block.len * 8);
                        for frame in &block.frames[..block.len] {
                            for sample in frame {
                                bytes.extend_from_slice(&sample.to_le_bytes());
                            }
                        }
                        stdin.write_all(&bytes).map_err(|_| {
                            "Utracono połączenie z koderem lub serwerem transmisji".to_owned()
                        })?;
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(_) => return Ok(()),
                }
            }
        };
        let status = run();
        drop(stdin);
        if let Ok(mut process) = monitor.lock() {
            let _ = process.kill();
            let _ = process.wait();
        }
        let _ = done.send(status);
    });
    Ok(Session {
        sink: CaptureSink {
            sender: tx,
            overflow,
        },
        result,
        child,
        stopped,
    })
}

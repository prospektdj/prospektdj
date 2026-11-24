//! Lazy metadata inspection; file parsing never runs on the UI thread.
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
pub struct Cache {
    tx: Sender<PathBuf>,
    rx: Receiver<(PathBuf, Result<Vec<String>, String>)>,
    pending: HashSet<PathBuf>,
    pub entries: HashMap<PathBuf, Result<Vec<String>, String>>,
}
impl Default for Cache {
    fn default() -> Self {
        let (tx, requests) = bounded::<PathBuf>(2);
        let (output, rx) = bounded(4);
        std::thread::spawn(move || {
            for path in requests {
                let result = inspect(&path);
                if output.send((path, result)).is_err() {
                    break;
                }
            }
        });
        Self {
            tx,
            rx,
            pending: Default::default(),
            entries: Default::default(),
        }
    }
}
impl Cache {
    pub fn request(&mut self, path: &Path) {
        while let Ok((path, result)) = self.rx.try_recv() {
            self.pending.remove(&path);
            if self.entries.len() >= 64 {
                self.entries.clear();
            }
            self.entries.insert(path, result);
        }
        if !self.entries.contains_key(path)
            && !self.pending.contains(path)
            && self.tx.try_send(path.to_owned()).is_ok()
        {
            self.pending.insert(path.to_owned());
        }
    }
}
pub fn inspect(path: &Path) -> Result<Vec<String>, String> {
    use lofty::{
        file::{AudioFile, TaggedFileExt},
        tag::ItemValue,
    };
    let file = lofty::probe::Probe::open(path)
        .and_then(|probe| {
            probe
                .options(lofty::config::ParseOptions::new().read_cover_art(false))
                .read()
        })
        .map_err(|error| error.to_string())?;
    let properties = file.properties();
    let mut rows = vec![
        format!(
            "Plik: {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        format!("Format: {:?}", file.file_type()),
        format!("Czas: {:.2} s", properties.duration().as_secs_f64()),
    ];
    if let Some(rate) = properties.sample_rate() {
        rows.push(format!("Próbkowanie: {rate} Hz"));
    }
    if let Some(channels) = properties.channels() {
        rows.push(format!("Kanały: {channels}"));
    }
    if let Some(bitrate) = properties.audio_bitrate() {
        rows.push(format!("Bitrate audio: {bitrate} kb/s"));
    }
    if file.tags().is_empty() {
        rows.push("Brak zapisanych tagów".into());
    }
    for tag in file.tags() {
        rows.push(format!("— {:?} —", tag.tag_type()));
        for (index, item) in tag.items().enumerate() {
            if index >= 1024 {
                rows.push("Dalsze tagi pominięte: limit podglądu 1024 pól na blok".into());
                break;
            }
            let value = match item.value() {
                ItemValue::Text(value) | ItemValue::Locator(value) => {
                    let mut text: String = value.chars().take(4096).collect();
                    if value.chars().count() > 4096 {
                        text.push_str("… [skrócono]");
                    }
                    text
                }
                ItemValue::Binary(bytes) => format!("Dane binarne: {} bajtów", bytes.len()),
            };
            rows.push(format!("{:?}: {value}", item.key()));
        }
    }
    rows.push("Osadzone obrazy nie są dekodowane w podglądzie tagów.".into());
    Ok(rows)
}

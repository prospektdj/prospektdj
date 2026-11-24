//! Mounted-volume discovery and read-only audio-file inventory, outside the UI/audio threads.
use crossbeam_channel::{unbounded, Receiver};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Volume {
    pub path: PathBuf,
    pub name: String,
    pub external: bool,
    pub model: String,
    pub vendor: String,
}
#[derive(Clone, Debug)]
pub struct Notice {
    pub volume: Volume,
    pub counts: Option<[u64; 5]>,
    pub errors: u64,
}
pub enum Event {
    Volumes(Vec<Volume>),
    Notice(Notice),
}
pub struct Storage {
    pub volumes: Vec<Volume>,
    pub notices: std::collections::VecDeque<Notice>,
    rx: Receiver<Event>,
}
impl Default for Storage {
    fn default() -> Self {
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            let mut known = BTreeSet::new();
            let mut initial = true;
            let mut details = std::collections::BTreeMap::new();
            loop {
                let volumes = discover(&mut details);
                let current: BTreeSet<_> = volumes.iter().map(|v| v.path.clone()).collect();
                for volume in &volumes {
                    if !initial && !known.contains(&volume.path) {
                        let _ = tx.send(Event::Notice(Notice {
                            volume: volume.clone(),
                            counts: None,
                            errors: 0,
                        }));
                        let out = tx.clone();
                        let volume = volume.clone();
                        std::thread::spawn(move || {
                            let (counts, errors) = count_audio(&volume.path);
                            let _ = out.send(Event::Notice(Notice {
                                volume,
                                counts: Some(counts),
                                errors,
                            }));
                        });
                    }
                }
                if tx.send(Event::Volumes(volumes)).is_err() {
                    break;
                }
                known = current;
                initial = false;
                std::thread::sleep(Duration::from_secs(3));
            }
        });
        Self {
            volumes: Vec::new(),
            notices: Default::default(),
            rx,
        }
    }
}
impl Storage {
    pub fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Volumes(volumes) => self.volumes = volumes,
                Event::Notice(notice) => {
                    if let Some(old) = self
                        .notices
                        .iter_mut()
                        .find(|n| n.volume.path == notice.volume.path)
                    {
                        *old = notice;
                    } else {
                        self.notices.push_back(notice);
                    }
                }
            }
        }
    }
}
pub fn count_audio(root: &Path) -> ([u64; 5], u64) {
    let mut counts = [0; 5];
    let mut errors = 0;
    for item in walkdir::WalkDir::new(root)
        .follow_links(false)
        .same_file_system(true)
    {
        match item {
            Ok(entry) if entry.file_type().is_file() => {
                if let Some(index) =
                    entry
                        .path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .and_then(|s| {
                            ["mp3", "wav", "flac", "aac", "ogg"]
                                .iter()
                                .position(|ext| s.eq_ignore_ascii_case(ext))
                        })
                {
                    counts[index] += 1;
                }
            }
            Err(_) => errors += 1,
            _ => {}
        }
    }
    (counts, errors)
}
fn discover(details: &mut std::collections::BTreeMap<(PathBuf, String), Volume>) -> Vec<Volume> {
    sysinfo::Disks::new_with_refreshed_list()
        .list()
        .iter()
        .map(|disk| {
            let mut volume = Volume {
                path: disk.mount_point().to_owned(),
                name: disk.name().to_string_lossy().into_owned(),
                external: disk.is_removable(),
                model: String::new(),
                vendor: String::new(),
            };
            let key = (volume.path.clone(), volume.name.clone());
            if let Some(previous) = details.get(&key) {
                volume.model.clone_from(&previous.model);
                volume.vendor.clone_from(&previous.vendor);
                volume.external |= previous.external;
            } else {
                device_details(&mut volume);
                details.insert(key, volume.clone());
            }
            volume
        })
        .collect()
}
#[cfg(target_os = "linux")]
fn device_details(volume: &mut Volume) {
    let Some(name) = Path::new(&volume.name).file_name() else {
        return;
    };
    let Ok(mut sys) = std::fs::canonicalize(Path::new("/sys/class/block").join(name)) else {
        return;
    };
    if sys.join("partition").exists() {
        sys.pop();
    }
    volume.external |= sys.to_string_lossy().contains("/usb");
    for (field, output) in [("model", &mut volume.model), ("vendor", &mut volume.vendor)] {
        *output = std::fs::read_to_string(sys.join("device").join(field))
            .unwrap_or_default()
            .trim()
            .to_owned();
    }
}
#[cfg(target_os = "windows")]
fn device_details(volume: &mut Volume) {
    use std::os::windows::process::CommandExt;
    let Some(letter) = volume
        .path
        .to_str()
        .and_then(|s| s.chars().next())
        .filter(char::is_ascii_alphabetic)
    else {
        return;
    };
    // Only an alphabetic drive letter enters the command, never a volume label or user path.
    let script = format!("$ErrorActionPreference='Stop'; Get-Partition -DriveLetter '{letter}' | Get-Disk | Select-Object FriendlyName,Manufacturer,BusType | ConvertTo-Json -Compress");
    if let Ok(output) = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .output()
    {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            volume.model = value["FriendlyName"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            volume.vendor = value["Manufacturer"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            volume.external |= matches!(value["BusType"].as_u64(), Some(7 | 12 | 13));
        }
    }
}
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn device_details(_volume: &mut Volume) {}

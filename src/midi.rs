use anyhow::{bail, Context, Result};
use crossbeam_channel::Sender;
use midir::{Ignore, MidiInput, MidiInputConnection};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Encoding {
    #[default]
    #[serde(alias = "twos")]
    TwosComplement,
    #[serde(alias = "offset")]
    Offset64,
    #[serde(alias = "binary")]
    SignMagnitude,
}
pub fn relative(value: u8, encoding: Encoding) -> i16 {
    match encoding {
        Encoding::TwosComplement => {
            if value > 63 {
                value as i16 - 128
            } else {
                value as i16
            }
        }
        Encoding::Offset64 => value as i16 - 64,
        Encoding::SignMagnitude => {
            if value & 64 != 0 {
                -((value & 63) as i16)
            } else {
                value as i16
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    pub status: u8,
    pub number: u8,
    pub action: String,
    #[serde(default)]
    pub encoding: Encoding,
    #[serde(default)]
    pub inverted: bool,
    #[serde(default)]
    pub relative: bool,
}
pub fn key(data: &[u8]) -> Option<(u8, u8, u8)> {
    if data.len() < 3 || data[1] > 127 || data[2] > 127 {
        return None;
    }
    let s = data[0];
    if !matches!(s & 0xf0, 0x80 | 0x90 | 0xb0) {
        return None;
    }
    Some((
        if s & 0xf0 == 0x80 { 0x90 | (s & 15) } else { s },
        data[1],
        if s & 0xf0 == 0x80 { 0 } else { data[2] },
    ))
}
pub fn inputs() -> Vec<String> {
    MidiInput::new("Prospekt DJ")
        .map(|m| {
            m.ports()
                .iter()
                .filter_map(|p| m.port_name(p).ok())
                .collect()
        })
        .unwrap_or_default()
}
pub struct Connection {
    _connection: MidiInputConnection<()>,
}
pub fn connect(index: usize, tx: Sender<Vec<u8>>) -> Result<Connection> {
    let mut input = MidiInput::new("Prospekt DJ")?;
    input.ignore(Ignore::SysexAndTime);
    let ports = input.ports();
    let port = ports.get(index).context("Wejście MIDI nie istnieje")?;
    let connection = input
        .connect(
            port,
            "prospekt-controller",
            move |_, bytes, _| {
                let _ = tx.try_send(bytes.to_vec());
            },
            (),
        )
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(Connection {
        _connection: connection,
    })
}
/// Resolve the current port by name so hot-plug reordering cannot change the target.
pub fn connect_named(name: &str, tx: Sender<Vec<u8>>) -> Result<Connection> {
    let mut input = MidiInput::new("Prospekt DJ")?;
    input.ignore(Ignore::SysexAndTime);
    let ports = input.ports();
    let mut matching = ports
        .iter()
        .filter(|port| input.port_name(port).ok().as_deref() == Some(name));
    let port = matching
        .next()
        .context("Urządzenie MIDI zostało odłączone")?;
    anyhow::ensure!(
        matching.next().is_none(),
        "Kilka portów ma tę samą nazwę. Wybierz wejście ręcznie w ustawieniach MIDI."
    );
    let connection = input
        .connect(
            port,
            "prospekt-controller",
            move |_, bytes, _| {
                let _ = tx.try_send(bytes.to_vec());
            },
            (),
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(Connection {
        _connection: connection,
    })
}
pub fn validate(maps: &[Mapping], actions: &[String]) -> Result<()> {
    if maps.len() > 1024 {
        bail!("Mapa zawiera ponad 1024 przypisań");
    }
    for m in maps {
        if !matches!(m.status & 0xf0, 0x90 | 0xb0) || m.number > 127 || !actions.contains(&m.action)
        {
            bail!("Nieprawidłowe przypisanie MIDI: {}", m.action);
        }
    }
    Ok(())
}
pub fn import_json(data: &[u8], actions: &[String]) -> Result<Vec<Mapping>> {
    if data.len() > 256 * 1024 {
        bail!("Plik mapy jest za duży");
    }
    let value: serde_json::Value = serde_json::from_slice(data)?;
    let entries: Vec<serde_json::Value> =
        serde_json::from_value(value.get("mappings").cloned().unwrap_or(value))?;
    let mut maps = Vec::new();
    for mut entry in entries {
        if let Some(key) = entry.get("key").and_then(|k| k.as_str()).map(str::to_owned) {
            let (status, number) = key.split_once(':').context("Nieprawidłowy klucz MIDI")?;
            entry["status"] = serde_json::json!(status.parse::<u8>()?);
            entry["number"] = serde_json::json!(number.parse::<u8>()?);
        }
        maps.push(serde_json::from_value(entry)?);
    }
    validate(&maps, actions)?;
    Ok(maps)
}

/// A named user-provided mapping; no guessed hardware assignments.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControllerProfile {
    pub source: String,
    pub schema: String,
    pub authors: String,
    pub port: String,
    pub skipped: usize,
    pub unsupported: Vec<String>,
    pub manufacturer: String,
    pub model: String,
    pub name: String,
    pub mappings: Vec<Mapping>,
}
impl ControllerProfile {
    pub fn import(data: &[u8], filename: &str, actions: &[String]) -> Result<Self> {
        if data.len() > 2 * 1024 * 1024 {
            bail!("Mapping file exceeds 2 MiB");
        }
        if data.iter().copied().find(|b| !b.is_ascii_whitespace()) == Some(b'<') {
            return crate::controller_import::import_xml(
                std::str::from_utf8(data)?,
                filename,
                actions,
            );
        }
        let value: serde_json::Value = serde_json::from_slice(data)?;
        if value.get("format").and_then(|v| v.as_str()) == Some("opendj-mapping") {
            return crate::controller_import::import_opendj(&value, filename, actions);
        }
        if value.get("mappings").is_some() {
            let mut profile: Self = serde_json::from_value(value)?;
            validate(&profile.mappings, actions)?;
            if profile.name.is_empty() {
                profile.name = filename.chars().take(100).collect();
            }
            return Ok(profile);
        }
        let mappings = import_json(data, actions)?;
        let text = |key: &str| {
            value
                .get(key)
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim()
                .chars()
                .take(100)
                .collect::<String>()
        };
        let mut name = text("name");
        if name.is_empty() {
            name = filename.chars().take(100).collect();
        }
        Ok(Self {
            manufacturer: text("manufacturer"),
            model: text("model"),
            name,
            mappings,
            ..Default::default()
        })
    }
    pub fn label(&self) -> String {
        let device = format!("{} {}", self.manufacturer, self.model)
            .trim()
            .to_string();
        let name = if device.is_empty() || self.name.to_lowercase().contains(&device.to_lowercase())
        {
            self.name.clone()
        } else if self.name.is_empty() {
            device
        } else {
            format!("{device} · {}", self.name)
        };
        if self.schema.is_empty() {
            name
        } else {
            format!("{name} · {} · {} MIDI", self.schema, self.mappings.len())
        }
    }
}
pub fn bundled_profiles() -> &'static [ControllerProfile] {
    static PROFILES: std::sync::LazyLock<Vec<ControllerProfile>> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("../assets/controller-profiles.json"))
            .expect("validated bundled controller data")
    });
    &PROFILES
}
pub fn actions() -> Vec<String> {
    let mut out = [
        "cross",
        "master",
        "phones",
        "samplevolume",
        "browse",
        "record",
    ]
    .map(String::from)
    .to_vec();
    for i in 0..4 {
        for name in [
            "play",
            "touch",
            "loopin",
            "loopout",
            "loop1",
            "loop2",
            "loop4",
            "loop8",
            "loopdouble",
            "loophalf",
            "hotcue0",
            "hotcue1",
            "hotcue2",
            "hotcue3",
            "hotcue4",
            "hotcue5",
            "hotcue6",
            "hotcue7",
            "hotclear0",
            "hotclear1",
            "hotclear2",
            "hotclear3",
            "hotclear4",
            "hotclear5",
            "hotclear6",
            "hotclear7",
            "cue",
            "setcue",
            "sync",
            "scratch",
            "pfl",
            "loopoff",
            "load",
            "pitch",
            "gain",
            "pre_gain",
            "jog",
            "hot",
            "auto",
            "manual",
            "sample",
            "pad0",
            "pad1",
            "pad2",
            "pad3",
            "pad4",
            "pad5",
            "pad6",
            "pad7",
            "chorus_mix",
            "chorus_rate",
            "chorus_on",
            "tremolo_depth",
            "tremolo_rate",
            "tremolo_on",
            "drive",
            "distortion_mix",
            "distortion_on",
            "crusher_mix",
            "crusher_bits",
            "crusher_on",
            "eq0",
            "eq1",
            "eq2",
            "fx_bypass",
            "filter",
            "filter_on",
            "echo_on",
            "echo_mix",
            "echo_feedback",
            "echo_time",
            "echo_sync",
            "reverb_on",
            "reverb_mix",
            "reverb_room",
            "reverb_damping",
            "flanger_on",
            "flanger_mix",
            "flanger_rate",
            "flanger_depth",
        ] {
            out.push(format!("{i}.{name}"));
        }
    }
    out
}

impl Mapping {
    /// Map a 7-bit control to a normalized value, including inverted/relative controls.
    pub fn normalized(&self, value: u8, current: f32) -> f32 {
        if self.relative {
            let delta = relative(value, self.encoding) as f32 * 0.01;
            (current + if self.inverted { -delta } else { delta }).clamp(0., 1.)
        } else {
            let v = value.min(127) as f32 / 127.;
            if self.inverted {
                1. - v
            } else {
                v
            }
        }
    }
}

/// Match concrete port/model identifiers, never a manufacturer alone.
/// Keep ambiguous profile variants for explicit selection.
pub fn matching_profiles(port: &str, profiles: &[ControllerProfile]) -> Vec<usize> {
    let compact = |s: &str| {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let port = compact(port);
    let mut matches = Vec::new();
    for (index, profile) in profiles.iter().enumerate() {
        if profile.mappings.is_empty() {
            continue;
        }
        let model = compact(&profile.model);
        if model.len() < 4 || !model.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        let Some(position) = port.find(&model) else {
            continue;
        };
        let suffix = &port[position + model.len()..];
        if !suffix.is_empty()
            && !["midi", "usb", "port", "controller"]
                .iter()
                .any(|prefix| suffix.starts_with(prefix))
        {
            continue;
        }
        matches.push((index, model.len()));
    }
    let longest = matches.iter().map(|(_, len)| *len).max().unwrap_or(0);
    matches
        .into_iter()
        .filter(|(_, len)| *len == longest)
        .map(|(i, _)| i)
        .collect()
}

/// Select a device-specific mapping bank before looking up Note/CC numbers.
pub fn mapping_bank<'a>(
    first: &'a mut Vec<Mapping>,
    second: &'a mut Vec<Mapping>,
    second_device: bool,
) -> &'a mut Vec<Mapping> {
    if second_device {
        second
    } else {
        first
    }
}

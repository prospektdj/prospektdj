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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub status: u8,
    pub number: u8,
    pub action: String,
    pub encoding: Encoding,
}
pub fn key(data: &[u8]) -> Option<(u8, u8, u8)> {
    if data.len() < 3 {
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
pub fn validate(maps: &[Mapping], actions: &[String]) -> Result<()> {
    if maps.len() > 200 {
        bail!("Mapa zawiera ponad 200 przypisań");
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
    let entries: Vec<serde_json::Value> = serde_json::from_slice(data)?;
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

//! Declarative MIDI import only. Script bodies and HID packets are never evaluated.
use crate::midi::{validate, ControllerProfile, Encoding, Mapping};
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;

fn number(text: &str) -> Option<u8> {
    let t = text.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u8::from_str_radix(hex, 16).ok()
    } else {
        t.parse().ok()
    }
}
fn channel(group: &str) -> Option<usize> {
    let at = group.find("[Channel")? + 8;
    let tail = &group[at..];
    let end = tail.find(']')?;
    let n = tail[..end].parse::<usize>().ok()?;
    (1..=4).contains(&n).then_some(n - 1)
}
fn identify(name: &str, filename: &str) -> (String, String) {
    let search = format!("{name} {filename}").to_lowercase();
    let brands = [
        ("Allen & Heath", "allen"),
        ("Native Instruments", "native instruments"),
        ("American Audio", "american"),
        ("AlphaTheta", "alphatheta"),
        ("Pioneer", "pioneer"),
        ("Numark", "numark"),
        ("Hercules", "hercules"),
        ("Behringer", "behringer"),
        ("Denon", "denon"),
        ("Reloop", "reloop"),
        ("Vestax", "vestax"),
        ("Stanton", "stanton"),
        ("Novation", "novation"),
        ("Akai", "akai"),
        ("Arturia", "arturia"),
        ("Roland", "roland"),
        ("Rane", "rane"),
        ("Korg", "korg"),
        ("M-Audio", "m-audio"),
        ("Native Instruments", "traktor"),
    ];
    let brand = brands
        .iter()
        .find(|(_, needle)| search.contains(needle))
        .map(|(name, _)| *name)
        .unwrap_or(
            if search.contains("ddj-") || search.contains("cdj-") || search.contains("xdj-") {
                "Pioneer"
            } else {
                ""
            },
        );
    let model = name.strip_prefix(brand).unwrap_or(name).trim().to_owned();
    (brand.into(), model)
}
fn profile(name: &str, file: &str, authors: &str, port: &str) -> ControllerProfile {
    let (manufacturer, model) = identify(name, file);
    ControllerProfile {
        manufacturer,
        model,
        name: name.chars().take(160).collect(),
        source: format!("import:{file}"),
        authors: authors.chars().take(500).collect(),
        port: port.chars().take(160).collect(),
        ..Default::default()
    }
}
fn reject(profile: &mut ControllerProfile, reason: String) {
    profile.skipped += 1;
    if profile.unsupported.len() < 80 && !profile.unsupported.contains(&reason) {
        profile.unsupported.push(reason);
    }
}
fn push(profile: &mut ControllerProfile, conflicts: &mut BTreeSet<(u8, u8)>, m: Mapping) {
    let key = (m.status, m.number);
    if conflicts.contains(&key) {
        profile.skipped += 1;
        return;
    }
    if let Some(index) = profile
        .mappings
        .iter()
        .position(|x| (x.status, x.number) == key)
    {
        let old = &profile.mappings[index];
        if old.action == m.action && old.inverted == m.inverted && old.relative == m.relative {
            return;
        }
        profile.mappings.remove(index);
        conflicts.insert(key);
        reject(
            profile,
            format!(
                "Conflicting/layered MIDI binding {:02X}:{:02X}",
                key.0, key.1
            ),
        );
        return;
    }
    profile.mappings.push(m);
}
fn mixxx_action(group: &str, key: &str) -> Option<String> {
    if group == "[Master]" {
        return match key {
            "crossfader" => Some("cross".into()),
            "volume" | "gain" => Some("master".into()),
            "headVolume" | "headGain" => Some("phones".into()),
            _ => None,
        };
    }
    if matches!(group, "[Playlist]" | "[Library]") {
        return matches!(key, "SelectTrackKnob" | "MoveVertical" | "SelectTrack")
            .then(|| "browse".into());
    }
    let i = channel(group)?;
    if group.starts_with("[EqualizerRack") {
        return match key {
            "parameter1" => Some(format!("{i}.eq2")),
            "parameter2" => Some(format!("{i}.eq1")),
            "parameter3" => Some(format!("{i}.eq0")),
            _ => None,
        };
    }
    let simple = match key {
        "play" | "play_toggle" => "play",
        "cue_default" | "cue_goto" => "cue",
        "cue_set" => "setcue",
        "beatsync" | "beatsync_tempo" => "sync",
        "volume" | "pregain" => "gain",
        "rate" => "pitch",
        "pfl" => "pfl",
        "LoadSelectedTrack" => "load",
        "filterLow" => "eq2",
        "filterMid" => "eq1",
        "filterHigh" => "eq0",
        "wheel" | "jog" => "jog",
        "flanger" => "flanger_on",
        "loop_in" => "loopin",
        "loop_out" => "loopout",
        "reloop_exit" | "loop_enabled" => "loopoff",
        "loop_double" => "loopdouble",
        "loop_halve" => "loophalf",
        "beatloop_1_toggle" => "loop1",
        "beatloop_2_toggle" => "loop2",
        "beatloop_4_toggle" => "loop4",
        "beatloop_8_toggle" => "loop8",
        _ => "",
    };
    if !simple.is_empty() {
        return Some(format!("{i}.{simple}"));
    }
    if let Some(tail) = key.strip_prefix("hotcue_") {
        let (n, op) = tail.split_once('_')?;
        let n = n.parse::<usize>().ok()?;
        if (1..=8).contains(&n) {
            return match op {
                "activate" | "goto" => Some(format!("{i}.hotcue{}", n - 1)),
                "clear" => Some(format!("{i}.hotclear{}", n - 1)),
                _ => None,
            };
        }
    }
    None
}
fn openfade_action(action: &str, deck: usize, arg: Option<u8>) -> Option<String> {
    if deck > 3 {
        return None;
    }
    let global = match action {
        "crossfader" => "cross",
        "master" | "hw-master" => "master",
        "hp-level" => "phones",
        "browse" => "browse",
        "rec" => "record",
        _ => "",
    };
    if !global.is_empty() {
        return Some(global.into());
    }
    let native = match action {
        "play" => "play",
        "cue" => "cue",
        "sync" => "sync",
        "load" => "load",
        "fader" | "trim" => "gain",
        "tempo" | "tempo-hw" => "pitch",
        "cue-monitor" => "pfl",
        "vinyl" => "scratch",
        "jog" | "jog-2c" | "jog-bend" => "jog",
        "jog-touch" => "touch",
        "filter" => "filter",
        "eq-low" => "eq2",
        "eq-mid" => "eq1",
        "eq-hi" => "eq0",
        "loop-in" => "loopin",
        "loop-out" => "loopout",
        "loop-1" => "loop1",
        "loop-2" => "loop2",
        "loop-4" => "loop4",
        "loop-8" => "loop8",
        "loop-double" => "loopdouble",
        "loop-halve" => "loophalf",
        _ => "",
    };
    if !native.is_empty() {
        return Some(format!("{deck}.{native}"));
    }
    if let Some(p) = action
        .strip_prefix("pad")
        .and_then(|p| p.parse::<usize>().ok())
    {
        if (1..=8).contains(&p) {
            return Some(format!("{deck}.pad{}", p - 1));
        }
    }
    if matches!(action, "hw-pad" | "hw-pad-delete") {
        let p = arg.unwrap_or(0) as usize;
        if p < 8 {
            return Some(format!(
                "{deck}.{}{}",
                if action == "hw-pad" {
                    "pad"
                } else {
                    "hotclear"
                },
                p
            ));
        }
    }
    None
}
pub fn import_xml(text: &str, filename: &str, actions: &[String]) -> Result<ControllerProfile> {
    if text.len() > 2 * 1024 * 1024 {
        bail!("Mapping file exceeds 2 MiB");
    }
    let doc = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 100_000,
        },
    )
    .context("Invalid controller XML")?;
    let root = doc.root_element();
    let dialect = root.tag_name().name();
    if !matches!(
        dialect,
        "OpenFadeRMapping" | "MixxxMIDIPreset" | "MixxxControllerPreset"
    ) {
        bail!("Unsupported controller XML schema: {dialect}");
    }
    let info = root.children().find(|n| n.has_tag_name("info"));
    let field = |key: &str| {
        info.and_then(|n| n.children().find(|c| c.has_tag_name(key)))
            .and_then(|n| n.text())
            .unwrap_or("")
    };
    let name = if field("name").is_empty() {
        filename
    } else {
        field("name")
    };
    let port = root
        .descendants()
        .find(|n| n.has_tag_name("controller"))
        .and_then(|n| n.attribute("id"))
        .unwrap_or_else(|| field("port"));
    let mut p = profile(name, filename, field("author"), port);
    p.schema = dialect.to_string();
    let mut conflicts = BTreeSet::new();
    if filename.ends_with(".hid.xml") {
        reject(
            &mut p,
            "HID transport requires a native device driver; this profile has no MIDI bindings"
                .into(),
        );
        return Ok(p);
    }
    for c in root.descendants().filter(|n| {
        n.has_tag_name("control") && n.parent().is_some_and(|n| n.has_tag_name("controls"))
    }) {
        let child = |key: &str| {
            c.children()
                .find(|n| n.has_tag_name(key))
                .and_then(|n| n.text())
                .unwrap_or("")
        };
        let attr = |key: &str| c.attribute(key).unwrap_or("");
        let (status, midino) = if dialect == "OpenFadeRMapping" {
            (attr("status"), attr("midino"))
        } else {
            (child("status"), child("midino"))
        };
        let Some(status) = number(status) else {
            reject(&mut p, "Invalid/unsupported MIDI status".into());
            continue;
        };
        if status & 0xf0 == 0x80 {
            continue;
        } // Note-off is already normalized by the input layer.
        let Some(number) = number(midino).filter(|n| *n < 128) else {
            reject(&mut p, "Invalid/unsupported MIDI number".into());
            continue;
        };
        if !matches!(status & 0xf0, 0x90 | 0xb0) {
            reject(&mut p, "Only Note and 7-bit CC input are supported".into());
            continue;
        }
        let options: Vec<String> = c
            .descendants()
            .filter(|n| n.parent().is_some_and(|p| p.has_tag_name("options")))
            .map(|n| n.tag_name().name().to_ascii_lowercase())
            .collect();
        if options
            .iter()
            .any(|s| s.contains("script") || s.contains("fourteen-bit") || s == "hercjogfast")
        {
            reject(
                &mut p,
                format!("Script/14-bit/custom encoder binding: {}", child("key")),
            );
            continue;
        }
        let action = if dialect == "OpenFadeRMapping" {
            let deck = attr("deck").parse::<usize>().unwrap_or(0);
            openfade_action(attr("action"), deck, number_arg(attr("arg")))
        } else {
            mixxx_action(child("group"), child("key"))
        };
        let Some(action) = action.filter(|a| actions.contains(a)) else {
            reject(
                &mut p,
                format!(
                    "Unsupported action: {} {}",
                    if dialect == "OpenFadeRMapping" {
                        attr("action")
                    } else {
                        child("group")
                    },
                    child("key")
                ),
            );
            continue;
        };
        let encoding = if options
            .iter()
            .any(|s| matches!(s.as_str(), "spread64" | "rot64" | "rot64fast"))
        {
            Encoding::Offset64
        } else {
            Encoding::TwosComplement
        };
        let relative = options.iter().any(|s| {
            matches!(
                s.as_str(),
                "diff" | "selectknob" | "spread64" | "rot64" | "rot64fast"
            )
        });
        let inverted = options.iter().any(|s| s == "invert")
            || (dialect != "OpenFadeRMapping" && child("key") == "rate");
        push(
            &mut p,
            &mut conflicts,
            Mapping {
                status,
                number,
                action,
                encoding,
                inverted,
                relative,
            },
        );
    }
    p.mappings.sort_by_key(|m| (m.status, m.number));
    validate(&p.mappings, actions)?;
    Ok(p)
}
fn number_arg(text: &str) -> Option<u8> {
    number(text)
}

pub fn import_opendj(
    value: &serde_json::Value,
    filename: &str,
    actions: &[String],
) -> Result<ControllerProfile> {
    let mut p = profile(
        value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(filename),
        filename,
        value.get("author").and_then(|v| v.as_str()).unwrap_or(""),
        value
            .pointer("/controller/name")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    p.schema = "OpenDJ".into();
    let rows = value
        .get("rows")
        .and_then(|v| v.as_array())
        .context("Missing mapping rows")?;
    let mut conflicts = BTreeSet::new();
    for row in rows {
        if row.get("layer").is_some() || row.get("when").is_some() {
            reject(
                &mut p,
                "Layered/release scripts require a native implementation".into(),
            );
            continue;
        }
        let midi = row.get("midi").and_then(|v| v.as_str()).unwrap_or("");
        let bytes = midi
            .split_whitespace()
            .filter_map(|v| u8::from_str_radix(v, 16).ok())
            .collect::<Vec<_>>();
        if bytes.len() != 2 || bytes[1] > 127 || !matches!(bytes[0] & 0xf0, 0x90 | 0xb0) {
            reject(&mut p, "Unsupported OpenDJ MIDI input".into());
            continue;
        }
        let action = if let Some(bind) = row.get("bind").and_then(|v| v.as_str()) {
            bind.rsplit_once(',').and_then(|(g, k)| mixxx_action(g, k))
        } else {
            let script = row.get("script").and_then(|v| v.as_str()).unwrap_or("");
            let words = script.split_whitespace().collect::<Vec<_>>();
            if words.len() >= 3 && words[0] == "deck" {
                let i = words[1]
                    .parse::<usize>()
                    .ok()
                    .filter(|i| (1..=4).contains(i));
                i.and_then(|i| match words.as_slice() {
                    [_, _, "play_pause"] => Some(format!("{}.play", i - 1)),
                    [_, _, "sync"] => Some(format!("{}.sync", i - 1)),
                    [_, _, "filter"] => Some(format!("{}.filter", i - 1)),
                    [_, _, "hot_cue", n] => n
                        .parse::<usize>()
                        .ok()
                        .filter(|n| (1..=8).contains(n))
                        .map(|n| format!("{}.hotcue{}", i - 1, n - 1)),
                    _ => None,
                })
            } else {
                None
            }
        };
        let Some(action) = action.filter(|a| actions.contains(a)) else {
            reject(
                &mut p,
                "Unsupported declarative binding or script expression".into(),
            );
            continue;
        };
        push(
            &mut p,
            &mut conflicts,
            Mapping {
                status: bytes[0],
                number: bytes[1],
                action,
                encoding: Encoding::TwosComplement,
                inverted: false,
                relative: row.get("steps").and_then(|v| v.as_bool()).unwrap_or(false),
            },
        );
    }
    validate(&p.mappings, actions)?;
    Ok(p)
}

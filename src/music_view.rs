//! Shared, renderer-independent cue geometry, Unicode edits and tagged-key colors.
pub const CUE_RGB: [[u8; 3]; 8] = [
    [255, 41, 46],
    [222, 113, 59],
    [0, 139, 247],
    [250, 216, 59],
    [0, 255, 0],
    [251, 64, 170],
    [64, 249, 251],
    [190, 120, 255],
];
#[derive(Clone, Copy, Debug)]
pub struct Timeline {
    pub start: f64,
    pub span: f64,
    pub duration: f64,
}
impl Timeline {
    pub fn project(self, time: f64) -> Option<f32> {
        if !self.start.is_finite()
            || !self.duration.is_finite()
            || !time.is_finite()
            || !self.span.is_finite()
            || self.span <= 0.
            || time < 0.
            || time > self.duration
            || time < self.start
            || time > self.start + self.span
        {
            return None;
        }
        Some(((time - self.start) / self.span) as f32)
    }
    pub fn at(self, fraction: f32) -> f64 {
        (self.start + fraction.clamp(0., 1.) as f64 * self.span).clamp(0., self.duration.max(0.))
    }
}
pub fn insert_unicode(text: &mut String, range: std::ops::Range<usize>, addition: &str) -> usize {
    let offset = |n: usize| {
        text.char_indices()
            .nth(n)
            .map(|(i, _)| i)
            .unwrap_or(text.len())
    };
    let from = offset(range.start);
    let to = offset(range.end.max(range.start));
    text.replace_range(from..to, addition);
    range.start + addition.chars().count()
}
pub const EMOJI: [&str; 20] = [
    "🎧",
    "🎵",
    "🎶",
    "🔥",
    "❤️",
    "🌙",
    "🍕",
    "🎉",
    "💃",
    "🕺",
    "✨",
    "⭐",
    "🎤",
    "🎸",
    "🥁",
    "💿",
    "🌅",
    "💜",
    "👍🏽",
    "👩‍💻",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicalKey {
    pub number: u8,
    pub minor: bool,
}
impl MusicalKey {
    pub fn parse(raw: &str) -> Option<Self> {
        let canonical = raw.trim().to_uppercase();
        if let Some(mode) = canonical.chars().last().filter(|m| *m == 'A' || *m == 'B') {
            if let Ok(n) = canonical[..canonical.len() - 1].parse::<u8>() {
                if (1..=12).contains(&n) {
                    return Some(Self {
                        number: n,
                        minor: mode == 'A',
                    });
                }
            }
        }
        let value = raw
            .trim()
            .to_lowercase()
            .replace('♯', "#")
            .replace('♭', "b")
            .replace("minor", "m")
            .replace("major", "")
            .replace("min", "m")
            .replace("maj", "")
            .replace(' ', "");
        let minor = value.ends_with('m');
        let note = value.strip_suffix('m').unwrap_or(&value);
        let pitch = match note {
            "c" | "b#" => 0,
            "c#" | "db" => 1,
            "d" => 2,
            "d#" | "eb" => 3,
            "e" | "fb" => 4,
            "f" | "e#" => 5,
            "f#" | "gb" => 6,
            "g" => 7,
            "g#" | "ab" => 8,
            "a" => 9,
            "a#" | "bb" => 10,
            "b" | "cb" => 11,
            _ => return None,
        };
        let ring = if minor {
            [8, 3, 10, 5, 0, 7, 2, 9, 4, 11, 6, 1]
        } else {
            [11, 6, 1, 8, 3, 10, 5, 0, 7, 2, 9, 4]
        };
        Some(Self {
            number: (ring.iter().position(|p| *p == pitch)? + 1) as u8,
            minor,
        })
    }
    pub fn camelot(self) -> String {
        format!("{}{}", self.number, if self.minor { 'A' } else { 'B' })
    }
    pub fn rgb(self) -> [u8; 3] {
        // A separate harmonic wheel palette: stable key hue, never the cue index palette.
        const COLORS: [[u8; 3]; 12] = [
            [79, 172, 217],
            [83, 198, 188],
            [107, 206, 151],
            [161, 207, 108],
            [214, 205, 104],
            [229, 168, 110],
            [226, 132, 143],
            [212, 127, 188],
            [172, 139, 221],
            [136, 155, 224],
            [108, 176, 222],
            [89, 191, 211],
        ];
        let c = COLORS[(self.number - 1) as usize];
        if self.minor {
            c.map(|v| (v as f32 * 0.88) as u8)
        } else {
            c
        }
    }
}

/// Twemoji omits VS16 in standalone filenames, retaining it in ZWJ sequences.
pub fn emoji_key(text: &str) -> String {
    let joined = text.contains('\u{200d}');
    text.chars()
        .filter(|c| joined || *c != '\u{fe0f}')
        .map(|c| format!("{:x}", c as u32))
        .collect::<Vec<_>>()
        .join("-")
}
pub fn emoji_png(text: &str) -> Option<&'static [u8]> {
    let key = emoji_key(text);
    let atlas = crate::emoji_assets::EMOJI_PNGS;
    atlas
        .binary_search_by_key(&key.as_str(), |(k, _)| *k)
        .ok()
        .map(|i| atlas[i].1)
}

/// Release/focus guards for the spring pitch-bend fader and existing mouse/keyboard controls.
pub fn momentary_bend(
    slider: f64,
    buttons: f64,
    keys: f64,
    mouse_down: bool,
    enabled: bool,
) -> f64 {
    if !enabled {
        return 0.;
    }
    let finite = |v: f64| if v.is_finite() { v } else { 0. };
    (finite(keys)
        + if mouse_down {
            finite(slider).clamp(-0.08, 0.08) + finite(buttons)
        } else {
            0.
        })
    .clamp(-0.25, 0.25)
}

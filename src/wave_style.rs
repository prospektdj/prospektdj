use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Preset {
    #[default]
    Standard,
    Vivid,
    Dynamic,
    Muted,
    Dark,
    Monochrome,
    HighContrast,
    Custom,
}
impl Preset {
    pub const ALL: [(Self, &'static str); 8] = [
        (Self::Standard, "Standard"),
        (Self::Vivid, "Vivid"),
        (Self::Dynamic, "Dynamic"),
        (Self::Muted, "Muted"),
        (Self::Dark, "Dark"),
        (Self::Monochrome, "Monochrome"),
        (Self::HighContrast, "High contrast"),
        (Self::Custom, "Custom"),
    ];
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WaveStyle {
    pub preset: Preset,
    pub brightness: f32,
    pub saturation: f32,
    pub height: f32,
    pub grid: bool,
    pub beat_background: bool,
    pub reflections: bool,
    pub reflection_color: Option<[u8; 3]>,
    pub markers: bool,
    pub spread_cue_labels: bool,
    pub palette: [[u8; 3]; 3],
}
impl Default for WaveStyle {
    fn default() -> Self {
        Self {
            preset: Preset::Standard,
            brightness: 1.,
            saturation: 1.,
            height: 1.,
            grid: true,
            beat_background: true,
            reflections: true,
            reflection_color: None,
            markers: true,
            spread_cue_labels: false,
            palette: [[255, 80, 40], [70, 220, 100], [70, 130, 255]],
        }
    }
}
impl WaveStyle {
    pub fn color(&self, bands: [f32; 3], fallback: [u8; 3]) -> [u8; 3] {
        let max = bands
            .iter()
            .copied()
            .filter(|v| v.is_finite())
            .fold(0_f32, f32::max);
        let mut c = if max < 1e-6 {
            fallback.map(|v| v as f32 * 0.5)
        } else {
            std::array::from_fn(|n| 35. + 210. * (bands[n].max(0.) / max).sqrt())
        };
        let mut saturation = self.saturation.clamp(0., 2.);
        let mut brightness = self.brightness.clamp(0.2, 2.);
        match self.preset {
            Preset::Vivid => {
                saturation *= 1.4;
                brightness *= 1.15;
            }
            Preset::Dynamic => brightness *= 0.45 + 0.85 * max.clamp(0., 1.).sqrt(),
            Preset::Muted => saturation *= 0.4,
            Preset::Dark => brightness *= 0.5,
            Preset::Monochrome => saturation = 0.,
            Preset::HighContrast => {
                saturation *= 1.6;
                brightness *= 1.3;
            }
            Preset::Custom if max >= 1e-6 => {
                let sum = bands.iter().map(|v| v.max(0.)).sum::<f32>().max(1e-6);
                c = std::array::from_fn(|channel| {
                    (0..3)
                        .map(|band| self.palette[band][channel] as f32 * bands[band].max(0.) / sum)
                        .sum()
                });
            }
            _ => {}
        }
        let gray = c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
        c.map(|v| ((gray + (v - gray) * saturation) * brightness).clamp(0., 255.) as u8)
    }
}

/// Stable shuffled levels: labels do not jump when frames or playback positions change.
pub fn cue_label_level(track_id: &str, slot: usize) -> f32 {
    let seed = track_id.bytes().fold(0u64, |hash, byte| {
        hash.wrapping_mul(131).wrapping_add(u64::from(byte))
    });
    ((slot.wrapping_mul(5).wrapping_add(seed as usize)) % 8) as f32 / 7.
}

use crate::{midi::Mapping, recording::Format};
use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub effects: Vec<crate::effects::FxSettings>,
    pub library_sort: crate::library::Sort,
    pub library_descending: bool,
    pub show_effects: bool,
    pub wave_style: crate::wave_style::WaveStyle,
    pub reduced_motion: bool,
    pub blink_paused_play: bool,
    pub show_emojis: bool,
    pub ui_scale: f32,
    pub refresh_hz: u32,
    pub show_statistics: bool,
    pub cover_strip: bool,
    pub banger_site_name: bool,
    pub cover_scroll_animation: bool,
    pub track_preview: bool,
    pub preview_column: bool,
    pub live_recommendations: bool,
    pub history_threshold: f64,
    pub import_prompt_seen: bool,
    pub auto_cover_download: bool,
    pub key_camelot: bool,
    pub tree_width: f32,
    pub tree_open: BTreeSet<String>,
    pub library_scope: crate::library::Scope,
    pub dock_tab: usize,
    pub light: bool,
    pub white_jog: bool,
    pub deck_count: usize,
    pub language: String,
    pub logo_font: Option<PathBuf>,
    pub graphic_logo: bool,
    pub controller_profiles: Vec<crate::midi::ControllerProfile>,
    pub controller_profile: Option<usize>,
    pub controller_profile_source: String,
    pub master_device: String,
    pub mic_enabled: bool,
    pub mic_device: String,
    pub mic_volume: f32,
    pub cue_device: String,
    pub latency_ms: u32,
    pub audio_backend: crate::audio::Backend,
    pub audio_buffer_frames: Option<u32>,
    pub audio_master_pair: u16,
    pub audio_cue_pair: Option<u16>,
    pub format: Format,
    pub auto_bpm: bool,
    pub fade_seconds: f64,
    pub auto_favorites: bool,
    pub auto_shuffle: bool,
    pub auto_repeat: bool,
    pub linear_fade: bool,
    pub favorites: BTreeSet<PathBuf>,
    pub library: Vec<PathBuf>,
    pub midi: Vec<Mapping>,
    pub width: f32,
    pub height: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            effects: vec![crate::effects::FxSettings::default(); 4],
            library_sort: crate::library::Sort::Title,
            library_descending: false,
            show_effects: false,
            wave_style: Default::default(),
            reduced_motion: false,
            blink_paused_play: true,
            show_emojis: true,
            ui_scale: 1.,
            refresh_hz: 30,
            show_statistics: true,
            cover_strip: true,
            banger_site_name: true,
            cover_scroll_animation: true,
            track_preview: true,
            preview_column: true,
            live_recommendations: true,
            history_threshold: 30.,
            import_prompt_seen: false,
            auto_cover_download: true,
            key_camelot: true,
            tree_width: 210.,
            tree_open: BTreeSet::new(),
            library_scope: crate::library::Scope::All,
            dock_tab: 0,
            light: false,
            white_jog: false,
            deck_count: 2,
            language: "pl".into(),
            logo_font: None,
            graphic_logo: true,
            controller_profiles: Vec::new(),
            controller_profile: None,
            controller_profile_source: String::new(),
            master_device: String::new(),
            mic_enabled: false,
            mic_device: String::new(),
            mic_volume: 0.7,
            cue_device: String::new(),
            latency_ms: 12,
            audio_backend: Default::default(),
            audio_buffer_frames: None,
            audio_master_pair: 0,
            audio_cue_pair: None,
            format: Format::Wav,
            auto_bpm: true,
            fade_seconds: 8.,
            auto_favorites: false,
            auto_shuffle: false,
            auto_repeat: false,
            linear_fade: false,
            favorites: BTreeSet::new(),
            library: Vec::new(),
            midi: Vec::new(),
            width: 1440.,
            height: 940.,
        }
    }
}
pub fn config_path() -> PathBuf {
    if let Some(root) = std::env::var_os("PROSPEKT_CONFIG_DIR") {
        return PathBuf::from(root).join("settings.json");
    }
    ProjectDirs::from("org", "Prospekt", "Prospekt DJ Rust")
        .map(|p| p.config_dir().join("settings.json"))
        .unwrap_or_else(|| PathBuf::from("prospekt-settings.json"))
}
pub fn load() -> Settings {
    let mut s: Settings = std::fs::read(config_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    s.fade_seconds = s.fade_seconds.clamp(1., 30.);
    s.latency_ms = s.latency_ms.clamp(3, 100); // Retained for old configuration files.
    s.audio_buffer_frames = s.audio_buffer_frames.filter(|n| (16..=8192).contains(n));
    s.audio_master_pair = s.audio_master_pair.min(30) / 2 * 2;
    s.audio_cue_pair = s.audio_cue_pair.filter(|n| *n <= 30 && n.is_multiple_of(2));
    if !cfg!(target_os = "windows") && s.audio_backend == crate::audio::Backend::Asio {
        s.audio_backend = crate::audio::Backend::System;
        s.master_device.clear();
    }

    s.mic_volume = if s.mic_volume.is_finite() {
        s.mic_volume.clamp(0., 1.)
    } else {
        0.
    };
    s.width = s.width.clamp(1000., 3840.);
    s.height = s.height.clamp(720., 2160.);
    // Retire the old Samples/Mixer tabs without reopening a different panel.
    if s.dock_tab != 0 {
        s.show_effects = false;
    }
    s.dock_tab = 0;
    s.deck_count = if s.deck_count == 4 { 4 } else { 2 };
    s.effects.resize(4, crate::effects::FxSettings::default());
    s.effects.truncate(4);
    if !crate::i18n::LANGUAGES
        .iter()
        .any(|(code, _)| *code == s.language)
    {
        s.language = "pl".into();
    }
    s
}
pub fn save(settings: &Settings) -> Result<()> {
    // Built-in definitions live in the executable, not in the periodically saved user config.
    let mut saved = settings.clone();
    let selected = saved
        .controller_profile
        .and_then(|i| saved.controller_profiles.get(i))
        .cloned();
    saved.controller_profile_source = selected
        .as_ref()
        .filter(|p| p.source.starts_with("builtin:"))
        .map(|p| p.source.clone())
        .unwrap_or_default();
    saved
        .controller_profiles
        .retain(|p| !p.source.starts_with("builtin:"));
    saved.controller_profile = selected.and_then(|p| {
        saved
            .controller_profiles
            .iter()
            .position(|entry| *entry == p)
    });
    let settings = &saved;

    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(temp, path).context("Nie udało się zapisać ustawień")
}

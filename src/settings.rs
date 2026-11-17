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
    pub dock_tab: usize,
    pub light: bool,
    pub deck_count: usize,
    pub language: String,
    pub logo_font: Option<PathBuf>,
    pub graphic_logo: bool,
    pub controller_profiles: Vec<crate::midi::ControllerProfile>,
    pub controller_profile: Option<usize>,
    pub controller_profile_source: String,
    pub master_device: String,
    pub cue_device: String,
    pub latency_ms: u32,
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
            show_effects: true,
            dock_tab: 1,
            light: false,
            deck_count: 2,
            language: "pl".into(),
            logo_font: None,
            graphic_logo: true,
            controller_profiles: Vec::new(),
            controller_profile: None,
            controller_profile_source: String::new(),
            master_device: String::new(),
            cue_device: String::new(),
            latency_ms: 12,
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
    s.latency_ms = s.latency_ms.clamp(3, 100);
    s.width = s.width.clamp(1000., 3840.);
    s.height = s.height.clamp(720., 2160.);
    s.dock_tab = s.dock_tab.min(2);
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

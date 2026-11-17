use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use egui::{Align, Color32, FontId, RichText, Sense, Stroke, Vec2};
use prospekt_dj::i18n::{self, tr};
use prospekt_dj::{
    audio::{self, AudioRuntime},
    engine::{self, Command, DeckControl, DeckView, PadMode, Snapshot},
    media::{self, Track},
    midi::{self, Encoding, Mapping},
    recording,
    settings::{self, Settings},
};
use rand::seq::SliceRandom;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::widgets::{meter, Knob};
use prospekt_dj::{
    effects::{FxParam, FxSettings, FxSwitch},
    library::{Catalog, Entry, Query, Scope, Sort},
};

const ACCENTS: [Color32; 4] = [
    Color32::from_rgb(49, 188, 255),
    Color32::from_rgb(66, 196, 227),
    Color32::from_rgb(209, 118, 255),
    Color32::from_rgb(244, 164, 69),
];
fn large_fader_style(ui: &mut egui::Ui, accent: Color32) {
    ui.spacing_mut().interact_size.y = 24.;
    ui.spacing_mut().slider_rail_height = 2.5;
    let visuals = ui.visuals_mut();
    visuals.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
    visuals.widgets.inactive.bg_fill = Color32::from_gray(95);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1_f32, Color32::from_gray(170));
    visuals.widgets.hovered.bg_fill = Color32::from_gray(145);
    visuals.widgets.active.bg_fill = accent;
}
fn tree_choice(
    ui: &mut egui::Ui,
    emoji: &mut crate::emoji::EmojiAtlas,
    current: &mut Scope,
    value: Scope,
    label: String,
) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(20.), 21.),
        Sense::click(),
    );
    if *current == value || response.hovered() {
        ui.painter().rect_filled(
            rect,
            0.,
            if *current == value {
                Color32::from_rgb(35, 64, 79)
            } else {
                Color32::from_gray(30)
            },
        );
    }
    emoji.paint(ui, rect, &label, ui.visuals().text_color());
    if response.clicked() {
        *current = value;
        response.mark_changed();
    }
    response.on_hover_text(label)
}
fn tree_icon(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(13., 13.), Sense::hover());
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min + egui::vec2(0., 2.), egui::vec2(6., 4.)),
        1.,
        color,
    );
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min + egui::vec2(0., 5.), egui::vec2(13., 8.)),
        1.,
        color,
    );
}
fn cue_color(n: usize) -> Color32 {
    let [r, g, b] = prospekt_dj::music_view::CUE_RGB[n];
    Color32::from_rgb(r, g, b)
}
struct CrateEditor {
    id: Option<u64>,
    parent: Option<u64>,
    name: String,
    cursor: std::ops::Range<usize>,
}
struct ArtTexture {
    texture: egui::TextureHandle,
    used: u64,
}
enum WaveAction {
    Seek(f64),
    Cue(usize),
}
type LoadAuto = Option<(u64, usize, bool)>;
enum Work {
    Indexed {
        entries: Vec<Entry>,
        roots: Vec<PathBuf>,
        done: bool,
    },
    Loaded {
        deck: usize,
        token: u64,
        auto: LoadAuto,
        result: Result<Arc<Track>, String>,
    },
    Sample {
        slot: usize,
        token: u64,
        result: Result<Arc<Track>, String>,
    },
    Bpm {
        deck: usize,
        track: Arc<Track>,
        bpm: Option<f64>,
    },
}
struct AutoMix {
    queue: Vec<PathBuf>,
    index: usize,
    deck: usize,
    prepared: bool,
    preparing: bool,
    fading: bool,
    generation: u64,
    next_index: Option<usize>,
}

pub struct Studio {
    settings: Settings,
    tx: Sender<Command>,
    rx: Receiver<Command>,
    shared: Arc<Mutex<Snapshot>>,
    snapshot: Snapshot,
    runtime: Option<AudioRuntime>,
    offline: Option<(Sender<()>, std::thread::JoinHandle<()>)>,
    errors: Receiver<String>,
    error_tx: Sender<String>,
    work_tx: Sender<Work>,
    work: Receiver<Work>,
    library: Vec<PathBuf>,
    library_set: std::collections::BTreeSet<PathBuf>,
    search: String,
    scope: Scope,
    catalog: Catalog,
    browser_dirty: bool,
    visible_rows: Vec<usize>,
    db_dirty: bool,
    db_save: Option<(Sender<Catalog>, std::thread::JoinHandle<()>)>,
    index_jobs: usize,
    crate_editor: Option<CrateEditor>,
    folders: prospekt_dj::folder_tree::Folders,
    artwork: prospekt_dj::artwork::Artwork,
    art_textures: std::collections::HashMap<String, ArtTexture>,
    art_status: std::collections::HashMap<String, String>,
    art_tick: u64,
    cover_reveal: bool,
    scroll_selected: bool,
    fx_deck: usize,
    selected: usize,
    focused: usize,
    settings_open: bool,
    settings_tab: usize,
    library_focus: bool,
    dock_tab: usize,
    zoom: f64,
    status: String,
    deck_tokens: [u64; 4],
    sample_tokens: [u64; 4],
    loading: [bool; 4],
    covers: [Option<egui::TextureHandle>; 4],
    taps: [Vec<Instant>; 4],
    auto: Option<AutoMix>,
    generation: u64,
    record_result: Option<Receiver<Result<PathBuf, String>>>,
    record_stopping: bool,
    record_start: Option<Instant>,
    midi_names: Vec<String>,
    midi_selected: usize,
    midi_connection: Option<midi::Connection>,
    midi_reconnect_name: Option<String>,
    midi_poll: Instant,
    midi_tx: Sender<Vec<u8>>,
    midi_rx: Receiver<Vec<u8>>,
    learn: bool,
    learn_action: String,
    encoding: Encoding,
    last_midi: String,
    outputs: Vec<String>,
    bend: [f64; 4],
    bend_slider: [f64; 4],
    bend_buttons: [f64; 4],
    bend_keys: [f64; 4],
    allow_close: bool,
    had_focus: bool,
    close_confirm: bool,
    close_after_record: bool,
    last_save: Instant,
    monitor: prospekt_dj::telemetry::Monitor,
    font_name: String,
    emoji: crate::emoji::EmojiAtlas,
    brand: crate::brand::Brand,
    license_search: String,
    license_selected: usize,
    profile_manufacturer: String,
    profile_model: String,
    profile_name: String,
    controller_search: String,
}
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>, mut settings: Settings) -> Self {
        for profile in midi::bundled_profiles() {
            if !settings
                .controller_profiles
                .iter()
                .any(|p| p.source == profile.source)
            {
                settings.controller_profiles.push(profile.clone());
            }
        }
        if !settings.controller_profile_source.is_empty() {
            settings.controller_profile = settings
                .controller_profiles
                .iter()
                .position(|p| p.source == settings.controller_profile_source);
        }
        console_theme(&cc.egui_ctx, settings.light);
        i18n::set_language(&settings.language);
        let font_name = crate::fonts::install(&cc.egui_ctx, settings.logo_font.as_deref());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(6., 4.);
        style.spacing.slider_rail_height = 2.;
        style.spacing.button_padding = Vec2::new(7., 4.);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(12.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(11.));
        style
            .text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(10.));
        cc.egui_ctx.set_style(style);
        let (tx, rx) = unbounded();
        let (error_tx, errors) = bounded(64);
        let (work_tx, work) = unbounded();
        let (midi_tx, midi_rx) = bounded(256);
        let catalog_path = settings::config_path().with_file_name("library.json");
        let mut catalog = Catalog::load(&catalog_path);
        let mut library_set = std::collections::BTreeSet::new();
        let library: Vec<_> = settings
            .library
            .iter()
            .filter(|p| library_set.insert((*p).clone()))
            .cloned()
            .collect();
        for path in &library {
            catalog
                .entries
                .entry(path.clone())
                .or_insert_with(|| Entry::placeholder(path.clone()));
        }
        let (db_tx, db_rx) = unbounded::<Catalog>();
        let db_errors = error_tx.clone();
        let db_thread = std::thread::spawn(move || {
            while let Ok(mut catalog) = db_rx.recv() {
                while let Ok(new) = db_rx.try_recv() {
                    catalog = new;
                }
                if let Err(e) = catalog.save(&catalog_path) {
                    let _ = db_errors.try_send(e.to_string());
                }
            }
        });
        let dock_tab = settings.dock_tab;
        let scope = settings.library_scope.clone();
        let artwork = prospekt_dj::artwork::Artwork::new(
            settings::config_path().with_file_name("covers"),
            settings.auto_cover_download,
        );
        let mut studio = Self {
            settings,
            tx,
            rx,
            shared: Arc::new(Mutex::new(Snapshot::default())),
            snapshot: Snapshot::default(),
            runtime: None,
            offline: None,
            errors,
            error_tx,
            work_tx,
            work,
            library,
            library_set,
            search: String::new(),
            scope,
            catalog,
            browser_dirty: true,
            visible_rows: Vec::new(),
            db_dirty: false,
            db_save: Some((db_tx, db_thread)),
            index_jobs: 0,
            crate_editor: None,
            folders: Default::default(),
            artwork,
            art_textures: Default::default(),
            art_status: Default::default(),
            art_tick: 0,
            cover_reveal: false,
            scroll_selected: false,
            fx_deck: 0,
            selected: 0,
            focused: 0,
            settings_open: false,
            settings_tab: 4,
            library_focus: false,
            dock_tab,
            zoom: 8.,
            status: "Gotowy · BETA · Rust Edition · pliki pozostają na komputerze".into(),
            deck_tokens: [0; 4],
            sample_tokens: [0; 4],
            loading: [false; 4],
            covers: std::array::from_fn(|_| None),
            taps: std::array::from_fn(|_| Vec::new()),
            auto: None,
            generation: 0,
            record_result: None,
            record_stopping: false,
            record_start: None,
            midi_names: Vec::new(),
            midi_selected: 0,
            midi_connection: None,
            midi_reconnect_name: None,
            midi_poll: Instant::now(),
            midi_tx,
            midi_rx,
            learn: false,
            learn_action: "0.play".into(),
            encoding: Encoding::default(),
            last_midi: String::new(),
            outputs: Vec::new(),
            bend: [0.; 4],
            bend_slider: [0.; 4],
            bend_buttons: [0.; 4],
            bend_keys: [0.; 4],
            allow_close: false,
            had_focus: true,
            close_confirm: false,
            close_after_record: false,
            last_save: Instant::now(),
            monitor: prospekt_dj::telemetry::Monitor::new(settings::config_path()),
            font_name,
            emoji: Default::default(),
            brand: crate::brand::Brand::load(&cc.egui_ctx),
            license_search: String::new(),
            license_selected: 0,
            profile_manufacturer: String::new(),
            profile_model: String::new(),
            profile_name: String::new(),
            controller_search: String::new(),
        };
        for i in 0..4 {
            studio.snapshot.decks[i].fx = studio.settings.effects[i];
        }
        studio.snapshot.deck_count = studio.settings.deck_count;
        studio.restart_audio();
        let mut initial_paths = studio.library.clone();
        initial_paths.extend(studio.catalog.roots.iter().cloned());
        if !initial_paths.is_empty() {
            studio.index_paths(initial_paths, false);
        }
        if std::env::args().any(|a| a == "--demo") {
            studio.demo();
        }
        studio
    }
    fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }
    fn deck(&mut self, i: usize, control: DeckControl) {
        if let DeckControl::Bpm(bpm) = &control {
            if let Some(path) = self.snapshot.decks[i]
                .track
                .as_ref()
                .and_then(|t| t.path.as_ref())
            {
                if let Some(entry) = self.catalog.entries.get_mut(path) {
                    entry.bpm = Some(bpm.clamp(40., 300.));
                    self.db_dirty = true;
                    self.browser_dirty = true;
                }
            }
        }
        if let DeckControl::Pad(p, true) = &control {
            if self.snapshot.decks[i].mode == PadMode::Sampler && *p < 4 {
                self.sample_tokens[*p] += 1;
            }
        }
        self.stop_auto();
        self.focused = i;
        self.send(Command::Deck(i, control));
    }
    fn play(&mut self, i: usize) {
        if self.runtime.is_none() {
            self.status = "Brak wyjścia audio. Wybierz urządzenie w Ustawieniach.".into();
            return;
        }
        self.deck(i, DeckControl::Toggle);
    }
    fn restart_audio(&mut self) {
        if self.record_result.is_some() {
            self.status = "Zakończ nagrywanie przed zmianą urządzeń.".into();
            return;
        }
        self.stop_auto();
        self.settings.effects = self.snapshot.decks.iter().map(|d| d.fx).collect();
        self.runtime = None;
        if let Some((stop, thread)) = self.offline.take() {
            let _ = stop.send(());
            let _ = thread.join();
        }
        while self.rx.try_recv().is_ok() {}
        match audio::start(
            self.rx.clone(),
            self.shared.clone(),
            self.error_tx.clone(),
            &self.settings.master_device,
            &self.settings.cue_device,
            self.settings.latency_ms,
        ) {
            Ok(runtime) => {
                self.status = format!(
                    "Silnik Rust · {} Hz · bufor około {:.1} ms",
                    runtime.sample_rate, runtime.latency_ms
                );
                self.runtime = Some(runtime);
            }
            Err(e) => {
                self.status =
                    format!("Audio niedostępne: {e}. Możesz wybrać inne wyjście w ustawieniach.");
                let (stop, done) = bounded(1);
                let rx = self.rx.clone();
                let view = self.shared.clone();
                let thread = std::thread::spawn(move || engine::offline(rx, view, done));
                self.offline = Some((stop, thread));
            }
        }
        let mut previous = self.snapshot.clone();
        for deck in &mut previous.decks {
            deck.playing = false;
        }
        self.send(Command::Restore(Box::new(previous)));
        for i in 0..4 {
            self.send(Command::Deck(i, DeckControl::Fx(self.settings.effects[i])));
        }
    }
    fn stop_auto(&mut self) {
        if self.auto.take().is_some() {
            self.generation += 1;
            self.send(Command::CancelFade);
            self.status = "Automix zatrzymany — sterowanie ręczne".into();
        }
    }
    fn load(&mut self, deck: usize, path: PathBuf, auto: LoadAuto) {
        self.deck_tokens[deck] += 1;
        let token = self.deck_tokens[deck];
        self.loading[deck] = true;
        let tx = self.work_tx.clone();
        let analyse = self.settings.auto_bpm;
        self.status = format!("Wczytywanie → {}: {}", letter(deck), path.display());
        std::thread::spawn(move || {
            let result = media::load(&path, analyse).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Work::Loaded {
                deck,
                token,
                auto,
                result,
            });
        });
    }
    fn load_manual(&mut self, deck: usize, path: PathBuf) {
        self.stop_auto();
        self.load(deck, path, None);
    }
    fn pick_track(&mut self, deck: usize) {
        if let Some(path) = audio_dialog().pick_file() {
            self.add_paths(vec![path.clone()]);
            self.load_manual(deck, path);
        }
    }
    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        for path in &paths {
            if !path.is_dir() {
                self.catalog.excluded.remove(path);
            }
        }
        self.index_paths(paths, false);
    }
    fn index_paths(&mut self, paths: Vec<PathBuf>, force: bool) {
        if paths.is_empty() {
            return;
        }
        self.index_jobs += 1;
        let tx = self.work_tx.clone();
        let existing = self.library.clone();
        let cache = self.catalog.clone();
        std::thread::spawn(move || {
            let initial_roots = paths.iter().filter(|p| p.is_dir()).cloned().collect();
            let _ = tx.send(Work::Indexed {
                entries: Vec::new(),
                roots: initial_roots,
                done: false,
            });
            let roots = prospekt_dj::library::index(paths, existing, cache, force, |entries| {
                let _ = tx.send(Work::Indexed {
                    entries,
                    roots: Vec::new(),
                    done: false,
                });
            });
            let _ = tx.send(Work::Indexed {
                entries: Vec::new(),
                roots,
                done: true,
            });
        });
    }
    fn demo(&mut self) {
        self.stop_auto();
        for i in 0..self.settings.deck_count {
            self.deck_tokens[i] += 1;
            self.loading[i] = false;
            self.covers[i] = None;
            self.send(Command::Deck(i, DeckControl::Load(media::demo(i))));
        }
        self.status = "Demo 120 BPM gotowe — uruchom deck A lub B".into();
    }
    fn analyse(&mut self, deck: usize) {
        if let Some(track) = self.snapshot.decks[deck].track.clone() {
            let tx = self.work_tx.clone();
            self.status = format!("Analiza BPM decka {}…", letter(deck));
            std::thread::spawn(move || {
                let bpm = media::estimate_bpm(&track);
                let _ = tx.send(Work::Bpm { deck, track, bpm });
            });
        }
    }
    fn sample_file(&mut self, slot: usize) {
        if let Some(path) = audio_dialog().pick_file() {
            if std::fs::metadata(&path).is_ok_and(|m| m.len() > 32 * 1024 * 1024) {
                self.status = "Sample: maksymalny rozmiar pliku 32 MiB".into();
                return;
            }
            self.sample_tokens[slot] += 1;
            let token = self.sample_tokens[slot];
            let tx = self.work_tx.clone();
            std::thread::spawn(move || {
                let result = media::load(&path, false)
                    .and_then(|t| {
                        if t.duration() > 120. {
                            Err(anyhow::anyhow!("Sample może trwać maksymalnie 120 sekund"))
                        } else {
                            Ok(t)
                        }
                    })
                    .map_err(|e| format!("{e:#}"));
                let _ = tx.send(Work::Sample {
                    slot,
                    token,
                    result,
                });
            });
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        self.folders.poll();
        self.art_tick = self.art_tick.wrapping_add(1);
        for event in self.artwork.poll() {
            if let Some(image) = event.image {
                let texture = ctx.load_texture(
                    format!("art-{}", event.key),
                    egui::ColorImage::from_rgba_unmultiplied(image.size, &image.rgba),
                    Default::default(),
                );
                self.art_textures.insert(
                    event.key.clone(),
                    ArtTexture {
                        texture,
                        used: self.art_tick,
                    },
                );
            } else {
                self.art_textures.remove(&event.key);
            }
            self.art_status.insert(event.key, event.status);
        }
        while self.art_textures.len() > 128 {
            if let Some(key) = self
                .art_textures
                .iter()
                .min_by_key(|(_, t)| t.used)
                .map(|(k, _)| k.clone())
            {
                self.art_textures.remove(&key);
                self.artwork.forget(&key);
            } else {
                break;
            }
        }
        if self.art_status.len() > 4096 {
            self.art_status.clear();
        }
        for i in 0..self.settings.deck_count {
            if let Some(path) = self.snapshot.decks[i]
                .track
                .as_ref()
                .and_then(|t| t.path.as_ref())
            {
                if let Some(entry) = self.catalog.entries.get(path) {
                    let entry = entry.clone();
                    self.artwork.request(&entry);
                    let key = prospekt_dj::artwork::identity(&entry);
                    self.covers[i] = self.art_textures.get_mut(&key).map(|art| {
                        art.used = self.art_tick;
                        art.texture.clone()
                    });
                }
            }
        }
        if let Ok(view) = self.shared.try_lock() {
            for i in 0..4 {
                if view.decks[i].track.as_ref().and_then(|t| t.path.as_ref())
                    != self.snapshot.decks[i]
                        .track
                        .as_ref()
                        .and_then(|t| t.path.as_ref())
                {
                    self.browser_dirty = true;
                }
            }
            self.snapshot = view.clone();
        }
        for error in self.errors.try_iter() {
            self.status = error;
        }
        while let Ok(work) = self.work.try_recv() {
            match work {
                Work::Indexed {
                    entries,
                    roots,
                    done,
                } => {
                    for entry in entries {
                        if self.library_set.insert(entry.path.clone()) {
                            self.library.push(entry.path.clone());
                        }
                        self.catalog.merge(entry);
                    }
                    self.catalog.roots.extend(roots);
                    self.browser_dirty = true;
                    self.db_dirty = true;
                    if done {
                        self.index_jobs = self.index_jobs.saturating_sub(1);
                        self.status = format!(
                            "Biblioteka: {} utworów · indeksowanie zakończone",
                            self.library.len()
                        );
                    }
                }
                Work::Loaded {
                    deck,
                    token,
                    auto,
                    result,
                } => {
                    if self.deck_tokens[deck] != token {
                        continue;
                    }
                    self.loading[deck] = false;
                    if let Some((generation, _, _)) = auto {
                        if self
                            .auto
                            .as_ref()
                            .is_none_or(|a| a.generation != generation)
                        {
                            continue;
                        }
                    }
                    match result {
                        Ok(track) => {
                            if let Some(path) = &track.path {
                                let entry = self
                                    .catalog
                                    .entries
                                    .entry(path.clone())
                                    .or_insert_with(|| Entry::placeholder(path.clone()));
                                entry.title = track.title.clone();
                                entry.artist = track.artist.clone();
                                entry.duration = Some(track.duration());
                                entry.bpm = Some(track.bpm);
                                entry.missing = false;
                                self.browser_dirty = true;
                                self.db_dirty = true;
                            }
                            self.covers[deck] = None;
                            self.status = format!("{} → deck {}", track.title, letter(deck));
                            self.send(Command::Deck(deck, DeckControl::Load(track)));
                            if let Some((_, index, first)) = auto {
                                if first {
                                    self.send(Command::Cross(0.));
                                    self.send(Command::Deck(0, DeckControl::Play));
                                    if let Some(a) = &mut self.auto {
                                        a.index = index;
                                        a.preparing = false;
                                    }
                                    self.prepare_next();
                                } else if let Some(a) = &mut self.auto {
                                    a.prepared = true;
                                    a.preparing = false;
                                    a.next_index = Some(index);
                                }
                            }
                        }
                        Err(error) => {
                            self.status = error;
                            if auto.is_some() {
                                self.stop_auto();
                                self.status = "Automix zatrzymany: nie można wczytać utworu".into();
                            }
                        }
                    }
                }
                Work::Sample {
                    slot,
                    token,
                    result,
                } => {
                    if self.sample_tokens[slot] != token {
                        continue;
                    }
                    match result {
                        Ok(track) => {
                            self.send(Command::SampleLoad(slot, track));
                            for i in 0..4 {
                                self.send(Command::Deck(i, DeckControl::PadMode(PadMode::Sampler)));
                            }
                            self.dock_tab = 1;
                            self.settings.show_effects = true;
                        }
                        Err(error) => self.status = error,
                    }
                }
                Work::Bpm { deck, track, bpm } => {
                    if self.snapshot.decks[deck]
                        .track
                        .as_ref()
                        .is_some_and(|t| Arc::ptr_eq(t, &track))
                    {
                        if let Some(bpm) = bpm {
                            if let Some(path) = &track.path {
                                if let Some(entry) = self.catalog.entries.get_mut(path) {
                                    entry.bpm = Some(bpm);
                                    self.db_dirty = true;
                                    self.browser_dirty = true;
                                }
                            }
                            self.send(Command::Deck(deck, DeckControl::Bpm(bpm)));
                            self.status =
                                format!("Oszacowano {bpm:.1} BPM — sprawdź TAP / ×2 / ÷2");
                        } else {
                            self.status = "Za mało wyraźnego rytmu do analizy BPM".into();
                        }
                    }
                }
            }
        }
        if let Some(rx) = &self.record_result {
            if let Ok(result) = rx.try_recv() {
                self.record_result = None;
                self.record_stopping = false;
                self.record_start = None;
                match result {
                    Ok(path) => {
                        self.status = format!("Nagranie zapisane: {}", path.display());
                        if self.close_after_record {
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.close_after_record = false;
                    }
                }
            }
        }
        while let Ok(bytes) = self.midi_rx.try_recv() {
            self.midi_message(&bytes);
        }
        if self.midi_poll.elapsed() > Duration::from_secs(2) {
            self.midi_poll = Instant::now();
            if let Some(name) = self.midi_reconnect_name.clone() {
                self.midi_names = midi::inputs();
                if let Some(index) = self.midi_names.iter().position(|n| n == &name) {
                    self.midi_selected = index;
                    if self.midi_connection.is_none() {
                        if let Ok(connection) = midi::connect(index, self.midi_tx.clone()) {
                            self.midi_connection = Some(connection);
                            self.status = format!("MIDI połączone ponownie: {name}");
                        }
                    }
                } else if self.midi_connection.take().is_some() {
                    self.status = format!("MIDI odłączone: {name} — czekam na ponowne podłączenie");
                }
            }
        }
        self.tick_auto();
        if self.last_save.elapsed() > Duration::from_secs(5) {
            self.persist();
            self.last_save = Instant::now();
        }
    }
    fn start_auto(&mut self) {
        if self.auto.is_some() {
            self.stop_auto();
            return;
        }
        if self.runtime.is_none() {
            self.status = "Automix wymaga działającego wyjścia audio".into();
            return;
        }
        let mut queue: Vec<_> = self
            .visible()
            .iter()
            .map(|i| self.library[*i].clone())
            .filter(|p| {
                !self.catalog.entries.get(p).is_some_and(|e| e.missing)
                    && (!self.settings.auto_favorites || self.settings.favorites.contains(p))
            })
            .collect();
        if queue.len() < 2 {
            self.status = "Automix wymaga przynajmniej dwóch utworów w wybranej kolejce".into();
            return;
        }
        if self.settings.auto_shuffle {
            queue.shuffle(&mut rand::rng());
        } else if let Some(selected) = self.library.get(self.selected) {
            if let Some(i) = queue.iter().position(|p| p == selected) {
                queue.rotate_left(i);
            }
        }
        self.generation += 1;
        let generation = self.generation;
        let first = queue[0].clone();
        self.send(Command::PauseAll);
        self.auto = Some(AutoMix {
            queue,
            index: 0,
            deck: 0,
            prepared: false,
            preparing: true,
            fading: false,
            generation,
            next_index: None,
        });
        self.load(0, first, Some((generation, 0, true)));
    }
    fn prepare_next(&mut self) {
        let Some(a) = &mut self.auto else {
            return;
        };
        let next = if a.index + 1 < a.queue.len() {
            Some(a.index + 1)
        } else if self.settings.auto_repeat {
            Some(0)
        } else {
            None
        };
        a.next_index = next;
        a.prepared = false;
        if let Some(index) = next {
            let path = a.queue[index].clone();
            let deck = 1 - a.deck;
            let generation = a.generation;
            a.preparing = true;
            self.load(deck, path, Some((generation, index, false)));
        }
    }
    fn transition(&mut self) {
        let Some(a) = &mut self.auto else {
            return;
        };
        if a.fading || !a.prepared {
            return;
        }
        let from = a.deck;
        let target = 1 - from;
        let d = &self.snapshot.decks[from];
        let left = d
            .track
            .as_ref()
            .map(|t| (t.duration() - d.position) / d.rate)
            .unwrap_or(1.);
        let target_duration = self.snapshot.decks[target]
            .track
            .as_ref()
            .map(|t| t.duration() / 2.)
            .unwrap_or(1.);
        let seconds = self
            .settings
            .fade_seconds
            .min(left)
            .min(target_duration)
            .max(0.05);
        a.fading = true;
        self.send(Command::Fade {
            to: target,
            seconds,
            linear: self.settings.linear_fade,
        });
    }
    fn tick_auto(&mut self) {
        let Some(a) = &self.auto else {
            return;
        };
        if a.preparing {
            return;
        }
        let from = a.deck;
        let d = &self.snapshot.decks[from];
        let left = d
            .track
            .as_ref()
            .map(|t| (t.duration() - d.position) / d.rate)
            .unwrap_or(f64::INFINITY);
        if a.fading {
            let target = 1 - from;
            if !d.playing && (self.snapshot.cross - target as f32).abs() < 0.001 {
                let a = self.auto.as_mut().unwrap();
                a.index = a.next_index.unwrap_or(a.index);
                a.deck = target;
                a.fading = false;
                a.prepared = false;
                self.prepare_next();
            }
        } else if a.prepared && left <= self.settings.fade_seconds {
            self.transition();
        } else if a.next_index.is_none() && left <= 0.01 && !d.playing {
            self.auto = None;
            self.status = "Koniec kolejki automix".into();
        }
    }
    fn record(&mut self) {
        if self.record_result.is_some() {
            if !self.record_stopping {
                self.send(Command::Recording(None));
                self.record_stopping = true;
                self.status = "Kończenie zapisu nagrania…".into();
            }
            return;
        }
        let Some(runtime) = &self.runtime else {
            self.status = "Nagrywanie wymaga działającego wyjścia audio".into();
            return;
        };
        let format = self.settings.format;
        let rate = runtime.sample_rate;
        let name = format!(
            "Prospekt_Mix_{}.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            format.extension()
        );
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Zapisz miks")
            .set_file_name(&name)
            .add_filter("Nagranie", &[format.extension()])
            .save_file()
        {
            let path = path.with_extension(format.extension());
            match recording::start(path, format, rate) {
                Ok((sink, result)) => {
                    self.send(Command::Recording(Some(sink)));
                    self.record_result = Some(result);
                    self.record_start = Some(Instant::now());
                    self.status = "Nagrywanie miksu po limiterze".into();
                }
                Err(error) => self.status = error.to_string(),
            }
        }
    }
    fn persist(&mut self) {
        self.settings.library_scope = self.scope.clone();
        self.settings.dock_tab = self.dock_tab;
        self.settings.library = self.library.clone();
        self.settings.effects = self.snapshot.decks.iter().map(|d| d.fx).collect();
        if let Err(error) = settings::save(&self.settings) {
            self.status = format!("Ustawienia: {error}");
        }
        if self.db_dirty {
            if let Some((tx, _)) = &self.db_save {
                let _ = tx.send(self.catalog.clone());
            }
            self.db_dirty = false;
        }
    }
    fn tap(&mut self, i: usize) {
        let now = Instant::now();
        if self.taps[i]
            .last()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(2))
        {
            self.taps[i].clear();
        }
        self.taps[i].push(now);
        if self.taps[i].len() > 8 {
            self.taps[i].remove(0);
        }
        if self.taps[i].len() >= 4 {
            let bpm = 60. * (self.taps[i].len() - 1) as f64
                / now.duration_since(self.taps[i][0]).as_secs_f64();
            self.deck(i, DeckControl::Bpm(bpm));
        } else {
            self.status = format!("TAP: {}/4 uderzenia", self.taps[i].len());
        }
    }
    fn sync(&mut self, i: usize) {
        let other = &self.snapshot.decks[i ^ 1];
        let rate = other.bpm * other.rate / self.snapshot.decks[i].bpm;
        self.deck(i, DeckControl::Rate(rate));
        self.status = "Dopasowano tempo. SYNC nie wyrównuje fazy bitów.".into();
    }
    fn top(&mut self, ctx: &egui::Context) {
        self.monitor.poll();
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(if self.settings.light {
                        Color32::WHITE
                    } else {
                        Color32::BLACK
                    })
                    .inner_margin(5),
            )
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                ui.allocate_ui_with_layout(
                    Vec2::new(rect.width(), 35.),
                    egui::Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(chrono::Local::now().format("%H:%M:%S").to_string())
                                    .monospace()
                                    .size(13.),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "CPU {:.0}%   DISK {:.0}% · {:.1} GiB free",
                                    self.monitor.stats.cpu,
                                    self.monitor.stats.disk_used,
                                    self.monitor.stats.available as f64 / 1073741824.
                                ))
                                .size(9.),
                            )
                            .on_hover_text(format!(
                                "{} · disk capacity used / free",
                                self.monitor.stats.disk
                            ));
                        });
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(tr("Ustawienia")).clicked() {
                                self.settings_open = true;
                                self.outputs = audio::output_names();
                                self.midi_names = midi::inputs();
                            }
                            if ui.button("F11").clicked() {
                                toggle_fullscreen(ctx);
                            }
                            ui.toggle_value(&mut self.library_focus, tr("BIBLIOTEKA"));
                            if ui
                                .small_button(if self.runtime.is_some() {
                                    "AUDIO ON"
                                } else {
                                    "AUDIO OFF"
                                })
                                .clicked()
                            {
                                self.settings_open = true;
                                self.settings_tab = 0;
                                self.outputs = audio::output_names();
                            }
                        });
                    },
                );
                if self.settings.graphic_logo {
                    let size = Vec2::new(25. * self.brand.aspect, 25.);
                    let destination = egui::Rect::from_center_size(
                        egui::pos2(rect.center().x, rect.top() + 12.),
                        size,
                    );
                    ui.painter().image(
                        self.brand.texture.id(),
                        destination,
                        self.brand.uv,
                        Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.top() + 12.),
                        egui::Align2::CENTER_CENTER,
                        "PROSPEKT DJ",
                        FontId::new(24., egui::FontFamily::Name("logo".into())),
                        ui.visuals().text_color(),
                    );
                }
                ui.painter().text(
                    egui::pos2(rect.center().x, rect.top() + 32.),
                    egui::Align2::CENTER_CENTER,
                    format!("OPEN SOURCE · {} · BETA", env!("CARGO_PKG_VERSION")),
                    FontId::proportional(9.),
                    Color32::from_rgb(255, 99, 135),
                );
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add(egui::Label::new(RichText::new(tr(&self.status)).small()).truncate());
        });
    }
    fn decks_ui(&mut self, ui: &mut egui::Ui) {
        let middle = 172.;
        let width = ((ui.available_width() - middle - 12.) / 2.).max(300.);
        for row in 0..self.settings.deck_count / 2 {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(width, 0.),
                    egui::Layout::top_down(Align::Min),
                    |ui| self.deck_ui(ui, row * 2),
                );
                ui.allocate_ui_with_layout(
                    Vec2::new(middle, 0.),
                    egui::Layout::top_down(Align::Center),
                    |ui| {
                        self.mini_mixer(ui, row * 2);
                    },
                );
                ui.allocate_ui_with_layout(
                    Vec2::new(width, 0.),
                    egui::Layout::top_down(Align::Min),
                    |ui| self.deck_ui(ui, row * 2 + 1),
                );
            });
        }
        ui.add_space(3.);
        ui.horizontal(|ui| {
            ui.label(RichText::new("RGB WAVEFORMS").small().strong());
            ui.label(
                RichText::new("LOW / MID / HIGH · siatka BPM + CUE")
                    .small()
                    .weak(),
            );
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().slider_width = 85.;
                ui.add(egui::Slider::new(&mut self.zoom, 2.0..=16.).text("s"));
            });
        });
        for (i, accent) in ACCENTS.iter().take(self.settings.deck_count).enumerate() {
            let view = self.snapshot.decks[i].clone();
            if let Some(pos) = waveform(
                ui,
                &view,
                *accent,
                match (
                    self.settings.deck_count == 4,
                    ui.ctx().screen_rect().height() < 850.,
                ) {
                    (true, true) => 24.,
                    (true, false) => 36.,
                    (false, true) => 30.,
                    _ => 45.,
                },
                Some(self.zoom),
            ) {
                self.wave_action(i, pos);
            }
        }
    }
    fn deck_ui(&mut self, ui: &mut egui::Ui, i: usize) {
        let d = self.snapshot.decks[i].clone();
        let accent = ACCENTS[i];
        egui::Frame::new()
            .fill(if self.settings.light {
                Color32::from_gray(234)
            } else {
                Color32::BLACK
            })
            .inner_margin(5)
            .corner_radius(0)
            .stroke(Stroke::new(
                1_f32,
                accent.gamma_multiply(if self.focused == i { 0.9 } else { 0.5 }),
            ))
            .show(ui, |ui| {
                let show_platter = ui.ctx().screen_rect().width() >= 1200.;
                let info = (ui.available_width() - if show_platter { 192. } else { 0. }).max(180.);
                ui.horizontal_top(|ui| {
                    if show_platter && i % 2 == 1 {
                        self.jog_panel(ui, i, &d);
                    }
                    ui.allocate_ui_with_layout(
                        Vec2::new(info, 0.),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_width(info);
                            self.deck_info(ui, i, &d);
                        },
                    );
                    if show_platter && i.is_multiple_of(2) {
                        self.jog_panel(ui, i, &d);
                    }
                });
            });
    }
    fn wave_action(&mut self, i: usize, action: WaveAction) {
        match action {
            WaveAction::Seek(pos) => self.deck(i, DeckControl::Seek(pos)),
            WaveAction::Cue(n) => self.deck(i, DeckControl::HotCue(n, false)),
        }
    }
    fn jog_panel(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        ui.allocate_ui_with_layout(
            Vec2::new(186., 174.),
            egui::Layout::top_down(Align::Min),
            |ui| {
                ui.horizontal_top(|ui| {
                    if i.is_multiple_of(2) {
                        self.pitch_bend(ui, i);
                    }
                    self.white_platter(ui, i, d);
                    if i % 2 == 1 {
                        self.pitch_bend(ui, i);
                    }
                });
            },
        );
    }
    fn pitch_bend(&mut self, ui: &mut egui::Ui, i: usize) {
        ui.allocate_ui_with_layout(
            Vec2::new(32., 174.),
            egui::Layout::top_down(Align::Center),
            |ui| {
                ui.set_width(32.);
                ui.spacing_mut().item_spacing.y = 2.;
                ui.label(RichText::new("PITCH").size(7.).weak());
                ui.label(RichText::new("BEND").size(8.).color(ACCENTS[i]));
                let mut value = self.bend_slider[i] * 100.;
                ui.spacing_mut().slider_width = 112.;
                let response = egui::Frame::new()
                    .fill(Color32::from_gray(27))
                    .stroke(Stroke::new(1_f32, Color32::from_gray(70)))
                    .corner_radius(1)
                    .inner_margin(3)
                    .show(ui, |ui| {
                        ui.visuals_mut().widgets.inactive.bg_stroke =
                            Stroke::new(1_f32, Color32::from_gray(165));
                        ui.add(
                            egui::Slider::new(&mut value, -8.0..=8.)
                                .vertical()
                                .show_value(false),
                        )
                    })
                    .inner;
                self.bend_slider[i] = if response.is_pointer_button_down_on() || response.dragged()
                {
                    value / 100.
                } else {
                    0.
                };
                if response.double_clicked() {
                    self.bend_slider[i] = 0.;
                }
                response.on_hover_text("Momentary pitch bend ±8% · returns to zero on release");
                ui.label(
                    RichText::new(format!("{:+.0}%", self.bend_slider[i] * 100.))
                        .monospace()
                        .size(8.),
                );
            },
        );
    }
    fn apply_bends(&mut self, ctx: &egui::Context) {
        let (mouse, focused) = ctx.input(|input| (input.pointer.primary_down(), input.focused));
        for i in 0..4 {
            let value = prospekt_dj::music_view::momentary_bend(
                self.bend_slider[i],
                self.bend_buttons[i],
                self.bend_keys[i],
                mouse && !self.library_focus,
                focused && i < self.settings.deck_count,
            );
            if !mouse || !focused || self.library_focus || i >= self.settings.deck_count {
                self.bend_slider[i] = 0.;
                self.bend_buttons[i] = 0.;
            }
            if value != self.bend[i] {
                self.stop_auto();
                self.send(Command::Deck(i, DeckControl::Bend(value)));
                self.bend[i] = value;
            }
        }
    }
    fn white_platter(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        let (response, p) = ui.allocate_painter(Vec2::new(148., 174.), Sense::click_and_drag());
        let center = egui::pos2(response.rect.center().x, response.rect.top() + 75.);
        let text = if self.settings.light {
            Color32::BLACK
        } else {
            Color32::from_gray(235)
        };
        p.circle_filled(center, 72., Color32::from_gray(35));
        p.circle_filled(
            center,
            65.,
            if self.settings.light {
                Color32::from_gray(228)
            } else {
                Color32::from_gray(12)
            },
        );
        p.circle_stroke(center, 66., Stroke::new(1_f32, Color32::from_gray(70)));
        if d.track.is_some() {
            let angle = d.position as f32 * std::f32::consts::PI - std::f32::consts::FRAC_PI_2;
            for n in 0..24 {
                let theta = angle + n as f32 * std::f32::consts::TAU / 24.;
                p.line_segment(
                    [
                        center + Vec2::angled(theta) * 68.,
                        center + Vec2::angled(theta) * 71.,
                    ],
                    Stroke::new(1_f32, Color32::from_gray(if n % 3 == 0 { 140 } else { 65 })),
                );
            }
            let arc: Vec<_> = (0..16)
                .map(|n| center + Vec2::angled(angle + n as f32 * 0.05) * 69.5)
                .collect();
            p.add(egui::Shape::line(arc, Stroke::new(2_f32, ACCENTS[i])));
            p.line_segment(
                [
                    center + Vec2::angled(angle) * 64.,
                    center + Vec2::angled(angle) * 72.,
                ],
                Stroke::new(3_f32, ACCENTS[i]),
            );
        }
        for (dy, label, size, color) in [
            (
                -23.,
                if d.track.is_some() {
                    format!("{:.2}", d.bpm * d.rate * (1. + self.bend[i]))
                } else {
                    "—".into()
                },
                24.,
                text,
            ),
            (
                -3.,
                format!("{:+.1}%   ±50%", (d.rate - 1.) * 100.),
                10.,
                text,
            ),
            (
                15.,
                if d.track.is_some() {
                    short_time(d.position)
                } else {
                    "—:—".into()
                },
                16.,
                text,
            ),
            (
                32.,
                d.track
                    .as_ref()
                    .map(|t| format!("−{}", short_time((t.duration() - d.position).max(0.))))
                    .unwrap_or_else(|| "—:—".into()),
                13.,
                Color32::GRAY,
            ),
        ] {
            p.text(
                center + egui::vec2(0., dy),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::monospace(size),
                color,
            );
        }
        p.text(
            egui::pos2(center.x, response.rect.top() + 161.),
            egui::Align2::CENTER_CENTER,
            if d.scratch { "VINYL" } else { "JOG / BEND" },
            FontId::monospace(9.),
            ACCENTS[i],
        );
        if response.drag_started() {
            self.stop_auto();
            self.focused = i;
        }
        if response.dragged() {
            let delta = ui.input(|input| input.pointer.delta().x);
            self.send(Command::Deck(
                i,
                DeckControl::Jog(
                    if d.scratch {
                        (delta as f64 / 5.).clamp(-6., 6.)
                    } else {
                        d.rate + delta as f64 * 0.02
                    },
                    0.12,
                ),
            ));
        }
        if response.drag_stopped() {
            self.send(Command::Deck(i, DeckControl::Jog(0., 0.)));
        }
    }
    fn deck_info(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        let accent = ACCENTS[i];
        ui.horizontal(|ui| {
            ui.label(RichText::new(letter(i)).small().strong().color(accent));
            if ui.small_button("LOAD").clicked() {
                self.pick_track(i);
            }
            if self.loading[i] {
                ui.spinner();
            }
            if ui
                .selectable_label(self.focused == i, "●")
                .on_hover_text("Keyboard focus · F1–F8: hot cue")
                .clicked()
            {
                self.focused = i;
            }
            if let Some(action) = waveform(ui, d, accent, 22., None) {
                self.wave_action(i, action);
            }
        });
        ui.horizontal(|ui| {
            if let Some(texture) = &self.covers[i] {
                ui.image((texture.id(), Vec2::splat(30.)));
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            d.track
                                .as_ref()
                                .map(|t| t.title.as_str())
                                .unwrap_or("Załaduj utwór"),
                        )
                        .size(13.)
                        .strong(),
                    )
                    .truncate(),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            d.track
                                .as_ref()
                                .map(|t| {
                                    if t.artist.is_empty() {
                                        "Lokalny plik audio"
                                    } else {
                                        t.artist.as_str()
                                    }
                                })
                                .unwrap_or("MP3 / WAV / FLAC / AAC / OGG"),
                        )
                        .size(11.)
                        .weak(),
                    )
                    .truncate(),
                );
            });
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            if ui
                .add_sized(
                    [32., 21.],
                    egui::Button::new(
                        RichText::new(if d.playing { "II" } else { "▶" }).color(if d.playing {
                            Color32::BLACK
                        } else {
                            accent
                        }),
                    )
                    .fill(if d.playing {
                        Color32::from_rgb(99, 198, 126)
                    } else {
                        Color32::from_gray(12)
                    }),
                )
                .clicked()
            {
                self.play(i);
            }
            if ui.small_button("CUE").clicked() {
                self.deck(i, DeckControl::Cue);
            }
            if ui.small_button("SYNC").clicked() {
                self.sync(i);
            }
            let mut bpm = d.bpm;
            let response = ui.add(
                egui::DragValue::new(&mut bpm)
                    .range(40.0..=300.)
                    .speed(0.1)
                    .max_decimals(1),
            );
            if response.changed() {
                self.deck(i, DeckControl::Bpm(bpm));
            }
            response
                .on_hover_text("BPM · Right-click: TAP / analyze / ×2 / ÷2")
                .context_menu(|ui| {
                    if ui.button("TAP").clicked() {
                        self.tap(i);
                    }
                    if ui.button("Analyze BPM").clicked() {
                        self.analyse(i);
                        ui.close_menu();
                    }
                    if ui.button("×2").clicked() {
                        self.deck(i, DeckControl::Bpm(d.bpm * 2.));
                        ui.close_menu();
                    }
                    if ui.button("÷2").clicked() {
                        self.deck(i, DeckControl::Bpm(d.bpm / 2.));
                        ui.close_menu();
                    }
                });
            ui.spacing_mut().slider_width = 54.;
            let mut pitch = (d.rate - 1.) * 100.;
            let response = ui.add(egui::Slider::new(&mut pitch, -50.0..=50.).show_value(false));
            if response.changed() {
                self.deck(i, DeckControl::Rate(1. + pitch / 100.));
            }
            if response.double_clicked() {
                self.deck(i, DeckControl::Rate(1.));
            }
            ui.label(RichText::new(format!("{pitch:+.1}%")).size(10.).monospace());
        });
        let controls = 98.;
        let pads = (ui.available_width() - controls - 6.).clamp(172., 268.);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(pads, 60.),
                egui::Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(pads);
                    self.cue_pads(ui, i, d);
                },
            );
            ui.allocate_ui_with_layout(
                Vec2::new(controls, 60.),
                egui::Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(controls);
                    ui.spacing_mut().item_spacing = egui::vec2(3., 2.);
                    let mut mode = d.mode;
                    egui::ComboBox::from_id_salt(("pads", i))
                        .width(90.)
                        .selected_text(RichText::new(mode_label(mode)).size(9.))
                        .show_ui(ui, |ui| {
                            for option in PadMode::ALL {
                                ui.selectable_value(&mut mode, option, mode_label(option));
                            }
                        });
                    if mode != d.mode {
                        self.deck(i, DeckControl::PadMode(mode));
                    }
                    ui.horizontal(|ui| {
                        for beats in [1., 2., 4., 8.] {
                            if ui
                                .selectable_label(
                                    d.loop_range.is_some_and(|(a, b)| {
                                        ((b - a) * d.bpm / 60. - beats).abs() < 0.05
                                    }),
                                    format!("{}", beats as u8),
                                )
                                .clicked()
                            {
                                self.deck(i, DeckControl::Loop(beats));
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("LOOP").size(8.).weak());
                        if ui.small_button("OFF").clicked() {
                            self.deck(i, DeckControl::LoopOff);
                        }
                    });
                },
            );
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            if ui.small_button("SET CUE").clicked() {
                self.deck(i, DeckControl::SetCue);
            }
            if ui.selectable_label(d.pfl, "PFL").clicked() {
                self.send(Command::Deck(i, DeckControl::Pfl));
            }
            if ui.selectable_label(d.scratch, "VINYL").clicked() {
                self.deck(i, DeckControl::ScratchMode);
            }
            let minus = ui.small_button("−");
            let plus = ui.small_button("+");
            let amount = if minus.is_pointer_button_down_on() {
                -0.04
            } else if plus.is_pointer_button_down_on() {
                0.04
            } else {
                0.
            };
            self.bend_buttons[i] = amount;
            if let Some(key) = d
                .track
                .as_ref()
                .and_then(|t| t.path.as_ref())
                .and_then(|p| self.catalog.entries.get(p))
                .map(|e| e.key.as_str())
                .filter(|k| !k.is_empty())
            {
                ui.label(RichText::new(key).size(10.).color(accent));
            }
        });
    }
    fn cue_pads(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        if d.mode != PadMode::HotCue {
            ui.horizontal_wrapped(|ui| {
                for p in 0..4 {
                    let label = if d.mode == PadMode::ManualLoop {
                        ["IN", "OUT", "OFF", "RE"][p].to_owned()
                    } else {
                        (p + 1).to_string()
                    };
                    if ui.add_sized([38., 25.], egui::Button::new(label)).clicked() {
                        self.deck(i, DeckControl::Pad(p, ui.input(|v| v.modifiers.shift)));
                    }
                }
            });
            return;
        }
        let gap = 3.;
        let width = ((ui.available_width() - 3. * gap) / 4.).clamp(40., 64.);
        for row in 0..2 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for col in 0..4 {
                    let n = row * 4 + col;
                    let color = cue_color(n);
                    let active = d.hot[n].is_some();
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, 28.), Sense::click());
                    let fill = if response.is_pointer_button_down_on() {
                        color.gamma_multiply(0.4)
                    } else if response.hovered() {
                        Color32::from_gray(35)
                    } else {
                        Color32::from_gray(if active { 17 } else { 8 })
                    };
                    ui.painter().rect_filled(rect, 1., fill);
                    ui.painter().rect_stroke(
                        rect.shrink(0.5),
                        1.,
                        Stroke::new(1_f32, color.gamma_multiply(if active { 1. } else { 0.35 })),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(rect.left(), rect.bottom() - 3.),
                            rect.right_bottom(),
                        ),
                        0.,
                        color.gamma_multiply(if active { 1. } else { 0.25 }),
                    );
                    ui.painter().text(
                        rect.center() - egui::vec2(0., 6.),
                        egui::Align2::CENTER_CENTER,
                        format!("Cue {}", n + 1),
                        FontId::proportional(10.),
                        if active {
                            Color32::WHITE
                        } else {
                            Color32::GRAY
                        },
                    );
                    ui.painter().text(
                        rect.center() + egui::vec2(0., 5.),
                        egui::Align2::CENTER_CENTER,
                        d.hot[n].map(short_time).unwrap_or_else(|| "—:—".into()),
                        FontId::monospace(9.),
                        color,
                    );
                    if response.clicked() {
                        self.deck(i, DeckControl::HotCue(n, ui.input(|v| v.modifiers.shift)));
                    }
                    response.on_hover_text("Click: set / recall · Shift+click: erase");
                }
            });
        }
    }
    fn mini_mixer(&mut self, ui: &mut egui::Ui, start: usize) {
        let s = self.snapshot.clone();
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            ui.label(
                RichText::new(if start == 0 {
                    "MASTER / MONITOR"
                } else {
                    "CHANNEL C / D"
                })
                .size(8.)
                .weak(),
            );
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 92.) / 2.).max(0.));
                for (i, accent) in ACCENTS.iter().enumerate().skip(start).take(2) {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(letter(i)).size(9.).color(*accent));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().slider_width = 88.;
                            large_fader_style(ui, *accent);
                            let mut volume = s.decks[i].volume;
                            if ui
                                .add(
                                    egui::Slider::new(&mut volume, 0.0..=1.)
                                        .vertical()
                                        .show_value(false),
                                )
                                .changed()
                            {
                                self.send(Command::Deck(i, DeckControl::Volume(volume)));
                            }
                            meter(ui, s.decks[i].level, *accent, 90.);
                        });
                    });
                }
            });
            if start != 0 {
                ui.label("A/C < X > B/D");
                return;
            }
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 66.) / 2.).max(0.));
                let mut master = s.master;
                if ui
                    .add(
                        Knob::new(&mut master, 0.0..=1.)
                            .default(0.75)
                            .label("MASTER")
                            .unit("%")
                            .size(16.),
                    )
                    .changed()
                {
                    self.send(Command::Master(master));
                }
                let mut phones = s.phones;
                if ui
                    .add(
                        Knob::new(&mut phones, 0.0..=1.)
                            .default(0.7)
                            .label("PHONES")
                            .unit("%")
                            .size(16.),
                    )
                    .changed()
                {
                    self.send(Command::Phones(phones));
                }
            });
            ui.spacing_mut().slider_width = 156.;
            let mut cross = s.cross;
            let r = ui
                .horizontal(|ui| {
                    ui.add_space(((ui.available_width() - 156.) / 2.).max(0.));
                    large_fader_style(ui, ACCENTS[0]);
                    ui.add(egui::Slider::new(&mut cross, 0.0..=1.).show_value(false))
                })
                .inner;
            if r.changed() {
                self.stop_auto();
                self.send(Command::Cross(cross));
            }
            if r.double_clicked() {
                self.stop_auto();
                self.send(Command::Cross(0.5));
            }
            let recording = self.record_result.is_some();
            let response = ui
                .add_enabled_ui(!self.record_stopping, |ui| {
                    let (rect, response) = ui.allocate_exact_size(Vec2::splat(20.), Sense::click());
                    let red = if response.hovered() {
                        Color32::from_rgb(255, 85, 85)
                    } else {
                        Color32::from_rgb(225, 42, 52)
                    };
                    if self.record_stopping {
                        ui.painter()
                            .circle_stroke(rect.center(), 7., Stroke::new(2_f32, red));
                    } else if recording {
                        ui.painter().rect_filled(
                            egui::Rect::from_center_size(rect.center(), Vec2::splat(12.)),
                            1.,
                            red,
                        );
                    } else {
                        ui.painter().circle_filled(rect.center(), 7., red);
                    }
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            if recording {
                                "Zatrzymaj nagrywanie"
                            } else {
                                "Nagraj miks"
                            },
                        )
                    });
                    response
                })
                .inner
                .on_hover_text(if self.record_stopping {
                    "Kończenie zapisu…"
                } else if recording {
                    "Zatrzymaj nagrywanie"
                } else {
                    "Nagraj miks"
                });
            if response.clicked() {
                self.record();
            }
        });
    }
    fn mixer_ui(&mut self, ui: &mut egui::Ui) {
        let s = self.snapshot.clone();
        ui.columns(self.settings.deck_count, |columns| {
            for (i, column) in columns.iter_mut().enumerate() {
                egui::Frame::group(column.style()).show(column, |ui| {
                    ui.label(
                        RichText::new(format!("CHANNEL {} / EQ", letter(i)))
                            .small()
                            .strong()
                            .color(ACCENTS[i]),
                    );
                    ui.horizontal(|ui| {
                        for (band, label) in ["HIGH", "MID", "LOW"].iter().enumerate() {
                            let mut gain = s.decks[i].eq[band];
                            if ui
                                .add(
                                    Knob::new(&mut gain, -24.0..=12.)
                                        .label(label)
                                        .unit("dB")
                                        .color(ACCENTS[i])
                                        .size(31.),
                                )
                                .changed()
                            {
                                self.send(Command::Deck(i, DeckControl::Eq(band, gain)));
                            }
                        }
                        let mut volume = s.decks[i].volume;
                        if ui
                            .add(
                                Knob::new(&mut volume, 0.0..=1.)
                                    .default(0.8)
                                    .label("LEVEL")
                                    .unit("%")
                                    .color(ACCENTS[i])
                                    .size(31.),
                            )
                            .changed()
                        {
                            self.send(Command::Deck(i, DeckControl::Volume(volume)));
                        }
                        meter(ui, s.decks[i].level, ACCENTS[i], 60.);
                    });
                });
            }
        });
    }
    fn dock_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (i, label) in ["DJ-FX", "Samples", "Mixer"].iter().enumerate() {
                if ui
                    .selectable_label(self.settings.show_effects && self.dock_tab == i, *label)
                    .clicked()
                {
                    if self.dock_tab == i && self.settings.show_effects {
                        self.settings.show_effects = false;
                    } else {
                        self.settings.show_effects = true;
                        self.dock_tab = i;
                    }
                }
            }
            if self.dock_tab == 1 && self.settings.show_effects {
                let mut volume = self.snapshot.sample_volume;
                ui.label(RichText::new("VOLUME").size(8.).weak());
                ui.spacing_mut().slider_width = 80.;
                if ui
                    .add(egui::Slider::new(&mut volume, 0.0..=1.).show_value(false))
                    .changed()
                {
                    self.send(Command::SampleVolume(volume));
                }
            }
        });
        if self.settings.show_effects {
            match self.dock_tab {
                0 => self.effects_ui(ui),
                1 => self.sampler_ui(ui),
                _ => self.mixer_ui(ui),
            }
        }
    }
    fn effects_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("FX RACK").strong());
            for (i, accent) in ACCENTS.iter().take(self.settings.deck_count).enumerate() {
                ui.selectable_value(
                    &mut self.fx_deck,
                    i,
                    RichText::new(format!("DECK {}", letter(i))).color(*accent),
                );
            }
            let mut fx = self.snapshot.decks[self.fx_deck].fx;
            if ui.toggle_value(&mut fx.bypass, "BYPASS FX").changed() {
                self.send(Command::Deck(self.fx_deck, DeckControl::Fx(fx)));
            }
            ui.label(
                RichText::new(format!(
                    "{:.1} BPM · {} aktywne",
                    self.snapshot.decks[self.fx_deck].bpm * self.snapshot.decks[self.fx_deck].rate,
                    fx.active_count()
                ))
                .small()
                .weak(),
            );
            if ui.small_button("Reset FX").clicked() {
                self.send(Command::Deck(
                    self.fx_deck,
                    DeckControl::Fx(FxSettings::default()),
                ));
            }
        });
        let i = self.fx_deck;
        let mut fx = self.snapshot.decks[i].fx;
        let before = fx;
        let accent = ACCENTS[i];
        ui.columns(4, |columns| {
            for (unit, column) in columns.iter_mut().enumerate() {
                let enabled = match unit {
                    0 => &mut fx.filter_enabled,
                    1 => &mut fx.echo_enabled,
                    2 => &mut fx.reverb_enabled,
                    _ => &mut fx.flanger_enabled,
                };
                let active = *enabled && !fx.bypass;
                egui::Frame::new()
                    .fill(if self.settings.light {
                        Color32::from_gray(232)
                    } else {
                        Color32::BLACK
                    })
                    .inner_margin(6)
                    .corner_radius(3)
                    .stroke(Stroke::new(
                        1_f32,
                        if active {
                            accent.gamma_multiply(0.45)
                        } else {
                            Color32::from_gray(45)
                        },
                    ))
                    .show(column, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(
                                    ["01  FILTER", "02  ECHO", "03  REVERB", "04  FLANGER"][unit],
                                )
                                .small()
                                .strong(),
                            );
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                let label = if *enabled { "ON" } else { "OFF" };
                                ui.toggle_value(enabled, label);
                            });
                        });
                        ui.horizontal(|ui| match unit {
                            0 => {
                                ui.add(
                                    Knob::new(&mut fx.filter, -1.0..=1.)
                                        .label("LPF / HPF")
                                        .unit("%")
                                        .color(accent)
                                        .size(38.),
                                );
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("LPF < 0 > HPF").small());
                                    ui.label(
                                        RichText::new(
                                            "Środek: bez filtra\nDwuklik: reset\nSHIFT: dokładnie",
                                        )
                                        .small()
                                        .weak(),
                                    );
                                });
                            }
                            1 => {
                                ui.add(
                                    Knob::new(&mut fx.echo_mix, 0.0..=1.)
                                        .default(0.3)
                                        .label("DRY / WET")
                                        .unit("%")
                                        .color(accent)
                                        .size(32.),
                                );
                                ui.add(
                                    Knob::new(&mut fx.echo_feedback, 0.0..=0.85)
                                        .default(0.35)
                                        .label("FEEDBACK")
                                        .unit("%")
                                        .color(accent)
                                        .size(32.),
                                );
                                ui.vertical(|ui| {
                                    ui.checkbox(&mut fx.echo_sync, "BPM SYNC");
                                    if fx.echo_sync {
                                        egui::ComboBox::from_id_salt(("echo-beat", i))
                                            .width(65.)
                                            .selected_text(format!("{} beat", fx.echo_beats))
                                            .show_ui(ui, |ui| {
                                                for n in [0.125, 0.25, 0.5, 1., 2.] {
                                                    ui.selectable_value(
                                                        &mut fx.echo_beats,
                                                        n,
                                                        format!("{n} beat"),
                                                    );
                                                }
                                            });
                                    } else {
                                        ui.add(
                                            egui::DragValue::new(&mut fx.echo_ms)
                                                .range(25.0..=2000.)
                                                .suffix(" ms"),
                                        );
                                    }
                                });
                            }
                            2 => {
                                ui.add(
                                    Knob::new(&mut fx.reverb_mix, 0.0..=1.)
                                        .default(0.25)
                                        .label("DRY / WET")
                                        .unit("%")
                                        .color(accent)
                                        .size(32.),
                                );
                                ui.add(
                                    Knob::new(&mut fx.reverb_room, 0.0..=1.)
                                        .default(0.6)
                                        .label("ROOM")
                                        .unit("%")
                                        .color(accent)
                                        .size(32.),
                                );
                                ui.add(
                                    Knob::new(&mut fx.reverb_damping, 0.0..=1.)
                                        .default(0.45)
                                        .label("DAMP")
                                        .unit("%")
                                        .color(accent)
                                        .size(32.),
                                );
                            }
                            _ => {
                                ui.add(
                                    Knob::new(&mut fx.flanger_mix, 0.0..=1.)
                                        .default(0.5)
                                        .label("DRY / WET")
                                        .unit("%")
                                        .color(accent)
                                        .size(32.),
                                );
                                ui.add(
                                    Knob::new(&mut fx.flanger_rate, 0.05..=5.)
                                        .default(0.25)
                                        .label("RATE Hz")
                                        .unit("Hz")
                                        .color(accent)
                                        .size(32.),
                                );
                                ui.add(
                                    Knob::new(&mut fx.flanger_depth, 0.0..=1.)
                                        .default(0.6)
                                        .label("DEPTH")
                                        .unit("%")
                                        .color(accent)
                                        .size(32.),
                                );
                            }
                        });
                    });
            }
        });
        ui.add_space(4.);
        ui.columns(4, |columns| {
            for (unit, column) in columns.iter_mut().enumerate() {
                egui::Frame::group(column.style()).show(column, |ui| {
                    let enabled = match unit {
                        0 => &mut fx.chorus_enabled,
                        1 => &mut fx.tremolo_enabled,
                        2 => &mut fx.distortion_enabled,
                        _ => &mut fx.crusher_enabled,
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            [
                                "05  CHORUS",
                                "06  TREMOLO",
                                "07  DISTORTION",
                                "08  BITCRUSHER",
                            ][unit],
                        );
                        ui.toggle_value(enabled, if *enabled { "ON" } else { "OFF" });
                    });
                    ui.horizontal(|ui| {
                        let (mix, amount, label, min, max) = match unit {
                            0 => (&mut fx.chorus_mix, &mut fx.chorus_rate, "RATE Hz", 0.05, 5.),
                            1 => (
                                &mut fx.tremolo_depth,
                                &mut fx.tremolo_rate,
                                "RATE Hz",
                                0.1,
                                16.,
                            ),
                            2 => (&mut fx.distortion_mix, &mut fx.drive, "DRIVE", 1., 20.),
                            _ => (&mut fx.crusher_mix, &mut fx.crusher_bits, "BITS", 4., 16.),
                        };
                        ui.add(
                            Knob::new(mix, 0.0..=1.)
                                .default(0.4)
                                .label(if unit == 1 { "DEPTH" } else { "DRY / WET" })
                                .unit("%")
                                .color(accent)
                                .size(32.),
                        );
                        ui.add(
                            Knob::new(amount, min..=max)
                                .default(if unit == 2 {
                                    3.
                                } else if unit == 3 {
                                    10.
                                } else {
                                    0.6
                                })
                                .label(label)
                                .color(accent)
                                .size(32.),
                        );
                    });
                });
            }
        });
        if fx != before {
            self.send(Command::Deck(i, DeckControl::Fx(fx)));
        }
    }
    fn sampler_ui(&mut self, ui: &mut egui::Ui) {
        ui.columns(4, |columns| {
            for (i, column) in columns.iter_mut().enumerate() {
                let sample = self.snapshot.samples[i].clone();
                egui::Frame::new()
                    .fill(if self.settings.light {
                        Color32::from_gray(236)
                    } else {
                        Color32::BLACK
                    })
                    .inner_margin(4)
                    .stroke(Stroke::new(1_f32, Color32::from_gray(51)))
                    .show(column, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(
                                        sample
                                            .as_ref()
                                            .map(|(t, _, _)| t.title.as_str())
                                            .unwrap_or(&tr("Pusty slot")),
                                    )
                                    .size(11.)
                                    .strong(),
                                )
                                .truncate(),
                            );
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("x").clicked() {
                                    self.sample_tokens[i] += 1;
                                    self.send(Command::SampleClear(i));
                                }
                                if ui.small_button("+").clicked() {
                                    self.sample_file(i);
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    sample.is_some(),
                                    egui::Button::new(RichText::new(">").size(22.).strong())
                                        .min_size(Vec2::new(34., 34.)),
                                )
                                .clicked()
                            {
                                self.send(Command::SampleTrigger(i));
                            }
                            let (response, p) = ui.allocate_painter(
                                Vec2::new(ui.available_width(), 34.),
                                Sense::hover(),
                            );
                            p.rect_filled(response.rect, 0., Color32::BLACK);
                            if let Some((track, start, end)) = &sample {
                                let seconds = (end - start) / track.sample_rate as f64;
                                let first = start / track.sample_rate as f64;
                                let gain = 1.
                                    / track.peaks.iter().copied().fold(0f32, f32::max).max(0.001);
                                for x in (0..response.rect.width() as usize).step_by(2) {
                                    let time =
                                        first + x as f64 / response.rect.width() as f64 * seconds;
                                    let bin = track
                                        .detail
                                        .get((time * track.detail_rate) as usize)
                                        .copied()
                                        .unwrap_or_default();
                                    let level = (bin.level * gain).min(1.);
                                    let color = spectrum_color(bin.bands, ACCENTS[0]);
                                    let px = response.rect.left() + x as f32;
                                    p.line_segment(
                                        [
                                            egui::pos2(px, response.rect.center().y - level * 15.),
                                            egui::pos2(px, response.rect.center().y + level * 15.),
                                        ],
                                        Stroke::new(1_f32, color),
                                    );
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("SAMPLE {}", i + 1)).size(9.).strong());
                            if let Some((track, start, end)) = &sample {
                                ui.label(
                                    RichText::new(format!(
                                        "{:.2} s",
                                        (end - start) / track.sample_rate as f64
                                    ))
                                    .monospace()
                                    .size(9.)
                                    .weak(),
                                );
                            } else {
                                ui.label(RichText::new("Wczytaj + / pad z decka").size(9.).weak());
                            }
                        });
                    });
            }
        });
    }
    fn visible(&mut self) -> Vec<usize> {
        if self.browser_dirty {
            let loaded = std::array::from_fn(|i| {
                self.snapshot.decks[i]
                    .track
                    .as_ref()
                    .and_then(|t| t.path.as_deref())
            });
            self.visible_rows = self.catalog.visible(
                &self.library,
                &Query {
                    text: &self.search,
                    scope: &self.scope,
                    sort: self.settings.library_sort,
                    descending: self.settings.library_descending,
                    favorites: &self.settings.favorites,
                    loaded,
                },
            );
            self.browser_dirty = false;
            self.cover_reveal = true;
            if !self.visible_rows.contains(&self.selected) {
                if let Some(first) = self.visible_rows.first() {
                    self.selected = *first;
                }
            }
        }
        self.visible_rows.clone()
    }
    fn library_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("KOLEKCJA")).strong().size(13.));
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text(tr("Szukaj: tytuł, wykonawca, album, gatunek, tonacja…"))
                        .desired_width((ui.available_width() - 390.).max(160.)),
                )
                .changed()
            {
                self.browser_dirty = true;
            }
            if ui.button(tr("+ Pliki")).clicked() {
                if let Some(files) = audio_dialog().pick_files() {
                    self.add_paths(files);
                }
            }
            if ui.button(tr("+ Folder")).clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.add_paths(vec![folder]);
                }
            }
            if ui
                .add_enabled(self.index_jobs == 0, egui::Button::new(tr("Odśwież tagi")))
                .clicked()
            {
                self.index_paths(self.library.clone(), true);
            }
            if self.index_jobs > 0 {
                ui.spinner();
                ui.label(RichText::new("Indeksowanie…").small());
            }
        });
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(
                    self.settings.tree_width.clamp(150., 400.),
                    ui.available_height() - 52.,
                ),
                egui::Layout::top_down(Align::Min),
                |ui| {
                    let height = (ui.available_height() - 4.).max(100.);
                    egui::ScrollArea::vertical()
                        .id_salt("crate-tree")
                        .auto_shrink([false, false])
                        .max_height(height)
                        .show(ui, |ui| self.browser_sidebar(ui));
                },
            );
            let (handle, response) = ui.allocate_exact_size(
                Vec2::new(5., ui.available_height().max(100.)),
                Sense::drag(),
            );
            ui.painter().line_segment(
                [handle.center_top(), handle.center_bottom()],
                Stroke::new(1_f32, Color32::from_gray(50)),
            );
            if response.dragged() {
                self.settings.tree_width = (self.settings.tree_width
                    + ui.input(|v| v.pointer.delta().x))
                .clamp(150., 400.);
            }
            response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            let preview = if self.settings.track_preview && ui.ctx().screen_rect().width() >= 1600.
            {
                180.
            } else {
                0.
            };
            let center =
                (ui.available_width() - if preview > 0. { preview + 8. } else { 0. }).max(400.);
            ui.allocate_ui_with_layout(
                Vec2::new(center, ui.available_height()),
                egui::Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(center);
                    if self.settings.cover_strip {
                        self.cover_strip(ui);
                    }
                    egui::ScrollArea::horizontal()
                        .id_salt("table-columns")
                        .max_width(center)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(1160.);
                            self.browser_table(ui);
                        });
                },
            );
            if preview > 0. {
                ui.allocate_ui_with_layout(
                    Vec2::new(preview, ui.available_height()),
                    egui::Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(preview);
                        self.track_preview(ui);
                    },
                );
            }
        });
    }
    fn track_preview(&mut self, ui: &mut egui::Ui) {
        let Some(entry) = self
            .library
            .get(self.selected)
            .and_then(|p| self.catalog.entries.get(p))
            .cloned()
        else {
            return;
        };
        self.artwork.request(&entry);
        let key = prospekt_dj::artwork::identity(&entry);
        ui.label(RichText::new("TRACK INFO").small().weak());
        ui.separator();
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(150.), Sense::click());
        ui.painter().rect_filled(rect, 1., Color32::from_gray(18));
        if let Some(art) = self.art_textures.get_mut(&key) {
            art.used = self.art_tick;
            let size = art.texture.size_vec2();
            let fitted = size * (146. / size.x.max(size.y));
            ui.painter().image(
                art.texture.id(),
                egui::Rect::from_center_size(rect.center(), fitted),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                Color32::WHITE,
            );
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "♫",
                FontId::proportional(30.),
                Color32::GRAY,
            );
        }
        response.context_menu(|ui| self.art_menu(ui, &entry));
        ui.add(egui::Label::new(RichText::new(&entry.title).strong().size(13.)).truncate())
            .on_hover_text(&entry.title);
        ui.add(egui::Label::new(&entry.artist).truncate())
            .on_hover_text(&entry.artist);
        ui.label(
            RichText::new(format!(
                "{} · {} BPM · {}",
                entry.duration.map(short_time).unwrap_or_else(|| "—".into()),
                entry
                    .bpm
                    .map(|b| format!("{b:.1}"))
                    .unwrap_or_else(|| "—".into()),
                if entry.key.is_empty() {
                    "—"
                } else {
                    &entry.key
                }
            ))
            .small()
            .color(ACCENTS[0]),
        );
        for (label, value) in [
            ("Album", &entry.album),
            ("Genre", &entry.genre),
            ("Version", &entry.version),
        ] {
            if !value.is_empty() {
                ui.add(egui::Label::new(format!("{label}: {value}")).wrap());
            }
        }
        ui.horizontal(|ui| {
            for i in 0..self.settings.deck_count {
                if ui
                    .add_enabled(!entry.missing, egui::Button::new(letter(i)))
                    .clicked()
                {
                    self.load_manual(i, entry.path.clone());
                }
            }
        });
    }
    fn browser_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.set_max_width(self.settings.tree_width);
        for (scope, label) in [
            (Scope::All, tr("Wszystkie utwory")),
            (Scope::Favorites, tr("Ulubione")),
            (Scope::Loaded, tr("Na deckach")),
            (Scope::Missing, tr("Brakujące pliki")),
        ] {
            let count = match scope {
                Scope::All => self.library.len(),
                Scope::Favorites => self.settings.favorites.len(),
                Scope::Loaded => self
                    .snapshot
                    .decks
                    .iter()
                    .filter_map(|d| d.track.as_ref().and_then(|t| t.path.as_ref()))
                    .filter(|p| self.library_set.contains(*p))
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                _ => self.catalog.entries.values().filter(|e| e.missing).count(),
            };
            if ui
                .selectable_value(&mut self.scope, scope, format!("{label}  {count}"))
                .changed()
            {
                self.browser_dirty = true;
            }
        }
        ui.add_space(10.);
        ui.label(RichText::new(tr("SKRZYNKI / CRATES")).small().weak());
        self.crate_nodes(ui, None, 0);
        if ui.button(format!("+ {}", tr("Nowa skrzynka"))).clicked() {
            self.crate_editor = Some(CrateEditor {
                id: None,
                parent: None,
                name: String::new(),
                cursor: 0..0,
            });
        }
        ui.add_space(10.);
        ui.label(RichText::new(tr("FOLDERY")).small().weak());
        let mut roots = self.folders.roots.clone();
        roots.extend(self.catalog.roots.iter().cloned());
        roots.sort();
        roots.dedup();
        for root in roots {
            self.folder_node(ui, &root, 0);
        }
        ui.add_space(12.);
        ui.label(RichText::new("Upuść pliki lub folder na okno.\nPrawy przycisk: akcje utworu.\nUsuwanie z kolekcji nie usuwa muzyki z dysku.").small().weak());
    }
    fn crate_nodes(&mut self, ui: &mut egui::Ui, parent: Option<u64>, depth: usize) {
        if depth > 32 {
            return;
        }
        let nodes: Vec<_> = self
            .catalog
            .crates
            .iter()
            .filter(|c| c.parent == parent)
            .cloned()
            .collect();
        for node in nodes {
            let key = format!("crate:{}", node.id);
            let mut open = self.settings.tree_open.contains(&key);
            let child = self
                .catalog
                .crates
                .iter()
                .any(|c| c.parent == Some(node.id));
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 12.);
                if ui
                    .add_enabled(
                        child,
                        egui::Button::new(if open { "−" } else { "+" })
                            .frame(false)
                            .min_size(Vec2::new(12., 18.)),
                    )
                    .clicked()
                {
                    open = !open;
                }
                tree_icon(ui, Color32::from_rgb(190, 144, 71));
                let response = tree_choice(
                    ui,
                    &mut self.emoji,
                    &mut self.scope,
                    Scope::Crate(node.id),
                    format!("{}  {}", node.name, node.tracks.len()),
                );
                if response.changed() {
                    self.browser_dirty = true;
                }
                response.context_menu(|ui| {
                    if ui.button(tr("Nowa podskrzynka")).clicked() {
                        self.crate_editor = Some(CrateEditor {
                            id: None,
                            parent: Some(node.id),
                            name: String::new(),
                            cursor: 0..0,
                        });
                        ui.close_menu();
                    }
                    if ui.button(tr("Zmień nazwę")).clicked() {
                        self.crate_editor = Some(CrateEditor {
                            id: Some(node.id),
                            parent: node.parent,
                            name: node.name.clone(),
                            cursor: node.name.chars().count()..node.name.chars().count(),
                        });
                        ui.close_menu();
                    }
                    if let Some(path) = self.library.get(self.selected) {
                        if ui.button(tr("Dodaj zaznaczony utwór")).clicked() {
                            if let Some(c) =
                                self.catalog.crates.iter_mut().find(|c| c.id == node.id)
                            {
                                c.tracks.insert(path.clone());
                                self.db_dirty = true;
                                self.browser_dirty = true;
                            }
                            ui.close_menu();
                        }
                    }
                    if ui.button(tr("Usuń skrzynkę")).clicked() {
                        self.catalog.delete_crate(node.id);
                        if self.scope == Scope::Crate(node.id) {
                            self.scope = Scope::All;
                        }
                        self.db_dirty = true;
                        self.browser_dirty = true;
                        ui.close_menu();
                    }
                });
            });
            if open {
                self.settings.tree_open.insert(key);
                self.crate_nodes(ui, Some(node.id), depth + 1);
            } else {
                self.settings.tree_open.remove(&key);
            }
        }
    }
    fn folder_node(&mut self, ui: &mut egui::Ui, path: &PathBuf, depth: usize) {
        if depth > 24 {
            return;
        }
        let key = format!("folder:{}", path.display());
        let mut open = self.settings.tree_open.contains(&key);
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 12.);
            if ui.small_button(if open { "−" } else { "+" }).clicked() {
                open = !open;
            }
            let label = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy();
            tree_icon(ui, Color32::from_gray(168));
            let response = tree_choice(
                ui,
                &mut self.emoji,
                &mut self.scope,
                Scope::Folder(path.clone()),
                label.to_string(),
            );
            if response.changed() {
                self.browser_dirty = true;
            }
            response
                .on_hover_text(path.display().to_string())
                .context_menu(|ui| {
                    if ui.button(tr("Importuj folder")).clicked() {
                        self.add_paths(vec![path.clone()]);
                        ui.close_menu();
                    }
                    if ui.button(tr("Odśwież folder")).clicked() {
                        self.folders.retry(path);
                        ui.close_menu();
                    }
                });
        });
        if open {
            self.settings.tree_open.insert(key);
            self.folders.request(path);
            if let Some(list) = self.folders.cache.get(path).cloned() {
                if let Some(error) = list.error {
                    ui.label(RichText::new(error).small().color(Color32::LIGHT_RED));
                }
                for child in list.children {
                    self.folder_node(ui, &child, depth + 1);
                }
                if list.truncated && ui.button(tr("Wczytaj więcej folderów")).clicked() {
                    self.folders.more(path);
                }
            } else {
                ui.label(tr("Wczytywanie…"));
            }
        } else {
            self.settings.tree_open.remove(&key);
        }
    }
    fn crate_dialog(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.crate_editor else {
            return;
        };
        let mut open = true;
        let mut save = false;
        egui::Window::new(tr("Nazwa skrzynki"))
            .id(egui::Id::new("crate-editor"))
            .default_pos(egui::pos2(
                ((ctx.screen_rect().width() - 350.) * 0.5).max(0.),
                ((ctx.screen_rect().height() - 190.) * 0.4).max(0.),
            ))
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                let output = egui::TextEdit::singleline(&mut editor.name)
                    .id_salt("crate-name")
                    .desired_width(300.)
                    .show(ui);
                if let Some(range) = output.cursor_range {
                    editor.cursor = range.sorted_cursors()[0].ccursor.index
                        ..range.sorted_cursors()[1].ccursor.index;
                }
                let (preview, _) = ui.allocate_exact_size(Vec2::new(300., 23.), Sense::hover());
                self.emoji
                    .paint(ui, preview, &editor.name, ui.visuals().text_color());
                ui.menu_button("☺ Emoji", |ui| {
                    egui::Grid::new("emoji-picker")
                        .num_columns(5)
                        .show(ui, |ui| {
                            for (i, emoji) in prospekt_dj::music_view::EMOJI.iter().enumerate() {
                                if self.emoji.button(ui, emoji).clicked() {
                                    let cursor = prospekt_dj::music_view::insert_unicode(
                                        &mut editor.name,
                                        editor.cursor.clone(),
                                        emoji,
                                    );
                                    editor.cursor = cursor..cursor;
                                    let mut state = output.state.clone();
                                    state.cursor.set_char_range(Some(
                                        egui::text::CCursorRange::one(egui::text::CCursor::new(
                                            cursor,
                                        )),
                                    ));
                                    state.store(ctx, output.response.id);
                                    output.response.request_focus();
                                    ui.close_menu();
                                }
                                if i % 5 == 4 {
                                    ui.end_row();
                                }
                            }
                        });
                });
                if ui.button(tr("Zapisz")).clicked()
                    || output.response.has_focus() && ui.input(|v| v.key_pressed(egui::Key::Enter))
                {
                    save = true;
                }
            });
        if save {
            let editor = self.crate_editor.take().unwrap();
            let id = if let Some(id) = editor.id {
                self.catalog.rename_crate(id, &editor.name).then_some(id)
            } else {
                self.catalog.add_subcrate(editor.parent, &editor.name)
            };
            if let Some(id) = id {
                self.scope = Scope::Crate(id);
                if let Some(parent) = editor.parent {
                    self.settings.tree_open.insert(format!("crate:{parent}"));
                }
                self.browser_dirty = true;
                self.db_dirty = true;
            } else {
                self.status = tr("Nazwa jest pusta lub już istnieje.");
                self.crate_editor = Some(editor);
            }
        } else if !open {
            self.crate_editor = None;
        }
    }
    fn art_menu(&mut self, ui: &mut egui::Ui, entry: &Entry) {
        ui.menu_button(tr("Okładka"), |ui| {
            let key = prospekt_dj::artwork::identity(entry);
            ui.label(
                self.art_status
                    .get(&key)
                    .map(String::as_str)
                    .unwrap_or("Pending"),
            );
            if ui.button(tr("Wybierz obraz…")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
                    .pick_file()
                {
                    if !self.artwork.manual(entry, path) {
                        self.status = tr("Kolejka okładek zajęta — spróbuj ponownie.");
                    }
                }
                ui.close_menu();
            }
            if ui.button(tr("Odrzuć dopasowanie")).clicked() {
                self.art_textures.remove(&key);
                if !self.artwork.reject(entry) {
                    self.status = tr("Kolejka okładek zajęta — spróbuj ponownie.");
                }
                ui.close_menu();
            }
            if ui.button(tr("Spróbuj ponownie")).clicked() {
                if !self.artwork.force(entry) {
                    self.status = tr("Kolejka okładek zajęta — spróbuj ponownie.");
                }
                ui.close_menu();
            }
        });
    }
    fn cover_strip(&mut self, ui: &mut egui::Ui) {
        let rows = self.visible();
        if let Some(entry) = self
            .library
            .get(self.selected)
            .and_then(|p| self.catalog.entries.get(p))
        {
            self.artwork.request(entry);
        }
        let height = 76.;
        let step = 78.;
        let mut scroll = egui::ScrollArea::horizontal()
            .id_salt("cover-strip")
            .auto_shrink([false, false])
            .max_height(height + 4.);
        if self.cover_reveal {
            if let Some(n) = rows.iter().position(|i| *i == self.selected) {
                scroll = scroll.horizontal_scroll_offset(
                    (n as f32 * step - ui.available_width() * 0.4).max(0.),
                );
            }
            self.cover_reveal = false;
        }
        scroll.show_viewport(ui, |ui, viewport| {
            let origin = ui.cursor().min;
            let total = rows.len() as f32 * step;
            ui.set_min_size(Vec2::new(total, height));
            let first = (viewport.left() / step).floor().max(0.) as usize;
            let last = ((viewport.right() / step).ceil() as usize + 1).min(rows.len());
            for (n, &index) in rows.iter().enumerate().take(last).skip(first) {
                let path = self.library[index].clone();
                let Some(entry) = self.catalog.entries.get(&path).cloned() else {
                    continue;
                };
                self.artwork.request(&entry);
                let key = prospekt_dj::artwork::identity(&entry);
                let rect = egui::Rect::from_min_size(
                    origin + egui::vec2(n as f32 * step, 0.),
                    Vec2::splat(72.),
                );
                let response = ui.interact(rect, ui.id().with(("art", index)), Sense::click());
                ui.painter().rect_filled(rect, 1., Color32::from_gray(20));
                if let Some(art) = self.art_textures.get_mut(&key) {
                    art.used = self.art_tick;
                    let size = art.texture.size_vec2();
                    let fitted = size * (68. / size.x.max(size.y));
                    let target = egui::Rect::from_center_size(rect.center(), fitted);
                    ui.painter().image(
                        art.texture.id(),
                        target,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                        Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "♫",
                        FontId::proportional(26.),
                        Color32::from_gray(75),
                    );
                }
                if index == self.selected {
                    ui.painter().rect_stroke(
                        rect,
                        1.,
                        Stroke::new(2_f32, ACCENTS[0]),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    self.selected = index;
                    self.scroll_selected = true;
                }
                if response.double_clicked() && !entry.missing {
                    self.load_manual(self.focused, path);
                }
                response
                    .on_hover_text(format!(
                        "{}\n{} · {}\n{}",
                        entry.title,
                        entry.artist,
                        entry.album,
                        self.art_status
                            .get(&key)
                            .map(String::as_str)
                            .unwrap_or("Pending")
                    ))
                    .context_menu(|ui| self.art_menu(ui, &entry));
            }
        });
    }
    fn browser_table(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let artist = if width > 650. { 130. } else { 0. };
        let genre = if width > 950. { 90. } else { 0. };
        let version = if width > 1350. { 100. } else { 0. };
        let album = if width > 1100. { 115. } else { 0. };
        let size = if width > 950. { 68. } else { 0. };
        let title = (width
            - 30.
            - 34.
            - artist
            - genre
            - version
            - album
            - 64.
            - 48.
            - 62.
            - size
            - (36. * self.settings.deck_count as f32)
            - 4.)
            .max(130.);
        let columns = [
            (30., "", None),
            (34., "#", None),
            (title, "TYTUŁ", Some(Sort::Title)),
            (artist, "WYKONAWCA", Some(Sort::Artist)),
            (album, "ALBUM", Some(Sort::Album)),
            (version, "REMIX", Some(Sort::Version)),
            (genre, "GATUNEK", Some(Sort::Genre)),
            (64., "BPM", Some(Sort::Bpm)),
            (48., "KEY", Some(Sort::Key)),
            (62., "CZAS", Some(Sort::Duration)),
            (size, "BITRATE", Some(Sort::Bitrate)),
            (36. * self.settings.deck_count as f32, "DECK", None),
        ];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            for (w, label, sort) in columns {
                if w <= 0. {
                    continue;
                }
                let label = tr(label);
                let text = if sort == Some(self.settings.library_sort) {
                    format!(
                        "{label} {}",
                        if self.settings.library_descending {
                            "v"
                        } else {
                            "^"
                        }
                    )
                } else {
                    label
                };
                if ui
                    .add_sized(
                        [w, 24.],
                        egui::Button::new(RichText::new(text).small().strong()).frame(false),
                    )
                    .clicked()
                {
                    if let Some(sort) = sort {
                        if sort == self.settings.library_sort {
                            self.settings.library_descending = !self.settings.library_descending;
                        } else {
                            self.settings.library_sort = sort;
                            self.settings.library_descending = false;
                        }
                        self.browser_dirty = true;
                    }
                }
            }
        });
        let rows = self.visible();
        let row_height = 23.;
        let height = (ui.available_height() - 39.).max(90.);
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("library-table")
            .auto_shrink([false, false])
            .max_height(height)
            .min_scrolled_height(height);
        if self.scroll_selected {
            if let Some(row) = rows.iter().position(|i| *i == self.selected) {
                scroll =
                    scroll.vertical_scroll_offset((row as f32 * row_height - height * 0.4).max(0.));
            }
            self.scroll_selected = false;
        }
        let row_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.;
        scroll.show_rows(ui, row_height, rows.len(), |ui, range| {
            for visible_index in range {
                let index = rows[visible_index];
                let path = self.library[index].clone();
                let entry = self
                    .catalog
                    .entries
                    .get(&path)
                    .cloned()
                    .unwrap_or_else(|| Entry::placeholder(path.clone()));
                ui.push_id(index, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, row_height), Sense::click());
                    let selected = index == self.selected;
                    let fill = if selected {
                        Color32::from_rgb(35, 64, 79)
                    } else if response.hovered() {
                        Color32::from_gray(43)
                    } else {
                        Color32::from_gray(if visible_index % 2 == 0 { 17 } else { 27 })
                    };
                    let fill = if self.settings.light {
                        if selected {
                            Color32::from_rgb(193, 218, 232)
                        } else {
                            ui.visuals().faint_bg_color
                        }
                    } else {
                        fill
                    };
                    ui.painter().rect_filled(rect, 0., fill);
                    if selected {
                        ui.painter().line_segment(
                            [rect.left_top(), rect.left_bottom()],
                            Stroke::new(2_f32, ACCENTS[0]),
                        );
                    }
                    if response.clicked() {
                        self.selected = index;
                        self.cover_reveal = true;
                    }
                    if response.double_clicked() && !entry.missing {
                        self.load_manual(self.focused, path.clone());
                    }
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.;
                            let favorite = self.settings.favorites.contains(&path);
                            if ui
                                .add_sized(
                                    [30., row_height],
                                    egui::Button::new(
                                        RichText::new(if favorite { "★" } else { "☆" }).color(
                                            if favorite {
                                                Color32::GOLD
                                            } else {
                                                Color32::GRAY
                                            },
                                        ),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                if favorite {
                                    self.settings.favorites.remove(&path);
                                } else {
                                    self.settings.favorites.insert(path.clone());
                                }
                                self.browser_dirty = true;
                            }
                            cell(ui, 34., &format!("{:02}", visible_index + 1), Color32::GRAY);
                            let color = if entry.missing {
                                Color32::LIGHT_RED
                            } else {
                                ui.visuals().text_color()
                            };
                            cell(
                                ui,
                                title,
                                &format!(
                                    "{}{}",
                                    if entry.missing { "! " } else { "" },
                                    entry.title
                                ),
                                color,
                            );
                            if artist > 0. {
                                cell(ui, artist, &entry.artist, ui.visuals().weak_text_color());
                            }
                            if album > 0. {
                                cell(ui, album, &entry.album, ui.visuals().weak_text_color());
                            }
                            if version > 0. {
                                cell(ui, version, &entry.version, ui.visuals().weak_text_color());
                            }
                            if genre > 0. {
                                cell(ui, genre, &entry.genre, ui.visuals().weak_text_color());
                            }
                            cell(
                                ui,
                                64.,
                                &entry
                                    .bpm
                                    .map(|n| format!("{n:.1}"))
                                    .unwrap_or_else(|| "—".into()),
                                ACCENTS[0],
                            );
                            let tagged = prospekt_dj::music_view::MusicalKey::parse(&entry.key);
                            let key = if self.settings.key_camelot {
                                tagged
                                    .map(|k| k.camelot())
                                    .unwrap_or_else(|| entry.key.clone())
                            } else {
                                entry.key.clone()
                            };
                            let color = tagged
                                .map(|k| {
                                    let [r, g, b] = k.rgb();
                                    Color32::from_rgb(r, g, b)
                                })
                                .unwrap_or(Color32::GRAY);
                            cell(ui, 48., if key.is_empty() { "—" } else { &key }, color);
                            cell(
                                ui,
                                62.,
                                &entry.duration.map(short_time).unwrap_or_else(|| "—".into()),
                                ui.visuals().text_color(),
                            );
                            if size > 0. {
                                cell(
                                    ui,
                                    size,
                                    &entry
                                        .bitrate
                                        .map(|n| format!("{n} kbps"))
                                        .unwrap_or_else(|| "—".into()),
                                    ui.visuals().weak_text_color(),
                                );
                            }
                            for d in 0..self.settings.deck_count {
                                let loaded = self.snapshot.decks[d]
                                    .track
                                    .as_ref()
                                    .and_then(|t| t.path.as_ref())
                                    == Some(&path);
                                if ui
                                    .add_enabled(
                                        !entry.missing,
                                        egui::Button::new(RichText::new(letter(d)).color(
                                            if loaded { ACCENTS[d] } else { Color32::GRAY },
                                        ))
                                        .min_size(Vec2::new(30., 20.)),
                                    )
                                    .clicked()
                                {
                                    self.load_manual(d, path.clone());
                                }
                            }
                        });
                    });
                    response
                        .on_hover_text(format!(
                            "{}\n{} · {}\n{}",
                            entry.path.display(),
                            entry.genre,
                            path.extension()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_uppercase(),
                            if entry.missing {
                                "Brak pliku. Prawy przycisk → Wskaż nową lokalizację."
                            } else {
                                "Dwuklik ładuje na aktywny deck. Enter: A · Shift+Enter: B"
                            }
                        ))
                        .context_menu(|ui| {
                            self.art_menu(ui, &entry);
                            for i in 0..self.settings.deck_count {
                                if ui
                                    .add_enabled(
                                        !entry.missing,
                                        egui::Button::new(format!("Załaduj na deck {}", letter(i))),
                                    )
                                    .clicked()
                                {
                                    self.load_manual(i, path.clone());
                                    ui.close_menu();
                                }
                            }
                            ui.menu_button("Dodaj / usuń ze skrzynki", |ui| {
                                for c in &mut self.catalog.crates {
                                    let mut in_crate = c.tracks.contains(&path);
                                    if ui.checkbox(&mut in_crate, &c.name).changed() {
                                        if in_crate {
                                            c.tracks.insert(path.clone());
                                        } else {
                                            c.tracks.remove(&path);
                                        }
                                        self.db_dirty = true;
                                        self.browser_dirty = true;
                                    }
                                }
                                if self.catalog.crates.is_empty() {
                                    ui.label("Utwórz skrzynkę po lewej stronie.");
                                }
                            });
                            if ui.button(tr("Pokaż folder pliku")).clicked() {
                                let _ = std::process::Command::new(if cfg!(windows) {
                                    "explorer"
                                } else if cfg!(target_os = "macos") {
                                    "open"
                                } else {
                                    "xdg-open"
                                })
                                .arg(path.parent().unwrap_or(&path))
                                .spawn();
                                ui.close_menu();
                            }
                            if ui
                                .add_enabled(
                                    self.index_jobs == 0,
                                    egui::Button::new(tr("Wskaż nową lokalizację…")),
                                )
                                .clicked()
                            {
                                self.relink(&path);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(
                                    self.index_jobs == 0,
                                    egui::Button::new("Usuń z kolekcji (zachowaj plik)"),
                                )
                                .clicked()
                            {
                                self.remove_entry(&path);
                                ui.close_menu();
                            }
                        });
                });
            }
        });
        ui.spacing_mut().item_spacing.y = row_spacing;
        if rows.is_empty() {
            ui.label(
                RichText::new(if self.search.is_empty() {
                    "Ten widok jest pusty. Dodaj pliki lub wybierz inną skrzynkę."
                } else {
                    "Brak wyników. Wyszukiwanie obejmuje wszystkie słowa zapytania."
                })
                .weak(),
            );
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} / {} utworów", rows.len(), self.library.len()))
                    .small()
                    .weak(),
            );
            if let Some(path) = self.library.get(self.selected) {
                ui.separator();
                ui.add(
                    egui::Label::new(RichText::new(path.display().to_string()).small().weak())
                        .truncate(),
                );
            }
        });
    }
    fn remove_entry(&mut self, path: &std::path::Path) {
        self.stop_auto();
        self.library_set.remove(path);
        self.library.retain(|p| p != path);
        self.catalog.remove(path);
        self.settings.favorites.remove(path);
        self.selected = self.selected.min(self.library.len().saturating_sub(1));
        self.browser_dirty = true;
        self.db_dirty = true;
        self.status = "Usunięto wpis z kolekcji. Plik na dysku pozostał.".into();
    }
    fn relink(&mut self, old: &std::path::Path) {
        let Some(new) = audio_dialog().set_title("Wskaż plik utworu").pick_file() else {
            return;
        };
        if new == old {
            self.index_paths(vec![new], true);
            return;
        }
        if self.library_set.contains(&new) {
            self.status = "Ten plik jest już w kolekcji.".into();
            return;
        }
        self.library_set.remove(old);
        self.library_set.insert(new.clone());
        for path in &mut self.library {
            if path == old {
                *path = new.clone();
            }
        }
        if self.settings.favorites.remove(old) {
            self.settings.favorites.insert(new.clone());
        }
        for c in &mut self.catalog.crates {
            if c.tracks.remove(old) {
                c.tracks.insert(new.clone());
            }
        }
        self.catalog.excluded.insert(old.to_path_buf());
        self.catalog.excluded.remove(&new);
        self.catalog.entries.remove(old);
        self.catalog.merge(Entry::placeholder(new.clone()));
        self.browser_dirty = true;
        self.db_dirty = true;
        self.index_paths(vec![new], true);
    }
    fn automix_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    self.auto.is_some(),
                    if self.auto.is_some() {
                        "STOP AUTOMIX"
                    } else {
                        "AUTOMIX"
                    },
                )
                .clicked()
            {
                self.start_auto();
            }
            ui.add(
                egui::DragValue::new(&mut self.settings.fade_seconds)
                    .range(1.0..=30.)
                    .suffix(" s"),
            );
            if ui.button(tr("Następny >")).clicked() {
                self.transition();
            }
            ui.label(
                RichText::new(
                    self.auto
                        .as_ref()
                        .map(|a| {
                            format!(
                                "{} / {} · {}",
                                a.index + 1,
                                a.queue.len(),
                                if a.fading {
                                    "Przejście"
                                } else if a.preparing {
                                    "Wczytywanie…"
                                } else {
                                    "Odtwarzanie"
                                }
                            )
                        })
                        .unwrap_or_else(|| "Automix korzysta z bieżącego widoku biblioteki".into()),
                )
                .small()
                .weak(),
            );
        });
    }
    fn settings_ui(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        egui::Window::new(tr("Ustawienia studia")).id(egui::Id::new("studio-settings")).default_pos(egui::pos2(((ctx.screen_rect().width()-840.)/2.).max(0.),80.)).open(&mut open).default_width(840.).resizable(true).show(ctx,|ui|{ egui::ScrollArea::vertical().max_height((ctx.screen_rect().height()-160.).max(300.)).show(ui,|ui|{
            ui.horizontal(|ui|{for (i,label) in [(4,"Ogólne"),(0,"Audio i nagrywanie"),(1,"MIDI"),(2,"Automix"),(3,"Sterowanie"),(5,"O programie")] {ui.selectable_value(&mut self.settings_tab,i,tr(label));}});ui.separator();
            match self.settings_tab{
                0=>{
                    ui.heading("Silnik audio · CPAL");
                    if ui.button(tr("Odśwież urządzenia")).clicked(){self.outputs=audio::output_names();}
                    egui::ComboBox::from_id_salt("master-output").selected_text(if self.settings.master_device.is_empty(){"MASTER: domyślne"}else{&self.settings.master_device}).show_ui(ui,|ui|{ui.selectable_value(&mut self.settings.master_device,String::new(),tr("Domyślne wyjście systemowe"));for name in &self.outputs{ui.selectable_value(&mut self.settings.master_device,name.clone(),name);}});
                    egui::ComboBox::from_id_salt("cue-output").selected_text(if self.settings.cue_device.is_empty(){"SŁUCHAWKI: wyłączone"}else{&self.settings.cue_device}).show_ui(ui,|ui|{ui.selectable_value(&mut self.settings.cue_device,String::new(),tr("Odsłuch wyłączony"));for name in &self.outputs{ui.selectable_value(&mut self.settings.cue_device,name.clone(),name);}});
                    ui.add(egui::Slider::new(&mut self.settings.latency_ms,3..=100).suffix(" ms").text("Preferowany bufor"));
                    let busy=self.record_result.is_some()||self.snapshot.decks.iter().any(|d|d.playing);
                    if ui.add_enabled(!busy,egui::Button::new(tr("Zastosuj urządzenia audio"))).clicked(){self.restart_audio();}
                    ui.label("Przed zmianą wyjść zatrzymaj odtwarzanie i nagrywanie. Rozmiar bufora zależy od sterownika. Ustawienie nie jest pomiarem całkowitej latencji.");
                    ui.label("Osobny odsłuch wymaga niezależnego urządzenia systemowego. Brak rozdzielania par 1/2 i 3/4 pojedynczej karty.");
                    ui.separator();ui.heading(tr("Nagrywanie"));
                    egui::ComboBox::from_id_salt("record-format").selected_text(self.settings.format.extension().to_uppercase()).show_ui(ui,|ui|{for format in recording::Format::ALL{ui.selectable_value(&mut self.settings.format,format,format.extension().to_uppercase());}});
                    ui.label("WAV i AIFF zapisuje kod Rust. MP3, FLAC i Ogg/Opus wymagają FFmpeg w PATH lub obok pliku programu. W razie błędu konwersji pozostaje plik WAV.");
                    ui.checkbox(&mut self.settings.auto_bpm,tr("Analizuj BPM po wczytaniu utworu"));
                    ui.label(RichText::new("Analiza jest szacunkowa; dostępne są TAP oraz ×2 / ÷2. Zmiana tempa zmienia wysokość dźwięku — brak key lock.").weak());
                },
                1=>self.midi_ui(ui),
                2=>{
                    ui.heading(tr("Kolejka i przejścia"));ui.add(egui::Slider::new(&mut self.settings.fade_seconds,1.0..=30.).text("Czas przejścia / s"));
                    ui.checkbox(&mut self.settings.auto_favorites,tr("Tylko ulubione"));ui.checkbox(&mut self.settings.auto_shuffle,tr("Losuj kolejność"));ui.checkbox(&mut self.settings.auto_repeat,tr("Powtarzaj kolejkę"));ui.checkbox(&mut self.settings.linear_fade,tr("Liniowa krzywa przejścia (zamiast stałej mocy)"));
                    ui.label("Kolejność jest ustalana przy starcie automiksu. Ręczny transport, tempo, jog i crossfader zatrzymują automat. Automix nie wyrównuje beatów.");
                },
                3=>{
                    ui.heading(tr("Klawiatura"));keyboard_help(ui);
                    ui.separator();ui.label("Mysz: kliknij waveform, aby przewinąć. Przeciągaj talerz poziomo. Dwuklik suwaka EQ zeruje pasmo, a dwuklik tempa zeruje tempo. Manual loop: pad 1 IN, 2 OUT, 3 OFF, 4 powrót. SHIFT + pad usuwa hot cue lub sample.");
                    ui.label("Biblioteka i ulubione są zapisywane lokalnie. Audio samplera i punkty CUE dotyczą bieżącej sesji.");
                    ui.label(format!("Ustawienia: {}",settings::config_path().display()));
                    ui.hyperlink_to("Licencja AGPL-3.0-or-later","https://www.gnu.org/licenses/agpl-3.0.html");
                }
                4=> {
                    ui.heading(tr("Ogólne"));
                    ui.label(tr("Język"));
                    let previous=self.settings.language.clone();
                    egui::ComboBox::from_id_salt("language").selected_text(i18n::LANGUAGES.iter().find(|(code,_)|*code == self.settings.language).map(|(_,name)|*name).unwrap_or("Polski")).show_ui(ui,|ui|{for (code,name) in i18n::LANGUAGES {ui.selectable_value(&mut self.settings.language,code.into(),i18n::shape(name));}});
                    if self.settings.language != previous {i18n::set_language(&self.settings.language);}
                    ui.separator();ui.heading(tr("Wygląd"));
                    if ui.checkbox(&mut self.settings.light,tr("Tryb jasny")).changed(){console_theme(ctx,self.settings.light);}
                    ui.checkbox(&mut self.settings.cover_strip,tr("Pasek okładek"));
                    ui.checkbox(&mut self.settings.track_preview,"Track info sidebar (wide windows)");
                    ui.checkbox(&mut self.settings.key_camelot,tr("Tonacje Camelot"));
                    if ui.checkbox(&mut self.settings.auto_cover_download,tr("Pobieraj brakujące okładki")).changed(){self.artwork.enabled(self.settings.auto_cover_download);}
                    ui.label(tr("MusicBrainz / Cover Art Archive: wysyłane są wykonawca, album i tytuł; bez audio i prywatnych ścieżek."));
                    if ui.button(tr("Wyczyść pobrane okładki")).clicked(){if self.artwork.clear(){self.art_textures.clear();self.art_status.clear();}else{self.status=tr("Kolejka okładek zajęta — spróbuj ponownie.");}}
                    ui.label(tr("Liczba decków"));
                    let previous=self.settings.deck_count;
                    ui.horizontal(|ui|{ui.selectable_value(&mut self.settings.deck_count,2,"2 decks");ui.selectable_value(&mut self.settings.deck_count,4,"4 decks");});
                    if previous!=self.settings.deck_count {
                        self.send(Command::DeckCount(self.settings.deck_count));
                        self.stop_auto();
                        if self.settings.deck_count==2 {
                            for i in 2..4 {self.send(Command::Deck(i,DeckControl::Pause));self.send(Command::Deck(i,DeckControl::Jog(0.,0.)));self.send(Command::Deck(i,DeckControl::Bend(0.)));}
                            self.focused=self.focused.min(1);self.fx_deck=self.fx_deck.min(1);
                        }
                    }
                    ui.label(tr("4 decki: crossfader A/C ↔ B/D. Automix działa na A/B."));
                    ui.separator();ui.checkbox(&mut self.settings.graphic_logo,tr("Logo graficzne"));ui.label(tr("Font logo"));ui.label("Helvetica Neue 95 Black");ui.label(&self.font_name);
                    ui.horizontal(|ui|{
                        if ui.button(tr("Wybierz font…")).clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Font",&["ttf","otf"]).pick_file(){
                            match std::fs::read(&path) {
                                Ok(bytes) if bytes.len()<16*1024*1024 && ttf_parser::Face::parse(&bytes,0).is_ok()=>{self.settings.logo_font=Some(path);self.font_name=crate::fonts::install(ctx,self.settings.logo_font.as_deref());},
                                _=>self.status="Invalid or oversized font file".into()
                            }
                        }}
                        if ui.button(tr("Przywróć font domyślny")).clicked(){self.settings.logo_font=None;self.font_name=crate::fonts::install(ctx,None);}
                    });
                    ui.label("Helvetica Neue is used only from your licensed system/font file. The bundled fallback is DejaVu Sans Bold; no proprietary font is distributed.");
                },
                _=> {
                    ui.heading(format!("PROSPEKT DJ {} · BETA",env!("CARGO_PKG_VERSION")));
                    ui.label(tr("Open source — kod źródłowy dostępny publicznie"));ui.label("AGPL-3.0-or-later");
                    ui.horizontal_wrapped(|ui|{
                        ui.hyperlink_to(tr("Kod źródłowy"),"https://github.com/prospektdj/prospektdj");
                        ui.hyperlink_to(tr("Zgłoś błąd"),"https://github.com/prospektdj/prospektdj/issues");
                        ui.hyperlink_to(tr("Pobierz nowe wydanie"),"https://github.com/prospektdj/prospektdj/releases");
                        ui.hyperlink_to("GitHub · Licenses","https://github.com/prospektdj/prospektdj/tree/main/licenses");
                    });
                    ui.hyperlink_to("Color emoji: Twemoji · CC BY 4.0", "https://github.com/jdecked/twemoji");
                    ui.separator();ui.heading(tr("Licencje"));
                    ui.text_edit_singleline(&mut self.license_search);
                    let licenses=prospekt_dj::notices::LICENSES;
                    egui::ComboBox::from_id_salt("license-list").width(700.).selected_text(licenses[self.license_selected].0).show_ui(ui,|ui|{
                        for (i,(name,_)) in licenses.iter().enumerate(){if name.to_lowercase().contains(&self.license_search.to_lowercase()){ui.selectable_value(&mut self.license_selected,i,*name);}}
                    });
                    egui::ScrollArea::vertical().id_salt("license-text").max_height(320.).show(ui,|ui|{let mut text=licenses[self.license_selected].1;ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));});
                }
            }
            ui.separator();if ui.button(tr("Zapisz ustawienia")).clicked(){self.persist();self.status="Ustawienia zapisane".into();}
        });});
        self.settings_open = open;
    }
    fn midi_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("Profil kontrolera"));
        ui.add(
            egui::TextEdit::singleline(&mut self.controller_search)
                .hint_text(tr("Search manufacturer / model")),
        );
        let selected = self
            .settings
            .controller_profile
            .and_then(|i| self.settings.controller_profiles.get(i))
            .map(|p| p.label())
            .unwrap_or_else(|| tr("Brak profili — zaimportuj mapowanie"));
        egui::ComboBox::from_id_salt("controller-profile")
            .width(600.)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (i, profile) in self.settings.controller_profiles.iter().enumerate() {
                    if !profile
                        .label()
                        .to_lowercase()
                        .contains(&self.controller_search.to_lowercase())
                    {
                        continue;
                    }
                    ui.selectable_value(
                        &mut self.settings.controller_profile,
                        Some(i),
                        profile.label(),
                    );
                }
            });
        if let Some(index) = self.settings.controller_profile {
            if let Some(profile) = self.settings.controller_profiles.get_mut(index) {
                ui.add_enabled_ui(!profile.source.starts_with("builtin:"), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr("Producent"));
                        ui.text_edit_singleline(&mut profile.manufacturer);
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Model"));
                        ui.text_edit_singleline(&mut profile.model);
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Nazwa profilu"));
                        ui.text_edit_singleline(&mut profile.name);
                    });
                });
                ui.label(format!(
                    "{} MIDI bindings · {} skipped",
                    profile.mappings.len(),
                    profile.skipped
                ));
                if !profile.authors.is_empty() {
                    ui.label(format!("Authors: {}", profile.authors));
                }
                if !profile.source.is_empty() {
                    ui.label(format!("Source: {}", profile.source));
                }
                if !profile.unsupported.is_empty() {
                    ui.collapsing(tr("Compatibility details"), |ui| {
                        for reason in &profile.unsupported {
                            ui.label(reason);
                        }
                    });
                }
                if ui
                    .add_enabled(
                        !profile.mappings.is_empty(),
                        egui::Button::new(tr("Zastosuj profil")),
                    )
                    .clicked()
                {
                    self.settings.midi = profile.mappings.clone();
                }
                if !profile.source.starts_with("builtin:") && ui.button(tr("Usuń")).clicked() {
                    self.settings.controller_profiles.remove(index);
                    self.settings.controller_profile = None;
                }
            }
        }
        ui.collapsing(tr("Zapisz bieżącą mapę jako profil"), |ui| {
            ui.label(tr("Producent"));
            ui.text_edit_singleline(&mut self.profile_manufacturer);
            ui.label(tr("Model"));
            ui.text_edit_singleline(&mut self.profile_model);
            ui.label(tr("Nazwa profilu"));
            ui.text_edit_singleline(&mut self.profile_name);
            if ui
                .add_enabled(
                    !self.profile_name.trim().is_empty(),
                    egui::Button::new(tr("Zapisz bieżącą mapę jako profil")),
                )
                .clicked()
            {
                self.settings
                    .controller_profiles
                    .push(midi::ControllerProfile {
                        manufacturer: self.profile_manufacturer.clone(),
                        model: self.profile_model.clone(),
                        name: self.profile_name.clone(),
                        mappings: self.settings.midi.clone(),
                        ..Default::default()
                    });
                self.settings.controller_profile =
                    Some(self.settings.controller_profiles.len() - 1);
            }
        });
        ui.separator();
        ui.heading("MIDI Learn");
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Odśwież wejścia")).clicked() {
                self.midi_names = midi::inputs();
            }
            egui::ComboBox::from_id_salt("midi-input")
                .selected_text(
                    self.midi_names
                        .get(self.midi_selected)
                        .map(String::as_str)
                        .unwrap_or(&tr("Brak urządzeń")),
                )
                .show_ui(ui, |ui| {
                    for (i, name) in self.midi_names.iter().enumerate() {
                        ui.selectable_value(&mut self.midi_selected, i, name);
                    }
                });
            if ui
                .button(if self.midi_connection.is_some() {
                    tr("Połącz ponownie")
                } else {
                    tr("Połącz")
                })
                .clicked()
            {
                match midi::connect(self.midi_selected, self.midi_tx.clone()) {
                    Ok(connection) => {
                        self.midi_connection = Some(connection);
                        self.midi_reconnect_name = self.midi_names.get(self.midi_selected).cloned();
                        self.status = "MIDI połączone".into();
                    }
                    Err(error) => self.status = format!("MIDI: {error}"),
                }
            }
            if ui.button(tr("Rozłącz")).clicked() {
                self.midi_connection = None;
                self.midi_reconnect_name = None;
                self.learn = false;
            }
        });
        egui::ComboBox::from_id_salt("midi-action")
            .selected_text(&self.learn_action)
            .show_ui(ui, |ui| {
                for action in actions() {
                    ui.selectable_value(&mut self.learn_action, action.clone(), action);
                }
            });
        egui::ComboBox::from_id_salt("midi-encoding")
            .selected_text(format!("{:?}", self.encoding))
            .show_ui(ui, |ui| {
                for encoding in [
                    Encoding::TwosComplement,
                    Encoding::Offset64,
                    Encoding::SignMagnitude,
                ] {
                    ui.selectable_value(&mut self.encoding, encoding, format!("{encoding:?}"));
                }
            });
        if ui
            .selectable_label(
                self.learn,
                if self.learn {
                    tr("Anuluj — czekam na MIDI…")
                } else {
                    tr("Przypisz MIDI")
                },
            )
            .clicked()
        {
            self.learn = !self.learn;
        }
        ui.label(&self.last_midi);
        ui.horizontal(|ui| {
            if ui.button(tr("Eksport mapy JSON")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("Prospekt_MIDI.json")
                    .save_file()
                {
                    let profile = midi::ControllerProfile {
                        manufacturer: self
                            .settings
                            .controller_profile
                            .and_then(|i| self.settings.controller_profiles.get(i))
                            .map(|p| p.manufacturer.clone())
                            .unwrap_or_default(),
                        model: self
                            .settings
                            .controller_profile
                            .and_then(|i| self.settings.controller_profiles.get(i))
                            .map(|p| p.model.clone())
                            .unwrap_or_default(),
                        name: self
                            .settings
                            .controller_profile
                            .and_then(|i| self.settings.controller_profiles.get(i))
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "Custom".into()),
                        mappings: self.settings.midi.clone(),
                        ..Default::default()
                    };
                    match serde_json::to_vec_pretty(&profile)
                        .map_err(anyhow::Error::from)
                        .and_then(|data| Ok(std::fs::write(path, data)?))
                    {
                        Ok(()) => self.status = "Mapa MIDI zapisana".into(),
                        Err(e) => self.status = e.to_string(),
                    }
                }
            }
            if ui.button(tr("Import mapy JSON")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("MIDI mapping", &["json", "xml", "opendj-mapping"])
                    .pick_file()
                {
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let result =
                        std::fs::read(path)
                            .map_err(anyhow::Error::from)
                            .and_then(|data| {
                                midi::ControllerProfile::import(&data, &filename, &actions())
                            });
                    match result {
                        Ok(profile) => {
                            self.settings.midi = profile.mappings.clone();
                            self.settings.controller_profiles.push(profile);
                            self.settings.controller_profile =
                                Some(self.settings.controller_profiles.len() - 1);
                            self.status = "Mapa MIDI wczytana".into();
                        }
                        Err(e) => self.status = format!("Mapa MIDI: {e}"),
                    }
                }
            }
        });
        let mut remove = None;
        egui::ScrollArea::vertical()
            .max_height(210.)
            .show(ui, |ui| {
                for (i, m) in self.settings.midi.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} ← {}:{} · {:?}",
                            m.action, m.status, m.number, m.encoding
                        ));
                        if ui.small_button(tr("Usuń")).clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
        if let Some(i) = remove {
            self.settings.midi.remove(i);
        }
        ui.label("Obsługa Note i CC, względnych enkoderów oraz import map z poprzedniej wersji HTML. Profile kontrolerów pochodzą z zaimportowanych map; sprzętowe LED nie są obsługiwane. Połączenie odnawia się po ponownym podłączeniu kontrolera o tej samej nazwie.");
    }
    fn midi_message(&mut self, bytes: &[u8]) {
        let Some((status, number, value)) = midi::key(bytes) else {
            return;
        };
        self.last_midi = format!("MIDI {status} / {number} / {value}");
        if self.learn && value > 0 {
            self.settings.midi.retain(|m| {
                !(m.status == status && m.number == number) && m.action != self.learn_action
            });
            self.settings.midi.push(Mapping {
                status,
                number,
                action: self.learn_action.clone(),
                encoding: self.encoding,
                inverted: false,
                relative: false,
            });
            self.learn = false;
            self.persist();
            return;
        }
        let Some(map) = self
            .settings
            .midi
            .iter()
            .find(|m| m.status == status && m.number == number)
            .cloned()
        else {
            return;
        };
        let action = map.action.as_str();
        let current = match action {
            "cross" => self.snapshot.cross,
            "master" => self.snapshot.master,
            "phones" => self.snapshot.phones,
            "samplevolume" => self.snapshot.sample_volume,
            _ => 0.5,
        };
        let absolute = map.normalized(value, current);
        match action {
            "cross" => {
                self.stop_auto();
                self.send(Command::Cross(absolute));
            }
            "master" => self.send(Command::Master(absolute)),
            "phones" => self.send(Command::Phones(absolute)),
            "samplevolume" => self.send(Command::SampleVolume(absolute)),
            "record" if value > 0 => self.record(),
            "browse" => {
                let delta = midi::relative(value, map.encoding) * if map.inverted { -1 } else { 1 };
                self.scroll_selected = true;
                let visible = self.visible();
                if let Some(current) = visible.iter().position(|i| *i == self.selected) {
                    self.selected = visible[(current as isize + delta.signum() as isize)
                        .clamp(0, visible.len().saturating_sub(1) as isize)
                        as usize];
                } else if let Some(first) = visible.first() {
                    self.selected = *first;
                }
            }
            _ => {
                let Some((deck, control)) = action.split_once('.') else {
                    return;
                };
                let Ok(i) = deck.parse::<usize>() else {
                    return;
                };
                if i >= self.settings.deck_count {
                    return;
                }
                let d = &self.snapshot.decks[i];
                let current = match control {
                    "gain" => d.volume,
                    "pitch" => ((d.rate as f32 - 1.) / 0.16 + 0.5).clamp(0., 1.),
                    "filter" => (d.fx.filter + 1.) * 0.5,
                    "eq0" | "eq1" | "eq2" => {
                        let db = d.eq[(control.as_bytes()[2] - b'0') as usize];
                        0.5 + if db < 0. { db / 48. } else { db / 24. }
                    }
                    _ => 0.5,
                };
                let absolute = map.normalized(value, current);
                if control == "touch" {
                    if (value > 0) != d.scratch {
                        self.send(Command::Deck(i, DeckControl::ScratchMode));
                    }
                    if value == 0 {
                        self.send(Command::Deck(i, DeckControl::Jog(0., 0.)));
                    }
                    return;
                }
                if value > 0 {
                    if let Some(p) = control
                        .strip_prefix("hotcue")
                        .and_then(|p| p.parse::<usize>().ok())
                        .filter(|p| *p < 8)
                    {
                        self.deck(i, DeckControl::PadMode(PadMode::HotCue));
                        self.deck(i, DeckControl::Pad(p, false));
                        return;
                    }
                    if let Some(p) = control
                        .strip_prefix("hotclear")
                        .and_then(|p| p.parse::<usize>().ok())
                        .filter(|p| *p < 8)
                    {
                        self.deck(i, DeckControl::PadMode(PadMode::HotCue));
                        self.deck(i, DeckControl::Pad(p, true));
                        return;
                    }
                    if control == "loopin" || control == "loopout" {
                        self.deck(i, DeckControl::PadMode(PadMode::ManualLoop));
                        self.deck(
                            i,
                            DeckControl::Pad(usize::from(control == "loopout"), false),
                        );
                        return;
                    }
                    let beats = match control {
                        "loop1" => Some(1.),
                        "loop2" => Some(2.),
                        "loop4" => Some(4.),
                        "loop8" => Some(8.),
                        _ => None,
                    };
                    if let Some(beats) = beats {
                        self.deck(i, DeckControl::Loop(beats));
                        return;
                    }
                    if control == "loopdouble" || control == "loophalf" {
                        self.deck(
                            i,
                            DeckControl::LoopScale(if control == "loopdouble" { 2. } else { 0.5 }),
                        );
                        return;
                    }
                }
                let extra = match control {
                    "chorus_mix" => Some((FxParam::ChorusMix, absolute)),
                    "chorus_rate" => Some((FxParam::ChorusRate, 0.05 + absolute * 4.95)),
                    "tremolo_depth" => Some((FxParam::TremoloDepth, absolute)),
                    "tremolo_rate" => Some((FxParam::TremoloRate, 0.1 + absolute * 15.9)),
                    "drive" => Some((FxParam::Drive, 1. + absolute * 19.)),
                    "distortion_mix" => Some((FxParam::DistortionMix, absolute)),
                    "crusher_bits" => Some((FxParam::CrusherBits, 4. + absolute * 12.)),
                    "crusher_mix" => Some((FxParam::CrusherMix, absolute)),
                    _ => None,
                };
                if let Some((param, value)) = extra {
                    self.send(Command::Deck(i, DeckControl::FxParam(param, value)));
                    return;
                }
                let switch = match control {
                    "chorus_on" => Some(FxSwitch::Chorus),
                    "tremolo_on" => Some(FxSwitch::Tremolo),
                    "distortion_on" => Some(FxSwitch::Distortion),
                    "crusher_on" => Some(FxSwitch::Crusher),
                    _ => None,
                };
                if let Some(switch) = switch {
                    if value > 0 {
                        self.send(Command::Deck(i, DeckControl::FxToggle(switch)));
                    }
                    return;
                }
                match control {
                    "filter" | "echo_mix" | "echo_feedback" | "echo_time" | "reverb_mix"
                    | "reverb_room" | "reverb_damping" | "flanger_mix" | "flanger_rate"
                    | "flanger_depth" => {
                        let (param, value) = match control {
                            "filter" => (FxParam::Filter, absolute * 2. - 1.),
                            "echo_mix" => (FxParam::EchoMix, absolute),
                            "echo_feedback" => (FxParam::EchoFeedback, absolute * 0.85),
                            "echo_time" => (FxParam::EchoMs, 25. + absolute * 1975.),
                            "reverb_mix" => (FxParam::ReverbMix, absolute),
                            "reverb_room" => (FxParam::ReverbRoom, absolute),
                            "reverb_damping" => (FxParam::ReverbDamping, absolute),
                            "flanger_mix" => (FxParam::FlangerMix, absolute),
                            "flanger_rate" => (FxParam::FlangerRate, 0.05 + absolute * 4.95),
                            _ => (FxParam::FlangerDepth, absolute),
                        };
                        self.send(Command::Deck(i, DeckControl::FxParam(param, value)));
                    }
                    "pitch" => self.deck(i, DeckControl::Rate(1. + (absolute as f64 - 0.5) * 0.16)),
                    "gain" => self.send(Command::Deck(i, DeckControl::Volume(absolute))),
                    "eq0" | "eq1" | "eq2" => self.send(Command::Deck(
                        i,
                        DeckControl::Eq(
                            control.as_bytes()[2] as usize - b'0' as usize,
                            if absolute < 0.5 {
                                (absolute - 0.5) * 48.
                            } else {
                                (absolute - 0.5) * 24.
                            },
                        ),
                    )),
                    "jog" => {
                        let value = midi::relative(value, map.encoding) as f64
                            * if map.inverted { -1. } else { 1. };
                        let d = &self.snapshot.decks[i];
                        let speed = if d.scratch {
                            value * 0.45
                        } else {
                            d.rate + value * 0.015
                        };
                        self.deck(i, DeckControl::Jog(speed, 0.09));
                    }
                    _ if value > 0 => match control {
                        "fx_bypass" | "filter_on" | "echo_on" | "echo_sync" | "reverb_on"
                        | "flanger_on" => {
                            let switch = match control {
                                "fx_bypass" => FxSwitch::Bypass,
                                "filter_on" => FxSwitch::Filter,
                                "echo_on" => FxSwitch::Echo,
                                "echo_sync" => FxSwitch::EchoSync,
                                "reverb_on" => FxSwitch::Reverb,
                                _ => FxSwitch::Flanger,
                            };
                            self.send(Command::Deck(i, DeckControl::FxToggle(switch)));
                        }
                        "play" => self.play(i),
                        "cue" => self.deck(i, DeckControl::Cue),
                        "setcue" => self.deck(i, DeckControl::SetCue),
                        "sync" => self.sync(i),
                        "scratch" => self.deck(i, DeckControl::ScratchMode),
                        "pfl" => self.send(Command::Deck(i, DeckControl::Pfl)),
                        "loopoff" => self.deck(i, DeckControl::LoopOff),
                        "load" => {
                            if let Some(path) = self.library.get(self.selected).cloned() {
                                self.load_manual(i, path);
                            }
                        }
                        "hot" | "auto" | "manual" | "sample" => {
                            self.send(Command::Deck(
                                i,
                                DeckControl::PadMode(match control {
                                    "hot" => PadMode::HotCue,
                                    "auto" => PadMode::AutoLoop,
                                    "manual" => PadMode::ManualLoop,
                                    _ => PadMode::Sampler,
                                }),
                            ));
                        }
                        "pad0" | "pad1" | "pad2" | "pad3" | "pad4" | "pad5" | "pad6" | "pad7" => {
                            self.deck(
                                i,
                                DeckControl::Pad((control.as_bytes()[3] - b'0') as usize, false),
                            )
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        self.bend_keys = [0.; 4];
        if self.crate_editor.is_some() {
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::F11)) {
            toggle_fullscreen(ctx);
        }
        if self.settings_open
            || ctx.wants_keyboard_input()
            || ctx.input(|input| {
                input.modifiers.ctrl || input.modifiers.alt || input.modifiers.command
            })
        {
            return;
        }
        let pressed = |key| ctx.input(|input| input.key_pressed(key));
        let shift = ctx.input(|input| input.modifiers.shift);
        if pressed(egui::Key::Q) {
            self.play(0);
        }
        if pressed(egui::Key::P) {
            self.play(1);
        }
        if pressed(egui::Key::A) {
            self.deck(0, DeckControl::Cue);
        }
        if pressed(egui::Key::L) {
            self.deck(1, DeckControl::Cue);
        }
        if pressed(egui::Key::Space) {
            self.play(self.focused);
        }
        if pressed(egui::Key::Escape) {
            self.stop_auto();
            self.send(Command::PauseAll);
        }
        if pressed(egui::Key::ArrowLeft) || pressed(egui::Key::ArrowRight) {
            let x = self.snapshot.cross
                + if pressed(egui::Key::ArrowLeft) {
                    -0.05
                } else {
                    0.05
                };
            self.stop_auto();
            self.send(Command::Cross(x));
        }
        if pressed(egui::Key::Enter) {
            if let Some(path) = self.library.get(self.selected).cloned() {
                self.load_manual(if shift { 1 } else { 0 }, path);
            }
        }
        if pressed(egui::Key::ArrowUp) || pressed(egui::Key::ArrowDown) {
            self.scroll_selected = true;
            let visible = self.visible();
            if let Some(current) = visible.iter().position(|i| *i == self.selected) {
                let next = (current as isize + if pressed(egui::Key::ArrowUp) { -1 } else { 1 })
                    .clamp(0, visible.len().saturating_sub(1) as isize)
                    as usize;
                self.selected = visible[next];
                self.cover_reveal = true;
            }
        }
        for (p, key) in [
            egui::Key::F1,
            egui::Key::F2,
            egui::Key::F3,
            egui::Key::F4,
            egui::Key::F5,
            egui::Key::F6,
            egui::Key::F7,
            egui::Key::F8,
        ]
        .into_iter()
        .enumerate()
        {
            if pressed(key) {
                self.deck(self.focused, DeckControl::HotCue(p, shift));
            }
        }
        for (i, keys) in [
            [
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
            ],
            [
                egui::Key::Num7,
                egui::Key::Num8,
                egui::Key::Num9,
                egui::Key::Num0,
            ],
        ]
        .iter()
        .enumerate()
        {
            for (p, key) in keys.iter().enumerate() {
                if pressed(*key) {
                    self.deck(i, DeckControl::Pad(p, shift));
                }
            }
        }
        for (i, (minus, plus)) in [(egui::Key::Z, egui::Key::X), (egui::Key::N, egui::Key::M)]
            .iter()
            .enumerate()
        {
            let value = ctx.input(|input| {
                if input.key_down(*minus) {
                    -0.04
                } else if input.key_down(*plus) {
                    0.04
                } else {
                    0.
                }
            });
            self.bend_keys[i] = value;
        }
    }
    fn confirm_close(&mut self, ctx: &egui::Context) {
        if !self.close_confirm {
            return;
        }
        egui::Window::new("Zamknąć studio?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                if self.record_result.is_some() {
                    ui.label(
                        "Trwa nagrywanie lub kodowanie miksu. Zaczekaj na zakończenie zapisu.",
                    );
                    if ui
                        .add_enabled(
                            !self.record_stopping,
                            egui::Button::new("Zatrzymaj, zapisz i zamknij"),
                        )
                        .clicked()
                    {
                        self.close_after_record = true;
                        self.record();
                        self.close_confirm = false;
                    }
                } else {
                    ui.label("Trwa odtwarzanie. Zamknięcie zatrzyma dźwięk.");
                    if ui.button("Zatrzymaj i zamknij").clicked() {
                        self.send(Command::PauseAll);
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                if ui.button("Wróć do studia").clicked() {
                    self.close_confirm = false;
                }
            });
    }
}
impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.bend_buttons = [0.; 4];
        self.crate_dialog(ctx);
        self.poll(ctx);
        self.keyboard(ctx);
        if ctx.input(|input| input.viewport().close_requested())
            && !self.allow_close
            && (self.record_result.is_some() || self.snapshot.decks.iter().any(|d| d.playing))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_confirm = true;
        }
        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        if !dropped.is_empty() {
            self.add_paths(dropped);
        }
        let focused = ctx.input(|input| input.focused);
        if !focused && self.had_focus {
            for i in 0..4 {
                self.send(Command::Deck(i, DeckControl::Jog(0., 0.)));
                self.send(Command::Deck(i, DeckControl::Bend(0.)));
            }
        }
        self.had_focus = focused;
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            self.settings.width = rect.width();
            self.settings.height = rect.height();
        }
        self.top(ctx);
        if !self.library_focus {
            egui::TopBottomPanel::top(egui::Id::new((
                "performance",
                self.dock_tab,
                self.settings.deck_count,
                self.settings.show_effects,
            )))
            .resizable(true)
            .default_height(if self.settings.deck_count == 4 {
                640.
            } else if self.settings.show_effects {
                match self.dock_tab {
                    0 => 620.,
                    1 => 440.,
                    _ => 500.,
                }
            } else {
                380.
            })
            .min_height(220.)
            .max_height(
                (ctx.screen_rect().height()
                    - if ctx.screen_rect().height() < 850. {
                        370.
                    } else {
                        300.
                    })
                .max(220.),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("console-scroll")
                    .show(ui, |ui| {
                        self.decks_ui(ui);
                        self.dock_ui(ui);
                    });
            });
        }
        egui::TopBottomPanel::bottom("automix-controls").show(ctx, |ui| self.automix_bar(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.library_ui(ui));
        if self.settings_open {
            self.settings_ui(ctx);
        }
        self.apply_bends(ctx);
        self.confirm_close(ctx);
        ctx.request_repaint_after(Duration::from_millis(33));
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist();
        self.send(Command::Recording(None));
        if let Some((tx, thread)) = self.db_save.take() {
            drop(tx);
            let _ = thread.join();
        }
        if let Some((tx, thread)) = self.offline.take() {
            let _ = tx.send(());
            let _ = thread.join();
        }
    }
}
fn letter(i: usize) -> &'static str {
    ["A", "B", "C", "D"].get(i).copied().unwrap_or("?")
}
fn time(seconds: f64) -> String {
    let s = seconds.max(0.);
    format!(
        "{:02}:{:02}.{}",
        (s / 60.) as u64,
        (s as u64) % 60,
        ((s % 1.) * 10.) as u8
    )
}
fn mode_label(mode: PadMode) -> &'static str {
    match mode {
        PadMode::HotCue => "HOT CUE",
        PadMode::AutoLoop => "AUTO LOOP",
        PadMode::ManualLoop => "MANUAL LOOP",
        PadMode::Sampler => "SAMPLER",
    }
}
fn audio_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter(
        "Audio",
        &[
            "mp3", "wav", "flac", "ogg", "opus", "aiff", "aif", "m4a", "aac", "webm",
        ],
    )
}
fn toggle_fullscreen(ctx: &egui::Context) {
    let fullscreen = ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
}
fn keyboard_help(ui: &mut egui::Ui) {
    ui.label("Q / P — play deck A / B; A / L — CUE; Spacja — play aktywnego decka; Escape — pauza obu; ← / → — crossfader; Z / X i N / M — pitch bend; 1–4 i 7–0 — pady; SHIFT + pad — usuń; Enter / SHIFT+Enter — załaduj A / B; ↑ / ↓ — wybór w bibliotece; F11 — pełny ekran.");
}
fn actions() -> Vec<String> {
    midi::actions()
}
fn waveform(
    ui: &mut egui::Ui,
    deck: &DeckView,
    accent: Color32,
    height: f32,
    span: Option<f64>,
) -> Option<WaveAction> {
    let (response, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), height), Sense::click());
    let rect = response.rect;
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 3., ui.visuals().extreme_bg_color);
    painter.line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1_f32, accent.gamma_multiply(0.2)),
    );
    let Some(track) = &deck.track else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "BRAK UTWORU",
            FontId::proportional(11.),
            ui.visuals().weak_text_color(),
        );
        return None;
    };
    let duration = track.duration();
    let range = span.unwrap_or(duration).max(0.001);
    let start = if span.is_some() {
        deck.position - range * 0.28
    } else {
        0.
    };
    let width = rect.width() as usize;
    if span.is_some() {
        let beat = 60. / deck.bpm.clamp(40., 300.);
        let first = ((start - deck.cue) / beat).floor() as i64;
        for n in first..=first + (range / beat).ceil() as i64 + 1 {
            let t = deck.cue + n as f64 * beat;
            if t < 0. || t >= duration || t < start || t > start + range {
                continue;
            }
            let x = rect.left() + ((t - start) / range) as f32 * rect.width();
            let bar = n.rem_euclid(4) == 0;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(1_f32, accent.gamma_multiply(if bar { 0.25 } else { 0.10 })),
            );
            if bar && n >= 0 {
                painter.text(
                    egui::pos2(x + 4., rect.top() + 2.),
                    egui::Align2::LEFT_TOP,
                    format!("{}", n / 4 + 1),
                    FontId::monospace(8.),
                    accent.gamma_multiply(0.7),
                );
            }
        }
    }
    if let Some((a, b)) = deck.loop_range {
        let x1 = rect.left() + ((a - start) / range) as f32 * rect.width();
        let x2 = rect.left() + ((b - start) / range) as f32 * rect.width();
        let fill = egui::Rect::from_min_max(
            egui::pos2(x1.max(rect.left()), rect.top()),
            egui::pos2(x2.min(rect.right()).max(x1.max(rect.left())), rect.bottom()),
        )
        .intersect(rect);
        painter.rect_filled(fill, 0., accent.gamma_multiply(0.1));
    }
    let gain = 1. / track.peaks.iter().copied().fold(0f32, f32::max).max(0.001);
    for x in (0..width).step_by(2) {
        let t = start + x as f64 / width.max(1) as f64 * range;
        if t < 0. || t >= duration {
            continue;
        }
        let (peak, bands) = if span.is_some() {
            let bin = track
                .detail
                .get((t * track.detail_rate) as usize)
                .copied()
                .unwrap_or_default();
            (bin.level, bin.bands)
        } else {
            let index = (t / duration * track.peaks.len() as f64) as usize;
            (
                track.peaks.get(index).copied().unwrap_or(0.),
                track.bands.get(index).copied().unwrap_or([0.; 3]),
            )
        };
        let peak = (peak * gain).min(1.);
        let px = rect.left() + x as f32;
        painter.line_segment(
            [
                egui::pos2(px, rect.center().y - peak * height * 0.46),
                egui::pos2(px, rect.center().y + peak * height * 0.46),
            ],
            Stroke::new(1.5_f32, spectrum_color(bands, accent)),
        );
    }
    for (t, color) in [
        (deck.cue, Color32::from_rgb(230, 191, 100)),
        (deck.position, Color32::WHITE),
    ] {
        if t >= start && t < start + range {
            let x = rect.left() + ((t - start) / range) as f32 * rect.width();
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(1.5_f32, color),
            );
        }
    }
    let timeline = prospekt_dj::music_view::Timeline {
        start,
        span: range,
        duration,
    };
    let markers: Vec<_> = deck
        .hot
        .iter()
        .enumerate()
        .filter_map(|(n, t)| {
            t.and_then(|t| {
                timeline
                    .project(t)
                    .map(|fraction| (n, t, rect.left() + fraction * rect.width()))
            })
        })
        .collect();
    for &(n, _, x) in &markers {
        let color = cue_color(n);
        painter.line_segment(
            [egui::pos2(x, rect.top() + 7.), egui::pos2(x, rect.bottom())],
            Stroke::new(1_f32, color.gamma_multiply(0.65)),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(x - 4., rect.top()),
                egui::pos2(x + 4., rect.top()),
                egui::pos2(x, rect.top() + 7.),
            ],
            color,
            Stroke::NONE,
        ));
        if height > 25. {
            painter.text(
                egui::pos2(x + 4., rect.top() + 8.),
                egui::Align2::LEFT_TOP,
                (n + 1).to_string(),
                FontId::monospace(9.),
                color,
            );
        }
    }
    let hovered = ui
        .input(|v| v.pointer.hover_pos())
        .filter(|p| rect.contains(*p));
    let hits: Vec<_> = hovered
        .map(|p| {
            markers
                .iter()
                .filter(|(_, _, x)| (*x - p.x).abs() <= 6.)
                .collect()
        })
        .unwrap_or_default();
    if !hits.is_empty() {
        response.clone().on_hover_text(
            hits.iter()
                .map(|(n, t, _)| format!("Cue {} · {}", n + 1, time(*t)))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    let popup_id = response.id.with("cue-overlap");
    if response.clicked() && hits.len() > 1 {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    let mut chosen = None;
    egui::popup_below_widget(
        ui,
        popup_id,
        &response,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            // Full list remains stable after pointer moves into the popup.
            for (n, t) in deck
                .hot
                .iter()
                .enumerate()
                .filter_map(|(n, t)| t.map(|t| (n, t)))
            {
                if ui
                    .button(
                        RichText::new(format!("Cue {} · {}", n + 1, time(t))).color(cue_color(n)),
                    )
                    .clicked()
                {
                    chosen = Some(WaveAction::Cue(n));
                    ui.close_menu();
                }
            }
        },
    );
    if chosen.is_some() {
        return chosen;
    }
    if response.clicked() {
        if hits.len() == 1 {
            return Some(WaveAction::Cue(hits[0].0));
        }
        if hits.is_empty() {
            return response
                .interact_pointer_pos()
                .map(|p| WaveAction::Seek(timeline.at((p.x - rect.left()) / rect.width())));
        }
    }
    None
}

fn cell(ui: &mut egui::Ui, width: f32, text: &str, color: Color32) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 23.),
        egui::Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_size(Vec2::new(width, 23.));
            ui.add(egui::Label::new(RichText::new(text).color(color).size(11.)).truncate());
        },
    );
}
fn short_time(seconds: f64) -> String {
    format!("{:02}:{:02}", (seconds / 60.) as u64, seconds as u64 % 60)
}

fn console_theme(ctx: &egui::Context, light: bool) {
    let mut v = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    if !light {
        v.panel_fill = Color32::from_gray(8);
        v.window_fill = Color32::from_gray(8);
        v.extreme_bg_color = Color32::BLACK;
        v.faint_bg_color = Color32::BLACK;
        v.widgets.inactive.bg_fill = Color32::BLACK;
        v.widgets.inactive.weak_bg_fill = Color32::BLACK;
        v.widgets.inactive.bg_stroke = Stroke::new(1_f32, Color32::BLACK);
        v.widgets.hovered.bg_fill = Color32::BLACK;
        v.override_text_color = Some(Color32::from_rgb(216, 226, 237));
    }
    v.widgets.inactive.corner_radius = egui::CornerRadius::same(1);
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(1);
    v.widgets.active.corner_radius = egui::CornerRadius::same(1);
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.22 };
    v.selection.bg_fill = if light {
        Color32::from_rgb(27, 91, 126)
    } else {
        Color32::BLACK
    };
    if !light {
        v.window_fill = Color32::from_gray(8);
        v.panel_fill = Color32::from_gray(8);
        v.extreme_bg_color = Color32::BLACK;
        v.faint_bg_color = Color32::BLACK;
        v.code_bg_color = Color32::BLACK;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.bg_fill = Color32::BLACK;
            w.weak_bg_fill = Color32::BLACK;
        }
        v.selection.stroke = Stroke::new(1_f32, ACCENTS[0]);
    }
    ctx.set_visuals(v);
}

fn spectrum_color(bands: [f32; 3], fallback: Color32) -> Color32 {
    let max = bands.iter().copied().fold(0f32, f32::max);
    if max < 1e-6 {
        return fallback.gamma_multiply(0.5);
    }
    Color32::from_rgb(
        (35. + 210. * (bands[0] / max).sqrt()) as u8,
        (30. + 205. * (bands[1] / max).sqrt()) as u8,
        (35. + 210. * (bands[2] / max).sqrt()) as u8,
    )
}

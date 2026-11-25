use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use egui::{Align, Color32, FontId, RichText, Sense, Stroke, Vec2};
use prospekt_dj::i18n::{self, tr};
use prospekt_dj::theme::apply as console_theme;
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

struct ReflectionColorEditor {
    draft: Color32,
    previous: Color32,
    html: String,
}
struct CloudBrowser {
    drive: prospekt_dj::google_drive::Drive,
    root: PathBuf,
    client: Option<prospekt_dj::google_drive::OAuthClient>,
    open: bool,
    connected: bool,
    busy: bool,
    status: String,
    auth_url: Option<String>,
    folder: String,
    breadcrumbs: Vec<(String, String)>,
    files: Vec<prospekt_dj::google_drive::DriveFile>,
    next: Option<String>,
}
impl CloudBrowser {
    fn new(root: PathBuf) -> Self {
        Self {
            drive: prospekt_dj::google_drive::Drive::new(root.clone()),
            root,
            client: None,
            open: false,
            connected: false,
            busy: false,
            status: "Niepołączony".into(),
            auth_url: None,
            folder: "root".into(),
            breadcrumbs: vec![("root".into(), "Mój dysk".into())],
            files: Vec::new(),
            next: None,
        }
    }
}

type FileJobResult = Result<(PathBuf, PathBuf, bool), String>;

const ACCENTS: [Color32; 4] = [
    Color32::from_rgb(49, 188, 255),
    Color32::from_rgb(66, 196, 227),
    Color32::from_rgb(209, 118, 255),
    Color32::from_rgb(244, 164, 69),
];
fn tree_choice(
    ui: &mut egui::Ui,
    emoji: &mut crate::emoji::EmojiAtlas,
    current: &mut Scope,
    value: Scope,
    label: String,
) -> egui::Response {
    let is_sampler = matches!(&value, Scope::Folder(path)
        if prospekt_dj::portable::sampler_dir().ok().as_ref() == Some(path));
    let (rect, mut response) = ui.allocate_exact_size(
        Vec2::new(
            ui.available_width().max(20.),
            if is_sampler { 28. } else { 21. },
        ),
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
    if is_sampler {
        ui.painter().with_clip_rect(rect).text(
            egui::pos2(rect.left(), rect.center().y),
            egui::Align2::LEFT_CENTER,
            &label,
            FontId::new(18., egui::FontFamily::Name("Arial Bold".into())),
            ui.visuals().text_color(),
        );
    } else if matches!(value, Scope::Loaded | Scope::Favorites) {
        let icon_rect = egui::Rect::from_min_size(rect.min, Vec2::new(24., rect.height()));
        emoji.paint(
            ui,
            icon_rect,
            if value == Scope::Favorites {
                "❤️"
            } else {
                "💿"
            },
            Color32::WHITE,
        );
        ui.painter().with_clip_rect(rect).text(
            egui::pos2(rect.left() + 25., rect.center().y),
            egui::Align2::LEFT_CENTER,
            label.to_uppercase(),
            FontId::new(12., egui::FontFamily::Name("Arial Bold".into())),
            if value == Scope::Favorites {
                Color32::from_rgb(255, 105, 160)
            } else {
                Color32::from_rgb(255, 35, 48)
            },
        );
    } else {
        emoji.paint(ui, rect, &label, ui.visuals().text_color());
    }
    if response.clicked() {
        *current = value;
        response.mark_changed();
    }
    response.on_hover_text(label)
}
fn tree_icon(ui: &mut egui::Ui, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(13., 13.), Sense::click());
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
    response
}
// Original vector artwork: familiar desktop/document/drive silhouettes.
fn storage_category(ui: &mut egui::Ui, label: &str, kind: usize, body: impl FnOnce(&mut egui::Ui)) {
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.make_persistent_id(("storage-category", label)),
        false,
    )
    .show_header(ui, |ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(20., 20.), Sense::hover());
        let p = ui.painter();
        let box_at =
            |x, y, w, h| egui::Rect::from_min_size(rect.min + egui::vec2(x, y), egui::vec2(w, h));
        let blue = Color32::from_rgb(47, 167, 231);
        let silver = Color32::from_rgb(185, 199, 211);
        match kind {
            0 => {
                p.rect_filled(box_at(0., 1., 20., 14.), 1., silver);
                p.rect_filled(box_at(2., 3., 16., 10.), 0., blue);
                p.rect_filled(box_at(9., 15., 2., 3.), 0., silver);
                p.rect_filled(box_at(5., 18., 10., 2.), 0., silver);
            }
            1 => {
                p.rect_filled(box_at(1., 4., 8., 5.), 1., Color32::from_rgb(235, 175, 51));
                p.rect_filled(
                    box_at(1., 8., 18., 11.),
                    1.,
                    Color32::from_rgb(255, 204, 85),
                );
                p.rect_filled(
                    box_at(7., 1., 10., 15.),
                    0.,
                    Color32::from_rgb(235, 243, 250),
                );
                for y in [5., 8., 11.] {
                    p.rect_filled(box_at(9., y, 6., 1.), 0., blue);
                }
            }
            _ => {
                p.rect_filled(box_at(1., 4., 18., 14.), 2., silver);
                p.rect_filled(
                    box_at(2., 4., 16., 9.),
                    1.,
                    Color32::from_rgb(223, 230, 237),
                );
                p.rect_filled(box_at(3., 14., 14., 2.), 0., Color32::from_rgb(76, 91, 103));
                p.circle_filled(
                    rect.min + egui::vec2(15., 15.),
                    1.,
                    Color32::from_rgb(75, 231, 125),
                );
                if kind == 3 {
                    p.rect_filled(box_at(8., 0., 5., 6.), 0., blue);
                    for x in [9., 12.] {
                        p.rect_filled(box_at(x, 0., 1., 2.), 0., silver);
                    }
                }
            }
        }
        ui.label(label);
    })
    .body(body);
}
/// Header label: the original text in the classic skin, a short form in the modern one
/// so the whole top bar stays inside the window.
/// Size for the modern (compact) skins; the original skin keeps its size.
fn modern_size(classic: f32, modern: f32) -> f32 {
    if prospekt_dj::theme::classic() {
        classic
    } else {
        modern
    }
}
fn header_label(_long: &'static str, short: &'static str) -> &'static str {
    short
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
    Zoom(f64),
    Seek(f64),
    Cue(usize),
}
type LoadAuto = Option<(u64, usize, bool)>;
enum Work {
    Preview(u64, Result<Arc<Track>, String>),
    History(Option<String>, Result<prospekt_dj::history::View, String>),
    ImportInspected(Result<prospekt_dj::dj_import::Report, String>),
    ImportApplied(Result<prospekt_dj::dj_import::Applied, String>),
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
    microphone: Option<prospekt_dj::microphone::Microphone>,
    mic_names: Vec<String>,
    mic_status: String,
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
    history: prospekt_dj::history::View,
    history_poll: Instant,
    history_loading: bool,
    history_error: Option<String>,
    recommendation_job: Option<(String, Receiver<prospekt_dj::recommendations::Batch>)>,
    recommendation_results: Vec<prospekt_dj::recommendations::Suggestion>,
    recommendation_context: String,
    recommendation_updated: Option<Instant>,
    recommendation_selected: usize,
    recommendation_advanced: Option<Instant>,
    recommendation_memory: prospekt_dj::recommendations::SessionMemory,
    recommendation_restart_at: Option<Instant>,
    recommendation_stats: prospekt_dj::recommendations::Statistics,
    recommendation_log: prospekt_dj::recommendation_log::Logger,
    recommendation_history_open: bool,
    crate_editor: Option<CrateEditor>,
    folders: prospekt_dj::folder_tree::Folders,
    storage: prospekt_dj::storage::Storage,
    artwork: prospekt_dj::artwork::Artwork,
    art_textures: std::collections::HashMap<String, ArtTexture>,
    art_status: std::collections::HashMap<String, String>,
    art_tick: u64,
    cover_reveal: bool,
    scroll_selected: bool,
    deck_effect: [usize; 4],
    selected: usize,
    focused: usize,
    settings_open: bool,
    settings_tab: usize,
    settings_search: String,
    import_source: prospekt_dj::dj_import::Source,
    import_path: String,
    import_report: Option<prospekt_dj::dj_import::Report>,
    import_busy: bool,
    import_cue_revision: (u64, u64),
    import_cancel: Arc<std::sync::atomic::AtomicBool>,
    import_messages: Vec<String>,
    library_focus: bool,
    dock_tab: usize,
    zoom: f64,
    status: String,
    deck_tokens: [u64; 4],
    sample_tokens: [u64; 4],
    loading: [bool; 4],
    eject_pending: Option<(usize, String, u64)>,
    reverse_requested: [bool; 4],
    reverse_sent: [bool; 4],
    covers: [Option<egui::TextureHandle>; 4],
    taps: [Vec<Instant>; 4],
    synced_at: [Option<Instant>; 4],
    auto: Option<AutoMix>,
    generation: u64,
    record_result: Option<Receiver<Result<PathBuf, String>>>,
    waveform_detail: Option<PathBuf>,
    rename_file: Option<(PathBuf, String)>,
    file_clipboard: Option<PathBuf>,
    new_folder: Option<(PathBuf, String)>,
    file_job: Option<Receiver<FileJobResult>>,
    metadata_preview: prospekt_dj::metadata_preview::Cache,
    cover_drag: Option<(PathBuf, egui::Pos2, bool)>,
    cover_reflections: Vec<egui::epaint::Mesh>,
    library_last_row_bottom: Option<f32>,
    waveform_cover_reflection: Option<egui::Rect>,
    header_cover_reflection: Option<egui::Rect>,
    footer_cover_reflection: Option<egui::Rect>,
    sidebar_cover_reflection: Option<egui::Rect>,
    cover_reflection_left: f32,
    deck_drop_rects: [egui::Rect; 4],
    deck_rows_height: f32,
    preview_path: Option<PathBuf>,
    preview_token: u64,
    preview_loading: bool,
    record_stopping: bool,
    broadcast: Option<prospekt_dj::broadcast::Session>,
    broadcast_status: String,
    record_start: Option<Instant>,
    midi_names: Vec<String>,
    midi_selected: usize,
    midi_connection: Option<midi::Connection>,
    midi_second_connection: Option<midi::Connection>,
    midi_second_selected: usize,
    midi_second_name: Option<String>,
    midi_second_profile: Option<usize>,
    midi_second_tx: Sender<Vec<u8>>,
    midi_second_rx: Receiver<Vec<u8>>,
    midi_learn_second: bool,
    midi_learn_reveal: bool,
    key_window: bool,
    reflection_color_editor: Option<ReflectionColorEditor>,
    device_window: bool,
    cloud: CloudBrowser,
    dual_midi_pair: Option<(String, String)>,
    dual_midi_notice: bool,
    midi_reconnect_name: Option<String>,
    midi_poll: Instant,
    midi_scan: Option<Receiver<Vec<String>>>,
    controller_notices: std::collections::VecDeque<String>,
    detected_profile: Option<usize>,
    detected_status: String,
    midi_tx: Sender<Vec<u8>>,
    midi_rx: Receiver<Vec<u8>>,
    learn: bool,
    learn_action: String,
    encoding: Encoding,
    last_midi: String,
    outputs: Vec<String>,
    audio_diagnostics: String,
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
    frame_times: std::collections::VecDeque<Instant>,
    measured_fps: f32,
    library_data: prospekt_dj::library_data::LibraryData,
    data_refresh: Instant,
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
        prospekt_dj::theme::set_skin(settings.skin);
        console_theme(&cc.egui_ctx, settings.light);
        i18n::set_language(&settings.language);
        crate::fonts::install(&cc.egui_ctx, settings.logo_font.as_deref(), &settings);
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
        cc.egui_ctx
            .set_zoom_factor(settings.ui_scale.clamp(0.75, 1.75));
        let (tx, rx) = unbounded();
        let (error_tx, errors) = bounded(64);
        let (work_tx, work) = unbounded();
        let (midi_tx, midi_rx) = bounded(256);
        let (midi_second_tx, midi_second_rx) = bounded(256);
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
            prospekt_dj::portable::covers_dir()
                .unwrap_or_else(|_| settings::config_path().with_file_name("covers")),
            settings.auto_cover_download,
        );
        let mut studio = Self {
            settings,
            tx,
            rx,
            shared: Arc::new(Mutex::new(Snapshot::default())),
            snapshot: Snapshot::default(),
            runtime: None,
            microphone: None,
            mic_names: Vec::new(),
            mic_status: String::new(),
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
            history: Default::default(),
            history_poll: Instant::now() - Duration::from_secs(3),
            history_loading: false,
            history_error: None,
            recommendation_job: None,
            recommendation_results: Vec::new(),
            recommendation_context: String::new(),
            recommendation_updated: None,
            recommendation_selected: 0,
            recommendation_advanced: None,
            recommendation_memory: Default::default(),
            recommendation_restart_at: None,
            recommendation_stats: Default::default(),
            recommendation_log: prospekt_dj::recommendation_log::Logger::new(
                settings::config_path().with_file_name("recommendations"),
            ),
            recommendation_history_open: false,
            crate_editor: None,
            folders: Default::default(),
            storage: Default::default(),
            artwork,
            art_textures: Default::default(),
            art_status: Default::default(),
            art_tick: 0,
            cover_reveal: false,
            scroll_selected: false,
            deck_effect: [0; 4],
            selected: 0,
            focused: 0,
            settings_open: false,
            settings_tab: 4,
            settings_search: String::new(),
            import_source: prospekt_dj::dj_import::Source::RekordboxXml,
            import_path: String::new(),
            import_report: None,
            import_busy: false,
            import_cue_revision: (0, 0),
            import_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            import_messages: vec![],
            library_focus: false,
            dock_tab,
            zoom: 8.,
            status: "Gotowy · pre-alpha 0.0.1 test · pliki pozostają na komputerze".into(),
            deck_tokens: [0; 4],
            sample_tokens: [0; 4],
            loading: [false; 4],
            eject_pending: None,
            reverse_requested: [false; 4],
            reverse_sent: [false; 4],
            covers: std::array::from_fn(|_| None),
            taps: std::array::from_fn(|_| Vec::new()),
            synced_at: [None; 4],
            auto: None,
            generation: 0,
            record_result: None,
            waveform_detail: None,
            rename_file: None,
            file_clipboard: None,
            new_folder: None,
            file_job: None,
            metadata_preview: Default::default(),
            cover_drag: None,
            cover_reflections: Vec::new(),
            library_last_row_bottom: None,
            waveform_cover_reflection: None,
            header_cover_reflection: None,
            footer_cover_reflection: None,
            sidebar_cover_reflection: None,
            cover_reflection_left: 0.,
            deck_drop_rects: [egui::Rect::NOTHING; 4],
            deck_rows_height: 180.,
            preview_path: None,
            preview_token: 0,
            preview_loading: false,
            record_stopping: false,
            broadcast: None,
            broadcast_status: String::new(),
            record_start: None,
            midi_names: Vec::new(),
            midi_selected: 0,
            midi_connection: None,
            midi_second_connection: None,
            midi_second_selected: 0,
            midi_second_name: None,
            midi_second_profile: None,
            midi_second_tx,
            midi_second_rx,
            midi_learn_second: false,
            midi_learn_reveal: false,
            key_window: false,
            reflection_color_editor: None,
            device_window: false,
            cloud: CloudBrowser::new(
                settings::config_path()
                    .with_file_name("CLOUD")
                    .join("GoogleDrive"),
            ),
            dual_midi_pair: None,
            dual_midi_notice: false,
            midi_reconnect_name: None,
            midi_poll: Instant::now() - Duration::from_secs(3),
            midi_scan: None,
            controller_notices: Default::default(),
            detected_profile: None,
            detected_status: String::new(),
            midi_tx,
            midi_rx,
            learn: false,
            learn_action: "0.play".into(),
            encoding: Encoding::default(),
            last_midi: String::new(),
            outputs: Vec::new(),
            audio_diagnostics: String::new(),
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
            frame_times: std::collections::VecDeque::new(),
            measured_fps: 0.,
            library_data: prospekt_dj::library_data::LibraryData::new(
                settings::config_path().with_file_name("tracks.sqlite"),
            ),
            data_refresh: Instant::now(),
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
        studio.send(Command::HistoryThreshold(studio.settings.history_threshold));
        studio.send(Command::CrossCurve(studio.settings.cross_curve));
        studio.send(Command::CrossEnabled(studio.settings.cross_enabled));
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
        if !self.runtime.as_ref().is_some_and(|r| r.is_running()) {
            self.status = "Brak wyjścia audio. Wybierz urządzenie w Ustawieniach.".into();
            return;
        }
        self.deck(i, DeckControl::Toggle);
    }
    fn restart_audio(&mut self) {
        self.stop_preview();
        if self.broadcast.is_some() {
            self.send(Command::Broadcast(None));
            self.broadcast = None;
            self.broadcast_status = "Transmisja zatrzymana przy zmianie urządzenia audio".into();
        }
        if self.record_result.is_some() {
            self.status = "Zakończ nagrywanie przed zmianą urządzeń.".into();
            return;
        }
        self.stop_auto();
        self.settings.effects = self.snapshot.decks.iter().map(|d| d.fx).collect();
        self.microphone = None;
        self.send(Command::Microphone(None));
        self.runtime = None;
        if let Some((stop, thread)) = self.offline.take() {
            let _ = stop.send(());
            let _ = thread.join();
        }
        while self.rx.try_recv().is_ok() {}
        let options = audio::Options {
            backend: self.settings.audio_backend,
            master_name: self.settings.master_device.clone(),
            cue_name: self.settings.cue_device.clone(),
            buffer_frames: self.settings.audio_buffer_frames,
            master_pair: self.settings.audio_master_pair,
            cue_pair: self.settings.audio_cue_pair,
        };
        match audio::start_with_options(
            self.rx.clone(),
            self.shared.clone(),
            self.error_tx.clone(),
            &options,
        ) {
            Ok(runtime) => {
                self.audio_diagnostics = runtime.diagnostics();
                audio::save_diagnostics(&self.audio_diagnostics);
                self.status = format!(
                    "{} · {} · {} Hz{}",
                    runtime.backend.label(),
                    runtime.device_name,
                    runtime.sample_rate,
                    if runtime.warnings.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", runtime.warnings.join(" | "))
                    }
                );
                self.runtime = Some(runtime);
            }
            Err(e) => {
                self.audio_diagnostics=format!("Prospekt DJ {} PRE-ALPHA TEST\n{}\nOS: {} {}\nRequested configuration: {options:?}\nAudio startup error: {e:#}",env!("CARGO_PKG_VERSION"),chrono::Local::now().to_rfc3339(),std::env::consts::OS,std::env::consts::ARCH);
                audio::save_diagnostics(&self.audio_diagnostics);
                self.status = format!("Audio niedostępne: {e:#}. Szczegóły: Ustawienia → Audio.");
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
        self.send(Command::HistoryThreshold(self.settings.history_threshold));
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
        if self.file_job.is_some() {
            self.status = "Poczekaj na zakończenie operacji na plikach".into();
            return;
        }
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
        while let Ok(event) = self.cloud.drive.events.try_recv() {
            use prospekt_dj::google_drive::Event;
            match event {
                Event::Authorize(url) => {
                    self.cloud.auth_url = Some(url.clone());
                    self.cloud.status =
                        "Dokończ logowanie Google w przeglądarce (do 3 minut)…".into();
                    ctx.open_url(egui::OpenUrl::new_tab(url));
                }
                Event::Connected => {
                    self.cloud.connected = true;
                    self.cloud.auth_url = None;
                    self.cloud.status = "Połączono — wczytywanie plików…".into();
                    self.cloud.folder = "root".into();
                    self.cloud.breadcrumbs = vec![("root".into(), "Mój dysk".into())];
                    self.cloud.busy = self.cloud.drive.list("root".into(), None);
                }
                Event::Disconnected => {
                    self.cloud.connected = false;
                    self.cloud.busy = false;
                    self.cloud.files.clear();
                    self.cloud.auth_url = None;
                    self.cloud.status = "Rozłączono. Pobrane pliki pozostają na dysku.".into();
                }
                Event::Files {
                    folder,
                    files,
                    next,
                } => {
                    self.cloud.folder = folder;
                    self.cloud.files = files;
                    self.cloud.next = next;
                    self.cloud.busy = false;
                    self.cloud.status =
                        format!("Pliki i foldery na stronie: {}", self.cloud.files.len());
                }
                Event::Downloaded(path, deck) => {
                    self.cloud.busy = false;
                    self.cloud.status = format!("Zapisano: {}", path.display());
                    self.add_paths(vec![path.clone()]);
                    if let Some(deck) = deck {
                        self.load_manual(deck, path);
                    }
                }
                Event::Error(error) => {
                    self.cloud.busy = false;
                    self.cloud.status = error.clone();
                    self.status = error;
                }
            }
        }

        for error in self.recommendation_log.errors.try_iter() {
            self.status = error;
        }
        self.library_data.poll();
        if !self.history_loading && self.history_poll.elapsed() >= Duration::from_secs(2) {
            self.history_loading = true;
            self.history_poll = Instant::now();
            let selected = if let Scope::History(id) = &self.scope {
                Some(id.clone())
            } else {
                None
            };
            let root = settings::config_path().with_file_name("history");
            let tx = self.work_tx.clone();
            std::thread::spawn(move || {
                let result = prospekt_dj::history::view(&root, selected.as_deref())
                    .map_err(|error| format!("{error:#}"));
                let _ = tx.send(Work::History(selected, result));
            });
        }
        for (path, record) in &self.library_data.records {
            if let Some(entry) = self.catalog.entries.get_mut(path) {
                if entry.local_plays != record.play_count
                    || entry.local_last_play != record.last_play
                {
                    entry.local_plays = record.play_count;
                    entry.local_last_play = record.last_play;
                    self.db_dirty = true;
                    self.browser_dirty = true;
                }
                if entry.rating != record.user.rating {
                    entry.rating = record.user.rating;
                    self.db_dirty = true;
                    self.browser_dirty = true;
                }
            }
        }
        if let Some(error) = self.library_data.error.take() {
            self.status = format!("Track database: {error}");
        }
        if self.data_refresh.elapsed() > Duration::from_secs(1) {
            if let Some(path) = self.library.get(self.selected) {
                self.library_data.request(
                    path,
                    self.catalog
                        .entries
                        .get(path)
                        .map(|e| e.first_seen)
                        .unwrap_or(0),
                );
            }
            for deck in &self.snapshot.decks {
                if let Some(path) = deck.track.as_ref().and_then(|t| t.path.as_ref()) {
                    self.library_data.request(
                        path,
                        self.catalog
                            .entries
                            .get(path)
                            .map(|e| e.first_seen)
                            .unwrap_or(0),
                    );
                }
            }
            self.data_refresh = Instant::now();
        }
        for deck in &self.snapshot.decks {
            if let Some(track) = &deck.track {
                for record in self.library_data.records.values_mut() {
                    if record.analysis.id == track.id {
                        record.hot = deck.hot;
                    }
                }
            }
        }
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
            if self.snapshot.database_failed {
                self.status="Track database write failed — Cue changes may not survive restart. Check free disk space and database permissions.".into();
            }
        }
        for error in self.errors.try_iter() {
            self.audio_diagnostics
                .push_str(&format!("\n{}: {error}", chrono::Local::now().to_rfc3339()));
            audio::save_diagnostics(&self.audio_diagnostics);
            self.status = error;
        }
        if self.runtime.as_ref().is_some_and(|r| !r.is_running()) {
            self.stop_auto();
            self.runtime = None;
            while self.rx.try_recv().is_ok() {}
            let (stop, done) = bounded(1);
            let rx = self.rx.clone();
            let view = self.shared.clone();
            let thread = std::thread::spawn(move || engine::offline(rx, view, done));
            self.offline = Some((stop, thread));
            for deck in &mut self.snapshot.decks {
                deck.playing = false;
            }
            self.send(Command::Restore(Box::new(self.snapshot.clone())));
            self.send(Command::HistoryThreshold(self.settings.history_threshold));
            self.status="Audio zatrzymane przez urządzenie. Ustawienia → Audio: wybierz wyjście i ponów uruchomienie. Nagranie jest kończone; szczegóły w diagnostyce.".into();
        }
        while let Ok(work) = self.work.try_recv() {
            match work {
                Work::Preview(token, result) => {
                    if token == self.preview_token {
                        self.preview_loading = false;
                        match result {
                            Ok(track) => self.send(Command::Preview(Some(track))),
                            Err(error) => {
                                self.preview_path = None;
                                self.status = format!("Błąd odsłuchu: {error}");
                            }
                        }
                    }
                }
                Work::History(selected, result) => {
                    self.history_loading = false;
                    match result {
                        Ok(view) => {
                            self.history.sessions = view.sessions;
                            let current = if let Scope::History(id) = &self.scope {
                                Some(id.clone())
                            } else {
                                None
                            };
                            if current == selected && selected.is_some() {
                                if self.history.events != view.events {
                                    self.browser_dirty = true;
                                }
                                self.history.events = view.events;
                            }
                            self.history_error = None;
                        }
                        Err(error) => self.history_error = Some(error),
                    }
                }
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
                Work::ImportInspected(result) => {
                    self.import_busy = false;
                    match result {
                        Ok(_)
                            if self
                                .import_cancel
                                .load(std::sync::atomic::Ordering::Relaxed) =>
                        {
                            self.import_messages = vec!["Inspection cancelled".into()]
                        }
                        Ok(report) => self.import_report = Some(report),
                        Err(error) => self.import_messages = vec![error],
                    }
                }
                Work::ImportApplied(result) => {
                    self.import_busy = false;
                    match result {
                        Ok(applied) => {
                            self.import_messages = applied.messages;
                            for entry in applied.entries {
                                let path = entry.path.clone();
                                if self.library_set.insert(path.clone()) {
                                    self.library.push(path);
                                }
                                let rating = entry.rating;
                                let key = entry.path.clone();
                                self.catalog.merge(entry);
                                if let Some(e) = self.catalog.entries.get_mut(&key) {
                                    e.rating = rating;
                                }
                            }
                            for (name, paths) in applied.lists {
                                let existing = self
                                    .catalog
                                    .crates
                                    .iter()
                                    .find(|c| c.name == name)
                                    .map(|c| c.id);
                                if let Some(id) = existing.or_else(|| self.catalog.add_crate(&name))
                                {
                                    if let Some(c) =
                                        self.catalog.crates.iter_mut().find(|c| c.id == id)
                                    {
                                        c.tracks.extend(paths);
                                    }
                                }
                            }
                            for (id, bank) in applied.banks {
                                self.send(Command::ImportedCues(
                                    id,
                                    bank,
                                    self.import_cue_revision,
                                ));
                            }
                            self.library_data.records.clear();
                            self.db_dirty = true;
                            self.browser_dirty = true;
                            self.status="Import complete; review the report for skipped paths and limitations".into();
                        }
                        Err(error) => self.import_messages = vec![error],
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
                            self.dock_tab = 0;
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
            self.midi_message(&bytes, false);
        }
        while let Ok(bytes) = self.midi_second_rx.try_recv() {
            self.midi_message(&bytes, true);
        }
        if let Some(scan) = &self.midi_scan {
            if let Ok(names) = scan.try_recv() {
                let new_ports: Vec<_> = names
                    .iter()
                    .filter(|name| !self.midi_names.contains(name))
                    .cloned()
                    .collect();
                self.midi_names = names;
                self.midi_scan = None;
                if let Some(name) = self.midi_second_name.clone() {
                    if self.midi_names.contains(&name) {
                        if self.midi_second_connection.is_none()
                            && self.midi_reconnect_name.as_ref() != Some(&name)
                        {
                            match midi::connect_named(&name, self.midi_second_tx.clone()) {
                                Ok(connection) => self.midi_second_connection = Some(connection),
                                Err(error) => self.status = format!("Kontroler 2: {error}"),
                            }
                        }
                    } else if self.midi_second_connection.take().is_some() {
                        self.status =
                            format!("Kontroler 2 odłączony: {name} — czekam na podłączenie");
                    }
                }
                for name in new_ports {
                    self.controller_notices.push_back(name);
                }
                if let Some(name) = self.midi_reconnect_name.clone() {
                    if let Some(index) = self.midi_names.iter().position(|n| n == &name) {
                        self.midi_selected = index;
                        if self.midi_connection.is_none() {
                            match midi::connect_named(&name, self.midi_tx.clone()) {
                                Ok(connection) => {
                                    self.midi_connection = Some(connection);
                                    self.status = format!("MIDI połączone ponownie: {name}");
                                }
                                Err(error) => {
                                    self.status = format!("Nie można połączyć MIDI: {error:#}")
                                }
                            }
                        }
                    } else if self.midi_connection.take().is_some() {
                        self.status =
                            format!("MIDI odłączone: {name} — czekam na ponowne podłączenie");
                    }
                }
            }
        }
        if self.midi_poll.elapsed() > Duration::from_secs(2) && self.midi_scan.is_none() {
            self.midi_poll = Instant::now();
            let (tx, rx) = bounded(1);
            std::thread::spawn(move || {
                let _ = tx.send(midi::inputs());
            });
            self.midi_scan = Some(rx);
        }
        self.sync_microphone();
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
        match engine::sync_master(&self.snapshot, i) {
            Some(master) => {
                self.send(Command::Sync { deck: i, master });
                self.synced_at[i] = Some(Instant::now());
                self.status = format!(
                    "SYNC: tempo i faza bitów dopasowane do deku {}.",
                    deck_letter(master)
                );
            }
            None if self.snapshot.decks[i].track.is_none() => {
                self.status = "SYNC: najpierw załaduj utwór na ten dek.".into();
            }
            None => {
                self.status = "SYNC: brak innego deku z utworem.".into();
            }
        }
    }
    fn top(&mut self, ctx: &egui::Context) {
        self.monitor.poll();
        let now = Instant::now();
        self.frame_times.push_back(now);
        while self
            .frame_times
            .front()
            .is_some_and(|t| now.duration_since(*t).as_secs_f32() > 1.)
        {
            self.frame_times.pop_front();
        }
        if let Some(first) = self.frame_times.front() {
            let span = now.duration_since(*first).as_secs_f32();
            if span > 0.1 {
                self.measured_fps = (self.frame_times.len() - 1) as f32 / span;
            }
        }
        let header_width = ctx.screen_rect().width();
        // Narrow windows: smaller labels and a wrapped second row keep every
        // header function visible; icons only below 1100 px.
        self.emoji.compact = if prospekt_dj::theme::classic() {
            0
        } else if header_width < 1100. {
            2
        } else if header_width < 1800. {
            1
        } else {
            0
        };
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
                let header_background = egui::Rect::from_min_size(
                    rect.min + egui::vec2(-5., 24.),
                    Vec2::new(rect.width() + 10., 29.),
                );
                let painter = ui.painter().with_clip_rect(header_background);
                let stripe_width = 6. / ctx.pixels_per_point();
                for index in 0..(header_background.width() / stripe_width).ceil() as usize {
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            header_background.min + egui::vec2(index as f32 * stripe_width, 0.),
                            Vec2::new(stripe_width, header_background.height()),
                        ),
                        0.,
                        if index % 2 == 0 {
                            ui.visuals().panel_fill
                        } else {
                            Color32::from_rgba_unmultiplied(10, 10, 10, 122)
                        },
                    );
                }

                ui.allocate_ui_with_layout(
                    Vec2::new(rect.width(), 48.),
                    egui::Layout::left_to_right(Align::Center),
                    |ui| {
                        let icon_spacing = ui.spacing().item_spacing.x;
                        ui.spacing_mut().item_spacing.x = 0.;
                        for texture in [&self.brand.icon, &self.brand.blue, &self.brand.green] {
                            ui.add(egui::Image::new((texture.id(), Vec2::splat(48.))))
                                .on_hover_text("Prospekt DJ");
                        }
                        ui.spacing_mut().item_spacing.x = icon_spacing;
                        ui.add_space(icon_spacing);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.;
                            ui.add_space(4.);
                            ui.label(
                                RichText::new(chrono::Local::now().format("%H:%M:%S").to_string())
                                    .family(egui::FontFamily::Name("Arial Bold".into()))
                                    .size(18.),
                            );
                            if self.settings.show_statistics && self.monitor.stats.ram_total > 0 {
                                ui.label(
                                    RichText::new(format!(
                                        "CPU {:.0}% · DISK {:.0}% · {:.1} GiB free",
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
                                ui.label(
                                    RichText::new(format!(
                                        "RAM {:.1}/{:.1} GiB · {:.0} FPS",
                                        self.monitor.stats.ram_used as f64 / 1073741824.,
                                        self.monitor.stats.ram_total as f64 / 1073741824.,
                                        self.measured_fps
                                    ))
                                    .size(9.),
                                )
                                .on_hover_text("RAM: system memory used / total.");
                            }
                        });
                        // Wrap onto a second row instead of overlapping the clock.
                        ui.with_layout(
                            egui::Layout::right_to_left(Align::Center)
                                .with_main_wrap(!prospekt_dj::theme::classic()),
                            |ui| {
                                if ui
                                    .add(self.emoji.icon_button(
                                        ctx,
                                        "⚙️",
                                        header_label("USTAWIENIA", "USTAW."),
                                    ))
                                    .clicked()
                                {
                                    self.settings_open = true;
                                    self.refresh_audio_outputs();
                                    self.midi_names = midi::inputs();
                                }
                                if ui
                                    .add(self.emoji.icon_button(ctx, "🔌", "DEV"))
                                    .on_hover_text("Podłączone kontrolery i nośniki")
                                    .clicked()
                                {
                                    self.device_window = true;
                                    self.midi_names = midi::inputs();
                                }
                                if ui
                                    .add(self.emoji.icon_button(ctx, "⌨️", "KEY"))
                                    .on_hover_text("Skróty klawiaturowe")
                                    .clicked()
                                {
                                    self.key_window = true;
                                }
                                if ui
                                    .add(self.emoji.icon_button(
                                        ctx,
                                        "🎹",
                                        header_label("MIDI LEARN", "MIDI"),
                                    ))
                                    .clicked()
                                {
                                    self.settings_open = true;
                                    self.settings_tab = 1;
                                    self.midi_learn_reveal = true;
                                    self.midi_names = midi::inputs();
                                }
                                if ui
                                    .add(self.emoji.icon_button(
                                        ctx,
                                        "🖥️",
                                        header_label("PEŁNY EKRAN", "EKRAN"),
                                    ))
                                    .on_hover_text("Pełny ekran — F11")
                                    .clicked()
                                {
                                    toggle_fullscreen(ctx);
                                }
                                if ui
                                    .add(
                                        self.emoji
                                            .icon_button(
                                                ctx,
                                                "🎵",
                                                tr(header_label("BIBLIOTEKA", "BIBL.")),
                                            )
                                            .selected(self.library_focus),
                                    )
                                    .clicked()
                                {
                                    self.library_focus = !self.library_focus;
                                }
                                self.header_audio_controls(ui);
                                if prospekt_dj::theme::classic() {
                                    let _ = ui.add(self.emoji.icon_button(ctx, "🎚️", "MASTER"));
                                }
                                if ui
                                    .add(
                                        self.emoji
                                            .icon_button(
                                                ctx,
                                                "🔊",
                                                header_label(
                                                    if self
                                                        .runtime
                                                        .as_ref()
                                                        .is_some_and(|r| r.is_running())
                                                    {
                                                        "AUDIO ON"
                                                    } else {
                                                        "AUDIO OFF"
                                                    },
                                                    if self.emoji.hidden { "AUDIO" } else { "" },
                                                ),
                                            )
                                            .selected(
                                                self.runtime
                                                    .as_ref()
                                                    .is_some_and(|r| r.is_running()),
                                            ),
                                    )
                                    .clicked()
                                {
                                    self.settings_open = true;
                                    self.settings_tab = 0;
                                    self.refresh_audio_outputs();
                                }
                            },
                        );
                    },
                );
            });
        self.emoji.compact = 0;
        self.footer_cover_reflection = None;
        if self.settings.cover_strip && self.settings.cover_reflections {
            egui::TopBottomPanel::bottom("footer-cover-reflection")
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    ui.add_space(6.);
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 2. * 72. / 20.),
                        Sense::hover(),
                    );
                    self.footer_cover_reflection = Some(rect);
                    ui.add_space(6.);
                });
        }
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            let strip = egui::Rect::from_min_max(
                egui::pos2(ctx.screen_rect().center().x, ui.max_rect().top() - 3.),
                egui::pos2(ctx.screen_rect().right(), ctx.screen_rect().bottom()),
            );
            let painter = ui.painter().with_clip_rect(strip);
            let stripe_width = 6. / ctx.pixels_per_point();
            for index in 0..(strip.width() / stripe_width).ceil() as usize {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        strip.min + egui::vec2(index as f32 * stripe_width, 0.),
                        Vec2::new(stripe_width, strip.height()),
                    ),
                    0.,
                    if index % 2 == 0 {
                        ui.visuals().panel_fill
                    } else {
                        Color32::from_rgba_unmultiplied(10, 10, 10, 122)
                    },
                );
            }
            ui.horizontal(|ui| {
                let shown = if matches!(self.scope, Scope::History(_)) {
                    self.history.events.len()
                } else {
                    self.visible_rows.len()
                };
                ui.label(
                    RichText::new(format!("{shown} / {} utworów", self.library.len()))
                        .small()
                        .weak(),
                );
                ui.separator();
                let response = if self.snapshot.preview_playing {
                    ui.add(
                        egui::Label::new(
                            RichText::new("▶ ODTWARZANIE POZA DEKAMI — PHONES/PFL")
                                .strong()
                                .color(Color32::LIGHT_GREEN),
                        )
                        .truncate(),
                    )
                } else if self.preview_loading {
                    ui.label("Ładowanie odsłuchu poza dekami…")
                } else {
                    ui.add(egui::Label::new(RichText::new(tr(&self.status)).small()).truncate())
                };
                if let Some(path) = self.library.get(self.selected) {
                    response.on_hover_text(path.display().to_string());
                }
                if self.snapshot.preview_playing {
                    if let Some(path) = self.preview_path.clone() {
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.;
                            ui.spacing_mut().button_padding = Vec2::new(5., 0.);
                            for (deck, color) in
                                ACCENTS.iter().enumerate().take(self.settings.deck_count)
                            {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(letter(deck))
                                                .size(10.)
                                                .strong()
                                                .color(*color),
                                        )
                                        .min_size(Vec2::new(20., 15.))
                                        .corner_radius(prospekt_dj::theme::radius()),
                                    )
                                    .on_hover_text(format!(
                                        "Załaduj odsłuchiwany utwór na dek {}",
                                        letter(deck)
                                    ))
                                    .clicked()
                                {
                                    self.load_manual(deck, path.clone());
                                }
                            }
                            if let Some(name) = path.file_name() {
                                let name = name.to_string_lossy();
                                ui.add(
                                    egui::Label::new(RichText::new(name.as_ref()).small())
                                        .truncate(),
                                )
                                .on_hover_text(name.as_ref());
                            }
                        });
                    }
                }
            });
        });
    }
    /// Deck row (two decks around the mixer), drawn at full size.
    fn deck_rows(&mut self, ui: &mut egui::Ui, deck_spacing: Vec2) {
        self.deck_drop_rects = [egui::Rect::NOTHING; 4];
        let middle = 202.;
        let width = ((ui.available_width() - middle) / 2.).max(300.);
        for row in 0..self.settings.deck_count / 2 {
            let row_background = ui.painter().add(egui::Shape::Noop);
            let mixer_background = ui.painter().add(egui::Shape::Noop);
            let row_response = ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                let target = ui.allocate_ui_with_layout(
                    Vec2::new(width, 0.),
                    egui::Layout::top_down(Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing = deck_spacing;
                        self.deck_ui(ui, row * 2);
                    },
                );
                self.deck_drop_rects[row * 2] = target.response.rect;
                let mixer = ui.allocate_ui_with_layout(
                    Vec2::new(middle, 0.),
                    egui::Layout::top_down(Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing = deck_spacing;
                        self.mini_mixer(ui, row * 2);
                    },
                );
                let target = ui.allocate_ui_with_layout(
                    Vec2::new(width, 0.),
                    egui::Layout::top_down(Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing = deck_spacing;
                        self.deck_ui(ui, row * 2 + 1);
                    },
                );
                self.deck_drop_rects[row * 2 + 1] = target.response.rect;
                mixer.response.rect
            });
            // All three blocks share the tallest block's height, including when
            // an effect's controls make one deck taller than its neighbour.
            let rect = row_response.response.rect;
            ui.painter().set(
                row_background,
                egui::Shape::rect_filled(
                    rect,
                    prospekt_dj::theme::radius_f(4.),
                    prospekt_dj::theme::row_bg(),
                ),
            );
            let mixer_rect = egui::Rect::from_min_size(
                egui::pos2(row_response.inner.left(), rect.top()),
                Vec2::new(row_response.inner.width(), rect.height()),
            );
            ui.painter().set(
                mixer_background,
                egui::Shape::rect_filled(
                    mixer_rect,
                    prospekt_dj::theme::radius_f(4.),
                    prospekt_dj::theme::row_bg(),
                ),
            );
        }
    }
    /// Modern skins draw the deck rows at 90 % in their own layer, centred in the
    /// available width. Drop targets are converted to screen coordinates.
    fn scaled_deck_rows(&mut self, ui: &mut egui::Ui, deck_spacing: Vec2) {
        const DECK_SCALE: f32 = 0.9;
        let origin = ui.next_widget_position();
        let width = ui.available_width();
        let layer = egui::LayerId::new(ui.layer_id().order, ui.id().with("deck-rows-scaled"));
        ui.ctx().set_sublayer(ui.layer_id(), layer);
        let logical = egui::Rect::from_min_size(origin, Vec2::new(width, self.deck_rows_height));
        let mut child = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(logical));
        child.set_clip_rect(logical);
        self.deck_rows(&mut child, deck_spacing);
        let used = (child.min_rect().bottom() - origin.y).max(1.);
        self.deck_rows_height = used;
        let to_global = egui::emath::TSTransform::new(
            egui::vec2(
                origin.x * (1. - DECK_SCALE) + width * (1. - DECK_SCALE) / 2.,
                origin.y * (1. - DECK_SCALE),
            ),
            DECK_SCALE,
        );
        ui.ctx().set_transform_layer(layer, to_global);
        for rect in self.deck_drop_rects.iter_mut() {
            if rect.is_positive() {
                *rect = to_global * *rect;
            }
        }
        ui.allocate_exact_size(Vec2::new(width, used * DECK_SCALE), Sense::hover());
    }
    fn decks_ui(&mut self, ui: &mut egui::Ui) -> f32 {
        self.header_cover_reflection = None;
        if self.settings.cover_strip && self.settings.cover_reflections {
            ui.add_space(6.);
            let width = ui
                .available_width()
                .min(ui.clip_rect().right() - ui.cursor().left());
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(width, 2. * 72. / 20.), Sense::hover());
            self.header_cover_reflection = Some(egui::Rect::from_min_max(
                egui::pos2(ui.ctx().screen_rect().left(), rect.top()),
                egui::pos2(ui.ctx().screen_rect().right(), rect.bottom()),
            ));
            ui.add_space(6.);
        }

        let deck_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.y = 0.;
        if prospekt_dj::theme::classic() {
            self.deck_rows(ui, deck_spacing);
        } else {
            self.scaled_deck_rows(ui, deck_spacing);
        }
        ui.spacing_mut().item_spacing = deck_spacing;
        if !self.settings.show_main_waveforms {
            return self
                .deck_drop_rects
                .iter()
                .filter(|rect| rect.is_positive())
                .map(|rect| rect.bottom())
                .fold(ui.min_rect().top(), f32::max);
        }
        ui.add_space(8.);
        let mut waveform_bottom = ui.cursor().top();
        for (i, accent) in ACCENTS.iter().take(self.settings.deck_count).enumerate() {
            let view = self.snapshot.decks[i].clone();
            let height = self.settings.wave_style.height
                * match (
                    self.settings.deck_count == 4,
                    ui.ctx().screen_rect().height() < 850.,
                ) {
                    (true, true) => 24.,
                    (true, false) => 32.,
                    (false, true) => 40.,
                    _ => 60.,
                };
            let on_air = if i % 2 == 0 {
                self.snapshot.cross < 1.
            } else {
                self.snapshot.cross > 0.
            };
            let wave_width = (ui
                .available_width()
                .min(ui.clip_rect().right() - ui.cursor().left())
                - 40.)
                .max(50.);
            let row_spacing = ui.spacing().item_spacing.y;
            if i + 1 < self.settings.deck_count && self.settings.wave_style.reflections {
                ui.spacing_mut().item_spacing.y = 2.5;
            }
            let waveform_row = ui.push_id(("waveform-edge-controls", i), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.;
                    if vertical_wave_control(
                        ui,
                        "MASTER",
                        self.settings.waveform_master[i],
                        height,
                        true,
                    )
                    .on_hover_text("Znacznik MASTER waveformu — włącz / wyłącz")
                    .clicked()
                    {
                        self.settings.waveform_master[i] = !self.settings.waveform_master[i];
                    }
                    ui.allocate_ui_with_layout(
                        Vec2::new(wave_width, height),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(wave_width);
                            ui.set_max_width(wave_width);
                            if let Some(action) = waveform(
                                ui,
                                &view,
                                *accent,
                                height,
                                Some(self.zoom),
                                &self.settings.wave_style,
                            ) {
                                self.wave_action(i, action);
                            }
                        },
                    );
                    vertical_wave_control(ui, "(ON AIR)", on_air, height, false)
                        .on_hover_text("Udział kanału w miksie według crossfadera: A/C ↔ B/D");
                });
            });
            waveform_bottom = waveform_row.response.rect.bottom();
            if i + 1 < self.settings.deck_count && self.settings.wave_style.reflections {
                let width = ui
                    .available_width()
                    .min(ui.clip_rect().right() - ui.cursor().left());
                let reflection_height = height / 8.;
                let (strip, _) = ui
                    .allocate_exact_size(Vec2::new(width, 2. * reflection_height), Sense::hover());
                let top = egui::Rect::from_min_size(
                    strip.min + egui::vec2(20., 0.),
                    Vec2::new(wave_width, reflection_height),
                );
                let bottom = top.translate(egui::vec2(0., reflection_height));
                waveform_reflection(
                    ui,
                    (top, false),
                    &view,
                    *accent,
                    self.zoom,
                    &self.settings.wave_style,
                );
                waveform_reflection(
                    ui,
                    (bottom, true),
                    &self.snapshot.decks[i + 1],
                    ACCENTS[i + 1],
                    self.zoom,
                    &self.settings.wave_style,
                );
            }
            ui.spacing_mut().item_spacing.y = row_spacing;
        }
        waveform_bottom
    }
    fn deck_ui(&mut self, ui: &mut egui::Ui, i: usize) {
        let d = self.snapshot.decks[i].clone();
        egui::Frame::new()
            .fill(prospekt_dj::theme::row_bg())
            .inner_margin(5)
            .corner_radius(prospekt_dj::theme::radius())
            .stroke(Stroke::NONE)
            .show(ui, |ui| {
                ui.set_min_height(modern_size(204., 180.));
                let show_platter = ui.ctx().screen_rect().width() >= 1200.;
                // Modern skins use one gap between info and platter on both decks.
                let jog_gap = if prospekt_dj::theme::classic() {
                    if i.is_multiple_of(2) {
                        26.
                    } else {
                        ui.spacing().item_spacing.x
                    }
                } else {
                    prospekt_dj::theme::GAP
                };
                let info = (ui.available_width() - if show_platter { 186. + jog_gap } else { 0. })
                    .max(180.);
                ui.horizontal_top(|ui| {
                    let gap_background = ui.painter().add(egui::Shape::Noop);
                    let mut jog_rect = None;
                    if show_platter && i % 2 == 1 {
                        jog_rect = Some(self.jog_panel(ui, i, &d));
                        ui.add_space((jog_gap - ui.spacing().item_spacing.x).max(0.));
                    }
                    let info_panel = ui.allocate_ui_with_layout(
                        Vec2::new(info, 0.),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_width(info);
                            self.deck_info(ui, i, &d);
                            if !show_platter {
                                self.transport_ui(ui, i, &d);
                            }
                        },
                    );
                    if show_platter && i.is_multiple_of(2) {
                        ui.add_space((jog_gap - ui.spacing().item_spacing.x).max(0.));
                        jog_rect = Some(self.jog_panel(ui, i, &d));
                    }
                    if let Some(jog) = jog_rect {
                        let (left, right) = if i.is_multiple_of(2) {
                            (info_panel.response.rect.right(), jog.left())
                        } else {
                            (jog.right(), info_panel.response.rect.left())
                        };
                        if right > left {
                            ui.painter().set(
                                gap_background,
                                egui::Shape::rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(left, jog.top()),
                                        egui::pos2(right, jog.bottom()),
                                    ),
                                    0.,
                                    prospekt_dj::theme::deck_bg(),
                                ),
                            );
                        }
                    }
                });
            });
    }
    fn wave_action(&mut self, i: usize, action: WaveAction) {
        match action {
            WaveAction::Zoom(seconds) => self.zoom = seconds,
            WaveAction::Seek(pos) => self.deck(i, DeckControl::Seek(pos)),
            WaveAction::Cue(n) => self.deck(i, DeckControl::HotCue(n, false)),
        }
    }
    fn jog_panel(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) -> egui::Rect {
        let background = ui.painter().add(egui::Shape::Noop);
        let panel = ui.allocate_ui_with_layout(
            Vec2::new(186., 0.),
            egui::Layout::top_down(Align::Min),
            |ui| {
                ui.horizontal_top(|ui| {
                    if i % 2 == 1 {
                        self.pitch_bend(ui, i);
                    }
                    ui.vertical(|ui| {
                        ui.set_width(148.);
                        self.white_platter(ui, i, d);
                        self.transport_ui(ui, i, d);
                    });
                    if i.is_multiple_of(2) {
                        self.pitch_bend(ui, i);
                    }
                });
            },
        );
        ui.painter().set(
            background,
            egui::Shape::rect_filled(
                panel.response.rect,
                prospekt_dj::theme::radius_f(6.),
                prospekt_dj::theme::deck_bg(),
            ),
        );
        panel.response.rect
    }
    fn transport_ui(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        ui.push_id(("jog-transport", i), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                let accent = ACCENTS[i];
                let blink = self.settings.blink_paused_play
                    && !self.settings.reduced_motion
                    && d.track.is_some()
                    && !d.playing;
                let play_lit = d.playing
                    || (blink
                        && ui.input(|input| ((input.time * 2.).floor() as u64).is_multiple_of(2)));
                if blink {
                    ui.ctx().request_repaint_after(Duration::from_millis(33));
                }
                if transport_button(
                    ui,
                    if d.playing { "Ⅱ" } else { "▶" },
                    play_lit,
                    accent,
                    self.settings.reduced_motion,
                    48.,
                )
                .clicked()
                {
                    self.play(i);
                }
                if transport_button(ui, "CUE", false, accent, self.settings.reduced_motion, 48.)
                    .clicked()
                {
                    self.deck(i, DeckControl::Cue);
                }
                let utilities_width = ui
                    .ctx()
                    .data(|data| data.get_temp::<f32>(egui::Id::new(("jog-utilities-width", i))))
                    .unwrap_or(148.);
                if transport_button(
                    ui,
                    "SYNC",
                    self.synced_at[i].is_some_and(|t| t.elapsed() < Duration::from_millis(350)),
                    accent,
                    self.settings.reduced_motion,
                    if prospekt_dj::theme::classic() {
                        (utilities_width - 100.).max(48.)
                    } else {
                        (utilities_width - 100.).min(ui.available_width()).max(48.)
                    },
                )
                .on_hover_text("SYNC: tempo + faza bitów")
                .clicked()
                {
                    self.sync(i);
                }
            });
            if !prospekt_dj::theme::classic() {
                ui.horizontal(|ui| {
                    // Under the CUE button: play is 48 px wide plus a 2 px gap.
                    ui.add_space(50.);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.;
                            if ui
                                .add_sized(
                                    [30., 16.],
                                    egui::Button::new(RichText::new("SLIP").size(8.)).selected(d.slip),
                                )
                                .on_hover_text("Slip: pętle i skoki nie przerywają odtwarzania")
                                .clicked()
                            {
                                self.deck(i, DeckControl::Slip);
                            }
                            if ui
                                .add_sized(
                                    [30., 16.],
                                    egui::Button::new(RichText::new("BRK").size(8.)).selected(d.braking),
                                )
                                .on_hover_text("Brake: płyta zwalnia do zera jak gramofon")
                                .clicked()
                            {
                                self.deck(i, DeckControl::Brake);
                            }
                            if ui
                                .add_sized(
                                    [30., 16.],
                                    egui::Button::new(RichText::new("QNT").size(8.)).selected(d.quantize),
                                )
                                .on_hover_text("Quantize: punkty Hot Cue wpadają na siatkę bitów")
                                .clicked()
                            {
                                self.deck(i, DeckControl::Quantize);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.;
                            for (n, slot) in d.loop_slots.iter().enumerate() {
                                let filled = slot.is_some();
                                let response = ui
                                    .add_sized(
                                        [22., 16.],
                                        egui::Button::new(RichText::new(format!("L{}", n + 1)).size(8.))
                                            .selected(filled),
                                    )
                                    .on_hover_text(
                                        "Klik: odtwórz zapisaną pętlę · Shift+klik: zapisz aktualną pętlę (lub 4 takty)",
                                    );
                                if response.clicked() {
                                    let shift = ui.input(|input| input.modifiers.shift);
                                    if shift {
                                        self.deck(i, DeckControl::CaptureLoop(n));
                                    } else {
                                        self.deck(i, DeckControl::RecallLoop(n));
                                    }
                                }
                            }
                        });
                    });
                });
            }
        });
    }
    fn jog_utilities(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        ui.push_id(("jog-utilities", i), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                let button = |ui: &mut egui::Ui, text: &str, width: f32, active: bool| {
                    ui.add_sized(
                        [width, 24.],
                        egui::Button::new(RichText::new(text).size(9.).color(if active {
                            ACCENTS[i]
                        } else {
                            Color32::from_gray(220)
                        }))
                        .fill(Color32::from_gray(26))
                        .stroke(Stroke::new(
                            2_f32,
                            if active {
                                ACCENTS[i]
                            } else {
                                Color32::from_gray(70)
                            },
                        ))
                        .corner_radius(prospekt_dj::theme::radius()),
                    )
                };
                let set_cue = button(ui, "SET CUE", 45., false);
                if set_cue.clicked() {
                    self.deck(i, DeckControl::SetCue);
                }
                if button(ui, "PFL", 25., d.pfl).clicked() {
                    self.send(Command::Deck(i, DeckControl::Pfl));
                }
                if button(ui, "VINYL", 36., d.scratch).clicked() {
                    self.deck(i, DeckControl::ScratchMode);
                }
                if !prospekt_dj::theme::classic()
                    && button(ui, "KEYLOCK", 46., d.key_lock)
                        .on_hover_text("Key lock: tempo zmienia się bez zmiany tonacji")
                        .clicked()
                {
                    self.deck(i, DeckControl::KeyLock);
                }
                let minus = button(ui, "−", 17., false);
                let plus = button(ui, "+", 17., false);
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        egui::Id::new(("jog-utilities-width", i)),
                        plus.rect.right() - set_cue.rect.left(),
                    )
                });
                self.bend_buttons[i] = if minus.is_pointer_button_down_on() {
                    -0.04
                } else if plus.is_pointer_button_down_on() {
                    0.04
                } else {
                    0.
                };
            });
        });
    }
    /// Persistent tempo fader for modern skins; its range comes from settings.
    fn tempo_fader(&mut self, ui: &mut egui::Ui, i: usize) {
        let range = prospekt_dj::engine::tempo_range(self.settings.tempo_range);
        let rate = self.snapshot.decks[i].rate;
        let mut value = ((rate - 1.) / range * 100.).clamp(-100., 100.);
        ui.spacing_mut().item_spacing.y = 2.;
        ui.label(RichText::new("TEMPO").size(7.).weak());
        ui.label(
            RichText::new(format!("±{:.0}%", range * 100.))
                .size(8.)
                .color(ACCENTS[i]),
        );
        ui.spacing_mut().slider_width = 112.;
        let response = egui::Frame::new()
            .fill(prospekt_dj::theme::palette().panel)
            .stroke(Stroke::new(1_f32, prospekt_dj::theme::palette().edge))
            .corner_radius(prospekt_dj::theme::radius_f(3.))
            .inner_margin(3)
            .show(ui, |ui| {
                ui.add(
                    egui::Slider::new(&mut value, -100.0..=100.)
                        .vertical()
                        .show_value(false),
                )
            })
            .inner;
        if response.changed() {
            self.deck(i, DeckControl::Rate(1. + value / 100. * range));
        }
        if response.double_clicked() {
            self.deck(i, DeckControl::Rate(1.));
        }
        response.on_hover_text("Suwak tempa · dwuklik = 0 %. Chwilowy bend: przyciski − / +");
        ui.label(
            RichText::new(format!("{:+.1}%", (rate - 1.) * 100.))
                .monospace()
                .size(8.),
        );
    }
    fn pitch_bend(&mut self, ui: &mut egui::Ui, i: usize) {
        ui.allocate_ui_with_layout(
            Vec2::new(32., modern_size(174., 150.)),
            egui::Layout::top_down(Align::Center),
            |ui| {
                ui.set_width(32.);
                if !prospekt_dj::theme::classic() {
                    self.tempo_fader(ui, i);
                    return;
                }
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
                if prospekt_dj::theme::classic() {
                    self.bend_slider[i]
                } else {
                    0.
                },
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
    fn reverse_button(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) -> egui::Response {
        let response = jog_action_button(
            ui,
            "R",
            d.track.is_some() && d.playing && !self.loading[i],
            d.reverse,
        );
        self.reverse_requested[i] = response.is_pointer_button_down_on();
        response.on_hover_text("REVERS / Slip Reverse: trzymaj, aby grać do tyłu. Puść, aby wrócić do bieżącego miejsca utworu.")
    }
    fn white_platter(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        let k = modern_size(1., 0.86);
        let (response, p) = ui.allocate_painter(Vec2::new(148., 174. * k), Sense::click_and_drag());
        response.clone().context_menu(|ui| {
            if ui.button("Załaduj plik…").clicked() {
                self.pick_track(i);
                ui.close_menu();
            }
            if ui.button("TAP BPM").clicked() {
                self.tap(i);
            }
            if ui.button("Analizuj BPM").clicked() {
                self.analyse(i);
                ui.close_menu();
            }
            if ui.button("BPM ×2").clicked() {
                self.deck(i, DeckControl::Bpm(d.bpm * 2.));
                ui.close_menu();
            }
            if ui.button("BPM ÷2").clicked() {
                self.deck(i, DeckControl::Bpm(d.bpm / 2.));
                ui.close_menu();
            }
        });
        let center = egui::pos2(response.rect.center().x, response.rect.top() + 75. * k);
        let text = if self.settings.white_jog {
            Color32::BLACK
        } else {
            Color32::from_gray(235)
        };
        p.circle_filled(center, 72. * k, Color32::from_gray(35));
        p.circle_filled(
            center,
            65. * k,
            if self.settings.white_jog {
                Color32::WHITE
            } else {
                Color32::from_gray(12)
            },
        );
        p.circle_stroke(center, 66. * k, Stroke::new(1_f32, Color32::from_gray(70)));
        if d.track.is_some() {
            let angle = (if self.settings.reduced_motion {
                0.
            } else {
                d.position as f32
            }) * std::f32::consts::PI
                - std::f32::consts::FRAC_PI_2;
            for n in 0..24 {
                let theta = angle + n as f32 * std::f32::consts::TAU / 24.;
                p.line_segment(
                    [
                        center + Vec2::angled(theta) * 68. * k,
                        center + Vec2::angled(theta) * 71. * k,
                    ],
                    Stroke::new(1_f32, Color32::from_gray(if n % 3 == 0 { 140 } else { 65 })),
                );
            }
            let arc: Vec<_> = (0..16)
                .map(|n| center + Vec2::angled(angle + n as f32 * 0.05) * 69.5 * k)
                .collect();
            p.add(egui::Shape::line(arc, Stroke::new(2_f32, ACCENTS[i])));
            p.line_segment(
                [
                    center + Vec2::angled(angle) * 64. * k,
                    center + Vec2::angled(angle) * 72. * k,
                ],
                Stroke::new(3_f32, ACCENTS[i]),
            );
        }
        if d.track.is_some() && !self.settings.reduced_motion && !prospekt_dj::theme::classic() {
            // Follow the source playhead: pause holds the hand, reverse turns it back.
            let angle = (d.position.rem_euclid(2.) as f32) * std::f32::consts::PI
                - std::f32::consts::FRAC_PI_2;
            let direction = Vec2::angled(angle);
            let needle = Color32::BLACK;
            if !self.settings.white_jog {
                p.line_segment(
                    [center - direction * 9., center + direction * 61. * k],
                    Stroke::new(11_f32, Color32::from_gray(95)),
                );
            }
            p.line_segment(
                [center - direction * 9., center + direction * 61. * k],
                Stroke::new(9_f32, needle.gamma_multiply(0.85)),
            );
            p.circle_filled(center, 7.2, Color32::from_rgb(153, 221, 255));
            if d.playing {
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            }
        }
        if let Some(track) = d.track.as_ref().filter(|_| !prospekt_dj::theme::classic()) {
            // Track progress ring: elapsed part in the deck accent, rest dimmed.
            let progress = (d.position / track.duration().max(0.001)).clamp(0., 1.) as f32;
            let top = -std::f32::consts::FRAC_PI_2;
            let ring = |from: f32, to: f32| -> Vec<egui::Pos2> {
                let steps = ((to - from) * 40.).ceil().max(1.) as usize;
                (0..=steps)
                    .map(|n| {
                        let t = from + (to - from) * n as f32 / steps as f32;
                        center + Vec2::angled(top + t * std::f32::consts::TAU) * 74.5 * k
                    })
                    .collect()
            };
            p.add(egui::Shape::line(
                ring(0., 1.),
                Stroke::new(3_f32, Color32::from_gray(45)),
            ));
            if progress > 0. {
                let remaining = track.duration() - d.position;
                let color = if remaining < 30. && remaining > 0. {
                    Color32::from_rgb(235, 60, 60)
                } else {
                    ACCENTS[i]
                };
                p.add(egui::Shape::line(
                    ring(0., progress),
                    Stroke::new(3_f32, color),
                ));
            }
        }
        for (dy, label, size, color) in [
            (
                -39.,
                if d.track.is_some() {
                    format!("{:.2}", d.bpm * d.rate * (1. + self.bend[i]))
                } else {
                    "—".into()
                },
                24.,
                text,
            ),
            (
                -13.,
                format!(
                    "{:+.1}%   {}",
                    (d.rate - 1.) * 100.,
                    if prospekt_dj::theme::classic() {
                        "±50%".to_string()
                    } else {
                        format!(
                            "±{:.0}%",
                            prospekt_dj::engine::tempo_range(self.settings.tempo_range) * 100.
                        )
                    }
                ),
                10.,
                text,
            ),
            (
                31.,
                if d.track.is_some() {
                    short_time(d.position)
                } else {
                    "—:—".into()
                },
                16.,
                text,
            ),
            (
                49.,
                d.track
                    .as_ref()
                    .map(|t| format!("−{}", short_time((t.duration() - d.position).max(0.))))
                    .unwrap_or_else(|| "—:—".into()),
                13.,
                text,
            ),
        ] {
            p.text(
                center + egui::vec2(0., dy * k),
                egui::Align2::CENTER_CENTER,
                label,
                if size >= 13. {
                    FontId::new(size, egui::FontFamily::Name("Arial Bold".into()))
                } else {
                    FontId::proportional(size)
                },
                color,
            );
        }
        // Original skin: hand drawn over the readout, exactly as before.
        if d.track.is_some() && !self.settings.reduced_motion && prospekt_dj::theme::classic() {
            // Follow the source playhead: pause holds the hand, reverse turns it back.
            let angle = (d.position.rem_euclid(2.) as f32) * std::f32::consts::PI
                - std::f32::consts::FRAC_PI_2;
            let direction = Vec2::angled(angle);
            let needle = Color32::BLACK;
            if !self.settings.white_jog {
                p.line_segment(
                    [center - direction * 9., center + direction * 61. * k],
                    Stroke::new(11_f32, Color32::from_gray(95)),
                );
            }
            p.line_segment(
                [center - direction * 9., center + direction * 61. * k],
                Stroke::new(9_f32, needle),
            );
            p.circle_filled(center, 7.2, Color32::from_rgb(153, 221, 255));
            if d.playing {
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            }
        }
        p.text(
            egui::pos2(center.x - 18., response.rect.top() + 161. * k),
            egui::Align2::CENTER_CENTER,
            if d.scratch { "VINYL" } else { "JOG / BEND" },
            FontId::monospace(9.),
            ACCENTS[i],
        );
        let reverse_rect = egui::Rect::from_min_size(
            response.rect.right_top() + egui::vec2(-48., 150. * k),
            Vec2::splat(22.),
        );
        let reverse = ui
            .scope_builder(egui::UiBuilder::new().max_rect(reverse_rect), |ui| {
                self.reverse_button(ui, i, d)
            })
            .inner;
        let eject_rect = egui::Rect::from_min_size(
            response.rect.right_top() + egui::vec2(-22., 150. * k),
            Vec2::splat(22.),
        );
        let eject = ui
            .scope_builder(egui::UiBuilder::new().max_rect(eject_rect), |ui| {
                jog_action_button(ui, "↑", d.track.is_some() && !self.loading[i], false)
            })
            .inner
            .on_hover_text("Wyrzuć utwór z deku");
        if eject.clicked() {
            if let Some(track) = &d.track {
                self.eject_pending = Some((i, track.id.clone(), self.deck_tokens[i]));
            }
        }
        if response.drag_started() && !eject.contains_pointer() && !reverse.contains_pointer() {
            self.stop_auto();
            self.focused = i;
        }
        if response.dragged() && !eject.contains_pointer() && !reverse.contains_pointer() {
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
        let header_top = ui.next_widget_position();
        let header_background = ui.painter().add(egui::Shape::Noop);
        let row_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.;
        // Modern skins mirror deck B (letter on the outer edge); Classic keeps the original.
        let mirrored = i % 2 == 1 && !prospekt_dj::theme::classic();
        ui.horizontal_top(|ui| {
            let letter_size = egui::TextStyle::Small.resolve(ui.style()).size * 4.;
            let deck_letter = RichText::new(letter(i))
                .size(letter_size)
                .family(egui::FontFamily::Name("Arial Bold".into()))
                .color(accent);
            if !mirrored {
                ui.label(deck_letter.clone());
            }
            if ui.ctx().screen_rect().width() < 1200.
                && jog_action_button(ui, "↑", d.track.is_some() && !self.loading[i], false)
                    .on_hover_text("Wyrzuć utwór z deku")
                    .clicked()
            {
                if let Some(track) = &d.track {
                    self.eject_pending = Some((i, track.id.clone(), self.deck_tokens[i]));
                }
            }
            if ui.ctx().screen_rect().width() < 1200. {
                self.reverse_button(ui, i, d);
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
            let letter_width = if mirrored {
                ui.painter()
                    .layout_no_wrap(
                        letter(i).to_string(),
                        FontId::new(letter_size, egui::FontFamily::Name("Arial Bold".into())),
                        accent,
                    )
                    .size()
                    .x
                    + ui.spacing().item_spacing.x
            } else {
                0.
            };
            let covers = &self.covers;
            let title = |ui: &mut egui::Ui| {
                if let Some(texture) = &covers[i] {
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
                            .size(15.)
                            .family(egui::FontFamily::Name("Arial Bold".into())),
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
            };
            if mirrored {
                ui.allocate_ui_with_layout(
                    Vec2::new((ui.available_width() - letter_width).max(40.), 0.),
                    egui::Layout::left_to_right(Align::Min),
                    title,
                );
            } else {
                // Original layout path.
                ui.horizontal(title);
            }
            if mirrored {
                ui.with_layout(egui::Layout::right_to_left(Align::Min), |ui| {
                    ui.label(deck_letter);
                });
            }
        });
        ui.spacing_mut().item_spacing.y = 4.;
        let start = ui.next_widget_position();
        let right = ui.max_rect().right().min(ui.ctx().screen_rect().right())
            - if i % 2 == 1 && prospekt_dj::theme::classic() {
                26.
            } else {
                0.
            };
        let waveform_rect = egui::Rect::from_min_size(
            start,
            Vec2::new((right - start.x).max(1.), modern_size(33., 28.)),
        );
        ui.painter().set(
            header_background,
            egui::Shape::rect_filled(
                egui::Rect::from_min_max(header_top, waveform_rect.right_bottom()),
                0.,
                prospekt_dj::theme::deck_bg(),
            ),
        );
        let mut waveform_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(waveform_rect)
                .layout(egui::Layout::top_down(Align::Min)),
        );
        waveform_ui.set_clip_rect(ui.clip_rect().intersect(waveform_rect));
        let action = waveform(
            &mut waveform_ui,
            d,
            accent,
            33.,
            None,
            &self.settings.wave_style,
        );
        ui.allocate_rect(waveform_rect, Sense::hover());
        if let Some(action) = action {
            self.wave_action(i, action);
        }
        ui.spacing_mut().item_spacing.y = row_spacing;
        let controls = 98.;
        // Options column sits beside the pads whenever full-size pads still fit.
        let show_options_inline = if prospekt_dj::theme::classic() {
            ui.available_width() >= 460.
        } else {
            ui.available_width() >= 265. + controls + 8.
        };
        let pads = (ui.available_width() - if show_options_inline { controls } else { 0. } - 6.)
            .clamp(
                172.,
                if prospekt_dj::theme::classic() {
                    268.
                } else {
                    265.
                },
            );
        // Deck B mirrors deck A: pads sit at the outer edge, options face the platter.
        ui.horizontal_top(|ui| {
            let options = |studio: &mut Self, ui: &mut egui::Ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(controls, 60.),
                    egui::Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(controls);
                        studio.pad_options(ui, i, d);
                    },
                );
            };
            if mirrored {
                let used = pads
                    + if show_options_inline {
                        controls + ui.spacing().item_spacing.x
                    } else {
                        0.
                    };
                ui.add_space((ui.available_width() - used).max(0.));
                if show_options_inline {
                    options(self, ui);
                }
            }
            ui.allocate_ui_with_layout(
                Vec2::new(pads, 60.),
                egui::Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(pads);
                    self.cue_pads(ui, i, d);
                    let pad_gap = if prospekt_dj::theme::classic() {
                        3.
                    } else {
                        6.
                    };
                    let cue_width = (((pads - 3. * pad_gap) / 4.).clamp(40., 64.) * 4.
                        + 3. * pad_gap)
                        .min(pads);
                    let utilities_width = ui
                        .ctx()
                        .data(|data| {
                            data.get_temp::<f32>(egui::Id::new(("jog-utilities-width", i)))
                        })
                        .unwrap_or(168.);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.;
                        // Utilities hug the platter side under the pads on both decks.
                        if !mirrored {
                            ui.add_space((cue_width - utilities_width).max(0.));
                        }
                        self.jog_utilities(ui, i, d);
                    });
                },
            );
            if show_options_inline && !mirrored {
                options(self, ui);
            }
        });
        if !show_options_inline {
            ui.horizontal(|ui| {
                // Mirrored deck: right-align using last frame's measured row width.
                let id = egui::Id::new(("pad-options-width", i));
                if mirrored {
                    let width = ui.ctx().data(|data| data.get_temp::<f32>(id)).unwrap_or(0.);
                    ui.add_space((ui.available_width() - width).max(0.));
                }
                let start = ui.cursor().left();
                self.pad_options(ui, i, d);
                let width = ui.min_rect().right() - start;
                ui.ctx().data_mut(|data| data.insert_temp(id, width));
            });
        }
        let (lower_block, _) = ui.allocate_exact_size(
            Vec2::new(waveform_rect.width(), modern_size(28., 24.)),
            Sense::hover(),
        );
        ui.painter().rect_filled(
            lower_block,
            prospekt_dj::theme::radius_f(4.),
            prospekt_dj::theme::deck_bg(),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
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
    fn pad_options(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
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
        if d.mode == PadMode::Effects {
            return;
        }
        ui.horizontal(|ui| {
            for beats in [1., 2., 4., 8.] {
                if ui
                    .selectable_label(
                        d.loop_range
                            .is_some_and(|(a, b)| ((b - a) * d.bpm / 60. - beats).abs() < 0.05),
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
        if !prospekt_dj::theme::classic() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                for (label, beats) in [("◀4", -4_f64), ("◀1", -1_f64), ("1▶", 1_f64), ("4▶", 4_f64)]
                {
                    if ui
                        .add_sized([22., 16.], egui::Button::new(RichText::new(label).size(8.)))
                        .on_hover_text(format!("Skok o {} takt(y)", beats.abs()))
                        .clicked()
                    {
                        self.deck(i, DeckControl::BeatJump(beats));
                    }
                }
            });
        }
    }
    fn cue_pads(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        if d.mode == PadMode::Effects {
            self.deck_effects_ui(ui, i, d);
            return;
        }
        if d.mode == PadMode::Sampler {
            self.sampler_ui(ui);
            return;
        }
        if d.mode == PadMode::AutoLoop {
            for bank in [[1., 2., 4., 8.], [16., 32., 0.5, 0.25]] {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.;
                    for beats in bank {
                        let active = d
                            .loop_range
                            .is_some_and(|(a, b)| ((b - a) * d.bpm / 60. - beats).abs() < 0.01);
                        if ui
                            .add_sized(
                                [38., 25.],
                                egui::Button::new(format!("{beats}")).selected(active),
                            )
                            .clicked()
                        {
                            self.deck(i, DeckControl::Loop(beats));
                        }
                    }
                });
            }
            return;
        }
        if d.mode == PadMode::ManualLoop {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                for (p, label) in ["IN", "OUT", "EXIT", "RE"].iter().enumerate() {
                    if ui
                        .add_sized(
                            [38., 25.],
                            egui::Button::new(*label).selected(
                                (d.loop_range.is_some() && p == 1)
                                    || (d.manual_in.is_some() && p == 0),
                            ),
                        )
                        .clicked()
                    {
                        self.deck(i, DeckControl::Pad(p, false));
                    }
                }
            });
            ui.horizontal(|ui| {
                for (label, scale) in [("÷2", 0.5), ("×2", 2.)] {
                    if ui
                        .add_enabled(d.loop_range.is_some(), egui::Button::new(label))
                        .clicked()
                    {
                        self.deck(i, DeckControl::LoopScale(scale));
                    }
                }
            });
            return;
        }
        let gap = if prospekt_dj::theme::classic() {
            3.
        } else {
            6.
        };
        let width = ((ui.available_width() - 3. * gap) / 4.).clamp(40., 64.);
        for row in 0..2 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for col in 0..4 {
                    let n = row * 4 + col;
                    let color = cue_color(n);
                    let active = d.hot[n].is_some();
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(width, modern_size(28., 24.)),
                        Sense::click(),
                    );
                    if prospekt_dj::theme::classic() {
                        let fill = if response.is_pointer_button_down_on() {
                            color.gamma_multiply(0.4)
                        } else if response.hovered() {
                            Color32::from_gray(35)
                        } else {
                            Color32::from_gray(if active { 17 } else { 8 })
                        };
                        ui.painter().rect_filled(rect, 0., fill);
                        ui.painter().rect_stroke(
                            rect.shrink(0.5),
                            0.,
                            Stroke::new(
                                1_f32,
                                color.gamma_multiply(if active { 1. } else { 0.35 }),
                            ),
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
                    } else {
                        // Set cues are solid color pads; empty pads stay dark with a color hint.
                        let pressed = response.is_pointer_button_down_on();
                        let fill = if active {
                            if pressed {
                                color.gamma_multiply(0.55)
                            } else if response.hovered() {
                                color.linear_multiply(1.15)
                            } else {
                                color
                            }
                        } else if pressed {
                            color.gamma_multiply(0.4)
                        } else if response.hovered() {
                            prospekt_dj::theme::palette().raised_hover
                        } else {
                            prospekt_dj::theme::palette().raised
                        };
                        ui.painter()
                            .rect_filled(rect, prospekt_dj::theme::radius_f(3.), fill);
                        if !active {
                            ui.painter().rect_filled(
                                egui::Rect::from_min_max(
                                    egui::pos2(rect.left(), rect.bottom() - 3.),
                                    rect.right_bottom(),
                                ),
                                egui::CornerRadius {
                                    sw: 3,
                                    se: 3,
                                    ..Default::default()
                                },
                                color.gamma_multiply(0.6),
                            );
                        }
                        let label = if active {
                            prospekt_dj::theme::on_color(fill)
                        } else {
                            Color32::from_gray(150)
                        };
                        ui.painter().text(
                            rect.center() - egui::vec2(0., 6.),
                            egui::Align2::CENTER_CENTER,
                            format!("Cue {}", n + 1),
                            FontId::proportional(10.),
                            label,
                        );
                        ui.painter().text(
                            rect.center() + egui::vec2(0., 5.),
                            egui::Align2::CENTER_CENTER,
                            d.hot[n].map(short_time).unwrap_or_else(|| "—:—".into()),
                            FontId::monospace(9.),
                            if active {
                                label
                            } else {
                                color.gamma_multiply(0.8)
                            },
                        );
                    }
                    if response.clicked() {
                        self.deck(i, DeckControl::HotCue(n, ui.input(|v| v.modifiers.shift)));
                    }
                    response.on_hover_text("Click: set / recall · Shift+click: erase");
                }
            });
        }
    }
    fn sync_microphone(&mut self) {
        if let Some(mic) = &self.microphone {
            if !mic.is_running() {
                self.mic_status = format!(
                    "Mikrofon zatrzymany: {}",
                    mic.error().unwrap_or_else(|| "błąd wejścia".into())
                );
                self.settings.mic_enabled = false;
            }
        }
        let output_rate = self
            .runtime
            .as_ref()
            .filter(|runtime| runtime.is_running())
            .map(|runtime| runtime.sample_rate);
        if !self.settings.mic_enabled || output_rate.is_none() {
            if self.microphone.take().is_some() {
                self.send(Command::Microphone(None));
            }
            return;
        }
        let rate = output_rate.unwrap();
        if self
            .microphone
            .as_ref()
            .is_some_and(|mic| mic.output_rate != rate)
        {
            self.microphone = None;
            self.send(Command::Microphone(None));
        }
        if self.microphone.is_none() {
            match prospekt_dj::microphone::start(&self.settings.mic_device, rate) {
                Ok((microphone, input)) => {
                    self.mic_status =
                        format!("MIC ON: {} · kanał wejściowy 1", microphone.device_name);
                    self.send(Command::Microphone(Some(input)));
                    self.send(Command::MicVolume(self.settings.mic_volume));
                    self.microphone = Some(microphone);
                }
                Err(error) => {
                    self.mic_status = format!("Nie można uruchomić mikrofonu: {error:#}");
                    self.status = self.mic_status.clone();
                    self.settings.mic_enabled = false;
                }
            }
        }
    }
    fn header_audio_controls(&mut self, ui: &mut egui::Ui) {
        let s = self.snapshot.clone();
        let mut master = s.master;
        if ui
            .add(
                Knob::new(&mut master, 0.0..=1.)
                    .default(0.75)
                    .label("MASTER")
                    .unit("%")
                    .size(20.),
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
                    .size(20.),
            )
            .changed()
        {
            self.send(Command::Phones(phones));
        }
        let mut mic = self.settings.mic_volume;
        if ui
            .add(
                Knob::new(&mut mic, 0.0..=1.)
                    .default(0.7)
                    .label("MIC")
                    .unit("%")
                    .size(20.),
            )
            .on_hover_text(if self.settings.mic_enabled {
                "Poziom mikrofonu · ustawienia audio: wybór wejścia"
            } else {
                "Mikrofon wyłączony · włącz w Ustawienia → Audio → Mikrofon"
            })
            .changed()
        {
            self.settings.mic_volume = mic;
            self.send(Command::MicVolume(mic));
        }
        if ui
            .add(
                self.emoji
                    .icon_button(
                        ui.ctx(),
                        "📡",
                        RichText::new("BCAST")
                            .strong()
                            .color(if self.broadcast.is_some() {
                                Color32::from_rgb(255, 45, 55)
                            } else {
                                ui.visuals().text_color()
                            }),
                    )
                    .selected(self.broadcast.is_some())
                    .corner_radius(prospekt_dj::theme::radius())
                    .min_size(Vec2::new(56., 28.)),
            )
            .on_hover_text("Ustawienia transmisji Icecast / Shoutcast")
            .clicked()
        {
            self.settings_open = true;
            self.settings_tab = 10;
        }
        let recording = self.record_result.is_some();
        let response = ui
            .add_enabled(
                !self.record_stopping,
                // Red dot only, so the top bar fits on one row in every skin.
                egui::Button::new(RichText::new("●").size(14.).color(if recording {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(255, 45, 55)
                }))
                .fill(if recording {
                    Color32::from_rgb(180, 20, 30)
                } else {
                    Color32::from_gray(26)
                })
                .stroke(Stroke::new(2_f32, Color32::from_rgb(225, 42, 52)))
                .corner_radius(prospekt_dj::theme::radius())
                .min_size(Vec2::new(
                    if prospekt_dj::theme::classic() {
                        44.
                    } else {
                        30.
                    },
                    28.,
                )),
            )
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
        if ui
            .add(
                self.emoji
                    .icon_button(
                        ui.ctx(),
                        "🔀",
                        RichText::new("AUTOMIX").color(if self.auto.is_some() {
                            Color32::LIGHT_GREEN
                        } else {
                            ui.visuals().text_color()
                        }),
                    )
                    .corner_radius(prospekt_dj::theme::radius())
                    .min_size(Vec2::new(58., 28.)),
            )
            .on_hover_text("Otwórz ustawienia AutoMix")
            .clicked()
        {
            self.settings_open = true;
            self.settings_tab = 2;
        }
        ui.checkbox(
            &mut self.settings.white_jog,
            header_label("BIAŁE JOGI", "BIAŁE"),
        )
        .on_hover_text("Białe tło jogów i czarny tekst na wszystkich dekach");
        ui.checkbox(
            &mut self.settings.show_main_waveforms,
            header_label("WAVEFORMY", "WAVE"),
        )
        .on_hover_text("Pokaż lub ukryj główne waveformy");
        let previous = self.settings.deck_count;
        if prospekt_dj::theme::classic() {
            // Header flows right to left: emit 4 first to display 2 / 4 beside AutoMix.
            for count in [4, 2] {
                if ui
                    .add(
                        self.emoji
                            .icon_button(ui.ctx(), "💿", format!("{count} DEKI"))
                            .selected(self.settings.deck_count == count)
                            .corner_radius(0)
                            .min_size(Vec2::new(54., 28.)),
                    )
                    .on_hover_text(format!("Przełącz na {count} deki"))
                    .clicked()
                {
                    self.settings.deck_count = count;
                }
            }
            if previous != self.settings.deck_count {
                self.apply_deck_count();
            }
            return;
        }
        // One toggle replaces the separate 2 DEKI / 4 DEKI buttons.
        let count = self.settings.deck_count;
        let other = if count == 4 { 2 } else { 4 };
        if ui
            .add(
                self.emoji
                    .icon_button(ui.ctx(), "💿", format!("{count} DEKI"))
                    .selected(count == 4)
                    .corner_radius(prospekt_dj::theme::radius())
                    .min_size(Vec2::new(54., 28.)),
            )
            .on_hover_text(format!("Przełącz na {other} deki"))
            .clicked()
        {
            self.settings.deck_count = other;
        }
        if previous != self.settings.deck_count {
            self.apply_deck_count();
        }
    }
    fn apply_deck_count(&mut self) {
        self.send(Command::DeckCount(self.settings.deck_count));
        self.stop_auto();
        if self.settings.deck_count == 2 {
            for i in 2..4 {
                self.send(Command::Deck(i, DeckControl::Pause));
                self.send(Command::Deck(i, DeckControl::Jog(0., 0.)));
                self.send(Command::Deck(i, DeckControl::Bend(0.)));
            }
            self.focused = self.focused.min(1);
        }
    }
    fn stop_preview(&mut self) {
        self.preview_token = self.preview_token.wrapping_add(1);
        self.preview_loading = false;
        self.preview_path = None;
        self.send(Command::Preview(None));
    }
    fn start_preview(&mut self, path: PathBuf) {
        // Without a working PHONES/PFL output the preview would play into nothing.
        if !self
            .runtime
            .as_ref()
            .is_some_and(|r| r.is_running() && r.phones_routed())
        {
            self.status = "Odsłuch poza dekami wymaga wyjścia PFL/PHONES. Ustawienia → Audio: wybierz PFL i zastosuj ustawienia audio.".into();
            return;
        }
        self.stop_preview();
        self.preview_path = Some(path.clone());
        self.preview_loading = true;
        let token = self.preview_token;
        let tx = self.work_tx.clone();
        std::thread::spawn(move || {
            let result = prospekt_dj::media::load(&path, false).map_err(|e| e.to_string());
            let _ = tx.send(Work::Preview(token, result));
        });
    }
    fn broadcast_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Broadcast — transmisja miksu na żywo");
        ui.label("Wyjście MASTER z mikrofonem · MP3 · wymaga FFmpeg");
        ui.add_enabled_ui(self.broadcast.is_none(), |ui| {
            let config = &mut self.settings.broadcast;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut config.shoutcast, false, "Icecast");
                ui.selectable_value(&mut config.shoutcast, true, "Shoutcast (tryb zgodności v1)");
            });
            egui::Grid::new("broadcast-config").num_columns(2).show(ui, |ui| {
                ui.label("Serwer (nazwa lub IP)"); ui.text_edit_singleline(&mut config.host); ui.end_row();
                ui.label("Port źródła"); ui.add(egui::DragValue::new(&mut config.port).range(1..=65535)); ui.end_row();
                if !config.shoutcast {
                    ui.label("Punkt montowania"); ui.text_edit_singleline(&mut config.mount); ui.end_row();
                    ui.label("Użytkownik źródła"); ui.text_edit_singleline(&mut config.username); ui.end_row();
                }
                ui.label("Hasło źródła"); ui.add(egui::TextEdit::singleline(&mut config.password).password(true)); ui.end_row();
                ui.label("Nazwa transmisji"); ui.text_edit_singleline(&mut config.name); ui.end_row();
                ui.label("Bitrate MP3"); egui::ComboBox::from_id_salt("broadcast-bitrate").selected_text(format!("{} kb/s", config.bitrate)).show_ui(ui, |ui| { for rate in [64,96,128,160,192,256,320] { ui.selectable_value(&mut config.bitrate, rate, format!("{rate} kb/s")); } }); ui.end_row();
            });
            ui.checkbox(&mut config.tls, "Szyfrowane połączenie TLS (serwer musi je obsługiwać)");
            ui.label("Hasło pozostaje tylko w pamięci tej sesji. Transmisja nie uruchamia się automatycznie.");
        });
        if self.broadcast.is_some() {
            if ui.button("Zatrzymaj transmisję").clicked() {
                self.send(Command::Broadcast(None));
                self.broadcast = None;
                self.broadcast_status = "Transmisja zatrzymana".into();
            }
        } else if ui
            .add_enabled(
                self.runtime.is_some(),
                egui::Button::new("Rozpocznij transmisję"),
            )
            .clicked()
        {
            if let Some(runtime) = &self.runtime {
                match prospekt_dj::broadcast::start(&self.settings.broadcast, runtime.sample_rate) {
                    Ok(session) => {
                        self.send(Command::Broadcast(Some(session.sink.clone())));
                        self.broadcast = Some(session);
                        self.broadcast_status = "Koder uruchomiony — łączenie / przesyłanie audio. Odbiór sprawdź na serwerze.".into();
                    }
                    Err(error) => self.broadcast_status = error.to_string(),
                }
            }
        }
        if self.runtime.is_none() {
            ui.label("Włącz wyjście audio, aby nadawać miks.");
        }
        ui.label(&self.broadcast_status);
    }
    fn mixer_eq_column(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        ui.allocate_ui_with_layout(
            Vec2::new(36., 180.),
            egui::Layout::top_down(Align::Center),
            |ui| {
                ui.set_width(36.);
                ui.spacing_mut().item_spacing.y = 0.;
                for (band, (name, color)) in [
                    ("HIGH", Color32::from_rgb(40, 200, 0)),
                    ("MID", Color32::from_rgb(255, 215, 0)),
                    ("LOW", Color32::from_rgb(230, 0, 0)),
                ]
                .into_iter()
                .enumerate()
                {
                    let mut value = d.eq[band];
                    if ui
                        .add(
                            Knob::new(&mut value, -24.0..=12.)
                                .label(name)
                                .unit("dB")
                                .color(color)
                                .size(24.),
                        )
                        .changed()
                    {
                        self.send(Command::Deck(i, DeckControl::Eq(band, value)));
                    }
                }
                if ui
                    .selectable_label(d.fx.filter_enabled, "F")
                    .on_hover_text("Enable LPF / HPF")
                    .clicked()
                {
                    self.send(Command::Deck(i, DeckControl::FxToggle(FxSwitch::Filter)));
                }
                let mut filter = d.fx.filter;
                if ui
                    .add(Knob::new(&mut filter, -1.0..=1.).label("FILTER").size(24.))
                    .changed()
                {
                    self.send(Command::Deck(
                        i,
                        DeckControl::FxParam(FxParam::Filter, filter),
                    ));
                }
            },
        );
    }
    fn mixer_fader_column(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        ui.allocate_ui_with_layout(
            Vec2::new(36., 180.),
            egui::Layout::top_down(Align::Center),
            |ui| {
                ui.set_width(36.);
                ui.spacing_mut().item_spacing.y = 1.;
                let mut gain = d.gain_db;
                if ui
                    .add(
                        Knob::new(&mut gain, -24.0..=12.)
                            .label("GAIN")
                            .unit("dB")
                            .color(Color32::LIGHT_GRAY)
                            .size(24.),
                    )
                    .changed()
                {
                    self.send(Command::Deck(i, DeckControl::Gain(gain)));
                }
                ui.label(RichText::new(letter(i)).size(9.).color(ACCENTS[i]));
                ui.spacing_mut().slider_width = 100.;
                ui.spacing_mut().slider_rail_height = 2.;
                let mut volume = d.volume;
                let response = ui.add(
                    egui::Slider::new(&mut volume, 0.0..=1.)
                        .vertical()
                        .trailing_fill(false)
                        .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.35 })
                        .show_value(false),
                );
                if response.changed() {
                    self.send(Command::Deck(i, DeckControl::Volume(volume)));
                }
                if response.double_clicked() {
                    self.send(Command::Deck(i, DeckControl::Volume(0.8)));
                }
                if ui.selectable_label(d.pfl, "PFL").clicked() {
                    self.send(Command::Deck(i, DeckControl::Pfl));
                }
            },
        );
    }
    /// One channel strip (EQ + gain/fader/PFL). Modern skins put it on a raised card.
    fn mixer_strip(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView, eq_first: bool) {
        if prospekt_dj::theme::classic() {
            // Original skin: the two columns sit directly in the mixer row.
            if eq_first {
                self.mixer_eq_column(ui, i, d);
                self.mixer_fader_column(ui, i, d);
            } else {
                self.mixer_fader_column(ui, i, d);
                self.mixer_eq_column(ui, i, d);
            }
            return;
        }
        let card = if prospekt_dj::theme::classic() {
            egui::Frame::NONE
        } else {
            egui::Frame::new()
                .fill(prospekt_dj::theme::palette().raised)
                .stroke(Stroke::new(1_f32, prospekt_dj::theme::palette().edge))
                .corner_radius(prospekt_dj::theme::radius_f(4.))
                .inner_margin(egui::Margin::symmetric(1, 4))
        };
        card.show(ui, |ui| {
            ui.horizontal_top(|ui| {
                if eq_first {
                    self.mixer_eq_column(ui, i, d);
                    self.mixer_fader_column(ui, i, d);
                } else {
                    self.mixer_fader_column(ui, i, d);
                    self.mixer_eq_column(ui, i, d);
                }
            });
        });
    }
    fn mini_mixer(&mut self, ui: &mut egui::Ui, start: usize) {
        let s = self.snapshot.clone();
        let classic = prospekt_dj::theme::classic();
        egui::Frame::new()
            .fill(if classic {
                prospekt_dj::theme::row_bg()
            } else {
                prospekt_dj::theme::palette().panel
            })
            .stroke(if classic {
                Stroke::NONE
            } else {
                Stroke::new(1_f32, prospekt_dj::theme::palette().edge)
            })
            .corner_radius(prospekt_dj::theme::radius_f(4.))
            .inner_margin(3)
            .show(ui, |ui| {
                ui.set_width(196.);
                ui.set_min_height(208.);
                ui.spacing_mut().item_spacing = Vec2::new(2., 4.);
                ui.horizontal_top(|ui| {
                    ui.add_space(if classic { 10. } else { 2. });
                    self.mixer_strip(ui, start, &s.decks[start], true);
                    ui.vertical(|ui| {
                        ui.add_space(38.);
                        meter(ui, s.decks[start].level, ACCENTS[start], 108.);
                    });
                    ui.vertical(|ui| {
                        ui.add_space(38.);
                        meter(ui, s.decks[start + 1].level, ACCENTS[start + 1], 108.);
                    });
                    self.mixer_strip(ui, start + 1, &s.decks[start + 1], false);
                });
                ui.spacing_mut().slider_width = if classic { 184. } else { 158. };
                ui.spacing_mut().slider_rail_height = 2.;
                let mut cross = s.cross;
                let response = ui
                    .horizontal(|ui| {
                        ui.add_space(6.);
                        if !classic {
                            ui.label(RichText::new("A").strong().size(10.).color(ACCENTS[start]));
                        }
                        let slider = ui.add_enabled(
                            self.settings.cross_enabled,
                            egui::Slider::new(&mut cross, 0.0..=1.)
                                .trailing_fill(false)
                                .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.35 })
                                .show_value(false),
                        );
                        if !classic {
                            ui.label(
                                RichText::new("B")
                                    .strong()
                                    .size(10.)
                                    .color(ACCENTS[start + 1]),
                            );
                        }
                        slider
                    })
                    .inner;
                if response.changed() {
                    self.stop_auto();
                    self.send(Command::Cross(cross));
                }
                if response.double_clicked() {
                    self.stop_auto();
                    self.send(Command::Cross(0.5));
                }
            });
    }
    fn deck_effects_ui(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        let names = [
            "FILTER",
            "ECHO",
            "REVERB",
            "FLANGER",
            "CHORUS",
            "TREMOLO",
            "DISTORTION",
            "BITCRUSHER",
        ];
        let mut fx = d.fx;
        let before = fx;
        ui.push_id(("deck-effects", i), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(3., 2.);
            egui::ComboBox::from_id_salt("effect")
                .width(128.)
                .selected_text(names[self.deck_effect[i]])
                .show_ui(ui, |ui| {
                    for (unit, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.deck_effect[i], unit, *name);
                    }
                });
            let unit = self.deck_effect[i];
            ui.horizontal(|ui| {
                let enabled = match unit {
                    0 => &mut fx.filter_enabled,
                    1 => &mut fx.echo_enabled,
                    2 => &mut fx.reverb_enabled,
                    3 => &mut fx.flanger_enabled,
                    4 => &mut fx.chorus_enabled,
                    5 => &mut fx.tremolo_enabled,
                    6 => &mut fx.distortion_enabled,
                    _ => &mut fx.crusher_enabled,
                };
                let label = if *enabled { "ON" } else { "OFF" };
                ui.toggle_value(enabled, label);
                ui.toggle_value(&mut fx.bypass, "BYPASS");
                if ui.small_button("RESET FX").clicked() {
                    fx = FxSettings::default();
                }
            });
            let knob = |ui: &mut egui::Ui, value: &mut f32, range, default, label| {
                ui.add(
                    Knob::new(value, range)
                        .default(default)
                        .label(label)
                        .color(ACCENTS[i])
                        .size(24.),
                );
            };
            ui.horizontal(|ui| match unit {
                0 => knob(ui, &mut fx.filter, -1.0..=1., 0., "FILTER"),
                1 => {
                    knob(ui, &mut fx.echo_mix, 0.0..=1., 0.3, "MIX");
                    knob(ui, &mut fx.echo_feedback, 0.0..=0.85, 0.35, "FB");
                    ui.vertical(|ui| {
                        ui.checkbox(&mut fx.echo_sync, "BPM SYNC");
                        if fx.echo_sync {
                            egui::ComboBox::from_id_salt("echo-beat")
                                .width(65.)
                                .selected_text(format!("{} beat", fx.echo_beats))
                                .show_ui(ui, |ui| {
                                    for beat in [0.125, 0.25, 0.5, 1., 2.] {
                                        ui.selectable_value(
                                            &mut fx.echo_beats,
                                            beat,
                                            format!("{beat} beat"),
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
                    knob(ui, &mut fx.reverb_mix, 0.0..=1., 0.25, "MIX");
                    knob(ui, &mut fx.reverb_room, 0.0..=1., 0.6, "ROOM");
                    knob(ui, &mut fx.reverb_damping, 0.0..=1., 0.45, "DAMP");
                }
                3 => {
                    knob(ui, &mut fx.flanger_mix, 0.0..=1., 0.5, "MIX");
                    knob(ui, &mut fx.flanger_rate, 0.05..=5., 0.25, "RATE Hz");
                    knob(ui, &mut fx.flanger_depth, 0.0..=1., 0.6, "DEPTH");
                }
                _ => {
                    let (mix, amount, label, min, max, default) = match unit {
                        4 => (
                            &mut fx.chorus_mix,
                            &mut fx.chorus_rate,
                            "RATE Hz",
                            0.05,
                            5.,
                            0.6,
                        ),
                        5 => (
                            &mut fx.tremolo_depth,
                            &mut fx.tremolo_rate,
                            "RATE Hz",
                            0.1,
                            16.,
                            0.6,
                        ),
                        6 => (&mut fx.distortion_mix, &mut fx.drive, "DRIVE", 1., 20., 3.),
                        _ => (
                            &mut fx.crusher_mix,
                            &mut fx.crusher_bits,
                            "BITS",
                            4.,
                            16.,
                            10.,
                        ),
                    };
                    knob(
                        ui,
                        mix,
                        0.0..=1.,
                        0.4,
                        if unit == 5 { "DEPTH" } else { "MIX" },
                    );
                    knob(ui, amount, min..=max, default, label);
                }
            });
        });
        if fx != before {
            self.send(Command::Deck(i, DeckControl::Fx(fx)));
        }
    }
    fn sampler_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2., 2.);
            ui.label(RichText::new("SAMPLER").size(8.).weak());
            for row in 0..2 {
                ui.horizontal(|ui| {
                    for col in 0..2 {
                        let slot = row * 2 + col;
                        let sample = self.snapshot.samples[slot].clone();
                        let color = [
                            Color32::from_gray(26),
                            Color32::from_rgb(230, 0, 0),
                            Color32::from_rgb(255, 215, 0),
                            Color32::from_rgb(40, 200, 0),
                        ][slot];
                        let name = sample
                            .as_ref()
                            .map(|(t, _, _)| t.title.as_str())
                            .unwrap_or("Empty");
                        let short: String = name.chars().take(8).collect();
                        let foreground = if slot >= 2 && sample.is_some() {
                            Color32::BLACK
                        } else {
                            Color32::WHITE
                        };
                        let response = ui.add_sized(
                            [62., 27.],
                            egui::Button::new(
                                RichText::new(format!("{} {}", slot + 1, short))
                                    .size(9.)
                                    .color(foreground),
                            )
                            .selected(self.snapshot.sample_playing[slot])
                            .fill(if sample.is_some() {
                                color
                            } else {
                                Color32::BLACK
                            })
                            .stroke(Stroke::new(
                                1_f32,
                                if slot == 0 { Color32::GRAY } else { color },
                            )),
                        );
                        if response.clicked() {
                            if sample.is_some() {
                                self.send(Command::SampleTrigger(slot));
                            } else {
                                self.sample_file(slot);
                            }
                        }
                        response.clone().on_hover_ui(|ui| {
                            ui.label(name);
                            if let Some((track, start, end)) = &sample {
                                ui.label(format!(
                                    "{:.2} s · {} bytes file",
                                    (end - start) / track.sample_rate as f64,
                                    track.file_bytes
                                ));
                            }
                        });
                        response.context_menu(|ui| {
                            if ui.button("Load file…").clicked() {
                                self.sample_file(slot);
                                ui.close_menu();
                            }
                            if ui.button("Capture from focused deck").clicked() {
                                self.sample_tokens[slot] += 1;
                                self.send(Command::Deck(
                                    self.focused,
                                    DeckControl::PadMode(PadMode::Sampler),
                                ));
                                self.send(Command::SampleClear(slot));
                                self.send(Command::Deck(
                                    self.focused,
                                    DeckControl::Pad(slot, false),
                                ));
                                self.send(Command::Deck(
                                    self.focused,
                                    DeckControl::PadMode(self.snapshot.decks[self.focused].mode),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Clear").clicked() {
                                self.sample_tokens[slot] += 1;
                                self.send(Command::SampleClear(slot));
                                ui.close_menu();
                            }
                        });
                    }
                });
            }
            let mut volume = self.snapshot.sample_volume;
            ui.spacing_mut().slider_width = 126.;
            if ui
                .add(egui::Slider::new(&mut volume, 0.0..=1.).show_value(false))
                .on_hover_text("Shared sampler level")
                .changed()
            {
                self.send(Command::SampleVolume(volume));
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
            if matches!(self.scope, Scope::History(_)) {
                let allowed: std::collections::BTreeSet<_> = self
                    .catalog
                    .visible(
                        &self.library,
                        &Query {
                            text: &self.search,
                            scope: &Scope::All,
                            sort: self.settings.library_sort,
                            descending: self.settings.library_descending,
                            favorites: &self.settings.favorites,
                            loaded,
                        },
                    )
                    .into_iter()
                    .collect();
                let lookup: std::collections::BTreeMap<_, _> = self
                    .library
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (p.as_path(), i))
                    .collect();
                self.visible_rows = self
                    .history
                    .events
                    .iter()
                    .filter_map(|event| event.path.as_deref().and_then(|p| lookup.get(p)).copied())
                    .filter(|i| allowed.contains(i))
                    .collect();
            }
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
        self.sidebar_cover_reflection = None;
        let animation = |id, expanded| {
            if self.settings.reduced_motion {
                if expanded {
                    1.
                } else {
                    0.
                }
            } else {
                ui.ctx()
                    .animate_bool_with_time(egui::Id::new(id), expanded, 0.3)
            }
        };
        let left_progress = animation(
            "library-folders-slide",
            !self.settings.library_folders_collapsed,
        );
        let right_progress = animation("library-info-slide", !self.settings.library_info_collapsed);
        let left_width = self.settings.tree_width.clamp(150., 400.) * left_progress;
        ui.horizontal(|ui| {
            // Match the sidebar, resize handle and gaps in the content row below.
            let heading_width = (left_width + 5. + 18. + 6. - 16.).max(110.);
            let heading_background = egui::Rect::from_min_size(
                ui.next_widget_position(),
                Vec2::new(heading_width + ui.spacing().item_spacing.x, 21.),
            );
            let painter = ui.painter().with_clip_rect(heading_background);
            let stripe_width = 6. / ui.ctx().pixels_per_point();
            for index in 0..(heading_background.width() / stripe_width).ceil() as usize {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        heading_background.min + egui::vec2(index as f32 * stripe_width, 0.),
                        Vec2::new(stripe_width, heading_background.height()),
                    ),
                    0.,
                    if index % 2 == 0 {
                        ui.visuals().panel_fill
                    } else {
                        Color32::from_rgba_unmultiplied(10, 10, 10, 122)
                    },
                );
            }
            ui.allocate_ui_with_layout(
                Vec2::new(heading_width, 21.),
                egui::Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_min_width(heading_width);
                    ui.set_max_width(heading_width);
                    if self.settings.banger_site_name {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(22., 21.), Sense::hover());
                        self.emoji.paint(ui, rect, "🔥", Color32::WHITE);
                    }
                    ui.label(
                        RichText::new(if self.settings.banger_site_name {
                            "BANGER SITE".to_owned()
                        } else {
                            "Kolekcja".to_owned()
                        })
                        .family(egui::FontFamily::Name(
                            if self.settings.banger_site_name
                                && self.settings.banger_site_display_font
                            {
                                "Sedgwick Ave Display"
                            } else {
                                "Arial Bold"
                            }
                            .into(),
                        ))
                        .size(
                            if self.settings.banger_site_name
                                && self.settings.banger_site_display_font
                            {
                                18.
                            } else {
                                13.
                            },
                        ),
                    );
                },
            );
            let (magnifier, _) = ui.allocate_exact_size(Vec2::splat(16.), Sense::hover());
            let lens = magnifier.min + egui::vec2(6., 6.);
            let gray = Color32::from_gray(155);
            ui.painter()
                .circle_stroke(lens, 4.5, Stroke::new(1.7_f32, gray));
            ui.painter().line_segment(
                [
                    lens + egui::vec2(3.5, 3.5),
                    magnifier.min + egui::vec2(15., 15.),
                ],
                Stroke::new(2_f32, gray),
            );
            let refresh_width = ui
                .ctx()
                .data(|data| data.get_temp::<f32>(egui::Id::new("library-refresh-width")))
                .unwrap_or(100.);
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text(tr("Szukaj: tytuł, wykonawca, album, gatunek, tonacja…"))
                        .desired_width(
                            (ui.available_width()
                                - if self.settings.show_emojis {
                                    650.
                                } else {
                                    580.
                                }
                                - refresh_width)
                                .max(160.),
                        ),
                )
                .changed()
            {
                self.browser_dirty = true;
            }
            ui.label(RichText::new(format!("Utwory: {}", self.library.len())).strong())
                .on_hover_text("Wszystkie utwory w bibliotece");
            let toolbar_stripe_start = ui.next_widget_position();
            ui.add_space(refresh_width);
            let toolbar_background = egui::Rect::from_min_max(
                toolbar_stripe_start,
                egui::pos2(ui.clip_rect().right(), ui.next_widget_position().y + 21.),
            );
            let painter = ui.painter().with_clip_rect(toolbar_background);
            let stripe_width = 6. / ui.ctx().pixels_per_point();
            for index in 0..(toolbar_background.width() / stripe_width).ceil() as usize {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        toolbar_background.min + egui::vec2(index as f32 * stripe_width, 0.),
                        Vec2::new(stripe_width, toolbar_background.height()),
                    ),
                    0.,
                    if index % 2 == 0 {
                        ui.visuals().panel_fill
                    } else {
                        Color32::from_rgba_unmultiplied(10, 10, 10, 122)
                    },
                );
            }
            if self
                .emoji
                .labeled_button(ui, "🎵", &tr("+ Pliki"))
                .clicked()
            {
                if let Some(files) = audio_dialog().pick_files() {
                    self.add_paths(files);
                }
            }
            if self
                .emoji
                .labeled_button(ui, "📁", &tr("+ Folder"))
                .clicked()
            {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.add_paths(vec![folder]);
                }
            }
            let refresh = ui
                .add_enabled_ui(self.index_jobs == 0, |ui| {
                    self.emoji.labeled_button(ui, "#️⃣", &tr("Odśwież tagi"))
                })
                .inner;
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::new("library-refresh-width"), refresh.rect.width())
            });
            if refresh.clicked() {
                self.index_paths(self.library.clone(), true);
            }
            ui.checkbox(&mut self.settings.live_recommendations, "Rekomendacje live");
            let (live_dot, _) = ui.allocate_exact_size(Vec2::splat(10.), Sense::hover());
            ui.painter()
                .circle_filled(live_dot.center(), 4., Color32::from_rgb(255, 35, 48));
            if self.index_jobs > 0 {
                ui.spinner();
                ui.label(RichText::new("Indeksowanie…").small());
            }
            if self.settings.show_library_clock {
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let progress = if self.settings.reduced_motion {
                        f32::from(self.settings.library_clock_expanded)
                    } else {
                        ui.ctx().animate_bool_with_time(
                            egui::Id::new("library-clock-expand"),
                            self.settings.library_clock_expanded,
                            0.3,
                        )
                    };
                    let font = egui::TextStyle::Monospace.resolve(ui.style());
                    let time = chrono::Local::now().format("%H:%M:%S").to_string();
                    let galley = ui
                        .painter()
                        .layout_no_wrap(time, font, ui.visuals().text_color());
                    let text_width = galley.size().x;
                    let gap = ui.spacing().item_spacing.x;
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(17. + (gap + text_width) * progress, 21.),
                        Sense::hover(),
                    );
                    let clock = egui::Rect::from_min_size(
                        egui::pos2(rect.left(), rect.center().y - 8.5),
                        Vec2::splat(17.),
                    );
                    let response =
                        ui.interact(clock, ui.id().with("library-clock-toggle"), Sense::click());
                    if response.clicked() {
                        self.settings.library_clock_expanded =
                            !self.settings.library_clock_expanded;
                    }
                    response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(if self.settings.library_clock_expanded {
                            "Kliknij, aby schować czas"
                        } else {
                            "Kliknij, aby pokazać czas"
                        });
                    let text_top = rect.center().y - galley.size().y / 2.;
                    let center =
                        egui::pos2(clock.center().x, text_top + galley.mesh_bounds.center().y);
                    let gray = Color32::from_gray(180);
                    ui.painter()
                        .circle_stroke(center, 6.5, Stroke::new(1.4_f32, gray));
                    ui.painter().line_segment(
                        [center, center + egui::vec2(0., -4.)],
                        Stroke::new(1.4_f32, gray),
                    );
                    ui.painter().line_segment(
                        [center, center + egui::vec2(3.5, 1.5)],
                        Stroke::new(1.4_f32, gray),
                    );
                    if progress > 0. {
                        let clip = egui::Rect::from_min_max(
                            egui::pos2(clock.right(), rect.top()),
                            rect.right_bottom(),
                        );
                        let mut painter =
                            ui.painter().with_clip_rect(ui.clip_rect().intersect(clip));
                        painter.multiply_opacity(progress);
                        painter.galley(
                            egui::pos2(
                                clock.right() + gap - (1. - progress) * text_width,
                                rect.center().y - galley.size().y / 2.,
                            ),
                            galley,
                            ui.visuals().text_color(),
                        );
                    }
                });
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
        });
        ui.separator();
        ui.horizontal_top(|ui| {
            let panel_gap = ui.spacing().item_spacing.x;
            ui.spacing_mut().item_spacing.x = 0.;
            let (left_rect, _) = ui
                .allocate_exact_size(Vec2::new(left_width, ui.available_height()), Sense::hover());
            if left_width > 0.5 {
                let full = egui::Rect::from_min_size(
                    egui::pos2(
                        left_rect.right() - self.settings.tree_width.clamp(150., 400.),
                        left_rect.top(),
                    ),
                    Vec2::new(
                        self.settings.tree_width.clamp(150., 400.),
                        left_rect.height(),
                    ),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("library-folders-panel")
                        .max_rect(full)
                        .layout(egui::Layout::top_down(Align::Min)),
                );
                child.set_clip_rect(ui.clip_rect().intersect(left_rect));
                child.spacing_mut().item_spacing.x = panel_gap;
                let ui = &mut child;
                ui.painter().rect_filled(
                    ui.available_rect_before_wrap(),
                    0.,
                    Color32::from_rgb(10, 10, 10),
                );
                let height = ui.available_height().max(40.);
                if self.settings.cover_strip && self.settings.cover_reflections {
                    let bounds = ui.available_rect_before_wrap();
                    let strip_width = 2. * 72. / 20.;
                    self.sidebar_cover_reflection = Some(
                        egui::Rect::from_min_max(
                            egui::pos2(bounds.right() - 6. - strip_width, bounds.top() + 6.),
                            egui::pos2(bounds.right() - 6., bounds.bottom() - 6.),
                        )
                        .intersect(left_rect),
                    );
                    ui.set_max_width((bounds.width() - strip_width - 12.).max(100.));
                }
                egui::ScrollArea::vertical()
                    .id_salt("crate-tree")
                    .auto_shrink([false, false])
                    .max_height(height)
                    .show(ui, |ui| self.browser_sidebar(ui));
            }
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
            if library_panel_tab(ui, "FOLDERY", !self.settings.library_folders_collapsed).clicked()
            {
                self.settings.library_folders_collapsed = !self.settings.library_folders_collapsed;
            }
            ui.add_space(6.);
            let preview = if self.settings.track_preview && ui.ctx().screen_rect().width() >= 1200.
            {
                // Info panel mirrors the folder sidebar width.
                (if prospekt_dj::theme::classic() {
                    220.
                } else {
                    self.settings.tree_width.clamp(150., 400.)
                }) * right_progress
            } else {
                0.
            };
            let show_info_tab =
                self.settings.track_preview && ui.ctx().screen_rect().width() >= 1200.;
            let center = (ui.available_width()
                - if show_info_tab {
                    preview + 18. + 6.
                } else {
                    0.
                })
            .max(400.);
            ui.allocate_ui_with_layout(
                Vec2::new(center, ui.available_height()),
                egui::Layout::top_down(Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.x = panel_gap;
                    ui.set_width(center);
                    let library_rect = ui.available_rect_before_wrap();
                    let footer_height = if self.settings.live_recommendations {
                        34.
                    } else {
                        0.
                    };
                    // The list ends 12 px above the recommendations bar.
                    let classic_look = prospekt_dj::theme::classic();
                    let list_gap = if classic_look {
                        96.
                    } else if self.settings.live_recommendations {
                        12.
                    } else {
                        0.
                    };
                    let content_rect = egui::Rect::from_min_max(
                        library_rect.min,
                        egui::pos2(
                            library_rect.right(),
                            (library_rect.bottom() - footer_height - list_gap)
                                .max(library_rect.top()),
                        ),
                    );
                    // Modern skins: the watermark keeps its place in the bottom band and is
                    // drawn before the rows, so the generated track list covers it.
                    let watermark_band = egui::Rect::from_min_max(
                        egui::pos2(
                            library_rect.left(),
                            library_rect.bottom() - footer_height - 96.,
                        ),
                        egui::pos2(library_rect.right(), library_rect.bottom() - footer_height),
                    );
                    if !classic_look && watermark_band.height() >= 70. {
                        let painter = ui.painter().with_clip_rect(watermark_band);
                        let center = watermark_band.center();
                        let size = Vec2::splat(58. * 1.33);
                        let logo_rect =
                            egui::Rect::from_center_size(center - egui::vec2(0., 16.), size);
                        painter.image(
                            self.brand.icon.id(),
                            logo_rect,
                            egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                            Color32::from_white_alpha(14),
                        );
                        painter.text(
                            center + egui::vec2(0., 28.),
                            egui::Align2::CENTER_CENTER,
                            "PROSPEKT DJ v0.0.1 pre-alpha test",
                            FontId::new(13., egui::FontFamily::Name("Arial Bold".into())),
                            Color32::from_white_alpha(14),
                        );
                    }
                    self.library_last_row_bottom = None;
                    ui.scope_builder(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                        ui.set_clip_rect(ui.clip_rect().intersect(content_rect));
                        if let Scope::History(id) = &self.scope {
                            let id = id.clone();
                            self.history_table(ui, &id);
                        } else {
                            if self.settings.cover_strip {
                                self.cover_strip(ui);
                            }
                            egui::ScrollArea::horizontal()
                                .id_salt("table-columns")
                                .max_height(ui.available_height().max(40.))
                                .max_width(center)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_min_width(1160.);
                                    self.browser_table(ui);
                                });
                        }
                    });
                    if self.settings.cover_strip
                        && self.settings.cover_reflections
                        && !matches!(self.scope, Scope::History(_))
                    {
                        let reflection_top = self
                            .library_last_row_bottom
                            .unwrap_or(content_rect.bottom())
                            + 6.;
                        let reflection_clip = egui::Rect::from_min_max(
                            egui::pos2(content_rect.left(), reflection_top),
                            egui::pos2(content_rect.right(), reflection_top + 2. * 72. / 20.),
                        );
                        let painter = ui
                            .painter()
                            .with_clip_rect(ui.clip_rect().intersect(reflection_clip));
                        for mesh in &self.cover_reflections {
                            let mut mesh = mesh.clone();
                            for vertex in &mut mesh.vertices {
                                vertex.pos.y = reflection_top + vertex.pos.y * 2.;
                            }
                            painter.add(egui::Shape::mesh(mesh));
                        }
                    }
                    if classic_look {
                        let watermark_rect = egui::Rect::from_min_max(
                            egui::pos2(library_rect.left(), content_rect.bottom()),
                            egui::pos2(library_rect.right(), library_rect.bottom() - footer_height),
                        );
                        if watermark_rect.height() >= 70. {
                            let painter = ui.painter().with_clip_rect(watermark_rect);
                            let center = watermark_rect.center();
                            let size = Vec2::splat(58. * 1.33);
                            let logo_rect =
                                egui::Rect::from_center_size(center - egui::vec2(0., 16.), size);
                            painter.image(
                                self.brand.icon.id(),
                                logo_rect,
                                egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                                Color32::from_white_alpha(2),
                            );
                            painter.text(
                                center + egui::vec2(0., 28.),
                                egui::Align2::CENTER_CENTER,
                                "PROSPEKT DJ v0.0.1 pre-alpha test",
                                FontId::new(13., egui::FontFamily::Name("Arial Bold".into())),
                                Color32::from_white_alpha(2),
                            );
                        }
                    }
                    if self.settings.live_recommendations {
                        let footer_rect = egui::Rect::from_min_max(
                            egui::pos2(library_rect.left(), library_rect.bottom() - footer_height),
                            library_rect.max,
                        );
                        ui.scope_builder(egui::UiBuilder::new().max_rect(footer_rect), |ui| {
                            self.recommendations_ui(ui)
                        });
                    } else {
                        self.recommendation_advanced = None;
                        self.recommendation_memory.clear_current();
                        self.recommendation_restart_at = None;
                        self.recommendation_results.clear();
                        self.recommendation_updated = None;
                    }
                },
            );
            if show_info_tab {
                ui.add_space(6.);
                if library_panel_tab(ui, "INFO", !self.settings.library_info_collapsed).clicked() {
                    self.settings.library_info_collapsed = !self.settings.library_info_collapsed;
                }
                if preview > 0.5 {
                    let (right_rect, _) = ui.allocate_exact_size(
                        Vec2::new(preview, ui.available_height()),
                        Sense::hover(),
                    );
                    for offset in [-4., -1.] {
                        let x = right_rect.left() + offset;
                        ui.painter().line_segment(
                            [
                                egui::pos2(x, right_rect.top()),
                                egui::pos2(x, right_rect.bottom()),
                            ],
                            Stroke::new(1_f32, Color32::from_rgba_unmultiplied(49, 188, 255, 38)),
                        );
                    }
                    ui.painter()
                        .rect_filled(right_rect, 0., Color32::from_rgb(10, 10, 10));
                    let full = egui::Rect::from_min_size(
                        right_rect.min + egui::vec2(6., 0.),
                        Vec2::new(214., right_rect.height()),
                    );
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("library-info-panel")
                            .max_rect(full)
                            .layout(egui::Layout::top_down(Align::Min)),
                    );
                    child.set_clip_rect(ui.clip_rect().intersect(right_rect));
                    child.spacing_mut().item_spacing.x = panel_gap;
                    let ui = &mut child;
                    let panel_rect = ui.available_rect_before_wrap();
                    for offset in [-12., -9.] {
                        let x = panel_rect.left() + offset;
                        ui.painter().line_segment(
                            [
                                egui::pos2(x, panel_rect.top()),
                                egui::pos2(x, panel_rect.bottom()),
                            ],
                            Stroke::new(
                                1_f32,
                                Color32::from_rgba_unmultiplied(
                                    ACCENTS[0].r(),
                                    ACCENTS[0].g(),
                                    ACCENTS[0].b(),
                                    38,
                                ),
                            ),
                        );
                    }
                    ui.painter().rect_filled(
                        ui.available_rect_before_wrap(),
                        0.,
                        Color32::from_rgb(10, 10, 10),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(
                            &mut self.recommendation_history_open,
                            false,
                            "Track Info",
                        );
                        ui.selectable_value(
                            &mut self.recommendation_history_open,
                            true,
                            "Historia rekomendacji",
                        );
                    });
                    ui.separator();
                    if self.recommendation_history_open {
                        self.recommendation_history_ui(ui);
                    } else {
                        self.track_preview(ui);
                    }
                }
            }
        });
    }
    fn advance_recommendation(&mut self, backwards: bool) {
        self.recommendation_selected = self
            .recommendation_memory
            .next_available(&self.recommendation_results, backwards)
            .unwrap_or(usize::MAX);
        self.recommendation_memory.clear_current();
        self.recommendation_advanced = Some(Instant::now());
        self.recommendation_updated = None;
    }
    fn recommendations_ui(&mut self, ui: &mut egui::Ui) {
        if !self.settings.live_recommendations {
            self.recommendation_advanced = None;
            self.recommendation_memory.clear_current();
            self.recommendation_results.clear();
            self.recommendation_restart_at = None;
            self.recommendation_updated = None;
            return;
        }
        let mut playing = Vec::new();
        let mut loaded = std::collections::BTreeSet::new();
        let mut context = String::new();
        for (i, d) in self
            .snapshot
            .decks
            .iter()
            .take(self.settings.deck_count)
            .enumerate()
        {
            if let Some(track) = &d.track {
                if let Some(path) = &track.path {
                    loaded.insert(path.clone());
                }
                context.push_str(&format!(
                    "{i}:{}:{}:{:.2}|",
                    track.id,
                    d.playing,
                    d.bpm * d.rate
                ));
                if d.playing {
                    let mut entry = track
                        .path
                        .as_ref()
                        .and_then(|p| self.catalog.entries.get(p))
                        .cloned()
                        .unwrap_or_default();
                    entry.bpm = Some(d.bpm * d.rate);
                    playing.push(entry);
                }
            }
        }
        if context != self.recommendation_context {
            self.recommendation_restart_at = None;
            self.recommendation_memory.clear_current();
            self.recommendation_results.clear();
            self.recommendation_selected = 0;
            self.recommendation_advanced = None;
            self.recommendation_updated = None;
            self.recommendation_context = context.clone();
        }
        if self
            .recommendation_restart_at
            .is_some_and(|time| time.elapsed() >= Duration::from_secs(4))
        {
            self.recommendation_stats.restarts += 1;
            self.recommendation_memory = Default::default();
            self.recommendation_results.clear();
            self.recommendation_selected = 0;
            self.recommendation_advanced = None;
            self.recommendation_updated = None;
            self.recommendation_restart_at = None;
        }
        if let Some((job_context, rx)) = &self.recommendation_job {
            if let Ok(batch) = rx.try_recv() {
                self.recommendation_stats
                    .batch(&batch, job_context != &context);
                if job_context == &context {
                    let mut results = batch.results;
                    if batch.restart {
                        self.recommendation_restart_at = Some(Instant::now());
                    }
                    // Also filter on receipt: an in-flight ranking can predate a click.
                    self.recommendation_memory.filter(&mut results);
                    let shown = self
                        .recommendation_results
                        .get(self.recommendation_selected)
                        .map(|s| s.path.clone());
                    self.recommendation_results = results;
                    if let Some(index) = shown.and_then(|path| {
                        self.recommendation_results
                            .iter()
                            .position(|s| s.path == path)
                    }) {
                        self.recommendation_selected = index;
                    }
                    self.recommendation_selected = self
                        .recommendation_selected
                        .min(self.recommendation_results.len().saturating_sub(1));
                }
                self.recommendation_job = None;
            }
        }
        if !playing.is_empty()
            && self.recommendation_restart_at.is_none()
            && self.recommendation_job.is_none()
            && self
                .recommendation_updated
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(3))
        {
            let excluded = self.recommendation_memory.exclusions();
            let entries: Vec<_> = self.catalog.entries.values().cloned().collect();
            let (tx, rx) = bounded(1);
            std::thread::spawn(move || {
                let _ = tx.send(prospekt_dj::recommendations::rank_session(
                    &entries, &playing, &loaded, &excluded,
                ));
            });
            self.recommendation_job = Some((context, rx));
            self.recommendation_updated = Some(Instant::now());
        }
        if !self.recommendation_results.is_empty() {
            let elapsed = self
                .recommendation_advanced
                .get_or_insert_with(Instant::now)
                .elapsed();
            if elapsed
                >= Duration::from_secs(u64::from(
                    self.settings.recommendation_interval_seconds.clamp(1, 3600),
                ))
            {
                self.advance_recommendation(false);
            }
        }
        let red = Color32::from_rgb(255, 35, 48);
        let bold = |text: String| {
            RichText::new(text)
                .family(egui::FontFamily::Name("Arial Bold".into()))
                .color(red)
                .size(16.)
        };
        ui.horizontal(|ui| {
            let can_advance = self.recommendation_memory.next_available(&self.recommendation_results, false).is_some();
            if ui.add_enabled(can_advance, egui::Button::new(bold("◀◀◀".into())).frame(false)).clicked() {
                self.advance_recommendation(true);
            }
            let suggestion = self.recommendation_results.get(self.recommendation_selected).cloned();
            let label = suggestion.as_ref().and_then(|s| self.catalog.entries.get(&s.path)).map(|e| format!("Rekomendacje na żywo: • {} — {}", e.artist, e.title)).unwrap_or_else(|| {
                if !self.snapshot.decks.iter().any(|d| d.playing) { "Rekomendacje: uruchom utwór na deku".into() }
                else if self.recommendation_restart_at.is_some() { "Rekomendacje: rozpoczynam generowanie propozycji od nowa…".into() }
                else if self.recommendation_job.is_some() { "Rekomendacje: analiza całej biblioteki…".into() }
                else { "Rekomendacje: brak dopasowań — uzupełnij BPM, tonację lub gatunek".into() }
            });
            let load_width = if suggestion.is_some() { self.settings.deck_count as f32 * 38. + 8. } else { 0. };
            let response = ui.add_sized([ (ui.available_width()-68.-load_width).max(40.), 30. ], egui::Label::new(bold(label)).truncate().sense(Sense::click()));
            if let Some(s) = suggestion {
                if self.recommendation_memory.remember(s.path.clone()) {
                    let entry = self.catalog.entries.get(&s.path);
                    if self.settings.save_recommendation_history {
                    self.recommendation_log.record(format!(
                        "{} — {} | Dopasowanie: {:.2} | {} | Plik: {}",
                        entry.map(|e| e.artist.as_str()).unwrap_or(""),
                        entry.map(|e| e.title.as_str()).unwrap_or(""),
                        s.score, s.reason, s.path.display(),
                    ));
                    }
                    self.recommendation_stats.presented(prospekt_dj::recommendations::Presentation {
                        at: chrono::Local::now().format("%H:%M:%S").to_string(),
                        path: s.path.clone(),
                        title: entry.map(|e| e.title.clone()).unwrap_or_else(|| s.path.display().to_string()),
                        artist: entry.map(|e| e.artist.clone()).unwrap_or_default(),
                        reason: s.reason.clone(), score: s.score,
                    });
                }
                if response.clicked() {
                    if let Some(index) = self.library.iter().position(|p| p == &s.path) {
                        self.selected = index;
                        self.scope = Scope::All;
                        self.search.clear();
                        self.browser_dirty = true;
                        self.scroll_selected = true;
                    }
                }
                response.on_hover_text(format!("{}\nKliknij, aby wskazać w bibliotece. Dopasowanie lokalne na podstawie BPM, tonacji i gatunku; brakujące dane nie są zgadywane.", s.reason));
                for (deck, accent) in ACCENTS.iter().copied().enumerate().take(self.settings.deck_count) {
                    let button = egui::Button::new(
                        RichText::new(letter(deck)).size(14.)
                            .family(egui::FontFamily::Name("Arial Bold".into()))
                            .color(accent),
                    ).min_size(Vec2::new(30., 28.)).corner_radius(prospekt_dj::theme::radius());
                    if ui.add_enabled(!self.loading[deck], button)
                        .on_hover_text(format!("Załaduj rekomendowany utwór na kanał {}", letter(deck)))
                        .clicked() {
                        self.load_manual(deck, s.path.clone());
                    }
                }
            }
            if ui.add_enabled(can_advance, egui::Button::new(bold("▶▶▶".into())).frame(false)).clicked() {
                self.advance_recommendation(false);
            }
        });
    }
    fn recommendation_history_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("recommendation-history-sidebar")
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Ostatnie 10 rekomendacji")
                        .family(egui::FontFamily::Name("Arial Bold".into()))
                        .size(12.),
                );
                if self.recommendation_stats.recent.is_empty() {
                    ui.label("Brak rekomendacji w tej sesji.");
                }
                egui::ScrollArea::vertical()
                    .id_salt("recommendation-recent-list")
                    .max_height(200.)
                    .show(ui, |ui| {
                        let recent = self.recommendation_stats.recent.clone();
                        for (index, item) in recent.iter().enumerate() {
                            ui.label(
                                RichText::new(format!("{}. {}", index + 1, item.at))
                                    .small()
                                    .weak(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&item.title)
                                        .family(egui::FontFamily::Name("Arial Bold".into()))
                                        .size(11.),
                                )
                                .wrap(),
                            )
                            .on_hover_text(format!(
                                "{}\n{}\nDopasowanie: {:.1} pkt",
                                item.path.display(),
                                item.reason,
                                item.score
                            ));
                            ui.add(egui::Label::new(RichText::new(&item.artist).size(10.)).wrap());
                            ui.horizontal(|ui| {
                                let available = item.path.is_file();
                                for deck in [0, 1, 3] {
                                    if deck >= self.settings.deck_count {
                                        continue;
                                    }
                                    if ui
                                        .add_enabled(
                                            available && !self.loading[deck],
                                            egui::Button::new(
                                                RichText::new(letter(deck)).color(ACCENTS[deck]),
                                            )
                                            .min_size(Vec2::new(28., 22.))
                                            .corner_radius(prospekt_dj::theme::radius()),
                                        )
                                        .on_hover_text(if available {
                                            format!("Załaduj na dek {}", letter(deck))
                                        } else {
                                            "Plik utworu jest niedostępny".into()
                                        })
                                        .clicked()
                                    {
                                        self.load_manual(deck, item.path.clone());
                                    }
                                }
                            });
                            ui.separator();
                        }
                    });
                ui.add_space(4.);
                egui::CollapsingHeader::new(
                    RichText::new("STATYSTYKI GENEROWANIA")
                        .family(egui::FontFamily::Name("Arial Bold".into()))
                        .color(Color32::from_rgb(255, 35, 48))
                        .size(12.),
                )
                .id_salt("recommendation-statistics-expanded")
                .default_open(false)
                .show(ui, |ui| {
                    let stats = &self.recommendation_stats;
                    egui::Grid::new("recommendation-statistics")
                        .striped(true)
                        .spacing(Vec2::new(6., 5.))
                        .show(ui, |ui| {
                            for (label, value) in [
                                ("Generacje", stats.batches.to_string()),
                                ("Nieaktualne wyniki", stats.discarded.to_string()),
                                ("Analizowane wpisy łącznie", stats.scanned.to_string()),
                                ("Ostatnia pula", stats.last_candidates.to_string()),
                                ("Ostatni czas", format!("{:.1} ms", stats.last_ms)),
                                ("Średni czas", format!("{:.1} ms", stats.average_ms())),
                                ("Wyświetlenia", stats.presentations.to_string()),
                                ("Różne utwory", stats.unique_count().to_string()),
                                ("Cykl", (stats.restarts + 1).to_string()),
                                (
                                    "Odstęp",
                                    format!("{} s", self.settings.recommendation_interval_seconds),
                                ),
                            ] {
                                ui.add_sized(
                                    [130., 18.],
                                    egui::Label::new(RichText::new(label).size(10.)).wrap(),
                                );
                                ui.label(RichText::new(value).size(10.));
                                ui.end_row();
                            }
                        });
                });
                ui.label(
                    RichText::new("Historia i statystyki dotyczą bieżącej sesji programu.")
                        .small()
                        .weak(),
                );
            });
    }
    fn history_table(&mut self, ui: &mut egui::Ui, id: &str) {
        let label = self
            .history
            .sessions
            .iter()
            .find(|session| session.id == id)
            .map(|session| session.label.as_str())
            .unwrap_or(id);
        ui.heading(format!("Historia grania — {label}"));
        ui.label(
            RichText::new(
                settings::config_path()
                    .with_file_name("history")
                    .join(id)
                    .display()
                    .to_string(),
            )
            .small()
            .weak(),
        );
        if let Some(error) = &self.history_error {
            ui.colored_label(Color32::RED, error);
        }
        if self.history_loading {
            ui.spinner();
        }
        let query = self.search.to_lowercase();
        let events: Vec<_> = self
            .history
            .events
            .iter()
            .filter(|event| {
                let text = format!(
                    "{} {} {}",
                    event.title,
                    event.artist,
                    event
                        .path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                )
                .to_lowercase();
                query.split_whitespace().all(|word| text.contains(word))
            })
            .cloned()
            .collect();
        ui.label(format!(
            "{} odtworzeń · A–D · kolejność chronologiczna",
            events.len()
        ));
        let height = ui.available_height().max(40.);
        egui::ScrollArea::both()
            .id_salt("session-history-table")
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(640.);
                egui::Grid::new(("history-rows", id))
                    .striped(true)
                    .spacing(Vec2::new(10., 5.))
                    .show(ui, |ui| {
                        for name in ["#", "CZAS", "DEK", "TYTUŁ", "WYKONAWCA", "ZAŁADUJ"] {
                            ui.strong(name);
                        }
                        ui.end_row();
                        for (index, event) in events.iter().enumerate() {
                            ui.label((index + 1).to_string());
                            let timestamp = chrono::DateTime::parse_from_rfc3339(&event.played_at)
                                .map(|time| time.format("%H:%M:%S").to_string())
                                .unwrap_or_else(|_| event.played_at.clone());
                            ui.label(timestamp);
                            ui.label(letter(event.deck.min(3)));
                            let response = ui.selectable_label(false, &event.title);
                            if response.clicked() {
                                if let Some(index) = event
                                    .path
                                    .as_ref()
                                    .and_then(|path| self.library.iter().position(|p| p == path))
                                {
                                    self.selected = index;
                                }
                            }
                            ui.label(&event.artist);
                            ui.horizontal(|ui| {
                                let available =
                                    event.path.as_ref().is_some_and(|path| path.is_file());
                                for deck in 0..self.settings.deck_count {
                                    if ui
                                        .add_enabled(
                                            available && !self.loading[deck],
                                            egui::Button::new(letter(deck)),
                                        )
                                        .clicked()
                                    {
                                        if let Some(path) = &event.path {
                                            self.load_manual(deck, path.clone());
                                        }
                                    }
                                }
                                if !available {
                                    ui.label(
                                        RichText::new("Plik niedostępny / demo").small().weak(),
                                    );
                                }
                            });
                            ui.end_row();
                        }
                    });
                if events.is_empty() {
                    ui.label("Brak odtworzeń w tej sesji lub brak wyników wyszukiwania.");
                }
            });
    }
    fn waveform_detail_ui(&mut self, ctx: &egui::Context) {
        let Some(path) = self.waveform_detail.clone() else {
            return;
        };
        let Some(entry) = self.catalog.entries.get(&path).cloned() else {
            self.waveform_detail = None;
            return;
        };
        self.library_data.request(&path, entry.first_seen);
        self.metadata_preview.request(&path);
        let record = self.library_data.records.get(&path).cloned();
        let mut open = true;
        egui::Window::new(format!("Podgląd całego utworu — {}", entry.title))
            .id(egui::Id::new("full-track-waveform"))
            .open(&mut open)
            .default_size(Vec2::new(1050., 730.))
            .min_width(700.)
            .max_height((ctx.screen_rect().height() - 60.).max(400.))
            .default_pos(egui::pos2(100., 80.))
            .resizable(true)
            .show(ctx, |ui| {
                let width = ui.available_width();
                let height = 190.;
                let wave =
                    egui::Rect::from_min_size(ui.next_widget_position(), Vec2::new(width, height));
                preview_wave(
                    ui,
                    record.as_ref(),
                    &self.settings.wave_style,
                    width,
                    height,
                );
                let hot = self
                    .snapshot
                    .decks
                    .iter()
                    .find(|deck| {
                        deck.track.as_ref().and_then(|track| track.path.as_ref()) == Some(&path)
                    })
                    .map(|deck| deck.hot)
                    .or_else(|| record.as_ref().map(|r| r.hot))
                    .unwrap_or([None; 8]);
                let duration = record
                    .as_ref()
                    .map(|r| r.analysis.duration)
                    .or(entry.duration)
                    .unwrap_or(0.);
                if duration > 0. {
                    for (slot, position) in hot.iter().enumerate() {
                        if let Some(position) =
                            position.filter(|p| p.is_finite() && *p >= 0. && *p <= duration)
                        {
                            let x = wave.left() + (position / duration) as f32 * width;
                            let color = cue_color(slot);
                            ui.painter().line_segment(
                                [egui::pos2(x, wave.top()), egui::pos2(x, wave.bottom())],
                                Stroke::new(3_f32, color),
                            );
                            let y = wave.top() + 4. + (slot % 2) as f32 * 30.;
                            let flag = egui::Rect::from_min_size(
                                egui::pos2(x.min(wave.right() - 110.), y),
                                Vec2::new(110., 26.),
                            );
                            ui.painter().rect_filled(flag, 0., color);
                            ui.painter().text(
                                flag.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("CUE {} · {}", slot + 1, short_time(position)),
                                FontId::new(13., egui::FontFamily::Name("Arial Bold".into())),
                                Color32::BLACK,
                            );
                        }
                    }
                }
                if record.is_none() {
                    ui.label("Odczytywanie pełnego przebiegu utworu…");
                }
                ui.separator();
                let info_height = (ui.available_height() - 50.).clamp(230., 440.);
                ui.horizontal_top(|ui| {
                    ui.set_max_height(info_height);
                    ui.allocate_ui_with_layout(
                        Vec2::new(340., info_height),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(340.);
                            ui.push_id("detail-track-info", |ui| {
                                self.track_preview_entry(ui, entry.clone())
                            });
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), info_height),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            ui.strong("INFORMACJE O UTWORZE");
                            egui::ScrollArea::vertical()
                                .id_salt("full-wave-tags")
                                .max_height(info_height - 25.)
                                .show(ui, |ui| {
                                    ui.strong("LOKALIZACJA PLIKU");
                                    ui.add(egui::Label::new(path.display().to_string()).wrap());
                                    if ui.small_button("Kopiuj pełną ścieżkę").clicked() { ui.ctx().copy_text(path.display().to_string()); }
                                    ui.label(format!("Folder: {}", path.parent().unwrap_or(&path).display()));
                                    ui.label(format!("Rozmiar: {} bajtów", entry.bytes));
                                    ui.label(if entry.missing { "Stan: brak pliku w ostatnim skanowaniu" } else { "Stan: plik dostępny w ostatnim skanowaniu" });
                                    let date = |stamp: i64| chrono::DateTime::from_timestamp(stamp, 0)
                                        .map(|date| date.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
                                        .unwrap_or_else(|| "Brak danych".into());
                                    if entry.modified > 0 { ui.label(format!("Modyfikacja pliku: {}", date(entry.modified as i64))); }
                                    ui.separator();
                                    ui.strong("ODTWARZANIE I HISTORIA");
                                    ui.label(format!("Grany w tej sesji: {}", if self.snapshot.session_played.contains(&path) { "tak" } else { "nie" }));
                                    ui.label(format!("Odsłuch poza dekami: {}", if self.preview_path.as_ref() == Some(&path) && self.snapshot.preview_playing { "aktywny (PHONES/PFL)" } else { "nieaktywny" }));
                                    let mut loaded = false;
                                    for (deck, state) in self.snapshot.decks.iter().enumerate() {
                                        if state.track.as_ref().and_then(|track| track.path.as_ref()) == Some(&path) {
                                            loaded = true;
                                            ui.label(format!("Dek {}: {} · pozycja {} · tempo {:.1} BPM · {:+.1}%", letter(deck),
                                                if state.playing { "odtwarzanie" } else { "zatrzymany" }, time(state.position), state.bpm * state.rate, (state.rate - 1.) * 100.));
                                            if let Some((start, end)) = state.loop_range { ui.label(format!("Pętla: {} – {}", time(start), time(end))); }
                                            if state.reverse { ui.label("Odtwarzanie wstecz / slip reverse"); }
                                        }
                                    }
                                    if !loaded { ui.label("Utwór nie jest załadowany na żaden dek"); }
                                    if let Some(record) = &record {
                                        ui.label(format!("Liczba odtworzeń lokalnych: {}", record.play_count));
                                        for (label, value) in [("Pierwsze odtworzenie", record.first_play), ("Ostatnie odtworzenie", record.last_play), ("Odtworzenie przed tą sesją", record.previous_play)] {
                                            ui.label(format!("{label}: {}", value.map(date).unwrap_or_else(|| "Brak danych".into())));
                                        }
                                        ui.label(format!("Pierwsze wykrycie w bibliotece: {}", if record.user.first_seen > 0 { date(record.user.first_seen) } else { "Brak danych".into() }));
                                        ui.label(format!("Utworzenie pliku: {}", record.user.created.map(date).unwrap_or_else(|| "Brak danych".into())));
                                        for (source, imported) in &record.sources {
                                            ui.label(format!("Historia importowana: {source}"));
                                            ui.label(format!("Odtworzenia: {} · pierwsze: {} · ostatnie: {}", imported.play_count.map(|count| count.to_string()).unwrap_or_else(|| "Brak danych".into()),
                                                imported.first_play.as_deref().unwrap_or("Brak danych"), imported.last_play.as_deref().unwrap_or("Brak danych")));
                                        }
                                    } else { ui.label("Odczytywanie historii lokalnej…"); }
                                    ui.separator();
                                    ui.strong("TAGI ZAPISANE W PLIKU");
                                    match self.metadata_preview.entries.get(&path) {
                                        Some(Ok(rows)) => { for row in rows { ui.add(egui::Label::new(row).wrap()); } }
                                        Some(Err(error)) => { ui.label(error); }
                                        None => { ui.spinner(); }
                                    }
                                });
                        },
                    );
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong("Załaduj utwór na dek:");
                    for (deck, color) in ACCENTS.iter().enumerate().take(self.settings.deck_count) {
                        if ui
                            .add_enabled(
                                !entry.missing,
                                egui::Button::new(
                                    RichText::new(letter(deck)).strong().color(*color),
                                )
                                .corner_radius(prospekt_dj::theme::radius())
                                .min_size(Vec2::new(55., 28.)),
                            )
                            .clicked()
                        {
                            self.load_manual(deck, path.clone());
                        }
                    }
                });
            });
        if !open {
            self.waveform_detail = None;
        }
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
        self.track_preview_entry(ui, entry);
    }
    fn track_preview_entry(&mut self, ui: &mut egui::Ui, entry: Entry) {
        egui::ScrollArea::vertical()
            .id_salt("track-info-scroll")
            .max_height(ui.available_height().max(100.))
            .show(ui, |ui| {
                self.artwork.request(&entry);
                let key = prospekt_dj::artwork::identity(&entry);
                ui.label(RichText::new("TRACK INFO").small().weak());
                if !self.library_data.records.contains_key(&entry.path) {
                    self.library_data.request(&entry.path, entry.first_seen);
                }
                let record = self.library_data.records.get(&entry.path).cloned();
                let hot_cues = record
                    .as_ref()
                    .map(|r| r.hot)
                    .or_else(|| {
                        self.snapshot
                            .decks
                            .iter()
                            .find(|d| {
                                d.track.as_ref().and_then(|t| t.path.as_ref()) == Some(&entry.path)
                            })
                            .map(|d| d.hot)
                    })
                    .unwrap_or([None; 8]);
                if self
                    .preview_path
                    .as_ref()
                    .is_some_and(|path| path != &entry.path)
                {
                    self.stop_preview();
                }
                let preview_strip = ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    let playing = self.preview_path.as_ref() == Some(&entry.path)
                        && (self.snapshot.preview_playing || self.preview_loading);
                    if ui
                        .add_enabled(
                            self.runtime.is_some(),
                            egui::Button::new(if playing { "■" } else { "▶" })
                                .fill(Color32::from_rgb(20, 130, 35))
                                .stroke(Stroke::NONE)
                                .corner_radius(prospekt_dj::theme::radius())
                                .min_size(Vec2::new(24., 24.)),
                        )
                        .on_hover_text("Odsłuch utworu na wyjściu PHONES — bez ładowania na dek")
                        .clicked()
                    {
                        if playing {
                            self.stop_preview();
                        } else {
                            self.start_preview(entry.path.clone());
                        }
                    }
                    let rect = egui::Rect::from_min_size(
                        ui.next_widget_position(),
                        Vec2::new(ui.available_width(), 24.),
                    );
                    preview_wave(
                        ui,
                        record.as_ref(),
                        &self.settings.wave_style,
                        rect.width(),
                        24.,
                    );
                    let response =
                        ui.interact(rect, ui.id().with("track-preview-seek"), Sense::click());
                    if playing {
                        if let Some(duration) = entry.duration.filter(|d| *d > 0.) {
                            let x = rect.left()
                                + (self.snapshot.preview_position / duration).clamp(0., 1.) as f32
                                    * rect.width();
                            ui.painter().line_segment(
                                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                                Stroke::new(2_f32, Color32::WHITE),
                            );
                            if response.clicked() {
                                if let Some(pointer) = response.interact_pointer_pos() {
                                    self.send(Command::PreviewSeek(
                                        ((pointer.x - rect.left()) / rect.width()) as f64
                                            * duration,
                                    ));
                                }
                            }
                        }
                    }
                });
                ui.painter().rect_stroke(
                    preview_strip.response.rect, 0.,
                    Stroke::new(1_f32, Color32::from_rgb(40, 125, 155)),
                    egui::StrokeKind::Inside,
                );
                if self.snapshot.preview_playing && self.preview_path.as_ref() == Some(&entry.path) {
                    ui.label(RichText::new("ODTWARZANIE POZA DEKAMI").strong().color(Color32::LIGHT_GREEN));
                }
                ui.label(RichText::new("Odsłuch: PHONES / PFL").small().weak())
                    .on_hover_text("Wybierz urządzenie PFL w ustawieniach Audio. Podgląd nie jest wysyłany na MASTER.");
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
                ui.add(
                    egui::Label::new(
                        RichText::new(&entry.title)
                            .family(egui::FontFamily::Name("Arial Bold".into()))
                            .size(13.),
                    )
                    .truncate(),
                )
                .on_hover_text(&entry.title);
                ui.add(egui::Label::new(&entry.artist).truncate())
                    .on_hover_text(&entry.artist);
                ui.label(
                    RichText::new(format!(
                        "{} · {} BPM",
                        entry.duration.map(short_time).unwrap_or_else(|| "—".into()),
                        entry
                            .bpm
                            .map(|b| format!("{b:.1}"))
                            .unwrap_or_else(|| "—".into()),
                    ))
                    .small()
                    .color(ACCENTS[0]),
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
                ui.label(
                    RichText::new(format!("KEY: {}", if key.is_empty() { "—" } else { &key }))
                        .family(egui::FontFamily::Name("Arial Bold".into()))
                        .strong()
                        .color(color),
                );
                ui.label(format!(
                    "Year: {}",
                    entry
                        .year
                        .map(|y| y.to_string())
                        .unwrap_or_else(|| "Brak danych".into())
                ));
                for (label, value) in [
                    ("Album", &entry.album),
                    ("Genre", &entry.genre),
                    ("Version", &entry.version),
                ] {
                    if !value.is_empty() {
                        ui.add(egui::Label::new(format!("{label}: {value}")).wrap());
                    }
                }
                if let Some(record) = record {
                    let mut user = record.user.clone();
                    ui.horizontal(|ui| {
                        for stars in 1..=5 {
                            if ui
                                .small_button(
                                    RichText::new(if user.rating >= stars { "★" } else { "☆" })
                                        .color(Color32::from_rgb(255, 215, 0)),
                                )
                                .clicked()
                            {
                                user.rating = if user.rating == stars { 0 } else { stars };
                            }
                        }
                    });
                    ui.label("Komentarz");
                    let output = egui::TextEdit::multiline(&mut user.comment)
                        .id_salt(("track-comment", &entry.path))
                        .desired_rows(2)
                        .desired_width(170.)
                        .show(ui);
                    let cursor = output
                        .cursor_range
                        .map(|range| range.as_sorted_char_range())
                        .or_else(|| {
                            output.state.cursor.char_range().map(|range| {
                                let [start, end] = range.sorted();
                                start.index..end.index
                            })
                        })
                        .unwrap_or_else(|| {
                            let end = user.comment.chars().count();
                            end..end
                        });
                    ui.push_id(("comment-emoji", &entry.path), |ui| {
                        ui.menu_button("☺ Emoji", |ui| {
                            egui::Grid::new("comment-emoji-grid")
                                .num_columns(5)
                                .show(ui, |ui| {
                                    for (i, emoji) in
                                        prospekt_dj::music_view::EMOJI.iter().enumerate()
                                    {
                                        if self.emoji.button(ui, emoji).clicked() {
                                            let cursor = prospekt_dj::music_view::insert_unicode(
                                                &mut user.comment,
                                                cursor.clone(),
                                                emoji,
                                            );
                                            let mut state = output.state.clone();
                                            state.cursor.set_char_range(Some(
                                                egui::text::CCursorRange::one(
                                                    egui::text::CCursor::new(cursor),
                                                ),
                                            ));
                                            state.store(ui.ctx(), output.response.id);
                                            output.response.request_focus();
                                            ui.close_menu();
                                        }
                                        if i % 5 == 4 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                    });
                    self.emoji.preview(ui, &user.comment, 170., 3);
                    let date = |t: i64| {
                        if t <= 0 {
                            return "Brak danych".into();
                        }
                        chrono::DateTime::from_timestamp(t, 0)
                            .map(|t| {
                                t.with_timezone(&chrono::Local)
                                    .format("%Y-%m-%d %H:%M")
                                    .to_string()
                            })
                            .unwrap_or_else(|| "Brak danych".into())
                    };
                    ui.label(
                        RichText::new(format!("Pierwsze wykrycie: {}", date(user.first_seen)))
                            .size(9.),
                    );
                    ui.label(
                        RichText::new(format!(
                            "Utworzenie pliku: {}",
                            user.created
                                .map(date)
                                .unwrap_or_else(|| "Brak danych".into())
                        ))
                        .size(9.),
                    );
                    ui.label(
                        RichText::new(format!("Odtworzenia lokalne: {}", record.play_count))
                            .size(10.),
                    );
                    for (label, time) in [
                        ("Pierwsze lokalne", record.first_play),
                        ("Ostatnie lokalne", record.last_play),
                        ("Przed tą sesją", record.previous_play),
                    ] {
                        ui.label(
                            RichText::new(format!(
                                "{label}: {}",
                                time.map(date).unwrap_or_else(|| "Brak danych".into())
                            ))
                            .size(9.),
                        );
                    }
                    if !record.sources.is_empty() {
                        ui.collapsing("Dane z importu (oddzielne źródła)", |ui| {
                            for (source, imported) in &record.sources {
                                ui.label(RichText::new(source).size(9.));
                                for (label, value) in [
                                    ("Dodano", imported.added.as_deref()),
                                    ("Pierwsze odtworzenie", imported.first_play.as_deref()),
                                    ("Ostatnie odtworzenie", imported.last_play.as_deref()),
                                ] {
                                    ui.label(format!(
                                        "{label}: {}",
                                        value.unwrap_or("Brak danych")
                                    ));
                                }
                                ui.label(format!(
                                    "Licznik źródła: {}",
                                    imported
                                        .play_count
                                        .map(|v| v.to_string())
                                        .unwrap_or_else(|| "Brak danych".into())
                                ));
                            }
                        });
                    }
                    if (user.rating != record.user.rating || user.comment != record.user.comment)
                        && !self.library_data.edit(&entry.path, user)
                    {
                        self.status = "Database queue busy — changes not saved".into();
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
                ui.separator();
                ui.label(
                    RichText::new("PUNKTY CUE")
                        .family(egui::FontFamily::Name("Arial Bold".into()))
                        .size(11.),
                );
                egui::Grid::new(("track-info-cues", &entry.path))
                    .num_columns(2)
                    .spacing(Vec2::new(8., 4.))
                    .show(ui, |ui| {
                        for (slot, position) in hot_cues.iter().enumerate() {
                            let color = cue_color(slot);
                            ui.label(
                                RichText::new(format!("CUE {}", slot + 1))
                                    .family(egui::FontFamily::Name("Arial Bold".into()))
                                    .size(11.)
                                    .color(color),
                            );
                            egui::Frame::new()
                                .fill(Color32::from_gray(26))
                                .stroke(Stroke::new(1_f32, color))
                                .corner_radius(prospekt_dj::theme::radius())
                                .inner_margin(egui::Margin::symmetric(6, 3))
                                .show(ui, |ui| {
                                    ui.set_min_width(76.);
                                    let text = position
                                        .filter(|t| t.is_finite() && *t >= 0.)
                                        .map(time)
                                        .unwrap_or_else(|| "—".into());
                                    ui.label(
                                        RichText::new(text).monospace().size(12.).color(color),
                                    );
                                });
                            ui.end_row();
                        }
                    });
            });
    }
    fn browser_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.set_max_width(self.settings.tree_width);
        for (scope, label) in [
            (Scope::All, tr("Wszystkie utwory")),
            (Scope::Favorites, tr("Ulubione")),
            (Scope::Recent, "Ostatnio odtwarzane".into()),
            (Scope::Frequent, "Najczęściej odtwarzane".into()),
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
            let is_favorites = scope == Scope::Favorites;
            let response = if matches!(scope, Scope::Loaded | Scope::Favorites) {
                tree_choice(
                    ui,
                    &mut self.emoji,
                    &mut self.scope,
                    scope.clone(),
                    if scope == Scope::Favorites {
                        format!("ULUBIONE  {count}")
                    } else {
                        format!("NA DEKACH  {count}")
                    },
                )
            } else {
                ui.selectable_value(&mut self.scope, scope, format!("{label}  {count}"))
            };
            if response.changed() {
                self.browser_dirty = true;
            }
            if is_favorites {
                self.favorite_folders_ui(ui);
            }
        }
        ui.add_space(8.);
        ui.collapsing("Historia grania", |ui| {
            if ui.button("Odśwież historię").clicked() {
                self.history_poll = Instant::now() - Duration::from_secs(3);
            }
            let sessions = self.history.sessions.clone();
            if sessions.is_empty() {
                ui.label(RichText::new("Brak zapisanych sesji").small().weak());
            }
            for session in sessions {
                if ui
                    .selectable_value(
                        &mut self.scope,
                        Scope::History(session.id.clone()),
                        &session.label,
                    )
                    .on_hover_text(format!("Sesja: {}", session.created_at))
                    .changed()
                {
                    self.history.events.clear();
                    self.history_error = None;
                    self.history_poll = Instant::now() - Duration::from_secs(3);
                    self.browser_dirty = true;
                }
            }
        });
        ui.add_space(10.);
        ui.collapsing("Oceny", |ui| {
            for rating in 0..=5 {
                let count = self
                    .catalog
                    .entries
                    .values()
                    .filter(|e| e.rating == rating)
                    .count();
                let label = if rating == 0 {
                    "Bez oceny".into()
                } else {
                    "★".repeat(rating as usize)
                };
                if tree_choice(
                    ui,
                    &mut self.emoji,
                    &mut self.scope,
                    Scope::Rating(rating),
                    format!("{label} {count}"),
                )
                .changed()
                {
                    self.browser_dirty = true;
                }
            }
        });
        ui.label(RichText::new(tr("BANGER BOX")).small().weak());
        self.crate_nodes(ui, None, 0);
        if ui.button(format!("+ {}", tr("Nowy BANGER BOX"))).clicked() {
            self.crate_editor = Some(CrateEditor {
                id: None,
                parent: None,
                name: String::new(),
                cursor: 0..0,
            });
        }
        ui.add_space(10.);
        ui.label(RichText::new("CHMURA").small().weak());
        ui.horizontal(|ui| {
            ui.add(egui::Image::new((self.brand.drive.id(), Vec2::splat(24.))));
            if ui
                .button(
                    RichText::new("DYSK GOOGLE")
                        .family(egui::FontFamily::Name("Arial Bold".into()))
                        .size(14.)
                        .color(ACCENTS[0]),
                )
                .clicked()
            {
                self.cloud.open = true;
            }
        });
        ui.label(
            RichText::new(if self.cloud.connected {
                "● Połączono"
            } else {
                "Połącz z chmurą…"
            })
            .small()
            .color(if self.cloud.connected {
                Color32::LIGHT_GREEN
            } else {
                Color32::GRAY
            }),
        );
        if self.cloud.connected {
            for file in self.cloud.files.clone().into_iter().take(8) {
                let text = format!("{} {}", if file.folder() { "📁" } else { "🎵" }, file.name);
                if ui
                    .add(egui::Button::new(text).frame(false).truncate())
                    .clicked()
                {
                    self.cloud.open = true;
                    if file.folder() && !self.cloud.busy {
                        self.cloud
                            .breadcrumbs
                            .push((file.id.clone(), file.name.clone()));
                        self.cloud.busy = self.cloud.drive.list(file.id, None);
                    }
                }
            }
        }
        ui.add_space(6.);
        ui.label(RichText::new(tr("FOLDERY")).small().weak());
        let sampler = prospekt_dj::portable::sampler_dir().ok();
        if let Some(path) = &sampler {
            self.folder_node(ui, path, 0);
        }
        if let Some(user) = directories::UserDirs::new() {
            for (label, path) in [
                (
                    "PULPIT",
                    user.desktop_dir()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| user.home_dir().join("Desktop")),
                ),
                (
                    "MOJE DOKUMENTY",
                    user.document_dir()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| user.home_dir().join("Documents")),
                ),
            ] {
                storage_category(ui, label, usize::from(label == "MOJE DOKUMENTY"), |ui| {
                    self.folder_node(ui, &path, 0)
                });
            }
        }
        let volumes = self.storage.volumes.clone();
        for (label, external) in [("DYSKI TWARDE", false), ("URZĄDZENIA ZEWNĘTRZNE", true)] {
            storage_category(ui, label, if external { 3 } else { 2 }, |ui| {
                let mut any = false;
                for volume in volumes.iter().filter(|v| v.external == external) {
                    any = true;
                    self.folder_node(ui, &volume.path, 0);
                }
                if !any {
                    ui.label(RichText::new("Brak nośników").small().weak());
                }
            });
        }
        let roots = self.catalog.roots.clone();
        for root in roots {
            if sampler.as_ref() != Some(&root) {
                self.folder_node(ui, &root, 0);
            }
        }
        ui.add_space(12.);
        ui.label(RichText::new("Upuść pliki lub folder na okno.\nPrawy przycisk: akcje utworu.\nUsuwanie z kolekcji nie usuwa muzyki z dysku.").small().weak());
    }
    fn storage_dialog(&mut self, ctx: &egui::Context) {
        let Some(notice) = self.storage.notices.front().cloned() else {
            return;
        };
        let mut close = false;
        egui::Window::new("PODŁĄCZONO NOŚNIK Z DANYMI")
            .id(egui::Id::new("storage-connected"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Nośnik: {} ({})",
                    notice.volume.name,
                    notice.volume.path.display()
                ));
                ui.label(format!(
                    "Model: {}",
                    if notice.volume.model.is_empty() {
                        "Nieudostępniony przez system"
                    } else {
                        &notice.volume.model
                    }
                ));
                ui.label(format!(
                    "Producent: {}",
                    if notice.volume.vendor.is_empty() {
                        "Nieudostępniony przez system"
                    } else {
                        &notice.volume.vendor
                    }
                ));
                if let Some(counts) = notice.counts {
                    ui.strong(format!("Pliki audio: {}", counts.iter().sum::<u64>()));
                    for (extension, count) in
                        ["MP3", "WAV", "FLAC", "AAC", "OGG"].iter().zip(counts)
                    {
                        ui.label(format!("{extension}: {count}"));
                    }
                    if notice.errors > 0 {
                        ui.label(format!(
                            "Nie można odczytać części nośnika ({} błędów). Wynik jest częściowy.",
                            notice.errors
                        ));
                    }
                } else {
                    ui.spinner();
                    ui.label("Przeszukiwanie nośnika w tle…");
                }
                close = ui.button("OK").clicked();
            });
        if close {
            self.storage.notices.pop_front();
        }
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
                    if ui.button(tr("Nowy zagnieżdżony BANGER BOX")).clicked() {
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
                    if ui.button(tr("Usuń BANGER BOX")).clicked() {
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
    fn favorite_folders_ui(&mut self, ui: &mut egui::Ui) {
        let key = "favorite-folders-section".to_owned();
        let mut open = self.settings.tree_open.contains(&key);
        let gold = Color32::GOLD;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.;
            if ui.small_button(if open { "−" } else { "+" }).clicked() {
                open = !open;
            }
            ui.label(RichText::new("★").size(13.).color(gold));
            let _ = tree_icon(ui, gold);
            if ui
                .add(
                    egui::Label::new(
                        RichText::new("ULUBIONE FOLDERY")
                            .family(egui::FontFamily::Name("Arial Bold".into()))
                            .color(gold)
                            .size(10.),
                    )
                    .sense(Sense::click()),
                )
                .clicked()
            {
                open = !open;
            }
        });
        if open {
            self.settings.tree_open.insert(key);
            let folders: Vec<_> = self.settings.favorite_folders.iter().cloned().collect();
            if folders.is_empty() {
                ui.label(
                    RichText::new("Dodaj folder prawym przyciskiem myszy.")
                        .small()
                        .weak(),
                );
            }
            for folder in folders {
                self.folder_node(ui, &folder, 1);
            }
        } else {
            self.settings.tree_open.remove(&key);
        }
    }
    fn folder_context_menu(&mut self, ui: &mut egui::Ui, path: &PathBuf) {
        self.file_actions_menu(ui, path);
        ui.separator();
        let favorite = self.settings.favorite_folders.contains(path);
        if ui
            .button(if favorite {
                "Usuń folder z ulubionych"
            } else {
                "Dodaj folder do ulubionych"
            })
            .clicked()
        {
            if favorite {
                self.settings.favorite_folders.remove(path);
            } else {
                self.settings.favorite_folders.insert(path.clone());
                self.settings
                    .tree_open
                    .insert("favorite-folders-section".into());
            }
            ui.close_menu();
        }
        ui.menu_button("Kolor ikony folderu", |ui| {
            let current = self.settings.folder_colors.get(path).copied();
            for slot in 0..8 {
                let color = cue_color(slot);
                if ui
                    .selectable_label(
                        current == Some(slot),
                        RichText::new(format!("■  Kolor CUE {}", slot + 1)).color(color),
                    )
                    .clicked()
                {
                    self.settings.folder_colors.insert(path.clone(), slot);
                    ui.close_menu();
                }
            }
            if ui.button("Domyślny kolor").clicked() {
                self.settings.folder_colors.remove(path);
                ui.close_menu();
            }
        });
        ui.separator();
        if ui.button(tr("Importuj folder")).clicked() {
            self.add_paths(vec![path.clone()]);
            ui.close_menu();
        }
        if ui.button(tr("Odśwież folder")).clicked() {
            self.folders.retry(path);
            ui.close_menu();
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
            let color = self
                .settings
                .folder_colors
                .get(path)
                .copied()
                .filter(|slot| *slot < 8)
                .map(cue_color)
                .unwrap_or(Color32::from_gray(168));
            tree_icon(ui, color)
                .on_hover_text(path.display().to_string())
                .context_menu(|ui| self.folder_context_menu(ui, path));
            let response = tree_choice(
                ui,
                &mut self.emoji,
                &mut self.scope,
                Scope::Folder(path.clone()),
                label.to_string(),
            );
            if response.changed() {
                self.browser_dirty = true;
                if prospekt_dj::portable::sampler_dir().ok().as_ref() == Some(path) {
                    self.add_paths(vec![path.clone()]);
                }
            }
            response
                .on_hover_text(path.display().to_string())
                .context_menu(|ui| self.folder_context_menu(ui, path));
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
        egui::Window::new(tr("Nazwa BANGER BOX"))
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
        ui.menu_button("Plik", |ui| self.file_actions_menu(ui, &entry.path));
        ui.add_enabled_ui(!entry.missing, |ui| {
            ui.menu_button("Załaduj na dek", |ui| {
                for (deck, accent) in ACCENTS.iter().enumerate().take(self.settings.deck_count) {
                    if ui
                        .button(RichText::new(letter(deck)).color(*accent))
                        .clicked()
                    {
                        self.load_manual(deck, entry.path.clone());
                        ui.close_menu();
                    }
                }
            });
        });
        ui.separator();
        ui.menu_button(tr("Okładka"), |ui| {
            let key = prospekt_dj::artwork::identity(entry);
            ui.label(
                self.art_status
                    .get(&key)
                    .map(String::as_str)
                    .unwrap_or("Pending"),
            );
            if ui.button("Wczytaj ponownie okładkę").clicked() {
                self.status = if self.artwork.reload(entry) { "Ponowne wczytywanie okładki…".into() }
                    else { "Kolejka okładek zajęta — spróbuj ponownie".into() };
                ui.close_menu();
            }
            if ui.add_enabled(self.settings.auto_cover_download, egui::Button::new("Pobierz okładkę z internetu…"))
                .on_hover_text("Pobieranie okładek można włączyć w ustawieniach. Okładka zapisana w pliku zachowuje pierwszeństwo.")
                .clicked() {
                self.status = if self.artwork.download(entry) { "Wyszukiwanie i pobieranie okładki…".into() }
                    else { "Kolejka okładek zajęta — spróbuj ponownie".into() };
                ui.close_menu();
            }
            ui.separator();
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
        if ui
            .add_enabled(
                !entry.missing && self.index_jobs == 0,
                egui::Button::new("Wczytaj ponownie tagi z pliku"),
            )
            .clicked()
        {
            self.metadata_preview.entries.remove(&entry.path);
            self.index_paths(vec![entry.path.clone()], true);
            self.status = "Ponowne wczytywanie tagów z pliku…".into();
            ui.close_menu();
        }
    }
    fn cover_drag_pointer(&self) -> egui::PointerButton {
        match self.settings.cover_drag_button {
            crate::settings::CoverDragButton::Right => egui::PointerButton::Secondary,
            crate::settings::CoverDragButton::Left => egui::PointerButton::Primary,
        }
    }
    fn cover_drag_drop(&mut self, ctx: &egui::Context) {
        let Some((path, origin, mut moved)) = self.cover_drag.clone() else {
            return;
        };
        let position = ctx.input(|input| input.pointer.interact_pos());
        moved |= position.is_some_and(|point| point.distance(origin) >= 6.);
        if self.settings_open
            || !ctx.input(|input| input.focused)
            || ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.cover_drag = None;
            return;
        }
        self.cover_drag = Some((path.clone(), origin, moved));
        if let Some(point) = position.filter(|_| moved) {
            let target = self
                .deck_drop_rects
                .iter()
                .take(self.settings.deck_count)
                .position(|rect| rect.contains(point));
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("cover-drag-overlay"),
            ));
            if let Some(deck) = target {
                painter.rect_stroke(
                    self.deck_drop_rects[deck],
                    0.,
                    Stroke::new(2_f32, ACCENTS[deck]),
                    egui::StrokeKind::Inside,
                );
            }
            painter.text(
                point + egui::vec2(14., 14.),
                egui::Align2::LEFT_TOP,
                format!(
                    "{} → {}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    target.map(letter).unwrap_or("dek")
                ),
                FontId::proportional(12.),
                Color32::WHITE,
            );
            if ctx.input(|input| input.pointer.button_released(self.cover_drag_pointer())) {
                self.cover_drag = None;
                if let Some(deck) = target {
                    self.load_manual(deck, path);
                }
                return;
            }
        }
        if !ctx.input(|input| input.pointer.button_down(self.cover_drag_pointer())) {
            self.cover_drag = None;
        }
    }
    fn cover_strip(&mut self, ui: &mut egui::Ui) {
        self.cover_reflections.clear();
        self.cover_reflection_left = ui.clip_rect().left().max(ui.cursor().left());
        let rows = self.visible();
        if let Some(entry) = self
            .library
            .get(self.selected)
            .and_then(|p| self.catalog.entries.get(p))
        {
            self.artwork.request(entry);
        }
        let height = if self.settings.cover_reflections {
            72. + 72. / 18.5 + 72. / 20.
        } else {
            72.
        };
        let step = 72.;
        // With the left button a drag starts a cover drag, so it must not scroll the strip.
        let left_drag = self.settings.cover_drag_button == crate::settings::CoverDragButton::Left;
        let mut scroll = egui::ScrollArea::horizontal()
            .id_salt("cover-strip")
            .auto_shrink([false, false])
            .drag_to_scroll(!left_drag)
            .max_height(height + 4.);
        if self.cover_reveal {
            if let Some(n) = rows.iter().position(|i| *i == self.selected) {
                scroll = scroll.horizontal_scroll_offset(
                    (n as f32 * step - ui.available_width() * 0.4).max(0.),
                );
            }
            self.cover_reveal = false;
        }
        ui.scope(|ui| {
            // A regular vertical wheel scrolls this horizontal-only strip.
            // Keep this local: wheel input outside covers retains its own meaning.
            ui.style_mut().always_scroll_the_only_direction = true;
            let output = scroll.show_viewport(ui, |ui, viewport| {
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
                    let drag_button = self.cover_drag_pointer();
                    if !entry.missing
                        && response.contains_pointer()
                        && !self.settings_open
                        && ui.input(|input| input.pointer.button_pressed(drag_button))
                    {
                        if let Some(position) = ui.input(|input| input.pointer.interact_pos()) {
                            self.cover_drag = Some((path.clone(), position, false));
                        }
                    }

                    let animated =
                        self.settings.cover_scroll_animation && !self.settings.reduced_motion;
                    let opacity = if animated {
                        let center = n as f32 * step + 36.;
                        let left = if viewport.left() > 0.5 {
                            ((center - viewport.left()) / 56.).clamp(0., 1.)
                        } else {
                            1.
                        };
                        let right = if viewport.right() < total - 0.5 {
                            ((viewport.right() - center) / 56.).clamp(0., 1.)
                        } else {
                            1.
                        };
                        let t = left.min(right);
                        let target = t * t * (3. - 2. * t);
                        ui.ctx().animate_value_with_time(
                            ui.id().with(("cover-scroll-opacity", &path)),
                            target,
                            0.14,
                        )
                    } else {
                        1.
                    };
                    let mut painter = ui.painter().clone();
                    painter.multiply_opacity(opacity);
                    painter.rect_filled(rect, 0., Color32::from_gray(20));
                    if let Some(art) = self.art_textures.get_mut(&key) {
                        art.used = self.art_tick;
                        let size = art.texture.size_vec2();
                        let fitted = size * (72. / size.x.max(size.y));
                        let target = egui::Rect::from_center_size(rect.center(), fitted);
                        painter.image(
                            art.texture.id(),
                            target,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                            Color32::WHITE,
                        );
                        if self.settings.cover_reflections {
                            // Mirror the bottom edge below the cover; reuse the existing texture.
                            let reflection_height = rect.height() / 20.;
                            let reflection = egui::Rect::from_min_size(
                                egui::pos2(target.left(), rect.bottom() + rect.height() / 18.5),
                                Vec2::new(target.width(), reflection_height),
                            );
                            let uv_top = 1.;
                            let uv_bottom = (1. - reflection_height / target.height()).max(0.);
                            let mut mesh = egui::epaint::Mesh::with_texture(art.texture.id());
                            for (pos, uv, alpha) in [
                                (reflection.left_top(), egui::pos2(0., uv_top), 110),
                                (reflection.right_top(), egui::pos2(1., uv_top), 110),
                                (reflection.left_bottom(), egui::pos2(0., uv_bottom), 0),
                                (reflection.right_bottom(), egui::pos2(1., uv_bottom), 0),
                            ] {
                                mesh.vertices.push(egui::epaint::Vertex {
                                    pos,
                                    uv,
                                    color: Color32::from_white_alpha(alpha),
                                });
                            }
                            mesh.indices.extend_from_slice(&[0, 1, 2, 1, 3, 2]);
                            let mut lower = mesh.clone();
                            for vertex in &mut lower.vertices {
                                vertex.pos.y -= reflection.top();
                                vertex.color = vertex.color.gamma_multiply(opacity);
                            }
                            self.cover_reflections.push(lower);
                            painter.add(egui::Shape::mesh(mesh));
                        }
                    } else {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "♫",
                            FontId::proportional(26.),
                            Color32::from_gray(75),
                        );
                    }
                    if index == self.selected {
                        painter.rect_stroke(
                            rect,
                            0.,
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
                    if response.hovered()
                        && !self.cover_drag.as_ref().is_some_and(|(_, _, moved)| *moved)
                    {
                        self.metadata_preview.request(&entry.path);
                    }
                    let hover_id = response.id.with(("metadata-hover-start", &entry.path));
                    let popup_rect_id = hover_id.with("popup-rect");
                    let over_popup = ui
                        .ctx()
                        .data(|data| data.get_temp::<egui::Rect>(popup_rect_id))
                        .is_some_and(|popup| {
                            ui.input(|input| input.pointer.hover_pos())
                                .is_some_and(|point| {
                                    popup.union(response.rect).expand(5.).contains(point)
                                })
                        });
                    let show_metadata = if (response.hovered() || over_popup)
                        && !self.cover_drag.as_ref().is_some_and(|(_, _, moved)| *moved)
                    {
                        let started = ui.ctx().data_mut(|data| {
                            *data.get_temp_mut_or_insert_with(hover_id, Instant::now)
                        });
                        let elapsed = started.elapsed();
                        let delay = Duration::from_millis(6660);
                        if elapsed < delay {
                            ui.ctx().request_repaint_after(delay - elapsed);
                        }
                        elapsed >= delay
                    } else {
                        ui.ctx().data_mut(|data| {
                            data.remove::<Instant>(hover_id);
                            data.remove::<egui::Rect>(popup_rect_id);
                        });
                        false
                    };
                    if show_metadata {
                        response.show_tooltip_ui(|ui| {
                            ui.set_max_width(480.);
                            ui.set_min_size(Vec2::new(340., 360.));
                            ui.horizontal(|ui| {
                                let (cover, _) =
                                    ui.allocate_exact_size(Vec2::splat(96.), Sense::hover());
                                ui.painter().rect_filled(cover, 0., Color32::from_gray(18));
                                if let Some(art) = self.art_textures.get_mut(&key) {
                                    art.used = self.art_tick;
                                    let dimensions = art.texture.size_vec2();
                                    let fitted =
                                        dimensions * (92. / dimensions.x.max(dimensions.y));
                                    ui.painter().image(
                                        art.texture.id(),
                                        egui::Rect::from_center_size(cover.center(), fitted),
                                        egui::Rect::from_min_max(
                                            egui::Pos2::ZERO,
                                            egui::pos2(1., 1.),
                                        ),
                                        Color32::WHITE,
                                    );
                                } else {
                                    ui.painter().text(
                                        cover.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "♫",
                                        FontId::proportional(30.),
                                        Color32::GRAY,
                                    );
                                }
                                ui.vertical(|ui| {
                                    ui.strong("METADANE PLIKU");
                                    ui.label(&entry.title);
                                    ui.label(&entry.artist);
                                });
                            });
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .max_height(360.)
                                .show(ui, |ui| {
                                    match self.metadata_preview.entries.get(&entry.path) {
                                        Some(Ok(rows)) => {
                                            for row in rows {
                                                ui.add(egui::Label::new(row).wrap());
                                            }
                                        }
                                        Some(Err(error)) => {
                                            ui.label(format!("Nie można odczytać tagów: {error}"));
                                        }
                                        None => {
                                            ui.spinner();
                                            ui.label("Odczytywanie tagów w tle…");
                                            ui.ctx()
                                                .request_repaint_after(Duration::from_millis(100));
                                        }
                                    }
                                });
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label("Załaduj na dek:");
                                for (deck, accent) in
                                    ACCENTS.iter().enumerate().take(self.settings.deck_count)
                                {
                                    if ui
                                        .add_enabled(
                                            !entry.missing,
                                            egui::Button::new(
                                                RichText::new(letter(deck)).strong().color(*accent),
                                            )
                                            .corner_radius(prospekt_dj::theme::radius())
                                            .min_size(Vec2::new(30., 22.)),
                                        )
                                        .clicked()
                                    {
                                        self.load_manual(deck, entry.path.clone());
                                        ui.ctx().data_mut(|data| data.remove::<Instant>(hover_id));
                                    }
                                }
                            });
                            ui.ctx()
                                .data_mut(|data| data.insert_temp(popup_rect_id, ui.min_rect()));
                        });
                    }
                    if !self.cover_drag.as_ref().is_some_and(|(_, _, moved)| *moved) {
                        response.context_menu(|ui| self.art_menu(ui, &entry));
                    }
                }
            });
            let _ = output;
        });
    }
    fn browser_table(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let preview = if self.settings.preview_column {
            100.
        } else {
            0.
        };
        let artist = if width > 650. { 130. } else { 0. };
        let genre = if width > 950. { 90. } else { 0. };
        let version = if width > 1350. { 100. } else { 0. };
        let album = if width > 1100. { 115. } else { 0. };
        let size = if width > 950. { 68. } else { 0. };
        let title = (width
            - 30.
            - 26.
            - 24.
            - 34.
            - preview
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
            (24., "", None),
            (26., "", None),
            (34., "#", None),
            (preview, "PREVIEW", None),
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
            ui.spacing_mut().interact_size.y = 18.;
            ui.spacing_mut().button_padding = egui::vec2(3., 1.);
            let header = egui::Rect::from_min_size(ui.cursor().min, Vec2::new(width, 18.));
            ui.painter()
                .rect_filled(header, 0., Color32::from_rgb(15, 15, 15));
            let mut marker_index = 0;
            for (w, label, sort) in columns {
                if w <= 0. {
                    continue;
                }
                let has_marker = !label.is_empty();
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
                let response = ui.add_sized(
                    [w, 18.],
                    egui::Button::new(RichText::new(text).size(11.).color(Color32::from_gray(190)))
                        .fill(Color32::from_rgb(15, 15, 15))
                        .stroke(Stroke::new(0.5_f32, Color32::BLACK))
                        .corner_radius(prospekt_dj::theme::radius())
                        .min_size(Vec2::ZERO)
                        .frame(true),
                );
                if has_marker {
                    let marker = egui::Rect::from_min_size(
                        egui::pos2(response.rect.left() + 4., response.rect.center().y - 5.),
                        Vec2::new(3., 10.),
                    );
                    ui.painter()
                        .rect_filled(marker, 0., cue_color(marker_index % 8));
                    marker_index += 1;
                }
                if response.clicked() {
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
        let height = ui.available_height().max(40.);
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
        let output = scroll.show_rows(ui, row_height, rows.len(), |ui, range| {
            for visible_index in range {
                let index = rows[visible_index];
                let path = self.library[index].clone();
                let entry = self
                    .catalog
                    .entries
                    .get(&path)
                    .cloned()
                    .unwrap_or_else(|| Entry::placeholder(path.clone()));
                if preview > 0. {
                    self.library_data.request(&path, entry.first_seen);
                }
                ui.push_id(index, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width, row_height), Sense::click());
                    if rect.intersects(ui.clip_rect()) {
                        self.library_last_row_bottom =
                            Some(rect.bottom().min(ui.clip_rect().bottom()));
                    }
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
                    } else if prospekt_dj::theme::classic() {
                        fill
                    } else {
                        // Modern skins: translucent rows let the watermark show behind the list.
                        fill.gamma_multiply(0.45)
                    };
                    ui.painter()
                        .rect_filled(rect, prospekt_dj::theme::radius_f(3.), fill);
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
                    let in_preview = response.interact_pointer_pos().is_some_and(|p| {
                        p.x >= rect.left() + 114. && p.x < rect.left() + 114. + preview
                    });
                    if response.secondary_clicked() && in_preview {
                        self.waveform_detail = Some(path.clone());
                        self.selected = index;
                        self.library_data.request(&path, entry.first_seen);
                    }
                    if response.double_clicked() && !entry.missing && !in_preview {
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
                            let (heart_rect, heart_response) =
                                ui.allocate_exact_size(Vec2::new(24., row_height), Sense::hover());
                            if self.settings.favorites.contains(&path) {
                                self.emoji.paint(ui, heart_rect, "❤️", Color32::WHITE);
                                heart_response.on_hover_text("Ulubiony utwór");
                            }
                            let played = self.snapshot.session_played.contains(&path);
                            let (status_rect, status_response) =
                                ui.allocate_exact_size(Vec2::new(26., row_height), Sense::hover());
                            self.emoji.paint(ui, status_rect, "🎶", Color32::WHITE);
                            if played && self.settings.show_emojis {
                                let from = status_rect.left_bottom() + egui::vec2(4., -3.);
                                let to = status_rect.right_top() + egui::vec2(-4., 3.);
                                ui.painter()
                                    .line_segment([from, to], Stroke::new(5_f32, Color32::BLACK));
                                ui.painter().line_segment(
                                    [from, to],
                                    Stroke::new(3_f32, Color32::from_rgb(255, 35, 48)),
                                );
                            }
                            status_response.on_hover_text(if played {
                                "Utwór grany w bieżącej sesji"
                            } else {
                                "Utwór jeszcze niegrany w bieżącej sesji"
                            });
                            cell(ui, 34., &format!("{:02}", visible_index + 1), Color32::GRAY);
                            if preview > 0. {
                                preview_wave(
                                    ui,
                                    self.library_data.records.get(&path),
                                    &self.settings.wave_style,
                                    preview,
                                    row_height - 2.,
                                );
                            }
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
                            for (d, accent) in ACCENTS
                                .iter()
                                .copied()
                                .enumerate()
                                .take(self.settings.deck_count)
                            {
                                let loaded = self.snapshot.decks[d]
                                    .track
                                    .as_ref()
                                    .and_then(|t| t.path.as_ref())
                                    == Some(&path);
                                if ui
                                    .add_enabled(
                                        !entry.missing,
                                        egui::Button::new(
                                            RichText::new(letter(d)).color(if loaded {
                                                accent
                                            } else {
                                                Color32::GRAY
                                            }),
                                        )
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
                            if in_preview {
                                ui.close_menu();
                                return;
                            }
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
                            ui.menu_button(tr("Dodaj / usuń z BANGER BOX"), |ui| {
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
                                    ui.label(tr("Utwórz BANGER BOX po lewej stronie."));
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
            let message = if self.search.is_empty() {
                tr("Ten widok jest pusty. Dodaj pliki lub wybierz inny BANGER BOX.")
            } else {
                "Brak wyników. Wyszukiwanie obejmuje wszystkie słowa zapytania.".into()
            };
            ui.painter().with_clip_rect(output.inner_rect).text(
                output.inner_rect.center(),
                egui::Align2::CENTER_CENTER,
                message,
                FontId::proportional(11.),
                ui.visuals().weak_text_color(),
            );
        }
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

    fn file_path_busy(&self, path: &std::path::Path) -> bool {
        self.index_jobs > 0
            || self.file_job.is_some()
            || self.auto.is_some()
            || self
                .preview_path
                .as_ref()
                .is_some_and(|p| p.starts_with(path))
            || self.snapshot.decks.iter().any(|d| {
                d.track
                    .as_ref()
                    .and_then(|t| t.path.as_ref())
                    .is_some_and(|p| p.starts_with(path))
            })
    }
    fn file_actions_menu(&mut self, ui: &mut egui::Ui, path: &PathBuf) {
        let directory = if path.is_dir() {
            path.clone()
        } else {
            path.parent().unwrap_or(path).to_path_buf()
        };
        let busy = self.file_path_busy(path);
        let protected = prospekt_dj::portable::root()
            .ok()
            .is_some_and(|p| p.starts_with(path))
            || prospekt_dj::portable::sampler_dir().ok().as_ref() == Some(path)
            || path.parent().is_none();
        if ui
            .add_enabled(
                path.exists() && self.file_job.is_none(),
                egui::Button::new("Kopiuj"),
            )
            .clicked()
        {
            self.file_clipboard = Some(path.clone());
            ui.close_menu();
        }
        if ui
            .add_enabled(
                self.file_clipboard.is_some() && self.file_job.is_none(),
                egui::Button::new("Wklej"),
            )
            .clicked()
        {
            if let Some(source) = self.file_clipboard.clone() {
                if let Some(name) = source.file_name() {
                    self.start_file_transfer(source.clone(), directory.join(name), false);
                }
            }
            ui.close_menu();
        }
        if ui
            .add_enabled(
                !busy && !protected && path.exists(),
                egui::Button::new("Przenieś do…"),
            )
            .clicked()
        {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Przenieś do folderu")
                .pick_folder()
            {
                if let Some(name) = path.file_name() {
                    self.start_file_transfer(path.clone(), folder.join(name), true);
                }
            }
            ui.close_menu();
        }
        if ui
            .add_enabled(
                !busy && !protected && path.exists(),
                egui::Button::new("Zmień nazwę…"),
            )
            .clicked()
        {
            self.rename_file = Some((
                path.clone(),
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
            ));
            ui.close_menu();
        }
        if ui.button("Utwórz podfolder…").clicked() {
            self.new_folder = Some((directory, String::new()));
            ui.close_menu();
        }
        if ui
            .add_enabled(path.exists(), egui::Button::new("Właściwości systemowe…"))
            .clicked()
        {
            if let Err(error) = prospekt_dj::file_actions::properties(path) {
                self.status = error.to_string();
            }
            ui.close_menu();
        }
    }
    fn start_file_transfer(&mut self, old: PathBuf, new: PathBuf, moving: bool) {
        let (tx, rx) = bounded(1);
        self.file_job = Some(rx);
        self.status = "Operacja na plikach w toku…".into();
        std::thread::spawn(move || {
            let result = prospekt_dj::file_actions::transfer(&old, &new, moving)
                .map(|()| (old, new, moving))
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
    }
    fn remap_file_paths(&mut self, old: &std::path::Path, new: &std::path::Path) {
        let remap = |p: &PathBuf| {
            p.strip_prefix(old)
                .map(|tail| {
                    if tail.as_os_str().is_empty() {
                        new.to_path_buf()
                    } else {
                        new.join(tail)
                    }
                })
                .unwrap_or_else(|_| p.clone())
        };
        self.library = self.library.iter().map(remap).collect();
        self.library_set = self.library.iter().cloned().collect();
        self.settings.favorites = self.settings.favorites.iter().map(remap).collect();
        self.settings.favorite_folders = self.settings.favorite_folders.iter().map(remap).collect();
        self.settings.folder_colors = self
            .settings
            .folder_colors
            .iter()
            .map(|(p, c)| (remap(p), *c))
            .collect();
        self.catalog.roots = self.catalog.roots.iter().map(remap).collect();
        for c in &mut self.catalog.crates {
            c.tracks = c.tracks.iter().map(remap).collect();
        }
        self.catalog.entries = std::mem::take(&mut self.catalog.entries)
            .into_iter()
            .map(|(p, mut e)| {
                let updated = remap(&p);
                e.path = updated.clone();
                (updated, e)
            })
            .collect();
        self.library_data.records = std::mem::take(&mut self.library_data.records)
            .into_iter()
            .map(|(p, r)| (remap(&p), r))
            .collect();
        if let Scope::Folder(p) = &mut self.scope {
            *p = remap(p);
        }
        if let Some(p) = &mut self.waveform_detail {
            *p = remap(p);
        }
        self.folders.cache.clear();
        self.browser_dirty = true;
        self.db_dirty = true;
        self.persist();
    }
    fn rename_file_ui(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.file_job.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.file_job = None;
            match result {
                Ok((old, new, moving)) => {
                    if moving {
                        self.remap_file_paths(&old, &new);
                    }
                    self.folders.cache.clear();
                    self.add_paths(vec![new]);
                    self.status = "Operacja na plikach zakończona".into();
                }
                Err(error) => self.status = format!("Operacja nie powiodła się: {error}"),
            }
        }
        if let Some((parent, mut name)) = self.new_folder.clone() {
            let mut open = true;
            let mut create = false;
            egui::Window::new("Utwórz podfolder")
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(parent.display().to_string());
                    ui.text_edit_singleline(&mut name);
                    create = ui.button("Utwórz").clicked();
                });
            self.new_folder = open.then_some((parent.clone(), name.clone()));
            if create {
                match prospekt_dj::file_actions::create_folder(&parent, &name) {
                    Ok(_) => {
                        self.folders.retry(&parent);
                        self.new_folder = None;
                        self.status = "Utworzono podfolder".into();
                    }
                    Err(error) => self.status = error.to_string(),
                }
            }
        }

        let Some((old, mut name)) = self.rename_file.clone() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        egui::Window::new("Zmień nazwę pliku")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(old.display().to_string());
                ui.text_edit_singleline(&mut name);
                if old.is_file() {
                    ui.label("Rozszerzenie pliku musi pozostać bez zmian.");
                }
                save = ui.button("Zapisz nazwę").clicked();
            });
        self.rename_file = open.then_some((old.clone(), name.clone()));
        if !save {
            return;
        }
        if self.file_path_busy(&old) {
            self.status = "Plik lub folder jest teraz używany".into();
            return;
        }
        let result = if old.is_dir() {
            if name.trim().is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
                Err(anyhow::anyhow!("Niedozwolona nazwa folderu"))
            } else {
                let new = old.with_file_name(&name);
                self.start_file_transfer(old, new, true);
                self.rename_file = None;
                return;
            }
        } else {
            prospekt_dj::file_actions::rename(&old, &name)
        };
        match result {
            Err(error) => self.status = error.to_string(),
            Ok(new) => {
                self.remap_file_paths(&old, &new);
                self.rename_file = None;
                self.status = "Nazwa pliku została zmieniona".into();
            }
        }
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
    fn automix_controls(&mut self, ui: &mut egui::Ui) {
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
    fn import_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Import biblioteki i historii");
        ui.label(
            "Źródła są odczytywane bez zmian. Najpierw sprawdź podgląd i ograniczenia formatu.",
        );
        ui.add_enabled_ui(!self.import_busy,|ui|{
            egui::ComboBox::from_id_salt("library-import-source").selected_text(format!("{:?}",self.import_source)).show_ui(ui,|ui|{for (source,label) in prospekt_dj::dj_import::Source::ALL{if ui.selectable_value(&mut self.import_source,source,label).changed(){self.import_report=None;}}});
            if ui.button("Wybierz źródło…").clicked(){if let Some(path)=rfd::FileDialog::new().pick_file(){self.import_path=path.to_string_lossy().into_owned();self.import_report=None;}}
            if ui.text_edit_singleline(&mut self.import_path).changed(){self.import_report=None;}
            if ui.add_enabled(!self.import_path.is_empty(),egui::Button::new("Sprawdź import")).clicked(){
                self.import_busy=true;self.import_cancel.store(false,std::sync::atomic::Ordering::Relaxed);self.import_messages.clear();let tx=self.work_tx.clone();let path=PathBuf::from(&self.import_path);let source=self.import_source;
                std::thread::spawn(move||{let _=tx.send(Work::ImportInspected(prospekt_dj::dj_import::inspect(&path,source).map_err(|e|e.to_string())));});
            }
            if let Some(report)=&self.import_report {
                let missing=report.missing_paths;
                ui.label(format!("{} · {} utworów · {} istniejących ścieżek · {} brakujących · {} playlist",report.format,report.tracks.len(),report.tracks.len()-missing,missing,report.playlists.len()));
                for limitation in &report.limitations {ui.label(limitation);}
                ui.label("Ręczne oceny, komentarze i zajęte sloty Cue zostają zachowane. Liczniki źródłowe są zachowywane osobno; nie są bezwarunkowo sumowane.");
                if ui.button("Importuj rozpoznane pliki").clicked(){
                    let report=report.clone();let tx=self.work_tx.clone();let db=settings::config_path().with_file_name("tracks.sqlite");self.import_cancel.store(false,std::sync::atomic::Ordering::Relaxed);let cancel=self.import_cancel.clone();self.import_busy=true;self.import_cue_revision=self.snapshot.cue_revision;
                    std::thread::spawn(move||{let _=tx.send(Work::ImportApplied(prospekt_dj::dj_import::apply(&report,&db,&cancel).map_err(|e|e.to_string())));});
                }
            }
        });
        if self.import_busy {
            ui.spinner();
            if ui.button("Anuluj import").clicked() {
                self.import_cancel
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        for message in &self.import_messages {
            ui.label(message);
        }
        ui.label(format!(
            "Raport: {}",
            settings::config_path()
                .with_file_name("last-import-report.json")
                .display()
        ));
    }
    fn refresh_audio_outputs(&mut self) {
        match audio::output_names_for(self.settings.audio_backend) {
            Ok(outputs) => self.outputs = outputs,
            Err(e) => {
                self.outputs.clear();
                self.audio_diagnostics = format!("Device enumeration error: {e:#}");
                self.status = self.audio_diagnostics.clone();
            }
        }
    }
    fn microphone_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Mikrofon", |ui| {
            if ui.checkbox(&mut self.settings.mic_enabled, "Włącz mikrofon do miksu MASTER i nagrania").changed() {
                self.mic_status.clear();
            }
            if ui.button("Odśwież wejścia mikrofonowe").clicked() {
                match prospekt_dj::microphone::input_names() {
                    Ok(names) => self.mic_names = names,
                    Err(error) => self.mic_status = format!("Nie można odczytać wejść: {error:#}"),
                }
            }
            let previous = self.settings.mic_device.clone();
            egui::ComboBox::from_id_salt("microphone-input")
                .selected_text(if previous.is_empty() { "Domyślny mikrofon systemowy" } else { &previous })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.settings.mic_device, String::new(), "Domyślny mikrofon systemowy");
                    for name in &self.mic_names { ui.selectable_value(&mut self.settings.mic_device, name.clone(), name); }
                });
            if previous != self.settings.mic_device {
                self.microphone = None;
                self.send(Command::Microphone(None));
                self.mic_status.clear();
            }
            if !self.mic_status.is_empty() { ui.label(&self.mic_status); }
            ui.label("Wejście systemowe, kanał 1 → oba kanały MASTER i nagranie. Mikrofon nie trafia do PFL. Używaj słuchawek, aby uniknąć sprzężenia.");
            if self.runtime.is_none() { ui.label("Uruchom wyjście audio, aby korzystać z mikrofonu."); }
        });
    }
    fn audio_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(format!("Audio · {}", self.settings.audio_backend.label()));
        self.microphone_settings_ui(ui);
        let previous = self.settings.audio_backend;
        egui::ComboBox::from_id_salt("audio-backend")
            .selected_text(self.settings.audio_backend.label())
            .show_ui(ui, |ui| {
                for backend in audio::backends() {
                    ui.selectable_value(&mut self.settings.audio_backend, backend, backend.label());
                }
            });
        if previous != self.settings.audio_backend {
            self.settings.master_device.clear();
            self.refresh_audio_outputs();
        }
        if ui.button(tr("Odśwież urządzenia")).clicked() {
            self.refresh_audio_outputs();
        }
        if cfg!(target_os = "windows") {
            if ui.button("Wykryj i wybierz ASIO4ALL").clicked() {
                match audio::output_names_for(audio::Backend::Asio) {
                    Ok(names) => {
                        if let Some(name) = names.iter().find(|name| audio::is_asio4all(name)) {
                            self.settings.audio_backend = audio::Backend::Asio;
                            self.settings.master_device = name.clone();
                            self.settings.audio_buffer_frames = None;
                            self.status = format!("Wybrano {name}. Zastosuj ustawienia audio, aby uruchomić sterownik.");
                            self.outputs = names;
                        } else {
                            self.status = "Nie wykryto ASIO4ALL. Zainstaluj wersję zgodną z architekturą programu i ponów wykrywanie.".into();
                        }
                    }
                    Err(error) => self.status = format!("Wykrywanie ASIO4ALL: {error:#}"),
                }
            }
            ui.hyperlink_to(
                "Pobierz ASIO4ALL — oficjalna strona",
                "https://asio4all.org/",
            );
        }
        egui::ComboBox::from_id_salt("master-output")
            .selected_text(if self.settings.master_device.is_empty() {
                "MASTER: domyślne / pierwszy sterownik"
            } else {
                &self.settings.master_device
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.settings.master_device,
                    String::new(),
                    "Domyślne wyjście / pierwszy sterownik",
                );
                for name in &self.outputs {
                    ui.selectable_value(&mut self.settings.master_device, name.clone(), name);
                }
            });
        egui::ComboBox::from_id_salt("audio-buffer")
            .selected_text(
                self.settings
                    .audio_buffer_frames
                    .map(|n| format!("{n} próbek"))
                    .unwrap_or_else(|| "Bufor sterownika — zalecane".into()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.settings.audio_buffer_frames,
                    None,
                    "Bufor sterownika — zalecane",
                );
                for n in [64, 128, 256, 512, 1024, 2048] {
                    ui.selectable_value(
                        &mut self.settings.audio_buffer_frames,
                        Some(n),
                        format!("{n} próbek"),
                    );
                }
            });
        if self.settings.audio_backend == audio::Backend::Asio {
            ui.label("Realtek: zainstaluj ASIO4ALL zgodny z architekturą programu (32/64-bit). Wybierz ASIO4ALL w MASTER i bufor sterownika, a następnie zastosuj ustawienia audio. W panelu ASIO4ALL włącz właściwe wyjście Realtek.");
            let active_asio = self.runtime.as_ref().is_some_and(|runtime| {
                runtime.backend == audio::Backend::Asio && runtime.is_running()
            });
            if ui.add_enabled(active_asio, egui::Button::new("Panel sterownika ASIO / ASIO4ALL…"))
                .on_hover_text("Panel aktualnie działającego sterownika ASIO; po zmianie bufora lub wyjść może być konieczne ponowne uruchomienie audio.")
                .clicked() {
                if let Some(runtime) = &self.runtime {
                    if let Err(error) = runtime.open_asio_panel() { self.status = format!("Panel ASIO: {error:#}"); }
                }
            }
            egui::ComboBox::from_id_salt("asio-master-pair")
                .selected_text(format!(
                    "MASTER: {}–{}",
                    self.settings.audio_master_pair + 1,
                    self.settings.audio_master_pair + 2
                ))
                .show_ui(ui, |ui| {
                    for n in (0..32).step_by(2) {
                        ui.selectable_value(
                            &mut self.settings.audio_master_pair,
                            n,
                            format!("{}–{}", n + 1, n + 2),
                        );
                    }
                });
            if self.settings.audio_cue_pair == Some(self.settings.audio_master_pair) {
                self.settings.audio_cue_pair = None;
            }
            egui::ComboBox::from_id_salt("asio-cue-pair")
                .selected_text(
                    self.settings
                        .audio_cue_pair
                        .map(|n| format!("PFL: {}–{}", n + 1, n + 2))
                        .unwrap_or_else(|| "PFL: wyłączony".into()),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.settings.audio_cue_pair, None, "PFL: wyłączony");
                    for n in (0..32).step_by(2) {
                        if n != self.settings.audio_master_pair {
                            ui.selectable_value(
                                &mut self.settings.audio_cue_pair,
                                Some(n),
                                format!("{}–{}", n + 1, n + 2),
                            );
                        }
                    }
                });
            ui.label("ASIO otwiera jeden sterownik; MASTER i PFL mogą używać różnych par jego kanałów. Niedostępna para wywoła czytelny błąd.");
        } else {
            ui.label(if cfg!(target_os="windows"){ "Realtek: wybierz WASAPI, domyślne wyjście lub Speakers / Realtek Audio i bufor sterownika. Włącz głośniki w Ustawieniach dźwięku Windows." }else{"Wybierz domyślne wyjście systemowe i bufor sterownika. ASIO jest dostępne w wersji Windows."});
            egui::ComboBox::from_id_salt("cue-output")
                .selected_text(if self.settings.cue_device.is_empty() {
                    "PFL: wyłączony"
                } else {
                    &self.settings.cue_device
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.settings.cue_device,
                        String::new(),
                        tr("Odsłuch wyłączony"),
                    );
                    for name in &self.outputs {
                        ui.selectable_value(&mut self.settings.cue_device, name.clone(), name);
                    }
                });
            ui.label("PFL przez WASAPI wymaga osobnego urządzenia. Niedostępny odsłuch nie wyłącza MASTER.");
        }
        let busy = self.record_result.is_some() || self.snapshot.decks.iter().any(|d| d.playing);
        if ui
            .add_enabled(
                !busy,
                egui::Button::new("Zastosuj / ponów uruchomienie audio"),
            )
            .clicked()
        {
            self.restart_audio();
        }
        ui.label("Zatrzymaj decki i nagrywanie przed zmianą sterownika. Podany blok callbacku nie jest pomiarem latencji sprzętu.");
        if let Some(runtime) = &self.runtime {
            ui.label(runtime.buffer_description());
        }
        if ui.button("Kopiuj diagnostykę audio").clicked() {
            let text = self
                .runtime
                .as_ref()
                .map(|r| {
                    format!(
                        "{}\nStartup log:\n{}",
                        r.diagnostics(),
                        self.audio_diagnostics
                    )
                })
                .unwrap_or_else(|| self.audio_diagnostics.clone());
            audio::save_diagnostics(&text);
            ui.ctx().copy_text(text);
        }
        ui.label(format!(
            "Plik diagnostyki: {}",
            audio::diagnostic_path().display()
        ));
        ui.collapsing("Szczegóły uruchomienia / błędu", |ui| {
            let mut text = self.audio_diagnostics.as_str();
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(8),
            );
        });
    }
    fn eject_confirmation_ui(&mut self, ctx: &egui::Context) {
        let Some((deck, track_id, token)) = self.eject_pending.clone() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        egui::Window::new("Wyrzuć utwór z deku")
            .id(egui::Id::new("confirm-eject-track"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Czy na pewno chcesz wyrzucić utwór z deku {}?",
                    letter(deck)
                ));
                if let Some(track) = &self.snapshot.decks[deck].track {
                    ui.label(&track.title);
                }
                if self.snapshot.decks[deck].playing {
                    ui.colored_label(
                        Color32::RED,
                        "Utwór jest odtwarzany. Potwierdzenie zatrzyma ten dek.",
                    );
                }
                ui.label("Plik, punkty CUE i wpis w bibliotece zostaną zachowane.");
                ui.horizontal(|ui| {
                    if ui.button("Tak, wyrzuć").clicked() {
                        confirm = true;
                    }
                    if ui.button("Anuluj").clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            let unchanged = self.deck_tokens[deck] == token
                && !self.loading[deck]
                && self.snapshot.decks[deck]
                    .track
                    .as_ref()
                    .is_some_and(|track| track.id == track_id);
            if unchanged {
                self.stop_auto();
                self.deck_tokens[deck] += 1;
                self.loading[deck] = false;
                self.covers[deck] = None;
                self.bend_slider[deck] = 0.;
                self.bend_buttons[deck] = 0.;
                self.bend_keys[deck] = 0.;
                self.send(Command::Deck(deck, DeckControl::Unload));
                self.status = format!("Wyładowano utwór z deku {}", letter(deck));
            } else {
                self.status = "Utwór na deku zmienił się — ponów polecenie wyrzucenia.".into();
            }
        }
        if confirm || cancel || !open {
            self.eject_pending = None;
        }
    }
    fn connect_detected_controller(&mut self, name: &str, profile: usize) {
        let Some(index) = self.midi_names.iter().position(|n| n == name) else {
            self.detected_status = "Urządzenie zostało odłączone.".into();
            return;
        };
        let Some(mapping) = self
            .settings
            .controller_profiles
            .get(profile)
            .filter(|p| !p.mappings.is_empty())
            .cloned()
        else {
            return;
        };
        if self.midi_second_name.as_deref() == Some(name) {
            self.detected_status = "Ten port jest już przypisany do kontrolera 2".into();
            return;
        }
        // Drop the old connection before opening the same Windows MIDI port again.
        self.midi_connection = None;
        while self.midi_rx.try_recv().is_ok() {}
        match midi::connect_named(name, self.midi_tx.clone()) {
            Ok(connection) => {
                self.midi_connection = Some(connection);
                self.midi_selected = index;
                self.midi_reconnect_name = Some(name.to_owned());
                self.settings.midi = mapping.mappings;
                self.settings.controller_profile = Some(profile);
                self.settings.controller_profile_source = mapping.source;
                self.detected_status = format!(
                    "Połączono. Zastosowano mapowanie: {} {}",
                    mapping.manufacturer, mapping.model
                );
                self.status = self.detected_status.clone();
            }
            Err(error) => {
                self.detected_status = format!("Nie można otworzyć urządzenia MIDI: {error:#}")
            }
        }
    }
    fn reflection_color_window(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.reflection_color_editor else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Zmiana koloru odbić waveformów")
            .id(egui::Id::new("waveform-reflection-color-editor"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(570.)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().slider_width = 220.;
                        if egui::color_picker::color_picker_color32(
                            ui,
                            &mut editor.draft,
                            egui::color_picker::Alpha::Opaque,
                        ) {
                            editor.html = format!(
                                "#{:02X}{:02X}{:02X}",
                                editor.draft.r(),
                                editor.draft.g(),
                                editor.draft.b()
                            );
                        }
                    });
                    ui.vertical(|ui| {
                        ui.strong("RGB — 0–255");
                        let mut rgb = [editor.draft.r(), editor.draft.g(), editor.draft.b()];
                        let mut changed = false;
                        for (label, value) in ["R", "G", "B"].into_iter().zip(&mut rgb) {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                changed |=
                                    ui.add(egui::DragValue::new(value).range(0..=255)).changed();
                            });
                        }
                        if changed {
                            editor.draft = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                            editor.html = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                        }
                        ui.separator();
                        ui.label("Zapis HTML (#RRGGBB)");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut editor.html)
                                    .desired_width(110.)
                                    .char_limit(7),
                            )
                            .changed()
                        {
                            if let Some(color) = parse_hex_color(&editor.html) {
                                editor.draft = color;
                            }
                        }
                        if parse_hex_color(&editor.html).is_none() {
                            ui.colored_label(Color32::LIGHT_RED, "Wpisz 6 cyfr szesnastkowych");
                        }
                        if ui.button("Losuj kolor").clicked() {
                            let rgb = rand::random::<[u8; 3]>();
                            editor.draft = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                            editor.html = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                        }
                        ui.separator();
                        for (label, color) in
                            [("Bieżący", editor.draft), ("Poprzedni", editor.previous)]
                        {
                            ui.label(label);
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::new(110., 26.), Sense::hover());
                            ui.painter()
                                .rect_filled(rect, prospekt_dj::theme::radius_f(3.), color);
                            ui.painter().rect_stroke(
                                rect,
                                prospekt_dj::theme::radius_f(3.),
                                Stroke::new(1_f32, Color32::GRAY),
                                egui::StrokeKind::Inside,
                            );
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Kolory CUE:");
                    for slot in 0..8 {
                        let color = cue_color(slot);
                        if ui
                            .add_sized([24., 22.], egui::Button::new("").fill(color))
                            .clicked()
                        {
                            editor.draft = color;
                            editor.html =
                                format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b());
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Przywróć").clicked() {
                        editor.draft = editor.previous;
                        editor.html = format!(
                            "#{:02X}{:02X}{:02X}",
                            editor.draft.r(),
                            editor.draft.g(),
                            editor.draft.b()
                        );
                    }
                    apply = ui
                        .add_enabled(
                            parse_hex_color(&editor.html).is_some(),
                            egui::Button::new("OK"),
                        )
                        .clicked();
                    cancel = ui.button("Anuluj").clicked();
                });
            });
        if apply {
            self.settings.wave_style.reflection_color =
                Some([editor.draft.r(), editor.draft.g(), editor.draft.b()]);
            self.reflection_color_editor = None;
            self.persist();
        } else if cancel || !open {
            self.reflection_color_editor = None;
        }
    }

    fn cloud_window(&mut self, ctx: &egui::Context) {
        if !self.cloud.open {
            return;
        }
        let mut open = true;
        egui::Window::new("DYSK GOOGLE — Pliki w chmurze").open(&mut open).default_width(780.).resizable(true).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new((self.brand.drive.id(), Vec2::splat(36.))));
                ui.label(RichText::new("DYSK GOOGLE").size(22.).family(egui::FontFamily::Name("Arial Bold".into())).color(ACCENTS[0]));
            });
            ui.label("Przeglądaj foldery i pobieraj muzykę do programu. Pliki są pobierane przed załadowaniem na dek.");
            ui.label(RichText::new(format!("Folder lokalny: {}", self.cloud.root.display())).small());
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.cloud.busy, egui::Button::new("Wczytaj konfigurację OAuth…")).clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("Google OAuth Desktop JSON", &["json"]).pick_file() {
                        match std::fs::read(path).map_err(anyhow::Error::from).and_then(|bytes| prospekt_dj::google_drive::OAuthClient::from_json(&bytes)) {
                            Ok(client) => { self.cloud.client = Some(client); self.cloud.status = "Konfiguracja gotowa — kliknij Połącz z Google".into(); }
                            Err(error) => self.cloud.status = error.to_string(),
                        }
                    }
                }
                if ui.add_enabled(!self.cloud.busy && self.cloud.client.is_some(), egui::Button::new("Połącz z Google")).clicked() {
                    if let Some(client) = self.cloud.client.clone() {
                        self.cloud.busy = self.cloud.drive.connect(client);
                        self.cloud.status = if self.cloud.busy { "Otwieranie logowania…".into() } else { "Kolejka zajęta".into() };
                    }
                }
                if ui.add_enabled(self.cloud.connected && !self.cloud.busy, egui::Button::new("Rozłącz")).clicked() {
                    self.cloud.busy = self.cloud.drive.disconnect();
                }
                if ui.button("Pobrane pliki w bibliotece").clicked() {
                    let root = self.cloud.root.clone(); self.scope = Scope::Folder(root.clone());
                    self.browser_dirty = true; self.add_paths(vec![root]);
                }
            });
            if self.cloud.client.is_none() {
                ui.label("Pierwsze połączenie: w Google Cloud włącz Drive API i pobierz plik JSON klienta OAuth typu Aplikacja komputerowa. Program nie prosi o hasło Google.");
                ui.hyperlink_to("Instrukcja Google: klient OAuth Desktop", "https://developers.google.com/drive/api/quickstart/desktop");
            }
            if let Some(url) = &self.cloud.auth_url { ui.hyperlink_to("Otwórz logowanie w przeglądarce", url); }
            ui.horizontal(|ui| { if self.cloud.busy { ui.spinner(); } ui.label(&self.cloud.status); });
            ui.separator();
            if self.cloud.connected {
                ui.horizontal_wrapped(|ui| {
                    for (i, (id, name)) in self.cloud.breadcrumbs.clone().iter().enumerate() {
                        if ui.add_enabled(!self.cloud.busy, egui::Button::new(name)).clicked() {
                            self.cloud.breadcrumbs.truncate(i + 1); self.cloud.busy = self.cloud.drive.list(id.clone(), None);
                        }
                    }
                    if ui.add_enabled(!self.cloud.busy, egui::Button::new("Odśwież")).clicked() { self.cloud.busy = self.cloud.drive.list(self.cloud.folder.clone(), None); }
                    if ui.add_enabled(!self.cloud.busy && self.cloud.next.is_some(), egui::Button::new("Następna strona")).clicked() {
                        self.cloud.busy = self.cloud.drive.list(self.cloud.folder.clone(), self.cloud.next.clone());
                    }
                });
                egui::ScrollArea::both().id_salt("google-drive-files").max_height(420.).show(ui, |ui| {
                    for file in self.cloud.files.clone() {
                        ui.horizontal(|ui| {
                            let folder = file.folder();
                            if ui.add_enabled(!self.cloud.busy, egui::Button::new(format!("{} {}", if folder { "📁" } else { "🎵" }, file.name))).clicked() && folder {
                                self.cloud.breadcrumbs.push((file.id.clone(), file.name.clone()));
                                self.cloud.busy = self.cloud.drive.list(file.id.clone(), None);
                            }
                            if !folder {
                                ui.label(file.size.as_deref().map(|s| format!("{s} B")).unwrap_or_default());
                                if ui.add_enabled(!self.cloud.busy, egui::Button::new("Pobierz")).clicked() { self.cloud.busy = self.cloud.drive.download(file.clone(), None); }
                                for (deck, accent) in ACCENTS.iter().enumerate().take(self.settings.deck_count) {
                                    if ui.add_enabled(!self.cloud.busy, egui::Button::new(RichText::new(letter(deck)).color(*accent))).clicked() {
                                        self.cloud.busy = self.cloud.drive.download(file.clone(), Some(deck));
                                    }
                                }
                            }
                        });
                    }
                    if self.cloud.files.is_empty() && !self.cloud.busy { ui.label("Brak folderów lub plików audio na tej stronie."); }
                });
            }
            ui.separator();
            ui.label(RichText::new("Tokeny dostępu są przechowywane tylko w pamięci bieżącej sesji. Pobrane utwory pozostają lokalnie po rozłączeniu.").small());
        });
        self.cloud.open = open;
    }

    fn utility_windows(&mut self, ctx: &egui::Context) {
        if self.key_window {
            egui::Window::new("KEY — Sterowanie klawiaturą").open(&mut self.key_window)
                .default_width(620.).resizable(true).show(ctx, |ui| {
                    ui.label("Aktywny dek to dek wybrany w programie. Skróty odtwarzania są wstrzymane podczas wpisywania tekstu, w ustawieniach i oknach dialogowych.");
                    egui::ScrollArea::vertical().id_salt("keyboard-help").max_height(560.).show(ui, |ui| {
                        egui::Grid::new("keyboard-shortcuts").striped(true).spacing([20., 8.]).show(ui, |ui| {
                            ui.strong("KLAWISZ"); ui.strong("DZIAŁANIE"); ui.end_row();
                            for (key, action) in [
                                ("F11", "Włącz / wyłącz pełny ekran"),
                                ("Spacja", "PLAY / pauza aktywnego deku"),
                                ("Q / P", "PLAY / pauza deku A / B"),
                                ("A / L", "CUE deku A / B"),
                                ("Esc", "Zatrzymaj AutoMix i wstrzymaj wszystkie deki"),
                                ("← / →", "Crossfader w lewo / prawo o 5%"),
                                ("↑ / ↓", "Poprzedni / następny utwór w bieżącej liście"),
                                ("Enter", "Załaduj zaznaczony utwór na dek A"),
                                ("Shift + Enter", "Załaduj zaznaczony utwór na dek B"),
                                ("F1–F8", "Wywołaj / ustaw punkt CUE 1–8 aktywnego deku"),
                                ("Shift + F1–F8", "Usuń punkt CUE 1–8 aktywnego deku"),
                                ("1, 2, 3, 4", "Pady 1–4 deku A — według wybranego trybu"),
                                ("7, 8, 9, 0", "Pady 1–4 deku B — według wybranego trybu"),
                                ("Shift + klawisz pada", "Alternatywna akcja pada, np. usunięcie CUE"),
                                ("Z / X (przytrzymaj)", "Pitch bend deku A: −4% / +4%"),
                                ("N / M (przytrzymaj)", "Pitch bend deku B: −4% / +4%"),
                            ] { ui.label(RichText::new(key).strong()); ui.label(action); ui.end_row(); }
                        });
                        ui.separator();
                        ui.label("W polach tekstowych działają standardowe skróty edycji. Ctrl, Alt i klawisz systemowy blokują skróty deków. W trybie 4 deków Spacja i F1–F8 dotyczą również aktywnego deku C lub D.");
                    });
                });
        }
        if self.device_window {
            egui::Window::new("DEV — Podłączone urządzenia").open(&mut self.device_window)
                .default_width(660.).resizable(true).show(ctx, |ui| {
                    ui.label("Lista wejść MIDI i zamontowanych nośników jest aktualizowana automatycznie.");
                    if ui.button("Odśwież wejścia MIDI").clicked() { self.midi_names = midi::inputs(); }
                    egui::ScrollArea::vertical().id_salt("devices-list").max_height(560.).show(ui, |ui| {
                        ui.heading("Kontrolery / wejścia MIDI");
                        if self.midi_names.is_empty() { ui.label("Brak dostępnych wejść MIDI"); }
                        for name in &self.midi_names {
                            ui.separator(); ui.strong(name);
                            let first = self.midi_connection.is_some() && self.midi_reconnect_name.as_ref() == Some(name);
                            let second = self.midi_second_connection.is_some() && self.midi_second_name.as_ref() == Some(name);
                            ui.label(if first { "Połączony — kontroler 1" } else if second { "Połączony — kontroler 2" } else { "Dostępny — niepołączony" });
                            let matches = midi::matching_profiles(name, &self.settings.controller_profiles);
                            if matches.len() == 1 {
                                let profile = &self.settings.controller_profiles[matches[0]];
                                ui.label(format!("Model dopasowany z bazy: {} {}", profile.manufacturer, profile.model));
                            } else { ui.label("Model: brak jednoznacznego dopasowania"); }
                        }
                        ui.separator(); ui.heading("Dyski i urządzenia zewnętrzne");
                        if self.storage.volumes.is_empty() { ui.label("Brak wykrytych zamontowanych nośników"); }
                        for volume in &self.storage.volumes {
                            ui.separator();
                            ui.strong(format!("{} — {}", if volume.external { "Nośnik zewnętrzny / USB" } else { "Dysk" }, volume.name));
                            ui.label(format!("Lokalizacja: {}", volume.path.display()));
                            ui.label(format!("Producent: {}", if volume.vendor.is_empty() { "Nieudostępniony przez system" } else { &volume.vendor }));
                            ui.label(format!("Model: {}", if volume.model.is_empty() { "Nieudostępniony przez system" } else { &volume.model }));
                        }
                    });
                });
        }
    }
    fn dual_midi_notice_ui(&mut self, ctx: &egui::Context) {
        let pair = if self.midi_connection.is_some() && self.midi_second_connection.is_some() {
            self.midi_reconnect_name
                .clone()
                .zip(self.midi_second_name.clone())
        } else if std::env::var_os("PROSPEKT_DEMO_DUAL_MIDI").is_some()
            && std::env::args().any(|arg| arg == "--demo")
        {
            Some((
                "DEMO — Kontroler MIDI 1".into(),
                "DEMO — Kontroler MIDI 2".into(),
            ))
        } else {
            None
        };
        if pair != self.dual_midi_pair {
            self.dual_midi_notice = pair.is_some();
            if let Some((first, second)) = &pair {
                self.status = format!("TRYB DWÓCH KONTROLERÓW AKTYWNY — {first} + {second}");
            }
            self.dual_midi_pair = pair;
        }
        if !self.dual_midi_notice {
            return;
        }
        let Some((first, second)) = self.dual_midi_pair.clone() else {
            return;
        };
        let mut open = true;
        let mut acknowledged = false;
        egui::Window::new("POŁĄCZONO DWA KONTROLERY")
            .id(egui::Id::new("dual-midi-connected"))
            .open(&mut open).collapsible(false).resizable(false).default_width(540.)
            .show(ctx, |ui| {
                ui.label(RichText::new("TRYB DWÓCH URZĄDZEŃ AKTYWNY").strong().color(ACCENTS[0]).size(18.));
                ui.label("Program obsługuje oba kontrolery jednocześnie. Każdy korzysta z własnego mapowania MIDI.");
                for (slot, name, selected, count) in [
                    (1, &first, self.settings.controller_profile, self.settings.midi.len()),
                    (2, &second, self.midi_second_profile, self.settings.midi_second.len()),
                ] {
                    ui.separator();
                    ui.strong(format!("KONTROLER {slot}"));
                    ui.label(format!("Nazwa portu: {name}"));
                    let candidates = midi::matching_profiles(name, &self.settings.controller_profiles);
                    let profile = selected.and_then(|i| self.settings.controller_profiles.get(i));
                    if let Some(profile) = profile {
                        ui.label(format!("Producent według wybranego profilu: {}", profile.manufacturer));
                        ui.label(format!("Model według wybranego profilu: {}", profile.model));
                    } else if candidates.len() == 1 {
                        let profile = &self.settings.controller_profiles[candidates[0]];
                        ui.label(format!("Dopasowany producent: {}", profile.manufacturer));
                        ui.label(format!("Dopasowany model: {}", profile.model));
                    } else {
                        ui.label("Producent / model: nierozpoznany — dostępna nazwa portu MIDI.");
                    }
                    ui.label(format!("Liczba przypisań: {count}"));
                    if count == 0 { ui.colored_label(Color32::YELLOW, "Wybierz profil lub skonfiguruj MIDI Learn w ustawieniach."); }
                }
                ui.separator();
                acknowledged = ui.button("OK — korzystaj z dwóch kontrolerów").clicked();
            });
        if !open || acknowledged {
            self.dual_midi_notice = false;
        }
    }
    fn detected_controller_ui(&mut self, ctx: &egui::Context) {
        let Some(name) = self.controller_notices.front().cloned() else {
            return;
        };
        let candidates = midi::matching_profiles(&name, &self.settings.controller_profiles);
        if self.detected_status.is_empty() {
            self.detected_profile = candidates.first().copied();
            if candidates.len() == 1 && self.midi_connection.is_none() {
                self.connect_detected_controller(&name, candidates[0]);
            } else {
                self.detected_status = if candidates.is_empty() {
                    "Nie znaleziono jednoznacznego modelu. Wybierz profil ręcznie lub użyj MIDI Learn w ustawieniach.".into()
                } else if candidates.len() > 1 {
                    "Znaleziono kilka wariantów mapowania. Wybierz właściwy profil.".into()
                } else {
                    "Inny kontroler jest już połączony. Możesz ręcznie przełączyć urządzenie."
                        .into()
                };
            }
        }
        let mut open = true;
        let mut close = false;
        egui::Window::new("ODNALEZIONO URZĄDZENIE — KONTROLER")
            .id(egui::Id::new("detected-controller"))
            .open(&mut open).collapsible(false).default_width(520.)
            .show(ctx, |ui| {
                ui.label(RichText::new(&name).family(egui::FontFamily::Name("Arial Bold".into())).size(16.));
                ui.label("Nazwa zgłoszona przez sterownik MIDI. Dopasowanie korzysta z lokalnej bazy profili.");
                egui::ComboBox::from_id_salt("detected-controller-profile")
                    .selected_text(self.detected_profile.and_then(|i| self.settings.controller_profiles.get(i)).map(|p| p.label()).unwrap_or_else(|| "Wybierz mapowanie".into()))
                    .show_ui(ui, |ui| {
                        for (i, profile) in self.settings.controller_profiles.iter().enumerate().filter(|(_, p)| !p.mappings.is_empty()) {
                            let recommended = if candidates.contains(&i) { "★ " } else { "" };
                            ui.selectable_value(&mut self.detected_profile, Some(i), format!("{recommended}{}", profile.label()));
                        }
                    });
                ui.label(&self.detected_status);
                if !self.midi_names.contains(&name) { ui.colored_label(Color32::RED, "Urządzenie jest obecnie odłączone."); }
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.detected_profile.is_some() && self.midi_names.contains(&name), egui::Button::new("Połącz i zastosuj mapowanie")).clicked() {
                        if let Some(profile) = self.detected_profile { self.connect_detected_controller(&name, profile); }
                    }
                    if ui.button("Ustawienia MIDI").clicked() {
                        self.settings_open = true;
                        self.settings_tab = 1;
                        close = true;
                    }
                    if ui.button("Zamknij").clicked() { close = true; }
                });
            });
        if !open || close {
            self.controller_notices.pop_front();
            self.detected_status.clear();
            self.detected_profile = None;
        }
    }
    fn settings_ui(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        egui::Window::new(tr("Ustawienia studia")).id(egui::Id::new("studio-settings")).default_pos(egui::pos2(((ctx.screen_rect().width()-840.)/2.).max(0.),80.)).open(&mut open).default_width(840.).resizable(true).show(ctx,|ui|{ egui::ScrollArea::vertical().max_height((ctx.screen_rect().height()-160.).max(300.)).show(ui,|ui|{
            ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(170.,400.),egui::Layout::top_down(Align::Min),|ui| {
                ui.set_width(170.);
                ui.add(egui::TextEdit::singleline(&mut self.settings_search).hint_text("Search settings…").desired_width(160.));
                for (i,label) in [(4,"Ogólne"),(6,"Wygląd i wydajność"),(7,"Waveformy"),(0,"Audio i nagrywanie"),(10,"Broadcast"),(1,"MIDI"),(2,"Sampler i automix"),(3,"Sterowanie"),(9,"Import i baza"),(5,"O programie")] {
                    if self.settings_search.is_empty() || tr(label).to_lowercase().contains(&self.settings_search.to_lowercase()) { ui.selectable_value(&mut self.settings_tab,i,tr(label)); }
                }
            });
            ui.separator();
            ui.vertical(|ui| {
            match self.settings_tab{
                10=>self.broadcast_ui(ui),
                9=>self.import_ui(ui),
                6=>{
                    ui.heading("Wygląd i wydajność");
                    if ui.checkbox(&mut self.settings.light,tr("Tryb jasny")).changed(){console_theme(ctx,self.settings.light);}
                    ui.horizontal(|ui|{ui.label("Skórka:");let before=self.settings.skin;egui::ComboBox::from_id_salt(("skin",ui.id())).selected_text(self.settings.skin.name()).show_ui(ui,|ui|{for (skin,name) in prospekt_dj::theme::Skin::ALL{ui.selectable_value(&mut self.settings.skin,skin,name);}});if before!=self.settings.skin{prospekt_dj::theme::set_skin(self.settings.skin);console_theme(ctx,self.settings.light);}});
                    ui.checkbox(&mut self.settings.reduced_motion,"Ogranicz animacje");
                    ui.separator();
                    ui.heading("Mikser i deki");
                    ui.horizontal(|ui| {
                        ui.label("Zakres suwaka tempa:");
                        egui::ComboBox::from_id_salt("tempo-range")
                            .selected_text(format!(
                                "±{:.0}%",
                                prospekt_dj::engine::tempo_range(self.settings.tempo_range) * 100.
                            ))
                            .show_ui(ui, |ui| {
                                for index in 0..prospekt_dj::engine::TEMPO_RANGES.len() {
                                    let pct = prospekt_dj::engine::tempo_range(index as u8) * 100.;
                                    ui.selectable_value(
                                        &mut self.settings.tempo_range,
                                        index as u8,
                                        format!("±{pct:.0}%"),
                                    );
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Krzywa suwaka krzyżowego:");
                        let before = self.settings.cross_curve;
                        egui::ComboBox::from_id_salt("cross-curve")
                            .selected_text(
                                prospekt_dj::dsp::CrossCurve::ALL
                                    .iter()
                                    .find(|(c, _)| *c == self.settings.cross_curve)
                                    .map_or("", |(_, name)| name),
                            )
                            .show_ui(ui, |ui| {
                                for (curve, name) in prospekt_dj::dsp::CrossCurve::ALL {
                                    ui.selectable_value(&mut self.settings.cross_curve, curve, name);
                                }
                            });
                        if before != self.settings.cross_curve {
                            self.send(Command::CrossCurve(self.settings.cross_curve));
                        }
                    });
                    if ui
                        .checkbox(&mut self.settings.cross_enabled, "Suwak krzyżowy włączony")
                        .on_hover_text("Wyłączony: obie strony miksu są otwarte, a suwak nic nie zmienia")
                        .changed()
                    {
                        self.send(Command::CrossEnabled(self.settings.cross_enabled));
                    }
                    ui.add_enabled(!self.settings.reduced_motion, egui::Checkbox::new(&mut self.settings.blink_paused_play,"Miganie PLAY co pół sekundy, gdy utwór jest zatrzymany"));
                    ui.checkbox(&mut self.settings.show_statistics,"Statystyki systemowe");
                    ui.add(egui::Slider::new(&mut self.settings.refresh_hz,10..=120).text("Limit odświeżania / Hz"));
                    // The slider only sets the target; `animate_zoom` glides to it each frame.
                    let mut zoom=self.settings.ui_scale;
                    if ui.add(egui::Slider::new(&mut zoom,0.75..=1.75).step_by(0.05).custom_formatter(|v,_| format!("{:.0}%", v*100.)).text("Skala interfejsu")).changed(){self.settings.ui_scale=zoom;}
                    if ui.button("Przywróć domyślne").clicked(){self.settings.light=false;self.settings.reduced_motion=false;self.settings.show_statistics=true;self.settings.refresh_hz=30;self.settings.ui_scale=1.;console_theme(ctx,false);}
                },
                7=>{
                    ui.heading("Waveformy");
                    for (preset,name) in prospekt_dj::wave_style::Preset::ALL {ui.selectable_value(&mut self.settings.wave_style.preset,preset,name);}
                    ui.add(egui::Slider::new(&mut self.settings.wave_style.brightness,0.2..=2.).text("Jasność"));
                    ui.add(egui::Slider::new(&mut self.settings.wave_style.saturation,0.0..=2.).text("Nasycenie"));
                    ui.add(egui::Slider::new(&mut self.settings.wave_style.height,0.6..=1.5).text("Wysokość"));
                    ui.checkbox(&mut self.settings.wave_style.grid,"Siatka beatów");
                    ui.checkbox(&mut self.settings.wave_style.beat_background,"Cieniowane pola rytmiczne waveformów");
                    ui.checkbox(&mut self.settings.wave_style.reflections,"Odbicia między waveformami A–B, B–C i C–D").on_hover_text("Każde odbicie ma wysokość 1/8 waveformu i odstęp 2,5 px: górnego waveformu w dół, dolnego w górę. Odbicia śledzą odtwarzanie i przybliżenie.");
                    ui.horizontal_wrapped(|ui| {
                        ui.label(if self.settings.wave_style.reflection_color.is_none() { "Kolor odbić: kolory waveformów (domyślnie)" } else { "Kolor odbić: własny" });
                        if ui.button("Zmień kolor odbić…").clicked() {
                            let rgb = self.settings.wave_style.reflection_color.unwrap_or([49, 188, 255]);
                            let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                            self.reflection_color_editor = Some(ReflectionColorEditor {
                                draft: color, previous: color, html: format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
                            });
                        }
                        if ui.button("Przywróć kolory waveformów").clicked() { self.settings.wave_style.reflection_color = None; }
                    });
                    ui.checkbox(&mut self.settings.wave_style.markers,"Markery Hot Cue");
                    ui.checkbox(&mut self.settings.wave_style.spread_cue_labels,"Rozmieszczaj napisy CUE na całej wysokości waveformu").on_hover_text("Wyłączone domyślnie: napisy przy górnej fladze. Po włączeniu każdy punkt otrzymuje stabilną, pseudolosową wysokość.");
                    for (n,name) in ["LOW","MID","HIGH"].iter().enumerate(){ui.horizontal(|ui|{ui.label(*name);ui.color_edit_button_srgb(&mut self.settings.wave_style.palette[n]);});}
                    ui.horizontal(|ui|{for bands in [[1.,0.,0.],[0.,1.,0.],[0.,0.,1.],[0.4,0.6,1.]] {let [r,g,b]=self.settings.wave_style.color(bands,[100;3]);let (rect,_)=ui.allocate_exact_size(Vec2::new(42.,20.),Sense::hover());ui.painter().rect_filled(rect,0.,Color32::from_rgb(r,g,b));}});
                    if ui.button("Przywróć domyślne").clicked(){self.settings.wave_style=Default::default();}
                },
                0=>{
                    self.audio_settings_ui(ui);
                    ui.separator();ui.heading(tr("Nagrywanie"));
                    egui::ComboBox::from_id_salt("record-format").selected_text(self.settings.format.extension().to_uppercase()).show_ui(ui,|ui|{for format in recording::Format::ALL{ui.selectable_value(&mut self.settings.format,format,format.extension().to_uppercase());}});
                    ui.label("WAV i AIFF zapisuje kod Rust. MP3, FLAC i Ogg/Opus wymagają FFmpeg w PATH lub obok pliku programu. W razie błędu konwersji pozostaje plik WAV.");
                    ui.checkbox(&mut self.settings.auto_bpm,tr("Analizuj BPM po wczytaniu utworu"));
                    ui.label(RichText::new("Analiza jest szacunkowa; dostępne są TAP oraz ×2 / ÷2. Zmiana tempa zmienia wysokość dźwięku — brak key lock.").weak());
                },
                1=>self.midi_ui(ui),
                2=>{
                    ui.heading("AutoMix");self.automix_controls(ui);
                    ui.heading(tr("Kolejka i przejścia"));ui.add(egui::Slider::new(&mut self.settings.fade_seconds,1.0..=30.).text("Czas przejścia / s"));
                    ui.checkbox(&mut self.settings.auto_favorites,tr("Tylko ulubione"));ui.checkbox(&mut self.settings.auto_shuffle,tr("Losuj kolejność"));ui.checkbox(&mut self.settings.auto_repeat,tr("Powtarzaj kolejkę"));ui.checkbox(&mut self.settings.linear_fade,tr("Liniowa krzywa przejścia (zamiast stałej mocy)"));
                    ui.label("Kolejność jest ustalana przy starcie automiksu. Ręczny transport, tempo, jog i crossfader zatrzymują automat. Automix nie wyrównuje beatów.");
                },
                3=>{
                    ui.heading(tr("Klawiatura"));keyboard_help(ui);
                    ui.separator();ui.label("Mysz: kliknij waveform, aby przewinąć. Przeciągaj talerz poziomo. Dwuklik suwaka EQ zeruje pasmo, a dwuklik tempa zeruje tempo. Manual loop: pad 1 IN, 2 OUT, 3 OFF, 4 powrót. SHIFT + pad usuwa hot cue lub sample.");
                    ui.label("Biblioteka i ulubione są zapisywane lokalnie. Audio samplera dotyczy bieżącej sesji; Hot Cue są zapisywane w tracks.sqlite.");
                    ui.label(format!("Ustawienia: {}",settings::config_path().display()));
                    ui.hyperlink_to("Licencja AGPL-3.0-or-later","https://www.gnu.org/licenses/agpl-3.0.html");
                }
                4=> {
                    ui.heading(tr("Ogólne"));
                    ui.collapsing("Czcionki programu", |ui| {
                        let mut changed = false;
                        for (id, label, value) in [
                            ("interface-font", "Interfejs", &mut self.settings.interface_font),
                            ("numeric-font", "Liczby i parametry", &mut self.settings.numeric_font),
                            ("heading-font", "Nagłówki i pogrubione opisy", &mut self.settings.heading_font),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                egui::ComboBox::from_id_salt(id).selected_text(value.label()).show_ui(ui, |ui| {
                                    for font in prospekt_dj::settings::Typeface::ALL { changed |= ui.selectable_value(value, font, font.label()).changed(); }
                                });
                            });
                        }
                        ui.label("Helvetica: czcionka komercyjna, wczytywana z systemu. Gdy jej brak, program używa dołączonej Nimbus Sans (AGPL-3.0).");
                        ui.label("Licencje dołączonych fontów: O programie → Licencje. Logo i BANGER SITE mają osobne ustawienia.");
                        if changed { crate::fonts::install(ctx, self.settings.logo_font.as_deref(), &self.settings); }
                    });

                    ui.label("Odstęp między rekomendacjami utworów");
                    if ui.add(egui::DragValue::new(&mut self.settings.recommendation_interval_seconds).range(1..=3600).suffix(" s")).changed() {
                        self.recommendation_advanced = Some(Instant::now());
                    }
                    ui.label("Domyślnie 60 s. Po zmianie czasu odliczanie zaczyna się od nowa.");
                    ui.separator();
                    ui.label(tr("Język"));
                    let previous=self.settings.language.clone();
                    egui::ComboBox::from_id_salt("language").selected_text(i18n::LANGUAGES.iter().find(|(code,_)|*code == self.settings.language).map(|(_,name)|*name).unwrap_or("Polski")).show_ui(ui,|ui|{for (code,name) in i18n::LANGUAGES {ui.selectable_value(&mut self.settings.language,code.into(),i18n::shape(name));}});
                    if self.settings.language != previous {i18n::set_language(&self.settings.language);}
                    ui.separator();ui.heading(tr("Wygląd"));
                    if ui.checkbox(&mut self.settings.light,tr("Tryb jasny")).changed(){console_theme(ctx,self.settings.light);}
                    ui.horizontal(|ui|{ui.label("Skórka:");let before=self.settings.skin;egui::ComboBox::from_id_salt(("skin",ui.id())).selected_text(self.settings.skin.name()).show_ui(ui,|ui|{for (skin,name) in prospekt_dj::theme::Skin::ALL{ui.selectable_value(&mut self.settings.skin,skin,name);}});if before!=self.settings.skin{prospekt_dj::theme::set_skin(self.settings.skin);console_theme(ctx,self.settings.light);}});
                    ui.label("Kolor wnętrza jogów");
                    egui::ComboBox::from_id_salt("jog-background")
                        .selected_text(if self.settings.white_jog { "Białe tło / czarny tekst" } else { "Czarne tło / jasny tekst" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.settings.white_jog, false, "Czarne tło / jasny tekst");
                            ui.selectable_value(&mut self.settings.white_jog, true, "Białe tło / czarny tekst");
                        });
                    ui.label("Nazwa biblioteki utworów");
                    egui::ComboBox::from_id_salt("library-heading-name")
                        .selected_text(if self.settings.banger_site_name { "BANGER SITE" } else { "Kolekcja" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.settings.banger_site_name, true, "BANGER SITE (z ikoną ognia)");
                            ui.selectable_value(&mut self.settings.banger_site_name, false, "Kolekcja");
                        }).response.on_hover_text("Nazwa nagłówka biblioteki utworów");
                    ui.label("Czcionka nagłówka BANGER SITE");
                    egui::ComboBox::from_id_salt("banger-site-heading-font")
                        .selected_text(if self.settings.banger_site_display_font { "Sedgwick Ave Display (domyślna)" } else { "Arial Bold (poprzednia)" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.settings.banger_site_display_font, true, "Sedgwick Ave Display (domyślna)");
                            ui.selectable_value(&mut self.settings.banger_site_display_font, false, "Arial Bold (poprzednia)");
                        });

                    if ui.checkbox(&mut self.settings.show_emojis,"Pokaż emotikony").changed() {
                        if let Some(result) = self.broadcast.as_ref().and_then(|s| s.result.try_recv().ok()) {
            self.send(Command::Broadcast(None));
            self.broadcast = None;
            self.broadcast_status = result.err().unwrap_or_else(|| "Transmisja zakończona".into());
            self.status = self.broadcast_status.clone();
        }
        if self.runtime.is_none() && self.broadcast.is_some() {
            self.send(Command::Broadcast(None));
            self.broadcast = None;
            self.broadcast_status = "Transmisja zatrzymana — wyjście audio niedostępne".into();
        }
        if self.preview_path.as_ref().is_some_and(|path| self.library.get(self.selected) != Some(path)) {
            self.stop_preview();
        }
        self.storage.poll();
        self.storage_dialog(ctx);
        if let Some((_, origin, moved)) = &mut self.cover_drag {
            *moved |= ctx.input(|input| input.pointer.interact_pos()).is_some_and(|point| point.distance(*origin) >= 6.);
        }
        self.emoji.hidden = !self.settings.show_emojis;
                    }
                    ui.label("Ukrywanie emotikon nie usuwa ich z nazw ani komentarzy. Wybór jest zapamiętywany.");
                    ui.checkbox(&mut self.settings.cover_strip,"Pokaż pasek okładek w bibliotece");
                    ui.horizontal(|ui| {
                        ui.label("Przeciąganie okładki na dek A–D:");
                        egui::ComboBox::from_id_salt("cover-drag-button")
                            .selected_text(match self.settings.cover_drag_button {
                                crate::settings::CoverDragButton::Right => "prawym przyciskiem",
                                crate::settings::CoverDragButton::Left => "lewym przyciskiem",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.settings.cover_drag_button,
                                    crate::settings::CoverDragButton::Right,
                                    "prawym przyciskiem",
                                );
                                ui.selectable_value(
                                    &mut self.settings.cover_drag_button,
                                    crate::settings::CoverDragButton::Left,
                                    "lewym przyciskiem",
                                );
                            });
                    });
                    ui.checkbox(&mut self.settings.show_library_clock,"Pokaż zegar z ikoną w bibliotece");
                    ui.checkbox(&mut self.settings.cover_reflections,"Paski odbić okładek").on_hover_text("Włącza odbicia pod okładkami, listą utworów oraz na górze i dole programu i pod waveformami.");
                    ui.checkbox(&mut self.settings.save_recommendation_history,"Zapisuj historię rekomendacji do plików TXT").on_hover_text("Folder recommendations w katalogu danych programu. Zapisy starsze niż 30 dni są usuwane automatycznie; wyłączenie zatrzymuje nowe wpisy.");
                    ui.add_enabled(!self.settings.reduced_motion, egui::Checkbox::new(&mut self.settings.cover_scroll_animation,"Płynne zanikanie okładek podczas przewijania"));
                    ui.checkbox(&mut self.settings.track_preview,"Panel boczny biblioteki: Track Info / historia rekomendacji");
                    ui.checkbox(&mut self.settings.preview_column,"PREVIEW waveform column");
                    if ui.add(egui::Slider::new(&mut self.settings.history_threshold,1.0..=300.).text("MASTER playback threshold / s")).changed(){self.send(Command::HistoryThreshold(self.settings.history_threshold));}
                    ui.label("Short tracks: half duration or threshold, whichever is shorter. PFL-only playback is not counted.");
                    ui.checkbox(&mut self.settings.key_camelot,tr("Tonacje Camelot"));
                    if ui.checkbox(&mut self.settings.auto_cover_download,tr("Pobieraj brakujące okładki")).changed(){self.artwork.enabled(self.settings.auto_cover_download);}
                    ui.label(tr("MusicBrainz / Cover Art Archive: wysyłane są wykonawca, album i tytuł; bez audio i prywatnych ścieżek."));
                    if ui.button(tr("Wyczyść pobrane okładki")).clicked(){if self.artwork.clear(){self.art_textures.clear();self.art_status.clear();}else{self.status=tr("Kolejka okładek zajęta — spróbuj ponownie.");}}
                    ui.label(tr("Liczba decków"));
                    let previous=self.settings.deck_count;
                    ui.horizontal(|ui|{ui.selectable_value(&mut self.settings.deck_count,2,"2 decks");ui.selectable_value(&mut self.settings.deck_count,4,"4 decks");});
                    if previous!=self.settings.deck_count { self.apply_deck_count(); }
                    ui.label(tr("4 decki: crossfader A/C ↔ B/D. Automix działa na A/B."));

                },
                _=> {
                    ui.heading(format!("PROSPEKT DJ {} · PRE-ALPHA TEST",env!("CARGO_PKG_VERSION")));
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
            });});
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
        ui.heading("Dwa kontrolery MIDI jednocześnie");
        ui.label("Kontroler 1 używa mapowania głównego. Kontroler 2 ma własne mapowanie; identyczne numery MIDI nie kolidują.");
        ui.horizontal_wrapped(|ui| {
            ui.label("Kontroler 2:");
            egui::ComboBox::from_id_salt("midi-second-input")
                .selected_text(
                    self.midi_names
                        .get(self.midi_second_selected)
                        .map(String::as_str)
                        .unwrap_or("Brak urządzeń"),
                )
                .show_ui(ui, |ui| {
                    for (i, name) in self.midi_names.iter().enumerate() {
                        ui.selectable_value(&mut self.midi_second_selected, i, name);
                    }
                });
            if ui.button("Połącz kontroler 2").clicked() {
                if let Some(name) = self.midi_names.get(self.midi_second_selected).cloned() {
                    if self.midi_reconnect_name.as_ref() == Some(&name) {
                        self.status = "Ten port jest już przypisany do kontrolera 1".into();
                    } else {
                        self.midi_second_connection = None;
                        while self.midi_second_rx.try_recv().is_ok() {}
                        match midi::connect_named(&name, self.midi_second_tx.clone()) {
                            Ok(connection) => {
                                self.midi_second_connection = Some(connection);
                                self.midi_second_name = Some(name);
                                self.status = "Kontroler 2 połączony".into();
                            }
                            Err(error) => self.status = format!("Kontroler 2: {error}"),
                        }
                    }
                }
            }
            if ui.button("Rozłącz kontroler 2").clicked() {
                self.midi_second_connection = None;
                self.midi_second_name = None;
                while self.midi_second_rx.try_recv().is_ok() {}
            }
            ui.label(if self.midi_second_connection.is_some() {
                "● Połączony"
            } else {
                "○ Niepołączony"
            });
        });
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("midi-second-profile")
                .selected_text(
                    self.midi_second_profile
                        .and_then(|i| self.settings.controller_profiles.get(i))
                        .map(|p| format!("{} {}", p.manufacturer, p.model))
                        .unwrap_or("Wybierz mapowanie kontrolera 2".into()),
                )
                .show_ui(ui, |ui| {
                    for (i, p) in self
                        .settings
                        .controller_profiles
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| !p.mappings.is_empty())
                    {
                        ui.selectable_value(
                            &mut self.midi_second_profile,
                            Some(i),
                            format!("{} {}", p.manufacturer, p.model),
                        );
                    }
                });
            if ui.button("Zastosuj do kontrolera 2").clicked() {
                if let Some(profile) = self
                    .midi_second_profile
                    .and_then(|i| self.settings.controller_profiles.get(i))
                {
                    self.settings.midi_second = profile.mappings.clone();
                    self.persist();
                }
            }
            ui.label(format!("Mapowania: {}", self.settings.midi_second.len()));
        });
        ui.separator();
        let learn_heading = ui.heading("MIDI Learn");
        if self.midi_learn_reveal {
            learn_heading.scroll_to_me(Some(Align::Min));
            self.midi_learn_reveal = false;
        }
        ui.horizontal(|ui| {
            ui.label("Przypisuj dla:");
            ui.selectable_value(&mut self.midi_learn_second, false, "Kontroler 1");
            ui.selectable_value(&mut self.midi_learn_second, true, "Kontroler 2");
        });
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
                let same =
                    self.midi_names.get(self.midi_selected) == self.midi_second_name.as_ref();
                let result = if same {
                    Err(anyhow::anyhow!(
                        "Ten port jest już przypisany do kontrolera 2"
                    ))
                } else {
                    self.midi_connection = None;
                    midi::connect(self.midi_selected, self.midi_tx.clone())
                };
                match result {
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
                for (i, m) in midi::mapping_bank(
                    &mut self.settings.midi,
                    &mut self.settings.midi_second,
                    self.midi_learn_second,
                )
                .iter()
                .enumerate()
                {
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
            midi::mapping_bank(
                &mut self.settings.midi,
                &mut self.settings.midi_second,
                self.midi_learn_second,
            )
            .remove(i);
        }
        ui.label("Obsługa Note i CC, względnych enkoderów oraz import map z poprzedniej wersji HTML. Profile kontrolerów pochodzą z zaimportowanych map; sprzętowe LED nie są obsługiwane. Połączenie odnawia się po ponownym podłączeniu kontrolera o tej samej nazwie.");
    }
    fn midi_message(&mut self, bytes: &[u8], second: bool) {
        let Some((status, number, value)) = midi::key(bytes) else {
            return;
        };
        self.last_midi = format!(
            "MIDI {}: {status} / {number} / {value}",
            if second { 2 } else { 1 }
        );
        let mappings = midi::mapping_bank(
            &mut self.settings.midi,
            &mut self.settings.midi_second,
            second,
        );
        if self.learn && self.midi_learn_second == second && value > 0 {
            mappings.retain(|m| {
                !(m.status == status && m.number == number) && m.action != self.learn_action
            });
            mappings.push(Mapping {
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
        let Some(map) = mappings
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
                    "pre_gain" => (d.gain_db + 24.) / 36.,
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
                    "pre_gain" => {
                        self.send(Command::Deck(i, DeckControl::Gain(absolute * 36. - 24.)))
                    }
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
                        "keylock" => self.deck(i, DeckControl::KeyLock),
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
            || !self.controller_notices.is_empty()
            || self.dual_midi_notice
            || self.key_window
            || self.reflection_color_editor.is_some()
            || self.device_window
            || self.cloud.open
            || !self.storage.notices.is_empty()
            || self.waveform_detail.is_some()
            || self.rename_file.is_some()
            || self.new_folder.is_some()
            || self.eject_pending.is_some()
            || ctx.wants_keyboard_input()
            || ctx.memory(|memory| memory.any_popup_open())
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
impl Studio {
    /// Moves the interface zoom a fraction of the remaining distance each frame, so
    /// changing the scale in settings glides instead of jumping.
    fn animate_zoom(&self, ctx: &egui::Context) {
        let target = self.settings.ui_scale.clamp(0.75, 1.75);
        let current = ctx.zoom_factor();
        if (current - target).abs() > 0.002 {
            ctx.set_zoom_factor(current + (target - current) * 0.25);
            ctx.request_repaint();
        } else if current != target {
            ctx.set_zoom_factor(target);
        }
    }
}
impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.animate_zoom(ctx);
        if let Some(result) = self
            .broadcast
            .as_ref()
            .and_then(|s| s.result.try_recv().ok())
        {
            self.send(Command::Broadcast(None));
            self.broadcast = None;
            self.broadcast_status = result
                .err()
                .unwrap_or_else(|| "Transmisja zakończona".into());
            self.status = self.broadcast_status.clone();
        }
        if self.runtime.is_none() && self.broadcast.is_some() {
            self.send(Command::Broadcast(None));
            self.broadcast = None;
            self.broadcast_status = "Transmisja zatrzymana — wyjście audio niedostępne".into();
        }
        if self
            .preview_path
            .as_ref()
            .is_some_and(|path| self.library.get(self.selected) != Some(path))
        {
            self.stop_preview();
        }
        self.storage.poll();
        self.storage_dialog(ctx);
        if let Some((_, origin, moved)) = &mut self.cover_drag {
            *moved |= ctx
                .input(|input| input.pointer.interact_pos())
                .is_some_and(|point| point.distance(*origin) >= 6.);
        }
        self.emoji.hidden = !self.settings.show_emojis;
        self.bend_buttons = [0.; 4];
        self.reverse_requested = [false; 4];
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
                "performance-header-and-waveform-reflections",
                self.settings.cover_strip,
                self.settings.cover_reflections,
                self.settings.wave_style.reflections,
                self.dock_tab,
                self.settings.deck_count,
                self.settings.show_effects,
                self.settings.show_main_waveforms,
            )))
            .resizable(true)
            .default_height(
                (if self.settings.deck_count == 4 {
                    640.
                } else {
                    380.
                }) + if self.settings.cover_strip && self.settings.cover_reflections {
                    42.
                } else {
                    0.
                } + if self.settings.wave_style.reflections {
                    let source_height = match (
                        self.settings.deck_count == 4,
                        ctx.screen_rect().height() < 850.,
                    ) {
                        (true, true) => 24.,
                        (true, false) => 32.,
                        (false, true) => 40.,
                        _ => 60.,
                    };
                    (self.settings.deck_count - 1) as f32
                        * (2. * source_height * self.settings.wave_style.height / 8. + 2. * 2.5
                            - 4.)
                } else {
                    0.
                },
            )
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
                let show_strip = self.settings.cover_strip && self.settings.cover_reflections;
                let strip_height = 2. * 72. / 20.;
                let reserved = if show_strip { strip_height + 14. } else { 0. };
                self.waveform_cover_reflection = None;
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("console-scroll")
                    .max_height((ctx.screen_rect().height() - 380. - reserved).max(180.))
                    .show(ui, |ui| {
                        let origin = ui.next_widget_position();
                        let mut content = ui.new_child(
                            egui::UiBuilder::new()
                                .id_salt("decks-content")
                                .max_rect(ui.available_rect_before_wrap())
                                .layout(egui::Layout::top_down(Align::Min)),
                        );
                        let waveform_bottom = self.decks_ui(&mut content);
                        ui.allocate_rect(
                            egui::Rect::from_min_max(
                                origin,
                                egui::pos2(ui.max_rect().right(), waveform_bottom),
                            ),
                            Sense::hover(),
                        );
                        let viewport = ui.clip_rect();
                        self.header_cover_reflection =
                            self.header_cover_reflection.and_then(|rect| {
                                let clipped = egui::Rect::from_min_max(
                                    egui::pos2(rect.left(), rect.top().max(viewport.top())),
                                    egui::pos2(rect.right(), rect.bottom().min(viewport.bottom())),
                                );
                                (clipped.height() > 0.).then_some(clipped)
                            });
                        waveform_bottom
                    });
                if show_strip {
                    let top = scroll.inner.min(scroll.inner_rect.bottom()) + 8.;
                    self.waveform_cover_reflection = Some(egui::Rect::from_min_max(
                        egui::pos2(ctx.screen_rect().left(), top),
                        egui::pos2(ctx.screen_rect().right(), top + strip_height),
                    ));
                    ui.allocate_rect(
                        egui::Rect::from_min_size(
                            egui::pos2(scroll.inner_rect.left(), top),
                            Vec2::new(scroll.inner_rect.width(), strip_height + 6.),
                        ),
                        Sense::hover(),
                    );
                }
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| self.library_ui(ui));
        self.cover_drag_drop(ctx);
        self.waveform_detail_ui(ctx);
        self.rename_file_ui(ctx);
        if self.settings.cover_strip
            && self.settings.cover_reflections
            && !matches!(self.scope, Scope::History(_))
        {
            for rect in [
                self.header_cover_reflection,
                self.waveform_cover_reflection,
                self.footer_cover_reflection,
            ]
            .into_iter()
            .flatten()
            .filter(|rect| !self.library_focus || Some(*rect) == self.footer_cover_reflection)
            {
                let painter = ctx
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Background,
                        egui::Id::new("waveform-cover-reflections"),
                    ))
                    .with_clip_rect(rect);
                let min = self
                    .cover_reflections
                    .iter()
                    .flat_map(|m| &m.vertices)
                    .map(|v| v.pos.x)
                    .fold(f32::INFINITY, f32::min);
                let max = self
                    .cover_reflections
                    .iter()
                    .flat_map(|m| &m.vertices)
                    .map(|v| v.pos.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                let tile_width = max - min;
                if tile_width.is_finite() && tile_width > 0. {
                    for tile in 0..=(rect.width() / tile_width).ceil() as usize {
                        for mesh in &self.cover_reflections {
                            let mut mesh = mesh.clone();
                            for vertex in &mut mesh.vertices {
                                vertex.pos.x += rect.left() - self.cover_reflection_left
                                    + tile as f32 * tile_width;
                                vertex.pos.y = rect.top() + vertex.pos.y * 2.;
                            }
                            painter.add(egui::Shape::mesh(mesh));
                        }
                    }
                }
            }
        }

        if self.settings.cover_strip
            && self.settings.cover_reflections
            && !matches!(self.scope, Scope::History(_))
        {
            if let Some(rect) = self.sidebar_cover_reflection {
                let painter = ctx
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Background,
                        egui::Id::new("sidebar-cover-reflections"),
                    ))
                    .with_clip_rect(rect);
                let min = self
                    .cover_reflections
                    .iter()
                    .flat_map(|m| &m.vertices)
                    .map(|v| v.pos.x)
                    .fold(f32::INFINITY, f32::min);
                let max = self
                    .cover_reflections
                    .iter()
                    .flat_map(|m| &m.vertices)
                    .map(|v| v.pos.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                let tile_height = max - min;
                if tile_height.is_finite() && tile_height > 0. {
                    for tile in 0..=(rect.height() / tile_height).ceil() as usize {
                        for mesh in &self.cover_reflections {
                            let mut mesh = mesh.clone();
                            for vertex in &mut mesh.vertices {
                                vertex.pos = egui::pos2(
                                    rect.left() + vertex.pos.y * 2.,
                                    rect.top() + vertex.pos.x - self.cover_reflection_left
                                        + tile as f32 * tile_height,
                                );
                            }
                            painter.add(egui::Shape::mesh(mesh));
                        }
                    }
                }
            }
        }

        if !self.settings.import_prompt_seen {
            egui::Window::new("Import istniejącej biblioteki DJ").collapsible(false).show(ctx,|ui|{
                ui.label("Czy chcesz zaimportować bibliotekę, historię odtworzeń i informacje o utworach z innego programu DJ?");
                ui.horizontal(|ui|{if ui.button("Wybierz program…").clicked(){self.settings.import_prompt_seen=true;self.settings_open=true;self.settings_tab=9;}
                    if ui.button("Pomiń — nie pytaj ponownie").clicked(){self.settings.import_prompt_seen=true;}});
            });
        }
        if self.settings_open {
            self.settings_ui(ctx);
        }
        self.detected_controller_ui(ctx);
        self.dual_midi_notice_ui(ctx);
        self.utility_windows(ctx);
        self.reflection_color_window(ctx);
        self.cloud_window(ctx);
        self.eject_confirmation_ui(ctx);
        for i in 0..4 {
            let held = self.reverse_requested[i] && focused && self.eject_pending.is_none();
            if held != self.reverse_sent[i] {
                if held {
                    self.stop_auto();
                }
                self.send(Command::Deck(i, DeckControl::Reverse(held)));
                self.reverse_sent[i] = held;
            }
        }
        self.apply_bends(ctx);
        self.confirm_close(ctx);
        ctx.request_repaint_after(Duration::from_secs_f64(
            1. / self.settings.refresh_hz.clamp(10, 120) as f64,
        ));
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist();
        self.send(Command::Recording(None));
        let (ack, done) = bounded(1);
        self.send(Command::Barrier(ack));
        if done.recv_timeout(Duration::from_secs(2)).is_err() {
            eprintln!("Audio command queue did not drain before shutdown");
        }
        self.runtime = None;
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
fn parse_hex_color(text: &str) -> Option<Color32> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(Color32::from_rgb(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
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
        PadMode::Effects => "EFFECTS",
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
fn jog_action_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    active: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.), Sense::click());
        let shade = if response.is_pointer_button_down_on() || active {
            60
        } else if response.hovered() {
            44
        } else {
            26
        };
        ui.painter().rect_filled(
            rect,
            prospekt_dj::theme::radius_f(3.),
            Color32::from_gray(shade),
        );
        ui.painter().rect_stroke(
            rect,
            prospekt_dj::theme::radius_f(3.),
            Stroke::new(1_f32, Color32::from_gray(90)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::new(12., egui::FontFamily::Name("Arial Bold".into())),
            Color32::from_gray(225),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        response
    })
    .inner
}
fn transport_button(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    color: Color32,
    reduced: bool,
    width: f32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, modern_size(28., 24.)), Sense::click());
    let hover = if reduced {
        if response.hovered() {
            1.
        } else {
            0.
        }
    } else {
        ui.ctx().animate_bool_with_time(
            response.id.with("transport-hover"),
            response.hovered(),
            0.12,
        )
    };
    let selected = if reduced {
        if active {
            1.
        } else {
            0.
        }
    } else {
        ui.ctx()
            .animate_bool_with_time(response.id.with("transport-state"), active, 0.12)
    };
    let p = ui.painter();
    p.rect_filled(rect, prospekt_dj::theme::radius_f(3.), Color32::BLACK);
    p.rect_filled(
        rect,
        prospekt_dj::theme::radius_f(3.),
        color.gamma_multiply(0.08 + 0.25 * selected + 0.12 * hover),
    );
    p.rect_stroke(
        rect.shrink(0.5),
        prospekt_dj::theme::radius_f(3.),
        Stroke::new(
            if response.is_pointer_button_down_on() {
                2_f32
            } else {
                1_f32
            },
            color.gamma_multiply(0.4 + 0.6 * hover.max(selected)),
        ),
        egui::StrokeKind::Inside,
    );
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.),
        if active { Color32::WHITE } else { color },
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}
fn preview_wave(
    ui: &mut egui::Ui,
    record: Option<&prospekt_dj::library_data::Record>,
    style: &prospekt_dj::wave_style::WaveStyle,
    width: f32,
    height: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let p = ui.painter().with_clip_rect(rect);
    p.rect_filled(rect, 0., Color32::BLACK);
    let Some(record) = record else {
        p.line_segment(
            [rect.left_center(), rect.right_center()],
            Stroke::new(1_f32, Color32::DARK_GRAY),
        );
        return;
    };
    let a = &record.analysis;
    let gain = 1. / a.peaks.iter().copied().fold(0_f32, f32::max).max(0.001);
    for x in (0..width as usize).step_by(2) {
        let n = (x as f32 / width * a.peaks.len() as f32) as usize;
        let level = a.peaks.get(n).copied().unwrap_or(0.) * gain;
        let [r, g, b] = style.color(a.bands.get(n).copied().unwrap_or_default(), [80, 140, 200]);
        let px = rect.left() + x as f32;
        p.line_segment(
            [
                egui::pos2(px, rect.center().y - level * height * 0.45),
                egui::pos2(px, rect.center().y + level * height * 0.45),
            ],
            Stroke::new(1_f32, Color32::from_rgb(r, g, b)),
        );
    }
    for (slot, time) in record.hot.iter().enumerate() {
        if let Some(t) = time {
            if a.duration > 0. && *t <= a.duration {
                let x = rect.left() + (*t / a.duration) as f32 * width;
                p.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    Stroke::new(1_f32, cue_color(slot)),
                );
            }
        }
    }
}
fn library_panel_tab(ui: &mut egui::Ui, label: &str, expanded: bool) -> egui::Response {
    let (column, _) = ui.allocate_exact_size(
        Vec2::new(18., ui.available_height().max(80.)),
        Sense::hover(),
    );
    let rect = egui::Rect::from_center_size(column.center(), Vec2::new(18., 80.));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("library-panel-tab", label))
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
    );
    vertical_wave_control(&mut child, label, expanded, 80., true)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if expanded {
            "Schowaj panel"
        } else {
            "Rozwiń panel"
        })
}

fn vertical_wave_control(
    ui: &mut egui::Ui,
    text: &str,
    active: bool,
    height: f32,
    clickable: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(18., height),
        if clickable {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let color = if active {
        Color32::from_rgb(0, 125, 175)
    } else {
        Color32::from_gray(20)
    };
    ui.painter()
        .rect_filled(rect, prospekt_dj::theme::radius_f(3.), color);
    ui.painter().rect_stroke(
        rect,
        prospekt_dj::theme::radius_f(3.),
        Stroke::new(
            1_f32,
            if active {
                ACCENTS[0]
            } else {
                Color32::from_gray(55)
            },
        ),
        egui::StrokeKind::Inside,
    );
    let size = ((height - 5.) / (text.len() as f32 * 0.62)).clamp(4., 9.);
    let foreground = if active {
        Color32::WHITE
    } else {
        Color32::GRAY
    };
    let galley = ui.painter().layout_no_wrap(
        text.into(),
        FontId::new(size, egui::FontFamily::Name("Arial Bold".into())),
        foreground,
    );
    let origin = rect.center() + egui::vec2(-galley.size().y / 2., galley.size().x / 2.);
    ui.painter().add(
        egui::epaint::TextShape::new(origin, galley, foreground)
            .with_angle(-std::f32::consts::FRAC_PI_2),
    );
    response
}
/// Inverted, fading lower half of the same detail samples and timeline as the main waveform.
fn waveform_reflection(
    ui: &egui::Ui,
    reflection: (egui::Rect, bool),
    deck: &DeckView,
    accent: Color32,
    span: f64,
    style: &prospekt_dj::wave_style::WaveStyle,
) {
    let (rect, upward) = reflection;
    let Some(track) = &deck.track else {
        return;
    };
    let range = span.max(0.001);
    let start = deck.position - range * 0.28;
    let duration = track.duration();
    let gain = 1. / track.peaks.iter().copied().fold(0_f32, f32::max).max(0.001);
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    let mut mesh = egui::Mesh::default();
    for x in (0..rect.width() as usize).step_by(2) {
        let time = start + x as f64 / f64::from(rect.width().max(1.)) * range;
        if time < 0. || time >= duration {
            continue;
        }
        let bin = track
            .detail
            .get((time * track.detail_rate) as usize)
            .copied()
            .unwrap_or_default();
        let peak = (bin.level * gain).clamp(0., 1.);
        let rgb = style
            .reflection_color
            .unwrap_or_else(|| style.color(bin.bands, [accent.r(), accent.g(), accent.b()]));
        let color = Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], 190);
        let left = rect.left() + x as f32;
        let bottom = rect.top() + peak * rect.height();
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(egui::pos2(left, rect.top()), color);
        mesh.colored_vertex(egui::pos2(left + 1.5, rect.top()), color);
        mesh.colored_vertex(egui::pos2(left, bottom), Color32::TRANSPARENT);
        mesh.colored_vertex(egui::pos2(left + 1.5, bottom), Color32::TRANSPARENT);
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
    }
    if upward {
        for vertex in &mut mesh.vertices {
            vertex.pos.y = rect.top() + rect.bottom() - vertex.pos.y;
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn waveform(
    ui: &mut egui::Ui,
    deck: &DeckView,
    accent: Color32,
    height: f32,
    span: Option<f64>,
    style: &prospekt_dj::wave_style::WaveStyle,
) -> Option<WaveAction> {
    let (response, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), height), Sense::click());
    let rect = response.rect;
    let wheel_zoom = span.filter(|_| response.hovered()).and_then(|seconds| {
        let delta = ui.input(|input| input.raw_scroll_delta.y);
        if delta == 0. || !delta.is_finite() {
            return None;
        }
        ui.input_mut(|input| {
            input.raw_scroll_delta.y = 0.;
            input.smooth_scroll_delta.y = 0.;
        });
        Some((seconds * (-f64::from(delta.clamp(-240., 240.)) / 300.).exp()).clamp(2., 16.))
    });
    if span.is_some() {
        response
            .clone()
            .on_hover_text("Kółko myszy: przybliż / oddal waveform");
    }
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
        return wheel_zoom.map(WaveAction::Zoom);
    };
    let duration = track.duration();
    let range = span.unwrap_or(duration).max(0.001);
    let start = if span.is_some() {
        deck.position - range * 0.28
    } else {
        0.
    };
    let width = rect.width() as usize;
    // Original skin anchors the grid at the CUE point, as before.
    let grid_anchor = if prospekt_dj::theme::classic() {
        deck.cue
    } else {
        track.beat_offset
    };
    if span.is_some() && style.beat_background {
        let beat = 60. / deck.bpm.clamp(40., 300.);
        let first = ((start - grid_anchor) / beat).floor() as i64;
        for n in first..=first + (range / beat).ceil() as i64 {
            let time = grid_anchor + n as f64 * beat;
            let left = rect.left() + ((time - start) / range) as f32 * rect.width();
            let right = rect.left() + ((time + beat - start) / range) as f32 * rect.width();
            let bright = Color32::from_gray(if n.rem_euclid(2) == 0 { 30 } else { 18 });
            let dark = Color32::from_gray(3);
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(egui::pos2(left, rect.top()), bright);
            mesh.colored_vertex(egui::pos2(right, rect.top()), dark);
            mesh.colored_vertex(egui::pos2(right, rect.bottom()), dark);
            mesh.colored_vertex(egui::pos2(left, rect.bottom()), bright);
            mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
            painter.add(egui::Shape::mesh(mesh));
            if style.grid {
                let tick = Stroke::new(1_f32, Color32::from_gray(105));
                painter.line_segment(
                    [
                        egui::pos2(left, rect.top()),
                        egui::pos2(left, rect.top() + 6.),
                    ],
                    tick,
                );
                painter.line_segment(
                    [
                        egui::pos2(left, rect.bottom() - 6.),
                        egui::pos2(left, rect.bottom()),
                    ],
                    tick,
                );
            }
        }
    }
    if span.is_some() && style.grid {
        let beat = 60. / deck.bpm.clamp(40., 300.);
        let first = ((start - grid_anchor) / beat).floor() as i64;
        for n in first..=first + (range / beat).ceil() as i64 + 1 {
            let t = grid_anchor + n as f64 * beat;
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
            Stroke::new(
                1.5_f32,
                Color32::from_rgb(
                    style.color(bands, [accent.r(), accent.g(), accent.b()])[0],
                    style.color(bands, [accent.r(), accent.g(), accent.b()])[1],
                    style.color(bands, [accent.r(), accent.g(), accent.b()])[2],
                ),
            ),
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
        .filter(|_| style.markers)
        .filter_map(|(n, t)| {
            t.and_then(|t| {
                timeline
                    .project(t)
                    .map(|fraction| (n, t, rect.left() + fraction * rect.width()))
            })
        })
        .collect();
    let flag_scale = if span.is_some() { 1.75 } else { 1. };
    let flag_half_width = 9. * flag_scale;
    let flag_height = 14. * flag_scale;
    for &(n, _, x) in &markers {
        let color = cue_color(n);
        painter.line_segment(
            [
                egui::pos2(x, rect.top() + flag_height),
                egui::pos2(x, rect.bottom()),
            ],
            Stroke::new(2_f32, color),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(x - flag_half_width, rect.top()),
                egui::pos2(x + flag_half_width, rect.top()),
                egui::pos2(x, rect.top() + flag_height),
            ],
            color,
            Stroke::new(1.5_f32, Color32::BLACK),
        ));
        if span.is_some() && height > 25. {
            let label_y = if span.is_some() && style.spread_cue_labels {
                rect.top()
                    + 2.
                    + (height - 24.).max(0.)
                        * prospekt_dj::wave_style::cue_label_level(&track.id, n)
            } else {
                rect.top() + 2.
            };
            painter.text(
                egui::pos2(x + flag_half_width + 3., label_y),
                egui::Align2::LEFT_TOP,
                format!("CUE{}", n + 1),
                FontId::new(18., egui::FontFamily::Name("Arial Bold".into())),
                color,
            );
        }
    }
    if let Some(seconds) = wheel_zoom {
        return Some(WaveAction::Zoom(seconds));
    }
    let hovered = ui
        .input(|v| v.pointer.hover_pos())
        .filter(|p| rect.contains(*p));
    let hits: Vec<_> = hovered
        .map(|p| {
            markers
                .iter()
                .filter(|(_, _, x)| (*x - p.x).abs() <= flag_half_width + 2.)
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

fn deck_letter(i: usize) -> char {
    (b'A' + i.min(3) as u8) as char
}

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use egui::{Align, Color32, FontId, RichText, Sense, Stroke, Vec2};
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

const ACCENTS: [Color32; 2] = [
    Color32::from_rgb(49, 188, 255),
    Color32::from_rgb(66, 196, 227),
];
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
    new_crate: String,
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
    deck_tokens: [u64; 2],
    sample_tokens: [u64; 4],
    loading: [bool; 2],
    covers: [Option<egui::TextureHandle>; 2],
    taps: [Vec<Instant>; 2],
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
    bend: [f64; 2],
    allow_close: bool,
    had_focus: bool,
    close_confirm: bool,
    close_after_record: bool,
    last_save: Instant,
}
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>, settings: Settings) -> Self {
        console_theme(&cc.egui_ctx, settings.light);
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
            scope: Scope::All,
            catalog,
            browser_dirty: true,
            visible_rows: Vec::new(),
            db_dirty: false,
            db_save: Some((db_tx, db_thread)),
            index_jobs: 0,
            new_crate: String::new(),
            scroll_selected: false,
            fx_deck: 0,
            selected: 0,
            focused: 0,
            settings_open: false,
            settings_tab: 0,
            library_focus: false,
            dock_tab,
            zoom: 8.,
            status: "Gotowy · Rust Edition · pliki pozostają na komputerze".into(),
            deck_tokens: [0; 2],
            sample_tokens: [0; 4],
            loading: [false; 2],
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
            bend: [0.; 2],
            allow_close: false,
            had_focus: true,
            close_confirm: false,
            close_after_record: false,
            last_save: Instant::now(),
        };
        for i in 0..2 {
            studio.snapshot.decks[i].fx = studio.settings.effects[i];
        }
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
        self.settings.effects = std::array::from_fn(|i| self.snapshot.decks[i].fx);
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
        for i in 0..2 {
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
        for i in 0..2 {
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
        if let Ok(view) = self.shared.try_lock() {
            for i in 0..2 {
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
                            self.covers[deck] = cover(ctx, deck, track.cover.as_deref());
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
                            for i in 0..2 {
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
        self.settings.dock_tab = self.dock_tab;
        self.settings.library = self.library.clone();
        self.settings.effects = std::array::from_fn(|i| self.snapshot.decks[i].fx);
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
        let other = &self.snapshot.decks[1 - i];
        let rate = other.bpm * other.rate / self.snapshot.decks[i].bpm;
        self.deck(i, DeckControl::Rate(rate));
        self.status = "Dopasowano tempo. SYNC nie wyrównuje fazy bitów.".into();
    }
    fn top(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(if self.settings.light {
                        Color32::from_gray(222)
                    } else {
                        Color32::from_gray(17)
                    })
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("PROSPEKT").size(18.).strong());
                    ui.label(RichText::new("DJ").size(18.).color(ACCENTS[0]).strong());
                    ui.label(RichText::new("RUST 2.1").small().weak());
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("SETUP").clicked() {
                            self.settings_open = true;
                            self.outputs = audio::output_names();
                            self.midi_names = midi::inputs();
                        }
                        if ui
                            .button(if self.settings.light {
                                "CIEMNY"
                            } else {
                                "JASNY"
                            })
                            .clicked()
                        {
                            self.settings.light = !self.settings.light;
                            console_theme(ctx, self.settings.light);
                        }
                        if ui.button("F11").clicked() {
                            toggle_fullscreen(ctx);
                        }
                        ui.toggle_value(&mut self.library_focus, "BIBLIOTEKA");
                        let label = if self.runtime.is_some() {
                            "AUDIO ON"
                        } else {
                            "AUDIO OFF"
                        };
                        if ui
                            .small_button(RichText::new(label).color(if self.runtime.is_some() {
                                Color32::from_rgb(81, 186, 116)
                            } else {
                                Color32::from_rgb(204, 150, 80)
                            }))
                            .on_hover_text("Wybierz wyjście audio w ustawieniach")
                            .clicked()
                        {
                            self.settings_open = true;
                            self.settings_tab = 0;
                            self.outputs = audio::output_names();
                        }
                        if self.record_result.is_some() {
                            ui.label(
                                RichText::new(if self.record_stopping {
                                    "ZAPIS…".into()
                                } else {
                                    format!("REC {}", time(self.snapshot.recorded_seconds))
                                })
                                .color(Color32::LIGHT_RED)
                                .small(),
                            );
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(&self.status).small()).truncate());
            });
        });
    }
    fn decks_ui(&mut self, ui: &mut egui::Ui) {
        let middle = 148.;
        let width = ((ui.available_width() - middle - 12.) / 2.).max(300.);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(width, 0.),
                egui::Layout::top_down(Align::Min),
                |ui| self.deck_ui(ui, 0),
            );
            ui.allocate_ui_with_layout(
                Vec2::new(middle, 0.),
                egui::Layout::top_down(Align::Min),
                |ui| self.mini_mixer(ui),
            );
            ui.allocate_ui_with_layout(
                Vec2::new(width, 0.),
                egui::Layout::top_down(Align::Min),
                |ui| self.deck_ui(ui, 1),
            );
        });
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
        for (i, accent) in ACCENTS.iter().enumerate() {
            let view = self.snapshot.decks[i].clone();
            if let Some(pos) = waveform(ui, &view, *accent, 57., Some(self.zoom)) {
                self.deck(i, DeckControl::Seek(pos));
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
                let info = (ui.available_width() - if show_platter { 112. } else { 0. }).max(180.);
                ui.horizontal_top(|ui| {
                    if show_platter && i == 1 {
                        self.white_platter(ui, i, &d);
                    }
                    ui.allocate_ui_with_layout(
                        Vec2::new(info, 0.),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_width(info);
                            self.deck_info(ui, i, &d);
                        },
                    );
                    if show_platter && i == 0 {
                        self.white_platter(ui, i, &d);
                    }
                });
            });
    }
    fn white_platter(&mut self, ui: &mut egui::Ui, i: usize, d: &DeckView) {
        let (response, p) = ui.allocate_painter(Vec2::new(106., 142.), Sense::click_and_drag());
        let center = response.rect.center();
        p.circle_filled(center, 51., Color32::from_gray(8));
        p.circle_filled(center, 45., Color32::from_gray(235));
        p.circle_stroke(center, 45., Stroke::new(5_f32, Color32::from_gray(145)));
        let angle = d.position as f32 * std::f32::consts::PI
            + if i == 0 { std::f32::consts::PI } else { -0.7 };
        p.line_segment(
            [center, center + Vec2::angled(angle) * 39.],
            Stroke::new(7_f32, Color32::BLACK),
        );
        p.circle_filled(center, 7., Color32::from_gray(181));
        p.text(
            egui::pos2(center.x, response.rect.bottom() - 3.),
            egui::Align2::CENTER_BOTTOM,
            if d.scratch { "VINYL" } else { "JOG / BEND" },
            FontId::monospace(8.),
            Color32::GRAY,
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
            if let Some(pos) = waveform(ui, d, accent, 17., None) {
                self.deck(i, DeckControl::Seek(pos));
            }
        });
        ui.horizontal(|ui| {
            if let Some(texture) = &self.covers[i] {
                ui.image((texture.id(), Vec2::splat(22.)));
            }
            ui.vertical(|ui| {
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
                        .strong(),
                    )
                    .truncate(),
                );
            });
        });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{:.1}", d.bpm * d.rate))
                    .monospace()
                    .size(20.)
                    .strong()
                    .color(accent),
            );
            ui.label(RichText::new("BPM").size(8.).weak());
            if let Some(key) = d
                .track
                .as_ref()
                .and_then(|t| t.path.as_ref())
                .and_then(|p| self.catalog.entries.get(p))
                .map(|e| e.key.as_str())
                .filter(|k| !k.is_empty())
            {
                ui.label(RichText::new(key).color(accent).strong());
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(time(d.position))
                        .monospace()
                        .size(19.)
                        .strong()
                        .color(accent),
                );
                ui.label(RichText::new("ELAPSED").size(8.).weak());
            });
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_sized(
                    [46., 22.],
                    egui::Button::new(
                        RichText::new(if d.playing { "II" } else { ">" })
                            .strong()
                            .color(if d.playing { Color32::BLACK } else { accent }),
                    )
                    .fill(if d.playing {
                        Color32::from_rgb(99, 198, 126)
                    } else {
                        Color32::from_gray(32)
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
            if ui
                .add(
                    egui::DragValue::new(&mut bpm)
                        .range(40.0..=300.)
                        .speed(0.1)
                        .max_decimals(1),
                )
                .changed()
            {
                self.deck(i, DeckControl::Bpm(bpm));
            }
            if ui.small_button("TAP").clicked() {
                self.tap(i);
            }
            if ui.small_button("BPM?").clicked() {
                self.analyse(i);
            }
            ui.spacing_mut().slider_width = 62.;
            let mut rate = (d.rate - 1.) * 100.;
            let r = ui.add(egui::Slider::new(&mut rate, -50.0..=50.).show_value(false));
            if r.changed() {
                self.deck(i, DeckControl::Rate(1. + rate / 100.));
            }
            if r.double_clicked() {
                self.deck(i, DeckControl::Rate(1.));
            }
            ui.label(RichText::new(format!("{rate:+.1}%")).monospace().size(10.));
        });
        ui.horizontal_wrapped(|ui| {
            let mut mode = d.mode;
            egui::ComboBox::from_id_salt(("pads", i))
                .width(78.)
                .selected_text(RichText::new(mode_label(mode)).size(9.))
                .show_ui(ui, |ui| {
                    for option in PadMode::ALL {
                        ui.selectable_value(&mut mode, option, mode_label(option));
                    }
                });
            if mode != d.mode {
                self.deck(i, DeckControl::PadMode(mode));
            }
            for p in 0..4 {
                let active = if mode == PadMode::HotCue {
                    d.hot[p].is_some()
                } else if mode == PadMode::Sampler {
                    self.snapshot.samples[p].is_some()
                } else {
                    false
                };
                let color = [
                    Color32::from_rgb(183, 69, 58),
                    Color32::from_rgb(179, 106, 42),
                    Color32::from_rgb(55, 105, 173),
                    Color32::from_rgb(175, 153, 48),
                ][p];
                let label = if mode == PadMode::ManualLoop {
                    ["IN", "OUT", "OFF", "RE"][p].into()
                } else {
                    format!("{}", p + 1)
                };
                if ui
                    .add_sized(
                        [23., 20.],
                        egui::Button::new(RichText::new(label).size(10.)).fill(if active {
                            color
                        } else {
                            color.gamma_multiply(0.35)
                        }),
                    )
                    .clicked()
                {
                    self.deck(
                        i,
                        DeckControl::Pad(p, ui.input(|input| input.modifiers.shift)),
                    );
                }
            }
            ui.label(RichText::new("LOOP").size(8.).weak());
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
            if ui.small_button("OFF").clicked() {
                self.deck(i, DeckControl::LoopOff);
            }
        });
        ui.horizontal_wrapped(|ui| {
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
            if amount != self.bend[i] {
                self.stop_auto();
                self.send(Command::Deck(i, DeckControl::Bend(amount)));
                self.bend[i] = amount;
            }
            if ui.small_button("×2").clicked() {
                self.deck(i, DeckControl::Bpm(d.bpm * 2.));
            }
            if ui.small_button("÷2").clicked() {
                self.deck(i, DeckControl::Bpm(d.bpm / 2.));
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "−{}",
                        time(
                            d.track
                                .as_ref()
                                .map(|t| t.duration() - d.position)
                                .unwrap_or(0.)
                        )
                    ))
                    .monospace()
                    .size(10.)
                    .weak(),
                );
            });
        });
    }
    fn mini_mixer(&mut self, ui: &mut egui::Ui) {
        let s = self.snapshot.clone();
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("MASTER / MONITOR").size(8.).weak());
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 80.) / 2.).max(0.));
                for (i, accent) in ACCENTS.iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(letter(i)).size(9.).color(*accent));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().slider_width = 61.;
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
                            meter(ui, s.decks[i].level, *accent, 63.);
                        });
                    });
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 66.) / 2.).max(0.));
                let mut master = s.master;
                if ui
                    .add(
                        Knob::new(&mut master, 0.0..=1.)
                            .default(0.75)
                            .label("MASTER")
                            .unit("%")
                            .size(18.),
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
                            .size(18.),
                    )
                    .changed()
                {
                    self.send(Command::Phones(phones));
                }
            });
            ui.spacing_mut().slider_width = 114.;
            let mut cross = s.cross;
            let r = ui
                .horizontal(|ui| {
                    ui.add_space(((ui.available_width() - 114.) / 2.).max(0.));
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
                    let (rect, response) = ui.allocate_exact_size(Vec2::splat(24.), Sense::click());
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
        ui.columns(2, |columns| {
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
            for (i, accent) in ACCENTS.iter().enumerate() {
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
                        Color32::from_rgb(23, 23, 23)
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
                                            .unwrap_or("Pusty slot"),
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
            ui.label(RichText::new("KOLEKCJA").strong().size(13.));
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Szukaj: tytuł, wykonawca, album, gatunek, tonacja…")
                        .desired_width((ui.available_width() - 390.).max(160.)),
                )
                .changed()
            {
                self.browser_dirty = true;
            }
            if ui.button("+ Pliki").clicked() {
                if let Some(files) = audio_dialog().pick_files() {
                    self.add_paths(files);
                }
            }
            if ui.button("+ Folder").clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.add_paths(vec![folder]);
                }
            }
            if ui
                .add_enabled(self.index_jobs == 0, egui::Button::new("Odśwież tagi"))
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
                Vec2::new(172., ui.available_height() - 52.),
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
            ui.separator();
            ui.vertical(|ui| {
                ui.set_min_width(ui.available_width());
                self.browser_table(ui);
            });
        });
    }
    fn browser_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.set_max_width(172.);
        for (scope, label) in [
            (Scope::All, "Wszystkie utwory"),
            (Scope::Favorites, "Ulubione"),
            (Scope::Loaded, "Na deckach"),
            (Scope::Missing, "Brakujące pliki"),
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
        ui.label(RichText::new("SKRZYNKI / CRATES").small().weak());
        let crates = self.catalog.crates.clone();
        let mut remove = None;
        for c in crates {
            let response = ui.selectable_value(
                &mut self.scope,
                Scope::Crate(c.id),
                format!("{}  {}", c.name, c.tracks.len()),
            );
            if response.changed() {
                self.browser_dirty = true;
            }
            response.context_menu(|ui| {
                if ui.button("Usuń skrzynkę").clicked() {
                    remove = Some(c.id);
                    ui.close_menu();
                }
            });
        }
        if let Some(id) = remove {
            self.catalog.crates.retain(|c| c.id != id);
            if self.scope == Scope::Crate(id) {
                self.scope = Scope::All;
            }
            self.browser_dirty = true;
            self.db_dirty = true;
        }
        ui.horizontal(|ui| {
            let input = ui.add(
                egui::TextEdit::singleline(&mut self.new_crate)
                    .hint_text("Nowa skrzynka")
                    .desired_width(123.),
            );
            let create = ui.small_button("+").clicked()
                || (input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if create {
                if let Some(id) = self.catalog.add_crate(&self.new_crate) {
                    self.new_crate.clear();
                    self.scope = Scope::Crate(id);
                    self.browser_dirty = true;
                    self.db_dirty = true;
                }
            }
        });
        ui.add_space(10.);
        ui.label(RichText::new("FOLDERY").small().weak());
        for folder in self.catalog.roots.clone() {
            let name = folder
                .file_name()
                .unwrap_or(folder.as_os_str())
                .to_string_lossy()
                .into_owned();
            let response = ui
                .selectable_value(&mut self.scope, Scope::Folder(folder.clone()), name)
                .on_hover_text(folder.display().to_string());
            if response.changed() {
                self.browser_dirty = true;
            }
        }
        ui.add_space(12.);
        ui.label(RichText::new("Upuść pliki lub folder na okno.\nPrawy przycisk: akcje utworu.\nUsuwanie z kolekcji nie usuwa muzyki z dysku.").small().weak());
    }
    fn browser_table(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let artist = if width > 730. { 145. } else { 0. };
        let album = if width > 1100. { 115. } else { 0. };
        let size = if width > 950. { 68. } else { 0. };
        let title =
            (width - 30. - 34. - artist - album - 64. - 48. - 62. - size - 72. - 4.).max(130.);
        let columns = [
            (30., "", None),
            (34., "#", None),
            (title, "TYTUŁ", Some(Sort::Title)),
            (artist, "WYKONAWCA", Some(Sort::Artist)),
            (album, "ALBUM", Some(Sort::Album)),
            (64., "BPM", Some(Sort::Bpm)),
            (48., "KEY", Some(Sort::Key)),
            (62., "CZAS", Some(Sort::Duration)),
            (size, "BITRATE", Some(Sort::Bitrate)),
            (72., "DECK", None),
        ];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            for (w, label, sort) in columns {
                if w <= 0. {
                    continue;
                }
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
                    label.into()
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
                        Color32::from_rgb(21, 91, 124)
                    } else if response.hovered() {
                        Color32::from_rgb(33, 39, 50)
                    } else if visible_index % 2 == 0 {
                        Color32::from_rgb(17, 17, 17)
                    } else {
                        Color32::from_rgb(24, 24, 24)
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
                    if response.clicked() {
                        self.selected = index;
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
                            cell(
                                ui,
                                64.,
                                &entry
                                    .bpm
                                    .map(|n| format!("{n:.1}"))
                                    .unwrap_or_else(|| "—".into()),
                                ACCENTS[0],
                            );
                            cell(
                                ui,
                                48.,
                                if entry.key.is_empty() {
                                    "—"
                                } else {
                                    &entry.key
                                },
                                Color32::from_rgb(155, 174, 222),
                            );
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
                            for d in 0..2 {
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
                            for i in 0..2 {
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
                            if ui.button("Pokaż folder pliku").clicked() {
                                let _ = std::process::Command::new(if cfg!(windows) {
                                    "explorer"
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
                                    egui::Button::new("Wskaż nową lokalizację…"),
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
            if ui.button("Następny >").clicked() {
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
        egui::Window::new("Ustawienia studia").open(&mut open).default_width(720.).resizable(true).show(ctx,|ui|{
            ui.horizontal(|ui|{for(i,label)in ["Audio i nagrywanie","MIDI","Automix","Sterowanie"].iter().enumerate(){ui.selectable_value(&mut self.settings_tab,i,*label);}});ui.separator();
            match self.settings_tab{
                0=>{
                    ui.heading("Silnik audio · CPAL");
                    if ui.button("Odśwież urządzenia").clicked(){self.outputs=audio::output_names();}
                    egui::ComboBox::from_id_salt("master-output").selected_text(if self.settings.master_device.is_empty(){"MASTER: domyślne"}else{&self.settings.master_device}).show_ui(ui,|ui|{ui.selectable_value(&mut self.settings.master_device,String::new(),"Domyślne wyjście systemowe");for name in &self.outputs{ui.selectable_value(&mut self.settings.master_device,name.clone(),name);}});
                    egui::ComboBox::from_id_salt("cue-output").selected_text(if self.settings.cue_device.is_empty(){"SŁUCHAWKI: wyłączone"}else{&self.settings.cue_device}).show_ui(ui,|ui|{ui.selectable_value(&mut self.settings.cue_device,String::new(),"Odsłuch wyłączony");for name in &self.outputs{ui.selectable_value(&mut self.settings.cue_device,name.clone(),name);}});
                    ui.add(egui::Slider::new(&mut self.settings.latency_ms,3..=100).suffix(" ms").text("Preferowany bufor"));
                    let busy=self.record_result.is_some()||self.snapshot.decks.iter().any(|d|d.playing);
                    if ui.add_enabled(!busy,egui::Button::new("Zastosuj urządzenia audio")).clicked(){self.restart_audio();}
                    ui.label("Przed zmianą wyjść zatrzymaj odtwarzanie i nagrywanie. Rozmiar bufora zależy od sterownika. Ustawienie nie jest pomiarem całkowitej latencji.");
                    ui.label("Osobny odsłuch wymaga niezależnego urządzenia systemowego. Brak rozdzielania par 1/2 i 3/4 pojedynczej karty.");
                    ui.separator();ui.heading("Nagrywanie");
                    egui::ComboBox::from_id_salt("record-format").selected_text(self.settings.format.extension().to_uppercase()).show_ui(ui,|ui|{for format in recording::Format::ALL{ui.selectable_value(&mut self.settings.format,format,format.extension().to_uppercase());}});
                    ui.label("WAV i AIFF zapisuje kod Rust. MP3, FLAC i Ogg/Opus wymagają FFmpeg w PATH lub obok pliku programu. W razie błędu konwersji pozostaje plik WAV.");
                    ui.checkbox(&mut self.settings.auto_bpm,"Analizuj BPM po wczytaniu utworu");
                    ui.label(RichText::new("Analiza jest szacunkowa; dostępne są TAP oraz ×2 / ÷2. Zmiana tempa zmienia wysokość dźwięku — brak key lock.").weak());
                },
                1=>self.midi_ui(ui),
                2=>{
                    ui.heading("Kolejka i przejścia");ui.add(egui::Slider::new(&mut self.settings.fade_seconds,1.0..=30.).text("Czas przejścia / s"));
                    ui.checkbox(&mut self.settings.auto_favorites,"Tylko ulubione");ui.checkbox(&mut self.settings.auto_shuffle,"Losuj kolejność");ui.checkbox(&mut self.settings.auto_repeat,"Powtarzaj kolejkę");ui.checkbox(&mut self.settings.linear_fade,"Liniowa krzywa przejścia (zamiast stałej mocy)");
                    ui.label("Kolejność jest ustalana przy starcie automiksu. Ręczny transport, tempo, jog i crossfader zatrzymują automat. Automix nie wyrównuje beatów.");
                },
                _=>{
                    ui.heading("Klawiatura");keyboard_help(ui);
                    ui.separator();ui.label("Mysz: kliknij waveform, aby przewinąć. Przeciągaj talerz poziomo. Dwuklik suwaka EQ zeruje pasmo, a dwuklik tempa zeruje tempo. Manual loop: pad 1 IN, 2 OUT, 3 OFF, 4 powrót. SHIFT + pad usuwa hot cue lub sample.");
                    ui.label("Biblioteka i ulubione są zapisywane lokalnie. Audio samplera i punkty CUE dotyczą bieżącej sesji.");
                    ui.label(format!("Ustawienia: {}",settings::config_path().display()));
                    ui.hyperlink_to("Licencja GPL-3.0-or-later","https://www.gnu.org/licenses/gpl-3.0.html");
                }
            }
            ui.separator();if ui.button("Zapisz ustawienia").clicked(){self.persist();self.status="Ustawienia zapisane".into();}
        });
        self.settings_open = open;
    }
    fn midi_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("MIDI Learn");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Odśwież wejścia").clicked() {
                self.midi_names = midi::inputs();
            }
            egui::ComboBox::from_id_salt("midi-input")
                .selected_text(
                    self.midi_names
                        .get(self.midi_selected)
                        .map(String::as_str)
                        .unwrap_or("Brak urządzeń"),
                )
                .show_ui(ui, |ui| {
                    for (i, name) in self.midi_names.iter().enumerate() {
                        ui.selectable_value(&mut self.midi_selected, i, name);
                    }
                });
            if ui
                .button(if self.midi_connection.is_some() {
                    "Połącz ponownie"
                } else {
                    "Połącz"
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
            if ui.button("Rozłącz").clicked() {
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
                    "Anuluj — czekam na MIDI…"
                } else {
                    "Przypisz MIDI"
                },
            )
            .clicked()
        {
            self.learn = !self.learn;
        }
        ui.label(&self.last_midi);
        ui.horizontal(|ui| {
            if ui.button("Eksport mapy JSON").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("Prospekt_MIDI.json")
                    .save_file()
                {
                    match serde_json::to_vec_pretty(&self.settings.midi)
                        .map_err(anyhow::Error::from)
                        .and_then(|data| Ok(std::fs::write(path, data)?))
                    {
                        Ok(()) => self.status = "Mapa MIDI zapisana".into(),
                        Err(e) => self.status = e.to_string(),
                    }
                }
            }
            if ui.button("Import mapy JSON").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Mapa MIDI", &["json"])
                    .pick_file()
                {
                    let result = std::fs::read(path)
                        .map_err(anyhow::Error::from)
                        .and_then(|data| midi::import_json(&data, &actions()));
                    match result {
                        Ok(maps) => {
                            self.settings.midi = maps;
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
                        if ui.small_button("Usuń").clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
        if let Some(i) = remove {
            self.settings.midi.remove(i);
        }
        ui.label("Obsługa Note i CC, względnych enkoderów oraz import map z poprzedniej wersji HTML. Brak fabrycznej mapy DJ2GO2 i sprzętowych LED. Połączenie odnawia się po ponownym podłączeniu kontrolera o tej samej nazwie.");
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
        let absolute = value as f32 / 127.;
        match action {
            "cross" => {
                self.stop_auto();
                self.send(Command::Cross(absolute));
            }
            "master" => self.send(Command::Master(absolute)),
            "phones" => self.send(Command::Phones(absolute)),
            "samplevolume" => self.send(Command::SampleVolume(absolute)),
            "browse" => {
                let delta = midi::relative(value, map.encoding);
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
                if i > 1 {
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
                            absolute * 36. - 24.,
                        ),
                    )),
                    "jog" => {
                        let value = midi::relative(value, map.encoding) as f64;
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
                        "pad0" | "pad1" | "pad2" | "pad3" => self.deck(
                            i,
                            DeckControl::Pad((control.as_bytes()[3] - b'0') as usize, false),
                        ),
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.key_pressed(egui::Key::F11)) {
            toggle_fullscreen(ctx);
        }
        for (i, (minus, plus)) in [(egui::Key::Z, egui::Key::X), (egui::Key::N, egui::Key::M)]
            .iter()
            .enumerate()
        {
            if ctx.input(|input| input.key_released(*minus) || input.key_released(*plus)) {
                self.send(Command::Deck(i, DeckControl::Bend(0.)));
            }
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
            if value != 0.
                || ctx.input(|input| input.key_released(*minus) || input.key_released(*plus))
            {
                self.stop_auto();
                self.send(Command::Deck(i, DeckControl::Bend(value)));
            }
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
            for i in 0..2 {
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
                self.settings.show_effects,
            )))
            .resizable(true)
            .default_height(if self.settings.show_effects {
                match self.dock_tab {
                    0 => 500.,
                    1 => 435.,
                    _ => 455.,
                }
            } else {
                350.
            })
            .min_height(220.)
            .max_height((ctx.screen_rect().height() - 260.).max(220.))
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
    if i == 0 {
        "A"
    } else {
        "B"
    }
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
    let mut out = ["cross", "master", "phones", "samplevolume", "browse"]
        .map(String::from)
        .to_vec();
    for i in 0..2 {
        for name in [
            "play",
            "cue",
            "setcue",
            "sync",
            "scratch",
            "pfl",
            "loopoff",
            "load",
            "pitch",
            "gain",
            "jog",
            "hot",
            "auto",
            "manual",
            "sample",
            "pad0",
            "pad1",
            "pad2",
            "pad3",
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
fn cover(ctx: &egui::Context, deck: usize, bytes: Option<&[u8]>) -> Option<egui::TextureHandle> {
    let bytes = bytes?;
    let dimensions = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if dimensions.0 > 4096 || dimensions.1 > 4096 {
        return None;
    }
    let image = image::load_from_memory(bytes)
        .ok()?
        .thumbnail(256, 256)
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Some(ctx.load_texture(
        format!("cover-{deck}"),
        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
        Default::default(),
    ))
}
fn waveform(
    ui: &mut egui::Ui,
    deck: &DeckView,
    accent: Color32,
    height: f32,
    span: Option<f64>,
) -> Option<f64> {
    let (response, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), height), Sense::click());
    let rect = response.rect;
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
    response
        .interact_pointer_pos()
        .filter(|_| response.clicked())
        .map(|p| {
            (start + (p.x - rect.left()) as f64 / rect.width() as f64 * range).clamp(0., duration)
        })
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
        v.panel_fill = Color32::from_rgb(17, 17, 17);
        v.window_fill = Color32::from_rgb(30, 30, 30);
        v.extreme_bg_color = Color32::BLACK;
        v.faint_bg_color = Color32::from_rgb(24, 24, 24);
        v.widgets.inactive.bg_fill = Color32::from_rgb(42, 42, 42);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(35, 35, 35);
        v.widgets.inactive.bg_stroke = Stroke::new(1_f32, Color32::from_rgb(62, 62, 62));
        v.widgets.hovered.bg_fill = Color32::from_rgb(55, 55, 55);
        v.override_text_color = Some(Color32::from_rgb(216, 226, 237));
    }
    v.widgets.inactive.corner_radius = egui::CornerRadius::same(1);
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(1);
    v.widgets.active.corner_radius = egui::CornerRadius::same(1);
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.22 };
    v.selection.bg_fill = Color32::from_rgb(27, 91, 126);
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

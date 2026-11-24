use crate::{
    dsp::{cross_curve_gains, Biquad, CrossCurve, PitchShifter},
    effects::{Effects, FxParam, FxSettings, FxSwitch},
    media::{Frame, Track},
    recording::{Block, CaptureSink, BLOCK_SIZE},
};
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::{
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PadMode {
    #[default]
    HotCue,
    AutoLoop,
    ManualLoop,
    Sampler,
    Effects,
}
impl PadMode {
    pub const ALL: [Self; 5] = [
        Self::HotCue,
        Self::AutoLoop,
        Self::ManualLoop,
        Self::Sampler,
        Self::Effects,
    ];
}
#[derive(Clone, Debug)]
pub struct DeckView {
    pub fx: FxSettings,
    pub level: f32,
    pub track: Option<Arc<Track>>,
    pub playing: bool,
    pub reverse: bool,
    pub position: f64,
    pub rate: f64,
    pub bpm: f64,
    pub cue: f64,
    pub hot: [Option<f64>; 8],
    pub loop_range: Option<(f64, f64)>,
    pub saved_loop: Option<(f64, f64)>,
    pub manual_in: Option<f64>,
    pub volume: f32,
    /// Independent pre-fader gain, neutral at 0 dB.
    pub gain_db: f32,
    pub eq: [f32; 3],
    pub pfl: bool,
    pub scratch: bool,
    /// Keep the original musical key while the tempo changes.
    pub key_lock: bool,
    pub mode: PadMode,
}
impl Default for DeckView {
    fn default() -> Self {
        Self {
            fx: FxSettings::default(),
            level: 0.,
            track: None,
            playing: false,
            reverse: false,
            position: 0.,
            rate: 1.,
            bpm: 120.,
            cue: 0.,
            hot: [None; 8],
            loop_range: None,
            saved_loop: None,
            manual_in: None,
            volume: 0.8,
            gain_db: 0.,
            eq: [0.; 3],
            pfl: false,
            scratch: false,
            key_lock: false,
            mode: PadMode::HotCue,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub deck_count: usize,
    pub decks: [DeckView; 4],
    pub cross: f32,
    pub master: f32,
    pub phones: f32,
    pub sample_volume: f32,
    pub peak: f32,
    pub samples: [Option<(Arc<Track>, f64, f64)>; 4],
    pub sample_playing: [bool; 4],
    pub preview_playing: bool,
    pub preview_position: f64,
    pub recording: bool,
    pub recorded_seconds: f64,
    pub database_failed: bool,
    pub cue_revision: (u64, u64),
    pub session_played: std::collections::BTreeSet<std::path::PathBuf>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            deck_count: 2,
            decks: std::array::from_fn(|_| DeckView::default()),
            cross: 0.5,
            master: 0.75,
            phones: 0.7,
            sample_volume: 0.8,
            peak: 0.,
            samples: std::array::from_fn(|_| None),
            sample_playing: [false; 4],
            preview_playing: false,
            preview_position: 0.,
            recording: false,
            recorded_seconds: 0.,
            database_failed: false,
            cue_revision: (0, 0),
            session_played: Default::default(),
        }
    }
}
#[derive(Clone)]
pub enum DeckControl {
    FxParam(FxParam, f32),
    FxToggle(FxSwitch),
    Fx(FxSettings),
    Load(Arc<Track>),
    Unload,
    Reverse(bool),
    Toggle,
    Play,
    Pause,
    Seek(f64),
    Cue,
    SetCue,
    Rate(f64),
    Bpm(f64),
    Volume(f32),
    Gain(f32),
    Eq(usize, f32),
    Loop(f64),
    LoopOff,
    LoopScale(f64),
    PadMode(PadMode),
    Pad(usize, bool),
    HotCue(usize, bool),
    Pfl,
    ScratchMode,
    KeyLock,
    Jog(f64, f64),
    Bend(f64),
    /// Jump by a number of beats (negative = back), using the track's BPM.
    BeatJump(f64),
}
pub enum Command {
    DeckCount(usize),
    Deck(usize, DeckControl),
    /// Match `deck` tempo and beat phase to `master`.
    Sync {
        deck: usize,
        master: usize,
    },
    Cross(f32),
    Master(f32),
    Microphone(Option<crossbeam_channel::Receiver<Frame>>),
    MicVolume(f32),
    Phones(f32),
    SampleVolume(f32),
    SampleLoad(usize, Arc<Track>),
    SampleTrigger(usize),
    SampleClear(usize),
    Preview(Option<Arc<Track>>),
    PreviewSeek(f64),
    Recording(Option<CaptureSink>),
    Broadcast(Option<CaptureSink>),
    Fade {
        to: usize,
        seconds: f64,
        linear: bool,
    },
    CancelFade,
    PauseAll,
    Barrier(Sender<()>),
    HistoryThreshold(f64),
    CrossCurve(CrossCurve),
    CrossEnabled(bool),
    ImportedCues(String, crate::track_store::CueBank, (u64, u64)),
    Restore(Box<Snapshot>),
}

struct Deck {
    effects: Effects,
    view: DeckView,
    frame: f64,
    slip_frame: Option<f64>,
    filters: [Biquad; 3],
    shifter: PitchShifter,
    smooth_gain: f32,
    pre_gain: f32,
    smooth_pre_gain: f32,
    jog: Option<(f64, u64)>,
    bend: f64,
    master_frames: u64,
    performance: u64,
    counted: bool,
    amp: f32,
}
impl Deck {
    fn new(sample_rate: u32) -> Self {
        Self {
            effects: Effects::new(sample_rate),
            view: DeckView::default(),
            frame: 0.,
            slip_frame: None,
            filters: [Biquad::default(); 3],
            shifter: PitchShifter::new(sample_rate),
            smooth_gain: 0.,
            pre_gain: 1.,
            smooth_pre_gain: 1.,
            jog: None,
            bend: 0.,
            master_frames: 0,
            performance: 0,
            counted: false,
            amp: 0.,
        }
    }
}
#[derive(Clone)]
struct Sample {
    track: Arc<Track>,
    start: f64,
    end: f64,
}
struct Voice {
    sample: Sample,
    frame: f64,
}
struct Fade {
    from: usize,
    to: usize,
    frames: u64,
    at: u64,
    linear: bool,
}
pub struct Engine {
    deck_count: usize,
    decks: [Deck; 4],
    pub sample_rate: u32,
    pub cross: f32,
    cross_curve: CrossCurve,
    cross_enabled: bool,
    pub master: f32,
    phones: f32,
    sample_volume: f32,
    samples: [Option<Sample>; 4],
    voices: [Option<Voice>; 4],
    preview: Option<(Arc<Track>, f64)>,
    capture: Option<CaptureSink>,
    broadcast: Option<CaptureSink>,
    broadcast_block: Block,
    block: Block,
    recorded: u64,
    limiter: f32,
    peak: f32,
    fade: Option<Fade>,
    cue_banks: std::collections::HashMap<String, crate::track_store::CueBank>,
    store: Option<crate::track_store::Store>,
    history_threshold: f64,
    microphone: Option<crossbeam_channel::Receiver<Frame>>,
    mic_volume: f32,
    instance: u64,
    database_unavailable: bool,
    cue_revision: u64,
    session_played: std::collections::BTreeSet<std::path::PathBuf>,
}
impl Engine {
    pub fn new(sample_rate: u32) -> Self {
        let _ = crate::track_store::session_start();
        static INSTANCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let instance = INSTANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            deck_count: 2,
            decks: std::array::from_fn(|_| Deck::new(sample_rate)),
            sample_rate,
            cross: 0.5,
            cross_curve: CrossCurve::Smooth,
            cross_enabled: true,
            master: 0.75,
            phones: 0.7,
            sample_volume: 0.8,
            samples: std::array::from_fn(|_| None),
            voices: std::array::from_fn(|_| None),
            preview: None,
            capture: None,
            broadcast: None,
            broadcast_block: Block::default(),
            block: Block::default(),
            recorded: 0,
            limiter: 1.,
            peak: 0.,
            fade: None,
            cue_banks: Default::default(),
            store: None,
            history_threshold: 30.,
            microphone: None,
            mic_volume: 0.,
            instance,
            database_unavailable: false,
            cue_revision: 0,
            session_played: Default::default(),
        }
    }
    pub fn with_store(sample_rate: u32, path: &std::path::Path) -> anyhow::Result<Self> {
        let (store, banks) = crate::track_store::Store::open(path)?;
        let mut engine = Self::new(sample_rate);
        engine.cue_banks = banks;
        engine.store = Some(store);
        Ok(engine)
    }
    /// Persistence failure must not prevent otherwise usable audio hardware from opening.
    pub fn with_store_or_memory(
        sample_rate: u32,
        path: &std::path::Path,
    ) -> (Self, Option<String>) {
        match Self::with_store(sample_rate, path) {
            Ok(engine) => (engine, None),
            Err(error) => {
                let mut engine = Self::new(sample_rate);
                engine.database_unavailable = true;
                (
                    engine,
                    Some(format!(
                        "Track database unavailable; session-only Cue bank: {error:#}"
                    )),
                )
            }
        }
    }
    pub fn flush_store(&self) -> Result<(), String> {
        self.store.as_ref().map_or(Ok(()), |store| store.flush())
    }
    pub fn apply(&mut self, command: Command) {
        match command {
            Command::Barrier(ack) => {
                let _ = ack.send(());
            }
            Command::ImportedCues(id, mut bank, expected) => {
                if expected != (self.instance, self.cue_revision) {
                    if let Some(current) = self.cue_banks.get(&id) {
                        bank = *current;
                        if let Some(store) = &self.store {
                            store.cues(&id, bank);
                        }
                    }
                }
                self.cue_banks.insert(id.clone(), bank);
                for deck in &mut self.decks {
                    if deck.view.track.as_ref().is_some_and(|t| t.id == id) {
                        deck.view.hot = bank;
                    }
                }
            }
            Command::CrossCurve(curve) => self.cross_curve = curve,
            Command::CrossEnabled(enabled) => self.cross_enabled = enabled,
            Command::HistoryThreshold(seconds) => {
                self.history_threshold = if seconds.is_finite() {
                    seconds.clamp(1., 300.)
                } else {
                    30.
                }
            }
            Command::DeckCount(count) => {
                self.deck_count = if count == 4 { 4 } else { 2 };
                for d in self.decks.iter_mut().skip(self.deck_count) {
                    d.view.playing = false;
                    d.jog = None;
                    d.bend = 0.;
                    d.amp = 0.;
                    d.smooth_gain = 0.;
                    d.effects.reset();
                }
            }
            Command::Deck(i, control) if i < 4 => self.deck_control(i, control),
            Command::Sync { deck, master } if deck < 4 && master < 4 && deck != master => {
                self.sync(deck, master);
            }
            Command::Cross(x) if x.is_finite() => self.cross = x.clamp(0., 1.),
            Command::Master(x) if x.is_finite() => self.master = x.clamp(0., 1.),
            Command::Microphone(input) => self.microphone = input,
            Command::MicVolume(volume) => {
                self.mic_volume = if volume.is_finite() {
                    volume.clamp(0., 1.)
                } else {
                    0.
                }
            }
            Command::Phones(x) if x.is_finite() => self.phones = x.clamp(0., 1.),
            Command::SampleVolume(x) if x.is_finite() => self.sample_volume = x.clamp(0., 1.),
            Command::SampleLoad(i, track) if i < 4 => {
                let end = track.frames.len() as f64;
                self.samples[i] = Some(Sample {
                    track,
                    start: 0.,
                    end,
                });
                self.voices[i] = None;
            }
            Command::SampleTrigger(i) if i < 4 => self.trigger(i),
            Command::SampleClear(i) if i < 4 => {
                self.samples[i] = None;
                self.voices[i] = None;
            }
            Command::Broadcast(sink) => {
                self.broadcast = sink;
                self.broadcast_block.len = 0;
            }
            Command::Preview(track) => {
                self.preview = track.map(|track| (track, 0.));
            }
            Command::PreviewSeek(seconds) => {
                if seconds.is_finite() {
                    if let Some((track, frame)) = &mut self.preview {
                        *frame = (seconds * track.sample_rate as f64)
                            .clamp(0., track.frames.len() as f64);
                    }
                }
            }
            Command::Recording(sink) => {
                self.flush_capture();
                self.capture = sink;
                self.recorded = 0;
            }
            Command::Fade {
                to,
                seconds,
                linear,
            } if to < 2 => {
                let from = 1 - to;
                self.decks[to].view.playing = self.decks[to].view.track.is_some();
                self.fade = Some(Fade {
                    from,
                    to,
                    frames: (seconds.max(0.05) * self.sample_rate as f64) as u64,
                    at: 0,
                    linear,
                });
            }
            Command::CancelFade => {
                if let Some(f) = self.fade.take() {
                    self.decks[f.to].view.playing = false;
                    self.cross = f.from as f32;
                }
            }
            Command::PauseAll => {
                for d in &mut self.decks {
                    d.view.playing = false;
                    d.jog = None;
                    d.bend = 0.;
                }
                self.fade = None;
            }
            Command::Restore(s) => {
                self.session_played.extend(s.session_played.iter().cloned());
                self.apply(Command::DeckCount(s.deck_count));
                self.cross = s.cross;
                self.master = s.master;
                self.phones = s.phones;
                self.sample_volume = s.sample_volume;
                for i in 0..4 {
                    if let Some(track) = &s.decks[i].track {
                        self.deck_control(i, DeckControl::Load(track.clone()));
                        self.decks[i].frame = s.decks[i].position * track.sample_rate as f64;
                        let restored_hot = self
                            .cue_banks
                            .get(&track.id)
                            .copied()
                            .unwrap_or(s.decks[i].hot);
                        self.decks[i].view = s.decks[i].clone();
                        self.decks[i].view.hot = restored_hot;
                        self.decks[i].pre_gain = 10_f32.powf(s.decks[i].gain_db / 20.);
                        self.cue_banks.insert(track.id.clone(), restored_hot);
                        if let Some(store) = &self.store {
                            store.cues(&track.id, restored_hot);
                        }
                        for b in 0..3 {
                            self.decks[i].filters[b].configure(
                                b,
                                s.decks[i].eq[b],
                                self.sample_rate,
                            );
                        }
                    }
                    self.deck_control(i, DeckControl::Fx(s.decks[i].fx));
                }
                for i in 0..4 {
                    self.samples[i] = s.samples[i].as_ref().map(|(track, start, end)| Sample {
                        track: track.clone(),
                        start: *start,
                        end: *end,
                    });
                }
            }
            _ => {}
        }
    }
    fn deck_control(&mut self, i: usize, control: DeckControl) {
        let d = &mut self.decks[i];
        if matches!(
            &control,
            DeckControl::Load(_)
                | DeckControl::Unload
                | DeckControl::Seek(_)
                | DeckControl::Cue
                | DeckControl::HotCue(..)
                | DeckControl::Pad(..)
                | DeckControl::Pause
                | DeckControl::Toggle
                | DeckControl::BeatJump(_)
        ) {
            if let Some(position) = d.slip_frame.take() {
                d.frame = position;
            }
            d.view.reverse = false;
        }
        let previous_hot = d.view.hot;
        let cue_edit = matches!(&control, DeckControl::HotCue(..))
            || (d.view.mode == PadMode::HotCue && matches!(&control, DeckControl::Pad(..)));
        match control {
            DeckControl::FxParam(param, value) => d.view.fx.set(param, value),
            DeckControl::FxToggle(switch) => d.view.fx.toggle(switch),
            DeckControl::Fx(fx) => {
                d.view.fx = fx.sanitized();
                d.effects.configure(d.view.fx, d.view.bpm * d.view.rate);
            }
            DeckControl::Reverse(held) => {
                if held && d.view.playing && d.view.track.is_some() && d.slip_frame.is_none() {
                    d.slip_frame = Some(d.frame);
                    d.view.reverse = true;
                    d.jog = None;
                } else if !held {
                    if let Some(position) = d.slip_frame.take() {
                        let end = d
                            .view
                            .track
                            .as_ref()
                            .map(|t| t.frames.len() as f64)
                            .unwrap_or(0.);
                        d.frame = position.clamp(0., end);
                        if d.frame >= end {
                            d.view.playing = false;
                        }
                    }
                    d.view.reverse = false;
                }
            }
            DeckControl::Unload => {
                d.view = DeckView {
                    fx: d.view.fx,
                    volume: d.view.volume,
                    gain_db: d.view.gain_db,
                    eq: d.view.eq,
                    pfl: d.view.pfl,
                    mode: d.view.mode,
                    scratch: d.view.scratch,
                    key_lock: d.view.key_lock,
                    ..Default::default()
                };
                d.frame = 0.;
                d.jog = None;
                d.bend = 0.;
                d.amp = 0.;
                d.master_frames = 0;
                d.counted = false;
                d.effects.reset();
                self.fade = None;
            }
            DeckControl::Load(track) => {
                let hot = self.cue_banks.get(&track.id).copied().unwrap_or([None; 8]);
                d.view = DeckView {
                    hot,
                    fx: d.view.fx,
                    bpm: sane_bpm(track.bpm),
                    track: Some(track),
                    volume: d.view.volume,
                    gain_db: d.view.gain_db,
                    eq: d.view.eq,
                    pfl: d.view.pfl,
                    mode: d.view.mode,
                    scratch: d.view.scratch,
                    key_lock: d.view.key_lock,
                    ..Default::default()
                };
                d.shifter.reset();
                d.frame = 0.;
                d.master_frames = 0;
                d.counted = false;
                d.performance = d.performance.wrapping_add(1);
                d.view.manual_in = None;
                d.jog = None;
                d.bend = 0.;
                d.amp = 0.;
                d.effects.reset();
                for f in &mut d.filters {
                    *f = Biquad::default();
                }
                for (b, g) in d.view.eq.iter().enumerate() {
                    d.filters[b].configure(b, *g, self.sample_rate);
                }
            }
            DeckControl::Toggle => {
                if let Some(track) = &d.view.track {
                    d.view.playing = !d.view.playing;
                    if d.view.playing && d.frame >= track.frames.len() as f64 {
                        d.frame = 0.;
                        d.master_frames = 0;
                        d.counted = false;
                        d.performance = d.performance.wrapping_add(1);
                    }
                }
            }
            DeckControl::Play => {
                if let Some(t) = &d.view.track {
                    if d.frame >= t.frames.len() as f64 {
                        d.frame = 0.;
                        d.master_frames = 0;
                        d.counted = false;
                        d.performance = d.performance.wrapping_add(1);
                    }
                    d.view.playing = true;
                }
            }
            DeckControl::Pause => {
                d.view.playing = false;
                d.jog = None;
            }
            DeckControl::Seek(s) => {
                if let (Some(t), true) = (&d.view.track, s.is_finite()) {
                    d.frame = s.clamp(0., t.duration()) * t.sample_rate as f64;
                    if d.view.loop_range.is_some_and(|(a, b)| s < a || s >= b) {
                        d.view.loop_range = None;
                    }
                }
            }
            DeckControl::BeatJump(beats) if beats.is_finite() => {
                if let Some(t) = &d.view.track {
                    let bpm = sane_bpm(d.view.bpm);
                    let seconds = (d.frame / t.sample_rate as f64 + beats * 60. / bpm)
                        .clamp(0., t.duration());
                    d.frame = seconds * t.sample_rate as f64;
                    if d.view
                        .loop_range
                        .is_some_and(|(a, b)| seconds < a || seconds >= b)
                    {
                        d.view.loop_range = None;
                    }
                }
            }
            DeckControl::Cue => {
                if let Some(t) = &d.view.track {
                    d.frame = d.view.cue * t.sample_rate as f64;
                    d.view.loop_range = None;
                }
            }
            DeckControl::SetCue => {
                if let Some(t) = &d.view.track {
                    d.view.cue = d.frame / t.sample_rate as f64;
                }
            }
            DeckControl::Rate(rate) if rate.is_finite() => d.view.rate = rate.clamp(0.5, 1.5),
            DeckControl::Bpm(bpm) if bpm.is_finite() => d.view.bpm = bpm.clamp(40., 300.),
            DeckControl::Gain(v) => {
                if v.is_finite() {
                    d.view.gain_db = v.clamp(-24., 12.);
                    d.pre_gain = 10_f32.powf(d.view.gain_db / 20.);
                }
            }
            DeckControl::Volume(v) if v.is_finite() => d.view.volume = v.clamp(0., 1.),
            DeckControl::Eq(b, g) if b < 3 && g.is_finite() => {
                d.view.eq[b] = g.clamp(-24., 12.);
                d.filters[b].configure(b, d.view.eq[b], self.sample_rate);
            }
            DeckControl::Loop(beats) => {
                if let Some(t) = &d.view.track {
                    let pos = d.frame / t.sample_rate as f64;
                    let end = (pos + beats * 60. / d.view.bpm).min(t.duration());
                    if end - pos > 0.01 {
                        d.view.loop_range = Some((pos, end));
                    }
                }
            }
            DeckControl::LoopOff => {
                if d.view.loop_range.is_some() {
                    d.view.saved_loop = d.view.loop_range;
                }
                d.view.loop_range = None;
            }
            DeckControl::LoopScale(scale) => {
                if let (Some((a, b)), Some(track)) = (d.view.loop_range, d.view.track.as_ref()) {
                    let end = (a + (b - a) * scale.clamp(0.5, 2.)).min(track.duration());
                    if end - a > 0.01 {
                        d.view.loop_range = Some((a, end));
                        if d.frame / track.sample_rate as f64 >= end {
                            d.frame = a * track.sample_rate as f64;
                        }
                    }
                }
            }
            DeckControl::PadMode(mode) => d.view.mode = mode,
            DeckControl::Pfl => d.view.pfl = !d.view.pfl,
            DeckControl::ScratchMode => d.view.scratch = !d.view.scratch,
            DeckControl::KeyLock => d.view.key_lock = !d.view.key_lock,
            DeckControl::Jog(speed, ttl) => {
                d.jog = if ttl.is_nan() || ttl <= 0. || !speed.is_finite() {
                    None
                } else {
                    Some((speed.clamp(-6., 6.), (ttl * self.sample_rate as f64) as u64))
                }
            }
            DeckControl::Bend(v) if v.is_finite() => d.bend = v.clamp(-0.25, 0.25),
            DeckControl::HotCue(p, erase) if p < 8 => {
                if let Some(t) = &d.view.track {
                    if erase {
                        d.view.hot[p] = None;
                    } else if let Some(cue) = d.view.hot[p] {
                        d.frame = cue * t.sample_rate as f64;
                        d.view.loop_range = None;
                    } else {
                        d.view.hot[p] = Some(d.frame / t.sample_rate as f64);
                    }
                }
            }
            DeckControl::Pad(p, erase) if p < 8 => {
                if d.view.mode != PadMode::HotCue && p >= 4 {
                    return;
                }
                if d.view.mode == PadMode::Sampler {
                    if erase {
                        self.samples[p] = None;
                        self.voices[p] = None;
                        return;
                    }
                    if self.samples[p].is_some() {
                        self.trigger(p);
                        return;
                    }
                    if let Some(t) = &d.view.track {
                        self.samples[p] = Some(Sample {
                            track: t.clone(),
                            start: d.frame,
                            end: (d.frame + 4. * 60. / d.view.bpm * t.sample_rate as f64)
                                .min(t.frames.len() as f64),
                        });
                    }
                    return;
                }
                let Some(t) = &d.view.track else {
                    return;
                };
                let pos = d.frame / t.sample_rate as f64;
                match d.view.mode {
                    PadMode::HotCue => {
                        if erase {
                            d.view.hot[p] = None;
                        } else if let Some(cue) = d.view.hot[p] {
                            d.frame = cue * t.sample_rate as f64;
                            d.view.loop_range = None;
                        } else {
                            d.view.hot[p] = Some(pos);
                        }
                    }
                    PadMode::AutoLoop => {
                        self.deck_control(i, DeckControl::Loop([1., 2., 4., 8.][p]));
                    }
                    PadMode::ManualLoop => match p {
                        0 => d.view.manual_in = Some(pos),
                        1 => {
                            if let Some(start) = d.view.manual_in {
                                if pos - start > 0.01 {
                                    d.view.loop_range = Some((start, pos));
                                }
                            }
                        }
                        2 => {
                            if d.view.loop_range.is_some() {
                                d.view.saved_loop = d.view.loop_range;
                            }
                            d.view.loop_range = None;
                        }
                        3 => {
                            if d.view.loop_range.is_none() {
                                d.view.loop_range = d.view.saved_loop;
                            }
                            if let Some((a, _)) = d.view.loop_range {
                                d.frame = a * t.sample_rate as f64;
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            _ => {}
        }
        let d = &self.decks[i];
        if cue_edit && previous_hot != d.view.hot {
            self.cue_revision = self.cue_revision.wrapping_add(1);
            if let Some(track) = &d.view.track {
                let id = track.id.clone();
                let bank = d.view.hot;
                self.cue_banks.insert(id.clone(), bank);
                if let Some(store) = &self.store {
                    store.cues(&id, bank);
                }
                for other in &mut self.decks {
                    if other.view.track.as_ref().is_some_and(|t| t.id == id) {
                        other.view.hot = bank;
                    }
                }
            }
        }
    }
    /// Tempo match plus beat-phase alignment against the master deck's beat grid.
    /// Half/double tempo is chosen when it keeps the pitch fader closer to zero.
    fn sync(&mut self, i: usize, master: usize) -> bool {
        let (Some(mt), Some(st)) = (
            self.decks[master].view.track.clone(),
            self.decks[i].view.track.clone(),
        ) else {
            return false;
        };
        let m = &self.decks[master];
        let m_bpm = sane_bpm(m.view.bpm);
        let m_tempo = m_bpm * m.view.rate;
        let m_beats = (m.frame / mt.sample_rate as f64 - mt.beat_offset) * m_bpm / 60.;
        let m_playing = m.view.playing;
        let d = &mut self.decks[i];
        let s_bpm = sane_bpm(d.view.bpm);
        let (k, rate) = [1., 0.5, 2.]
            .into_iter()
            .map(|k| (k, m_tempo * k / s_bpm))
            .min_by(|a, b| (a.1 - 1.).abs().total_cmp(&(b.1 - 1.).abs()))
            .unwrap_or((1., 1.));
        d.view.rate = rate.clamp(0.5, 1.5);
        if let Some(position) = d.slip_frame.take() {
            d.frame = position;
            d.view.reverse = false;
        }
        if m_playing && d.view.loop_range.is_none() {
            let s_beats = (d.frame / st.sample_rate as f64 - st.beat_offset) * s_bpm / 60.;
            let target = (m_beats * k).rem_euclid(1.);
            let mut delta = target - s_beats.rem_euclid(1.);
            if delta >= 0.5 {
                delta -= 1.;
            } else if delta < -0.5 {
                delta += 1.;
            }
            let seconds =
                (d.frame / st.sample_rate as f64 + delta * 60. / s_bpm).clamp(0., st.duration());
            d.frame = seconds * st.sample_rate as f64;
        }
        true
    }
    fn trigger(&mut self, i: usize) {
        if let Some(s) = &self.samples[i] {
            self.voices[i] = Some(Voice {
                sample: s.clone(),
                frame: s.start,
            });
        }
    }
    pub fn render(&mut self) -> (Frame, Frame) {
        let mut linear = false;
        if let Some(f) = &mut self.fade {
            let t = (f.at as f32 / f.frames as f32).min(1.);
            self.cross = if f.from == 0 { t } else { 1. - t };
            linear = f.linear;
            f.at += 1;
            if f.at > f.frames {
                self.decks[f.from].view.playing = false;
                self.fade = None;
            }
        }
        // Automix fades stay linear; with the crossfader switched off both sides stay open.
        let curve = if linear {
            CrossCurve::Const
        } else {
            self.cross_curve
        };
        let gains = if self.cross_enabled {
            cross_curve_gains(self.cross, curve)
        } else {
            [1., 1.]
        };
        let mut out = [0.; 2];
        let mut phones = [0.; 2];
        for (i, d) in self.decks.iter_mut().take(self.deck_count).enumerate() {
            let Some(track) = &d.view.track else {
                continue;
            };
            let mut speed = if d.view.playing {
                d.view.rate * (1. + d.bend)
            } else {
                0.
            };
            if let Some((jog, left)) = &mut d.jog {
                speed = *jog;
                *left = left.saturating_sub(1);
                if *left == 0 {
                    d.jog = None;
                }
            }
            if let Some(position) = &mut d.slip_frame {
                let forward = if d.view.playing {
                    d.view.rate * (1. + d.bend)
                } else {
                    0.
                };
                *position += forward * track.sample_rate as f64 / self.sample_rate as f64;
                if let Some((a, b)) = d.view.loop_range {
                    let start = a * track.sample_rate as f64;
                    let end = b * track.sample_rate as f64;
                    if end > start && *position >= end {
                        *position = start + (*position - start).rem_euclid(end - start);
                    }
                }
                speed = -forward.abs();
            }
            let threshold = if track.duration() < 60. {
                self.history_threshold.min(track.duration() * 0.5)
            } else {
                self.history_threshold
            };
            if d.view.playing
                && d.frame >= 0.
                && d.frame < track.frames.len() as f64
                && d.view.volume > 0.
                && gains[i % 2] > 0.0001
                && self.master > 0.
            {
                d.master_frames = d.master_frames.saturating_add(1);
                if !d.counted && d.master_frames as f64 / self.sample_rate as f64 >= threshold {
                    d.counted = true;
                    if let Some(path) = &track.path {
                        self.session_played.insert(path.clone());
                    }
                    if let Some(store) = &self.store {
                        store.play_details(crate::history::Play {
                            event_id: format!(
                                "{}-{}-{}-{}-{}",
                                crate::track_store::session_start(),
                                std::process::id(),
                                self.instance,
                                i,
                                d.performance
                            ),
                            track_id: track.id.clone(),
                            path: track.path.clone(),
                            title: track.title.clone(),
                            artist: track.artist.clone(),
                            deck: i,
                            played_at: String::new(),
                        });
                    }
                }
            }
            let active = speed != 0.
                && d.frame >= 0.
                && d.frame < track.frames.len() as f64
                && !(d.view.reverse && d.frame <= 0.);
            d.amp += (if active { 1. } else { 0. } - d.amp) * 0.04;
            let mut frame = track.sample(d.frame);
            let locked = d.view.key_lock && d.jog.is_none() && d.slip_frame.is_none() && speed > 0.;
            frame = d
                .shifter
                .process(frame, if locked { 1. / speed } else { 1. });
            d.smooth_pre_gain += (d.pre_gain - d.smooth_pre_gain) * 0.002;
            for x in &mut frame {
                *x *= d.amp * d.smooth_pre_gain;
            }
            for f in &mut d.filters {
                frame = f.process(frame);
            }
            d.effects.configure(d.view.fx, d.view.bpm * d.view.rate);
            frame = d.effects.process(frame);
            d.view.level = (d.view.level * 0.9995).max(frame[0].abs().max(frame[1].abs()).min(1.));
            d.smooth_gain += (d.view.volume * gains[i % 2] - d.smooth_gain) * 0.002;
            for c in 0..2 {
                out[c] += frame[c] * d.smooth_gain;
                if d.view.pfl {
                    phones[c] += frame[c] * self.phones;
                }
            }
            d.frame += speed * track.sample_rate as f64 / self.sample_rate as f64;
            if d.view.reverse {
                if let Some((a, b)) = d.view.loop_range {
                    let start = a * track.sample_rate as f64;
                    let end = b * track.sample_rate as f64;
                    if end > start && d.frame < start {
                        d.frame = start + (d.frame - start).rem_euclid(end - start);
                    }
                }
            }
            if d.jog.is_none() {
                if let Some((a, b)) = d.view.loop_range {
                    let start = a * track.sample_rate as f64;
                    let end = b * track.sample_rate as f64;
                    if d.frame >= end {
                        d.frame = start + (d.frame - start).rem_euclid(end - start);
                    }
                }
            }
            if d.frame >= track.frames.len() as f64 || d.frame < 0. {
                d.frame = d.frame.clamp(0., track.frames.len() as f64);
                if d.jog.is_none() && !d.view.reverse {
                    d.view.playing = false;
                }
            }
            d.view.position = d.frame / track.sample_rate as f64;
        }
        for voice in &mut self.voices {
            if let Some(v) = voice {
                let frame = v.sample.track.sample(v.frame);
                for c in 0..2 {
                    out[c] += frame[c] * self.sample_volume;
                }
                v.frame += v.sample.track.sample_rate as f64 / self.sample_rate as f64;
                if v.frame >= v.sample.end {
                    *voice = None;
                }
            }
        }
        if let Some(input) = &self.microphone {
            if let Ok(frame) = input.try_recv() {
                for (sample, mic) in out.iter_mut().zip(frame) {
                    if mic.is_finite() {
                        *sample += mic.clamp(-1., 1.) * self.mic_volume;
                    }
                }
            }
        }
        for x in &mut out {
            *x *= self.master;
        }
        let peak = out[0].abs().max(out[1].abs());
        let target = if peak > 0.98 { 0.98 / peak } else { 1. };
        self.limiter = if target < self.limiter {
            target
        } else {
            self.limiter + (target - self.limiter) * 0.00015
        };
        for x in &mut out {
            *x = (*x * self.limiter).clamp(-0.98, 0.98);
        }
        if let Some((track, position)) = &mut self.preview {
            let frame = track.sample(*position);
            for c in 0..2 {
                phones[c] += frame[c] * self.phones;
            }
            *position += track.sample_rate as f64 / self.sample_rate as f64;
            if *position >= track.frames.len() as f64 {
                self.preview = None;
            }
        }
        for x in &mut phones {
            *x = x.clamp(-0.98, 0.98);
        }
        self.peak = (self.peak * 0.9998).max(out[0].abs().max(out[1].abs()));
        if let Some(sink) = &self.broadcast {
            self.broadcast_block.frames[self.broadcast_block.len] = out;
            self.broadcast_block.len += 1;
            if self.broadcast_block.len == BLOCK_SIZE {
                if sink.sender.try_send(self.broadcast_block.clone()).is_err() {
                    sink.overflow
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                self.broadcast_block.len = 0;
            }
        }
        if self.capture.is_some() {
            self.block.frames[self.block.len] = out;
            self.block.len += 1;
            self.recorded += 1;
            if self.block.len == BLOCK_SIZE {
                self.flush_capture();
            }
        }
        (out, phones)
    }
    fn flush_capture(&mut self) {
        if self.block.len == 0 {
            return;
        }
        if let Some(sink) = &self.capture {
            if sink.sender.try_send(self.block.clone()).is_err() {
                sink.overflow.store(true, Ordering::Relaxed);
                self.capture = None;
            }
        }
        self.block.len = 0;
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            deck_count: self.deck_count,
            decks: std::array::from_fn(|i| {
                let d = &self.decks[i];
                let mut view = d.view.clone();
                if let Some(track) = &view.track {
                    view.position = d.frame / track.sample_rate as f64;
                }
                view
            }),
            cross: self.cross,
            master: self.master,
            phones: self.phones,
            sample_volume: self.sample_volume,
            peak: self.peak,
            samples: std::array::from_fn(|i| {
                self.samples[i]
                    .as_ref()
                    .map(|s| (s.track.clone(), s.start, s.end))
            }),
            sample_playing: std::array::from_fn(|i| self.voices[i].is_some()),
            preview_playing: self.preview.is_some(),
            preview_position: self
                .preview
                .as_ref()
                .map(|(track, frame)| frame / track.sample_rate as f64)
                .unwrap_or(0.),
            recording: self.capture.is_some(),
            recorded_seconds: self.recorded as f64 / self.sample_rate as f64,
            cue_revision: (self.instance, self.cue_revision),
            session_played: self.session_played.clone(),
            database_failed: self.database_unavailable
                || self.store.as_ref().is_some_and(|s| s.failed()),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.flush_capture();
        self.capture = None;
    }
}

// Used only when no output device exists. Commands and metadata stay usable,
// but this deliberately does not advance audio or pretend playback works.
pub fn offline(rx: Receiver<Command>, view: Arc<Mutex<Snapshot>>, stop: Receiver<()>) {
    let mut engine = match Engine::with_store(
        48000,
        &crate::settings::config_path().with_file_name("tracks.sqlite"),
    ) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("Track database: {error:#}");
            let mut engine = Engine::new(48000);
            engine.database_unavailable = true;
            engine
        }
    };
    while stop.try_recv().is_err() {
        while let Ok(command) = rx.try_recv() {
            engine.apply(command);
        }
        if let Ok(mut snapshot) = view.lock() {
            *snapshot = engine.snapshot();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
pub type CommandSender = Sender<Command>;

/// Tempo fader ranges selectable in settings: ±6 %, ±10 %, ±16 %, ±50 %.
pub const TEMPO_RANGES: [f64; 4] = [0.06, 0.10, 0.16, 0.50];
pub fn tempo_range(index: u8) -> f64 {
    TEMPO_RANGES[(index as usize).min(TEMPO_RANGES.len() - 1)]
}

/// Track BPM from tags or detection may be missing; loops and samples divide by it.
pub fn sane_bpm(bpm: f64) -> f64 {
    if bpm.is_finite() && bpm >= 40. {
        bpm.min(300.)
    } else {
        120.
    }
}

/// Picks the deck that SYNC follows: the paired deck when it plays, then any
/// playing deck, then any loaded deck. Never an empty deck.
pub fn sync_master(snapshot: &Snapshot, i: usize) -> Option<usize> {
    snapshot.decks.get(i)?.track.as_ref()?;
    let count = snapshot.deck_count.clamp(2, 4);
    let candidates: Vec<usize> = std::iter::once(i ^ 1)
        .chain(0..count)
        .filter(|&d| d != i && d < count && snapshot.decks[d].track.is_some())
        .collect();
    candidates
        .iter()
        .copied()
        .find(|&d| snapshot.decks[d].playing)
        .or_else(|| candidates.first().copied())
}

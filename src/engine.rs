use crate::{
    dsp::{cross_gains, Biquad},
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
}
impl PadMode {
    pub const ALL: [Self; 4] = [
        Self::HotCue,
        Self::AutoLoop,
        Self::ManualLoop,
        Self::Sampler,
    ];
}
#[derive(Clone, Debug)]
pub struct DeckView {
    pub fx: FxSettings,
    pub level: f32,
    pub track: Option<Arc<Track>>,
    pub playing: bool,
    pub position: f64,
    pub rate: f64,
    pub bpm: f64,
    pub cue: f64,
    pub hot: [Option<f64>; 8],
    pub loop_range: Option<(f64, f64)>,
    pub volume: f32,
    pub eq: [f32; 3],
    pub pfl: bool,
    pub scratch: bool,
    pub mode: PadMode,
}
impl Default for DeckView {
    fn default() -> Self {
        Self {
            fx: FxSettings::default(),
            level: 0.,
            track: None,
            playing: false,
            position: 0.,
            rate: 1.,
            bpm: 120.,
            cue: 0.,
            hot: [None; 8],
            loop_range: None,
            volume: 0.8,
            eq: [0.; 3],
            pfl: false,
            scratch: false,
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
    pub recording: bool,
    pub recorded_seconds: f64,
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
            recording: false,
            recorded_seconds: 0.,
        }
    }
}
#[derive(Clone)]
pub enum DeckControl {
    FxParam(FxParam, f32),
    FxToggle(FxSwitch),
    Fx(FxSettings),
    Load(Arc<Track>),
    Toggle,
    Play,
    Pause,
    Seek(f64),
    Cue,
    SetCue,
    Rate(f64),
    Bpm(f64),
    Volume(f32),
    Eq(usize, f32),
    Loop(f64),
    LoopOff,
    LoopScale(f64),
    PadMode(PadMode),
    Pad(usize, bool),
    HotCue(usize, bool),
    Pfl,
    ScratchMode,
    Jog(f64, f64),
    Bend(f64),
}
pub enum Command {
    DeckCount(usize),
    Deck(usize, DeckControl),
    Cross(f32),
    Master(f32),
    Phones(f32),
    SampleVolume(f32),
    SampleLoad(usize, Arc<Track>),
    SampleTrigger(usize),
    SampleClear(usize),
    Recording(Option<CaptureSink>),
    Fade {
        to: usize,
        seconds: f64,
        linear: bool,
    },
    CancelFade,
    PauseAll,
    Restore(Box<Snapshot>),
}

struct Deck {
    effects: Effects,
    view: DeckView,
    frame: f64,
    filters: [Biquad; 3],
    smooth_gain: f32,
    jog: Option<(f64, u64)>,
    bend: f64,
    manual_in: Option<f64>,
    amp: f32,
}
impl Deck {
    fn new(sample_rate: u32) -> Self {
        Self {
            effects: Effects::new(sample_rate),
            view: DeckView::default(),
            frame: 0.,
            filters: [Biquad::default(); 3],
            smooth_gain: 0.,
            jog: None,
            bend: 0.,
            manual_in: None,
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
    pub master: f32,
    phones: f32,
    sample_volume: f32,
    samples: [Option<Sample>; 4],
    voices: [Option<Voice>; 4],
    capture: Option<CaptureSink>,
    block: Block,
    recorded: u64,
    limiter: f32,
    peak: f32,
    fade: Option<Fade>,
}
impl Engine {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            deck_count: 2,
            decks: std::array::from_fn(|_| Deck::new(sample_rate)),
            sample_rate,
            cross: 0.5,
            master: 0.75,
            phones: 0.7,
            sample_volume: 0.8,
            samples: std::array::from_fn(|_| None),
            voices: std::array::from_fn(|_| None),
            capture: None,
            block: Block::default(),
            recorded: 0,
            limiter: 1.,
            peak: 0.,
            fade: None,
        }
    }
    pub fn apply(&mut self, command: Command) {
        match command {
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
            Command::Cross(x) => self.cross = x.clamp(0., 1.),
            Command::Master(x) => self.master = x.clamp(0., 1.),
            Command::Phones(x) => self.phones = x.clamp(0., 1.),
            Command::SampleVolume(x) => self.sample_volume = x.clamp(0., 1.),
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
                self.apply(Command::DeckCount(s.deck_count));
                self.cross = s.cross;
                self.master = s.master;
                self.phones = s.phones;
                self.sample_volume = s.sample_volume;
                for i in 0..4 {
                    if let Some(track) = &s.decks[i].track {
                        self.deck_control(i, DeckControl::Load(track.clone()));
                        self.decks[i].frame = s.decks[i].position * track.sample_rate as f64;
                        self.decks[i].view = s.decks[i].clone();
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
        match control {
            DeckControl::FxParam(param, value) => d.view.fx.set(param, value),
            DeckControl::FxToggle(switch) => d.view.fx.toggle(switch),
            DeckControl::Fx(fx) => {
                d.view.fx = fx.sanitized();
                d.effects.configure(d.view.fx, d.view.bpm * d.view.rate);
            }
            DeckControl::Load(track) => {
                d.view = DeckView {
                    fx: d.view.fx,
                    bpm: track.bpm,
                    track: Some(track),
                    volume: d.view.volume,
                    eq: d.view.eq,
                    pfl: d.view.pfl,
                    mode: d.view.mode,
                    scratch: d.view.scratch,
                    ..Default::default()
                };
                d.frame = 0.;
                d.manual_in = None;
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
                    }
                }
            }
            DeckControl::Play => {
                if let Some(t) = &d.view.track {
                    if d.frame >= t.frames.len() as f64 {
                        d.frame = 0.;
                    }
                    d.view.playing = true;
                }
            }
            DeckControl::Pause => {
                d.view.playing = false;
                d.jog = None;
            }
            DeckControl::Seek(s) => {
                if let Some(t) = &d.view.track {
                    d.frame = s.clamp(0., t.duration()) * t.sample_rate as f64;
                    if d.view.loop_range.is_some_and(|(a, b)| s < a || s >= b) {
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
            DeckControl::Rate(rate) => d.view.rate = rate.clamp(0.5, 1.5),
            DeckControl::Bpm(bpm) => d.view.bpm = bpm.clamp(40., 300.),
            DeckControl::Volume(v) => d.view.volume = v.clamp(0., 1.),
            DeckControl::Eq(b, g) if b < 3 => {
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
            DeckControl::LoopOff => d.view.loop_range = None,
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
            DeckControl::Jog(speed, ttl) => {
                d.jog = if ttl <= 0. {
                    None
                } else {
                    Some((speed.clamp(-6., 6.), (ttl * self.sample_rate as f64) as u64))
                }
            }
            DeckControl::Bend(v) => d.bend = v.clamp(-0.25, 0.25),
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
                        0 => d.manual_in = Some(pos),
                        1 => {
                            if let Some(start) = d.manual_in {
                                if pos - start > 0.01 {
                                    d.view.loop_range = Some((start, pos));
                                }
                            }
                        }
                        2 => d.view.loop_range = None,
                        3 => {
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
        let gains = cross_gains(self.cross, linear);
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
            let active = speed != 0. && d.frame >= 0. && d.frame < track.frames.len() as f64;
            d.amp += (if active { 1. } else { 0. } - d.amp) * 0.04;
            let mut frame = track.sample(d.frame);
            for x in &mut frame {
                *x *= d.amp;
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
                if d.jog.is_none() {
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
        for x in &mut phones {
            *x = x.clamp(-0.98, 0.98);
        }
        self.peak = (self.peak * 0.9998).max(out[0].abs().max(out[1].abs()));
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
            recording: self.capture.is_some(),
            recorded_seconds: self.recorded as f64 / self.sample_rate as f64,
        }
    }
}

// Used only when no output device exists. Commands and metadata stay usable,
// but this deliberately does not advance audio or pretend playback works.
pub fn offline(rx: Receiver<Command>, view: Arc<Mutex<Snapshot>>, stop: Receiver<()>) {
    let mut engine = Engine::new(48000);
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

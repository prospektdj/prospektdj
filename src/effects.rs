//! Native per-deck effects. Buffers are allocated when the engine is created.
use crate::media::Frame;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FxSettings {
    pub bypass: bool,
    pub chorus_enabled: bool,
    pub tremolo_enabled: bool,
    pub distortion_enabled: bool,
    pub crusher_enabled: bool,
    pub chorus_mix: f32,
    pub chorus_rate: f32,
    pub tremolo_depth: f32,
    pub tremolo_rate: f32,
    pub drive: f32,
    pub distortion_mix: f32,
    pub crusher_bits: f32,
    pub crusher_mix: f32,

    pub filter_enabled: bool,
    pub filter: f32,
    pub echo_enabled: bool,
    pub echo_mix: f32,
    pub echo_feedback: f32,
    pub echo_ms: f32,
    pub echo_sync: bool,
    pub echo_beats: f32,
    pub reverb_enabled: bool,
    pub reverb_mix: f32,
    pub reverb_room: f32,
    pub reverb_damping: f32,
    pub flanger_enabled: bool,
    pub flanger_mix: f32,
    pub flanger_rate: f32,
    pub flanger_depth: f32,
}
#[derive(Clone, Copy, Debug)]
pub enum FxParam {
    Filter,
    ChorusMix,
    ChorusRate,
    TremoloDepth,
    TremoloRate,
    Drive,
    DistortionMix,
    CrusherBits,
    CrusherMix,
    EchoMix,
    EchoFeedback,
    EchoMs,
    ReverbMix,
    ReverbRoom,
    ReverbDamping,
    FlangerMix,
    FlangerRate,
    FlangerDepth,
}
#[derive(Clone, Copy, Debug)]
pub enum FxSwitch {
    Chorus,
    Tremolo,
    Distortion,
    Crusher,
    Bypass,
    Filter,
    Echo,
    EchoSync,
    Reverb,
    Flanger,
}
impl Default for FxSettings {
    fn default() -> Self {
        Self {
            bypass: false,
            chorus_enabled: false,
            tremolo_enabled: false,
            distortion_enabled: false,
            crusher_enabled: false,
            chorus_mix: 0.35,
            chorus_rate: 0.6,
            tremolo_depth: 0.6,
            tremolo_rate: 4.0,
            drive: 3.0,
            distortion_mix: 0.3,
            crusher_bits: 10.0,
            crusher_mix: 0.4,

            filter_enabled: true,
            filter: 0.,
            echo_enabled: false,
            echo_mix: 0.3,
            echo_feedback: 0.35,
            echo_ms: 350.,
            echo_sync: true,
            echo_beats: 0.5,
            reverb_enabled: false,
            reverb_mix: 0.25,
            reverb_room: 0.6,
            reverb_damping: 0.45,
            flanger_enabled: false,
            flanger_mix: 0.5,
            flanger_rate: 0.25,
            flanger_depth: 0.6,
        }
    }
}
fn sane(value: f32, default: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}
impl FxSettings {
    pub fn set(&mut self, param: FxParam, value: f32) {
        match param {
            FxParam::ChorusMix => self.chorus_mix = value,
            FxParam::ChorusRate => self.chorus_rate = value,
            FxParam::TremoloDepth => self.tremolo_depth = value,
            FxParam::TremoloRate => self.tremolo_rate = value,
            FxParam::Drive => self.drive = value,
            FxParam::DistortionMix => self.distortion_mix = value,
            FxParam::CrusherBits => self.crusher_bits = value,
            FxParam::CrusherMix => self.crusher_mix = value,
            FxParam::Filter => self.filter = value,
            FxParam::EchoMix => self.echo_mix = value,
            FxParam::EchoFeedback => self.echo_feedback = value,
            FxParam::EchoMs => {
                self.echo_ms = value;
                self.echo_sync = false;
            }
            FxParam::ReverbMix => self.reverb_mix = value,
            FxParam::ReverbRoom => self.reverb_room = value,
            FxParam::ReverbDamping => self.reverb_damping = value,
            FxParam::FlangerMix => self.flanger_mix = value,
            FxParam::FlangerRate => self.flanger_rate = value,
            FxParam::FlangerDepth => self.flanger_depth = value,
        }
        *self = self.sanitized();
    }
    pub fn toggle(&mut self, switch: FxSwitch) {
        let value = match switch {
            FxSwitch::Chorus => &mut self.chorus_enabled,
            FxSwitch::Tremolo => &mut self.tremolo_enabled,
            FxSwitch::Distortion => &mut self.distortion_enabled,
            FxSwitch::Crusher => &mut self.crusher_enabled,
            FxSwitch::Bypass => &mut self.bypass,
            FxSwitch::Filter => &mut self.filter_enabled,
            FxSwitch::Echo => &mut self.echo_enabled,
            FxSwitch::EchoSync => &mut self.echo_sync,
            FxSwitch::Reverb => &mut self.reverb_enabled,
            FxSwitch::Flanger => &mut self.flanger_enabled,
        };
        *value = !*value;
    }
    pub fn sanitized(mut self) -> Self {
        self.chorus_mix = sane(self.chorus_mix, 0.35, 0.0, 1.0);
        self.chorus_rate = sane(self.chorus_rate, 0.6, 0.05, 5.0);
        self.tremolo_depth = sane(self.tremolo_depth, 0.6, 0.0, 1.0);
        self.tremolo_rate = sane(self.tremolo_rate, 4.0, 0.1, 16.0);
        self.drive = sane(self.drive, 3.0, 1.0, 20.0);
        self.distortion_mix = sane(self.distortion_mix, 0.3, 0.0, 1.0);
        self.crusher_bits = sane(self.crusher_bits, 10.0, 4.0, 16.0);
        self.crusher_mix = sane(self.crusher_mix, 0.4, 0.0, 1.0);
        self.filter = sane(self.filter, 0., -1., 1.);
        self.echo_mix = sane(self.echo_mix, 0.3, 0., 1.);
        self.echo_feedback = sane(self.echo_feedback, 0.35, 0., 0.85);
        self.echo_ms = sane(self.echo_ms, 350., 25., 2000.);
        self.echo_beats = sane(self.echo_beats, 0.5, 0.125, 2.);
        self.reverb_mix = sane(self.reverb_mix, 0.25, 0., 1.);
        self.reverb_room = sane(self.reverb_room, 0.6, 0., 1.);
        self.reverb_damping = sane(self.reverb_damping, 0.45, 0., 1.);
        self.flanger_mix = sane(self.flanger_mix, 0.5, 0., 1.);
        self.flanger_rate = sane(self.flanger_rate, 0.25, 0.05, 5.);
        self.flanger_depth = sane(self.flanger_depth, 0.6, 0., 1.);
        self
    }
    pub fn active_count(self) -> usize {
        usize::from(self.chorus_enabled)
            + usize::from(self.tremolo_enabled)
            + usize::from(self.distortion_enabled)
            + usize::from(self.crusher_enabled)
            + usize::from(self.filter_enabled && self.filter.abs() > 0.01)
            + usize::from(self.echo_enabled)
            + usize::from(self.reverb_enabled)
            + usize::from(self.flanger_enabled)
    }
}
struct Delay {
    data: Vec<Frame>,
    at: usize,
}
impl Delay {
    fn new(length: usize) -> Self {
        Self {
            data: vec![[0.; 2]; length.max(2)],
            at: 0,
        }
    }
    fn read(&self, delay: f32) -> Frame {
        let pos = (self.at as f32 - delay.clamp(1., (self.data.len() - 1) as f32))
            .rem_euclid(self.data.len() as f32);
        let a = pos as usize;
        let b = (a + 1) % self.data.len();
        let t = pos - a as f32;
        std::array::from_fn(|c| self.data[a][c] * (1. - t) + self.data[b][c] * t)
    }
    fn push(&mut self, frame: Frame) {
        self.data[self.at] = frame;
        self.at = (self.at + 1) % self.data.len();
    }
    fn clear(&mut self) {
        self.data.fill([0.; 2]);
        self.at = 0;
    }
}
struct Comb {
    delay: Delay,
    damp: Frame,
}
pub struct Effects {
    settings: FxSettings,
    sample_rate: f32,
    echo: Delay,
    echo_target: f32,
    echo_delay: f32,
    low: [Frame; 2],
    filter_alpha: f32,
    combs: [Comb; 4],
    allpass: [Delay; 2],
    flanger: Delay,
    phase: f32,
    bpm: f64,
    chorus: Delay,
    chorus_phase: f32,
    tremolo_phase: f32,
}
impl Effects {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(8000) as f32;
        let mut fx = Self {
            settings: FxSettings::default(),
            sample_rate: sr,
            echo: Delay::new((sr * 4.1) as usize),
            echo_target: sr * 0.25,
            echo_delay: sr * 0.25,
            low: [[0.; 2]; 2],
            filter_alpha: 1.,
            combs: std::array::from_fn(|i| Comb {
                delay: Delay::new((sr * [0.0297, 0.0371, 0.0411, 0.0437][i]) as usize),
                damp: [0.; 2],
            }),
            allpass: [
                Delay::new((sr * 0.005) as usize),
                Delay::new((sr * 0.0017) as usize),
            ],
            flanger: Delay::new((sr * 0.015) as usize),
            phase: 0.,
            bpm: 0.,
            chorus: Delay::new((sr * 0.05) as usize),
            chorus_phase: 0.,
            tremolo_phase: 0.,
        };
        fx.configure(FxSettings::default(), 120.);
        fx
    }
    pub fn configure(&mut self, settings: FxSettings, bpm: f64) {
        let settings = settings.sanitized();
        let bpm = if bpm.is_finite() {
            bpm.clamp(20., 450.)
        } else {
            120.
        };
        if self.settings == settings && self.bpm == bpm {
            return;
        }
        let previous = self.settings;
        self.bpm = bpm;
        self.settings = settings;
        let s = self.settings;
        let seconds = if s.echo_sync {
            60. / (bpm as f32).clamp(20., 450.) * s.echo_beats
        } else {
            s.echo_ms / 1000.
        };
        self.echo_target =
            (seconds * self.sample_rate).clamp(1., (self.echo.data.len() - 2) as f32);
        if s.echo_enabled && !previous.echo_enabled {
            self.echo.clear();
            self.echo_delay = self.echo_target;
        }
        if s.reverb_enabled && !previous.reverb_enabled {
            for comb in &mut self.combs {
                comb.delay.clear();
                comb.damp = [0.; 2];
            }
            for delay in &mut self.allpass {
                delay.clear();
            }
        }
        if s.flanger_enabled && !previous.flanger_enabled {
            self.flanger.clear();
            self.phase = 0.;
        }
        if s.chorus_enabled && !previous.chorus_enabled {
            self.chorus.clear();
            self.chorus_phase = 0.;
        }
        if s.tremolo_enabled && !previous.tremolo_enabled {
            self.tremolo_phase = 0.;
        }
        let cutoff = if s.filter < 0. {
            20. * 1000f32.powf(1. + s.filter)
        } else {
            20. * 1000f32.powf(s.filter)
        }
        .min(self.sample_rate * 0.45);
        self.filter_alpha = 1. - (-std::f32::consts::TAU * cutoff / self.sample_rate).exp();
    }
    pub fn reset(&mut self) {
        self.chorus.clear();
        self.chorus_phase = 0.;
        self.tremolo_phase = 0.;
        self.echo.clear();
        self.flanger.clear();
        for c in &mut self.combs {
            c.delay.clear();
            c.damp = [0.; 2];
        }
        for a in &mut self.allpass {
            a.clear();
        }
        self.low = [[0.; 2]; 2];
        self.echo_delay = self.echo_target;
        self.phase = 0.;
    }
    pub fn echo_seconds(&self) -> f32 {
        self.echo_target / self.sample_rate
    }
    pub fn process(&mut self, input: Frame) -> Frame {
        let s = self.settings;
        if s.bypass {
            return input;
        }
        let mut out = input;
        if s.filter_enabled && s.filter.abs() > 0.005 {
            for (c, sample) in out.iter_mut().enumerate() {
                self.low[0][c] += self.filter_alpha * (*sample - self.low[0][c]);
                self.low[1][c] += self.filter_alpha * (self.low[0][c] - self.low[1][c]);
                *sample = if s.filter < 0. {
                    self.low[1][c]
                } else {
                    *sample - self.low[0][c]
                };
            }
        }
        if s.echo_enabled {
            self.echo_delay += (self.echo_target - self.echo_delay) * 0.0005;
            let delayed = self.echo.read(self.echo_delay);
            self.echo.push(std::array::from_fn(|c| {
                (out[c] + delayed[c] * s.echo_feedback).clamp(-8., 8.)
            }));
            out = blend(out, delayed, s.echo_mix);
        }
        if s.reverb_enabled {
            let mut wet = [0.; 2];
            let feedback = 0.45 + s.reverb_room * 0.43;
            let damping = 0.05 + s.reverb_damping * 0.8;
            for comb in &mut self.combs {
                let delayed = comb.delay.data[comb.delay.at];
                for c in 0..2 {
                    comb.damp[c] = delayed[c] * (1. - damping) + comb.damp[c] * damping;
                    wet[c] += delayed[c] * 0.25;
                }
                comb.delay
                    .push(std::array::from_fn(|c| out[c] + comb.damp[c] * feedback));
            }
            for delay in &mut self.allpass {
                let old = delay.data[delay.at];
                let next = std::array::from_fn(|c| wet[c] + old[c] * 0.5);
                wet = std::array::from_fn(|c| old[c] - next[c] * 0.5);
                delay.push(next);
            }
            out = blend(out, wet, s.reverb_mix);
        }
        if s.flanger_enabled {
            let modulation = (self.phase.sin() + 1.) * 0.5;
            let delayed = self
                .flanger
                .read(self.sample_rate * (0.0007 + modulation * s.flanger_depth * 0.006));
            self.flanger
                .push(std::array::from_fn(|c| out[c] + delayed[c] * 0.35));
            out = blend(out, delayed, s.flanger_mix);
            self.phase = (self.phase + std::f32::consts::TAU * s.flanger_rate / self.sample_rate)
                % std::f32::consts::TAU;
        }
        if s.chorus_enabled {
            let delay = self.sample_rate * (0.018 + 0.006 * self.chorus_phase.sin());
            let wet = self.chorus.read(delay);
            self.chorus.push(out);
            out = blend(out, wet, s.chorus_mix);
            self.chorus_phase = (self.chorus_phase
                + std::f32::consts::TAU * s.chorus_rate / self.sample_rate)
                % std::f32::consts::TAU;
        }
        if s.tremolo_enabled {
            let gain = 1. - s.tremolo_depth * (0.5 + 0.5 * self.tremolo_phase.sin());
            out = out.map(|x| x * gain);
            self.tremolo_phase = (self.tremolo_phase
                + std::f32::consts::TAU * s.tremolo_rate / self.sample_rate)
                % std::f32::consts::TAU;
        }
        if s.distortion_enabled {
            out = blend(
                out,
                out.map(|x| (x * s.drive).tanh() / s.drive.tanh()),
                s.distortion_mix,
            );
        }
        if s.crusher_enabled {
            let steps = 2_f32.powf(s.crusher_bits.round() - 1.);
            out = blend(
                out,
                out.map(|x| (x.clamp(-1., 1.) * steps).round() / steps),
                s.crusher_mix,
            );
        }
        out.map(|x| if x.is_finite() { x.clamp(-8., 8.) } else { 0. })
    }
}
fn blend(dry: Frame, wet: Frame, mix: f32) -> Frame {
    std::array::from_fn(|c| dry[c] * (1. - mix) + wet[c] * mix)
}

//! Optional system capture, bounded non-blocking handoff to the MASTER engine.
use crate::media::Frame;
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
pub struct Microphone {
    _stream: cpal::Stream,
    pub output_rate: u32,
    pub device_name: String,
    healthy: Arc<AtomicBool>,
    errors: Receiver<String>,
}
impl Microphone {
    pub fn error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
    pub fn is_running(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }
}
pub fn input_names() -> Result<Vec<String>> {
    let mut names: Vec<_> = cpal::default_host()
        .input_devices()?
        .filter_map(|d| d.description().ok().map(|d| d.name().to_owned()))
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}
/// Linear streaming conversion; no allocation, waiting or file I/O per input frame.
pub struct Resampler {
    ratio: f64,
    phase: f64,
    previous: f32,
}
impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        Self {
            ratio: output_rate.max(1) as f64 / input_rate.max(1) as f64,
            phase: 0.,
            previous: 0.,
        }
    }
    pub fn push(&mut self, value: f32, mut emit: impl FnMut(Frame)) {
        let value = if value.is_finite() {
            value.clamp(-1., 1.)
        } else {
            0.
        };
        self.phase += self.ratio;
        while self.phase >= 1. {
            let t = (1. - (self.phase - 1.) / self.ratio).clamp(0., 1.) as f32;
            let sample = self.previous + (value - self.previous) * t;
            emit([sample; 2]);
            self.phase -= 1.;
        }
        self.previous = value;
    }
}
pub fn start(name: &str, output_rate: u32) -> Result<(Microphone, Receiver<Frame>)> {
    let host = cpal::default_host();
    let device = if name.is_empty() {
        host.default_input_device()
            .context("Brak domyślnego mikrofonu")?
    } else {
        host.input_devices()?
            .find(|d| d.description().ok().is_some_and(|d| d.name() == name))
            .context("Wybrany mikrofon jest niedostępny")?
    };
    let device_name = device.description()?.name().to_owned();
    let supported = device
        .default_input_config()
        .context("Brak konfiguracji wejścia mikrofonu")?;
    let config: cpal::StreamConfig = supported.clone().into();
    anyhow::ensure!(config.channels > 0, "Mikrofon zgłasza zero kanałów");
    let (tx, rx) = bounded((output_rate as usize / 20).max(64));
    let discard = rx.clone();
    let healthy = Arc::new(AtomicBool::new(true));
    let failure = healthy.clone();
    let (error_tx, errors) = bounded(1);
    let on_error = move |error: cpal::StreamError| {
        failure.store(false, Ordering::Relaxed);
        let _ = error_tx.try_send(error.to_string());
    };
    macro_rules! build {
        ($sample:ty) => {{
            let mut resampler = Resampler::new(config.sample_rate, output_rate);
            let channels = config.channels as usize;
            device.build_input_stream(
                &config,
                move |data: &[$sample], _| {
                    use cpal::Sample;
                    for frame in data.chunks_exact(channels) {
                        // A microphone is mono; use input channel 1 and send to both speakers.
                        resampler.push(frame[0].to_sample::<f32>(), |frame| {
                            if let Err(crossbeam_channel::TrySendError::Full(frame)) =
                                tx.try_send(frame)
                            {
                                let _ = discard.try_recv();
                                let _ = tx.try_send(frame);
                            }
                        });
                    }
                },
                on_error,
                None,
            )?
        }};
    }
    let stream = match supported.sample_format() {
        cpal::SampleFormat::I8 => build!(i8),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::I24 => build!(cpal::I24),
        cpal::SampleFormat::I32 => build!(i32),
        cpal::SampleFormat::I64 => build!(i64),
        cpal::SampleFormat::U8 => build!(u8),
        cpal::SampleFormat::U16 => build!(u16),
        cpal::SampleFormat::U32 => build!(u32),
        cpal::SampleFormat::U64 => build!(u64),
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::F64 => build!(f64),
        other => anyhow::bail!("Nieobsługiwany format mikrofonu: {other:?}"),
    };
    stream
        .play()
        .context("Nie można uruchomić wejścia mikrofonu")?;
    Ok((
        Microphone {
            _stream: stream,
            output_rate,
            device_name,
            healthy,
            errors,
        },
        rx,
    ))
}

use crate::{
    engine::{Command, Engine, Snapshot},
    media::Frame,
};
use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::{Arc, Mutex};

pub struct AudioRuntime {
    _master: cpal::Stream,
    _cue: Option<cpal::Stream>,
    pub sample_rate: u32,
    pub latency_ms: f32,
}
pub fn output_names() -> Vec<String> {
    cpal::default_host()
        .output_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}
fn device(name: &str) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if name.is_empty() {
        return host
            .default_output_device()
            .context("Brak domyślnego wyjścia audio");
    }
    host.output_devices()?
        .find(|d| d.name().ok().as_deref() == Some(name))
        .context("Wybrane wyjście audio jest niedostępne")
}
pub fn start(
    rx: Receiver<Command>,
    view: Arc<Mutex<Snapshot>>,
    errors: Sender<String>,
    master_name: &str,
    cue_name: &str,
    latency_ms: u32,
) -> Result<AudioRuntime> {
    if !cue_name.is_empty()
        && (master_name == cue_name
            || (master_name.is_empty() && device("")?.name().ok().as_deref() == Some(cue_name)))
    {
        bail!("MASTER i odsłuch muszą używać różnych urządzeń");
    }
    let output = device(master_name)?;
    let supported = output.default_output_config()?;
    let rate = supported.sample_rate().0;
    let mut config: cpal::StreamConfig = supported.clone().into();
    let target = (rate as u64 * latency_ms as u64 / 1000).clamp(64, 8192) as u32;
    config.buffer_size = match supported.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => {
            cpal::BufferSize::Fixed(target.clamp(*min, *max))
        }
        _ => cpal::BufferSize::Default,
    };
    let channels = config.channels as usize;
    let (cue_tx, cue_rx) = bounded::<Frame>(8192);
    let cue = if cue_name.is_empty() {
        None
    } else {
        Some(cue_stream(
            &device(cue_name)?,
            cue_rx,
            rate,
            errors.clone(),
        )?)
    };
    let sender = if cue.is_some() { Some(cue_tx) } else { None };
    let mut engine = Engine::new(rate);
    let mut publish_count = 0u32;
    let mut next = move || {
        if publish_count == 0 {
            for command in rx.try_iter().take(256) {
                engine.apply(command);
            }
        }
        let (frame, phones) = engine.render();
        if let Some(tx) = &sender {
            let _ = tx.try_send(phones);
        }
        publish_count += 1;
        if publish_count >= 256 {
            publish_count = 0;
            if let Ok(mut shared) = view.try_lock() {
                *shared = engine.snapshot();
            }
        }
        frame
    };
    let error_callback = move |error| {
        let _ = errors.try_send(format!("Audio: {error}"));
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => output.build_output_stream(
            &config,
            move |data: &mut [f32], _| fill(data, channels, &mut next, |x| x),
            error_callback,
            None,
        )?,
        cpal::SampleFormat::I16 => output.build_output_stream(
            &config,
            move |data: &mut [i16], _| {
                fill(data, channels, &mut next, |x| {
                    (x.clamp(-1., 1.) * 32767.) as i16
                })
            },
            error_callback,
            None,
        )?,
        cpal::SampleFormat::U16 => output.build_output_stream(
            &config,
            move |data: &mut [u16], _| {
                fill(data, channels, &mut next, |x| {
                    ((x.clamp(-1., 1.) + 1.) * 32767.5) as u16
                })
            },
            error_callback,
            None,
        )?,
        format => return Err(anyhow!("Nieobsługiwany format urządzenia: {format:?}")),
    };
    stream.play()?;
    Ok(AudioRuntime {
        _master: stream,
        _cue: cue,
        sample_rate: rate,
        latency_ms: target as f32 / rate as f32 * 1000.,
    })
}
fn fill<T: Copy>(
    data: &mut [T],
    channels: usize,
    next: &mut impl FnMut() -> Frame,
    convert: impl Fn(f32) -> T,
) {
    for samples in data.chunks_mut(channels) {
        let frame = next();
        for (c, sample) in samples.iter_mut().enumerate() {
            *sample = convert(if channels == 1 {
                (frame[0] + frame[1]) * 0.5
            } else if c < 2 {
                frame[c]
            } else {
                0.
            });
        }
    }
}
fn cue_stream(
    device: &cpal::Device,
    rx: Receiver<Frame>,
    source_rate: u32,
    errors: Sender<String>,
) -> Result<cpal::Stream> {
    let supported = device.default_output_config()?;
    let config: cpal::StreamConfig = supported.clone().into();
    let channels = config.channels as usize;
    let ratio = source_rate as f64 / config.sample_rate.0 as f64;
    let mut phase = 0.;
    let mut previous = [0.; 2];
    let mut next_frame = [0.; 2];
    let mut next = move || {
        // A small drift adjustment keeps independently clocked devices bounded.
        let correction = if rx.len() > 4096 {
            1.001
        } else if rx.len() < 256 {
            0.999
        } else {
            1.
        };
        phase += ratio * correction;
        while phase >= 1. {
            previous = next_frame;
            next_frame = rx.try_recv().unwrap_or([0.; 2]);
            phase -= 1.;
        }
        [
            previous[0] * (1. - phase as f32) + next_frame[0] * phase as f32,
            previous[1] * (1. - phase as f32) + next_frame[1] * phase as f32,
        ]
    };
    let error = move |e| {
        let _ = errors.try_send(format!("Odsłuch: {e}"));
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config,
            move |data: &mut [f32], _| fill(data, channels, &mut next, |x| x),
            error,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config,
            move |data: &mut [i16], _| {
                fill(data, channels, &mut next, |x| {
                    (x.clamp(-1., 1.) * 32767.) as i16
                })
            },
            error,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_output_stream(
            &config,
            move |data: &mut [u16], _| {
                fill(data, channels, &mut next, |x| {
                    ((x.clamp(-1., 1.) + 1.) * 32767.5) as u16
                })
            },
            error,
            None,
        )?,
        format => bail!("Nieobsługiwany format odsłuchu: {format:?}"),
    };
    stream.play()?;
    Ok(stream)
}

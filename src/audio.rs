//! Native output negotiation. Device/driver setup and logging never run in the callback.
use crate::{
    engine::{Command, Engine, Snapshot},
    media::Frame,
};
use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    #[default]
    System,
    Asio,
}
impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Asio => "ASIO (Windows)",
            Self::System if cfg!(target_os = "windows") => "WASAPI (Windows)",
            Self::System => "System / ALSA",
        }
    }
}
pub fn backends() -> Vec<Backend> {
    if cfg!(target_os = "windows") {
        vec![Backend::System, Backend::Asio]
    } else {
        vec![Backend::System]
    }
}
fn host(backend: Backend) -> Result<cpal::Host> {
    match backend {
        Backend::System => Ok(cpal::default_host()),
        #[cfg(target_os = "windows")]
        Backend::Asio => cpal::host_from_id(cpal::HostId::Asio).context("ASIO host unavailable"),
        #[cfg(not(target_os = "windows"))]
        Backend::Asio => bail!("ASIO is available only in the Windows build"),
    }
}
fn device_name(d: &cpal::Device) -> String {
    d.description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| "Unknown device".into())
}
pub fn output_names_for(backend: Backend) -> Result<Vec<String>> {
    #[cfg(target_os = "windows")]
    if backend == Backend::Asio {
        return Ok(asio_sys::Asio::new().driver_names());
    }
    let mut names: Vec<_> = host(backend)?
        .output_devices()?
        .map(|d| device_name(&d))
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}
pub fn output_names() -> Vec<String> {
    output_names_for(Backend::System).unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct Options {
    pub backend: Backend,
    pub master_name: String,
    pub cue_name: String,
    pub buffer_frames: Option<u32>,
    pub master_pair: u16,
    pub cue_pair: Option<u16>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            backend: Backend::System,
            master_name: String::new(),
            cue_name: String::new(),
            buffer_frames: None,
            master_pair: 0,
            cue_pair: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Routing {
    pub master: u16,
    pub cue: Option<u16>,
}
impl Routing {
    pub fn validate(self, channels: u16) -> Result<()> {
        anyhow::ensure!(channels > 0, "Device reports zero output channels");
        if channels == 1 && self.master == 0 && self.cue.is_none() {
            return Ok(());
        }
        anyhow::ensure!(
            self.master.is_multiple_of(2)
                && self.master.checked_add(1).is_some_and(|end| end < channels),
            "MASTER pair {}–{} unavailable on a {channels}-channel output",
            self.master as u32 + 1,
            self.master as u32 + 2
        );
        if let Some(cue) = self.cue {
            anyhow::ensure!(
                cue != self.master
                    && cue.is_multiple_of(2)
                    && cue.checked_add(1).is_some_and(|end| end < channels),
                "PFL pair must be a different available output pair"
            );
        }
        Ok(())
    }
}

/// A missing monitor pair may disable PFL, but must not silence a valid MASTER pair.
pub fn resolve_routing(request: Routing, channels: u16) -> Result<(Routing, Option<String>)> {
    match request.validate(channels) {
        Ok(()) => Ok((request, None)),
        Err(error) => {
            let master_only = Routing {
                master: request.master,
                cue: None,
            };
            if request.cue.is_some() && master_only.validate(channels).is_ok() {
                Ok((
                    master_only,
                    Some(format!("ASIO PFL disabled; MASTER remains active: {error}")),
                ))
            } else {
                Err(error)
            }
        }
    }
}

pub struct AudioRuntime {
    _master: cpal::Stream,
    _cue: Option<cpal::Stream>,
    pub sample_rate: u32,
    pub backend: Backend,
    pub device_name: String,
    pub channels: u16,
    pub format: cpal::SampleFormat,
    pub requested_buffer: Option<u32>,
    pub warnings: Vec<String>,
    pub routing: Routing,
    healthy: Arc<AtomicBool>,
    callback_frames: Arc<AtomicU32>,
}
impl AudioRuntime {
    pub fn is_running(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }
    pub fn buffer_description(&self) -> String {
        let n = self.callback_frames.load(Ordering::Relaxed);
        if n == 0 {
            "driver buffer; awaiting first callback".into()
        } else {
            format!(
                "callback block {n} frames ({:.2} ms); hardware latency not measured",
                n as f64 / self.sample_rate as f64 * 1000.
            )
        }
    }
    pub fn diagnostics(&self) -> String {
        format!("Prospekt DJ {} PRE-BETA\nBackend: {}\nMASTER device: {}\nFormat: {:?}; {} Hz; {} output channels\nRequested buffer: {:?}\n{}\nMASTER channels: {}–{}\nPFL pair: {:?}\nWarnings:\n{}\n",env!("CARGO_PKG_VERSION"),self.backend.label(),self.device_name,self.format,self.sample_rate,self.channels,self.requested_buffer,self.buffer_description(),self.routing.master as u32+1,self.routing.master as u32+2,self.routing.cue.map(|n|format!("{}–{}",n as u32+1,n as u32+2)),self.warnings.join("\n"))
    }
}
pub fn diagnostic_path() -> std::path::PathBuf {
    crate::settings::config_path().with_file_name("audio-diagnostics.txt")
}
pub fn save_diagnostics(text: &str) {
    let path = diagnostic_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        path,
        format!("{}\n{text}", chrono::Local::now().to_rfc3339()),
    );
}

/// Compatibility entry point. Automatic driver buffer replaces unsafe millisecond forcing.
pub fn start(
    rx: Receiver<Command>,
    view: Arc<Mutex<Snapshot>>,
    errors: Sender<String>,
    master_name: &str,
    cue_name: &str,
    _latency_ms: u32,
) -> Result<AudioRuntime> {
    start_with_options(
        rx,
        view,
        errors,
        &Options {
            master_name: master_name.into(),
            cue_name: cue_name.into(),
            ..Default::default()
        },
    )
}

pub fn start_with_options(
    rx: Receiver<Command>,
    view: Arc<Mutex<Snapshot>>,
    errors: Sender<String>,
    options: &Options,
) -> Result<AudioRuntime> {
    let host = host(options.backend)?;
    let mut warnings = vec![];
    let output = if options.master_name.is_empty() {
        host.default_output_device().or_else(||host.output_devices().ok()?.next()).context(if options.backend==Backend::Asio{"No ASIO output driver. Install the manufacturer's 64-bit ASIO driver, reconnect the interface and refresh devices."}else{"No active output. Enable speakers/headphones in Windows Sound settings and reconnect the device."})?
    } else {
        match host
            .output_devices()?
            .find(|d| device_name(d) == options.master_name)
        {
            Some(d) => d,
            None if options.backend == Backend::System => {
                warnings.push(format!(
                    "Saved MASTER '{}' unavailable; using the active system default",
                    options.master_name
                ));
                host.default_output_device()
                    .or_else(|| host.output_devices().ok()?.next())
                    .context("Saved output and system default are unavailable")?
            }
            None => bail!(
                "ASIO driver '{}' not available. Select an installed 64-bit driver in Settings.",
                options.master_name
            ),
        }
    };
    let name = device_name(&output);
    let routing = if options.backend == Backend::Asio {
        Routing {
            master: options.master_pair,
            cue: options.cue_pair,
        }
    } else {
        Routing::default()
    };
    let mut candidates = vec![];
    let mut failures = vec![];
    match output.default_output_config() {
        Ok(config) => candidates.push(config),
        Err(e) => failures.push(format!("default configuration: {e}")),
    }
    if let Ok(ranges) = output.supported_output_configs() {
        let required = routing.cue.unwrap_or(routing.master).max(routing.master) as u32 + 2;
        let mut extra = vec![];
        for range in ranges.take(512) {
            if range.channels() == 0
                || (options.backend == Backend::Asio && (range.channels() as u32) < required)
            {
                continue;
            }
            for rate in [48000, 44100, range.min_sample_rate()] {
                if rate >= range.min_sample_rate() && rate <= range.max_sample_rate() {
                    extra.push(range.with_sample_rate(rate));
                    break;
                }
            }
        }
        extra.sort_by_key(|c| {
            (
                if c.channels() == 2 { 0 } else { 1 },
                if c.sample_format() == cpal::SampleFormat::F32 {
                    0
                } else {
                    1
                },
                c.channels(),
            )
        });
        for c in extra {
            if !candidates.iter().any(|a| {
                a.channels() == c.channels()
                    && a.sample_rate() == c.sample_rate()
                    && a.sample_format() == c.sample_format()
            }) {
                candidates.push(c);
            }
        }
    }
    for supported in candidates.into_iter().take(24) {
        let (routing, route_warning) = match resolve_routing(routing, supported.channels()) {
            Ok(value) => value,
            Err(error) => {
                failures.push(error.to_string());
                continue;
            }
        };
        for buffer in buffer_attempts(options.buffer_frames, *supported.buffer_size()) {
            let mut config: cpal::StreamConfig = supported.clone().into();
            config.buffer_size = buffer;
            let mut candidate_warnings = warnings.clone();
            if let Some(warning) = &route_warning {
                candidate_warnings.push(warning.clone());
            }
            if options.buffer_frames.is_some() && matches!(buffer, cpal::BufferSize::Default) {
                candidate_warnings.push(
                    "Requested fixed buffer unavailable; using the driver's default buffer".into(),
                );
            }
            match start_configuration(
                &output,
                &config,
                supported.sample_format(),
                rx.clone(),
                view.clone(),
                errors.clone(),
                options,
                routing,
                &mut candidate_warnings,
            ) {
                Ok((master, cue, healthy, callback_frames)) => {
                    if !failures.is_empty() {
                        candidate_warnings.push(format!(
                            "Recovered after configuration failures: {}",
                            failures.join(" | ")
                        ));
                    }
                    return Ok(AudioRuntime {
                        _master: master,
                        _cue: cue,
                        sample_rate: config.sample_rate,
                        backend: options.backend,
                        device_name: name,
                        channels: config.channels,
                        format: supported.sample_format(),
                        requested_buffer: match buffer {
                            cpal::BufferSize::Fixed(n) => Some(n),
                            _ => None,
                        },
                        warnings: candidate_warnings,
                        routing,
                        healthy,
                        callback_frames,
                    });
                }
                Err(e) => failures.push(format!(
                    "{:?}, {} Hz, {} ch, {:?}: {e:#}",
                    supported.sample_format(),
                    config.sample_rate,
                    config.channels,
                    buffer
                )),
            }
        }
    }
    bail!(
        "Cannot open MASTER '{}' through {}. {}",
        name,
        options.backend.label(),
        failures.join(" | ")
    )
}
/// Fixed buffers are attempted only when explicitly requested and in the reported range.
pub fn buffer_attempts(
    request: Option<u32>,
    range: cpal::SupportedBufferSize,
) -> Vec<cpal::BufferSize> {
    let mut result = vec![];
    if let Some(n) = request {
        if n > 0
            && match range {
                cpal::SupportedBufferSize::Range { min, max } => n >= min && n <= max,
                cpal::SupportedBufferSize::Unknown => true,
            }
        {
            result.push(cpal::BufferSize::Fixed(n));
        }
    }
    result.push(cpal::BufferSize::Default);
    result
}
type Streams = (
    cpal::Stream,
    Option<cpal::Stream>,
    Arc<AtomicBool>,
    Arc<AtomicU32>,
);
#[allow(clippy::too_many_arguments)]
fn start_configuration(
    output: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    rx: Receiver<Command>,
    view: Arc<Mutex<Snapshot>>,
    errors: Sender<String>,
    options: &Options,
    routing: Routing,
    warnings: &mut Vec<String>,
) -> Result<Streams> {
    let (cue_tx, cue_rx) = bounded::<Frame>(8192);
    let cue_enabled = Arc::new(AtomicBool::new(false));
    let sender = if options.backend == Backend::System && !options.cue_name.is_empty() {
        Some((cue_tx, cue_enabled.clone()))
    } else {
        None
    };
    let (mut engine, warning) = Engine::with_store_or_memory(
        config.sample_rate,
        &crate::settings::config_path().with_file_name("tracks.sqlite"),
    );
    if let Some(warning) = warning {
        warnings.push(warning);
    }
    let mut count = 0_u32;
    let next = move || {
        if count == 0 {
            for command in rx.try_iter().take(256) {
                engine.apply(command);
            }
        }
        let (frame, phones) = engine.render();
        if let Some((tx, enabled)) = &sender {
            if enabled.load(Ordering::Relaxed) {
                let _ = tx.try_send(phones);
            }
        }
        count += 1;
        if count >= 256 {
            count = 0;
            if let Ok(mut shared) = view.try_lock() {
                *shared = engine.snapshot();
            }
        }
        (frame, phones)
    };
    let healthy = Arc::new(AtomicBool::new(true));
    let frames = Arc::new(AtomicU32::new(0));
    let stream = build_stream(
        output,
        config,
        format,
        next,
        routing,
        errors.clone(),
        healthy.clone(),
        frames.clone(),
    )?;
    stream.play().context("Starting MASTER stream")?;
    // Open optional monitoring only after the primary stream is working.
    let cue = if options.backend == Backend::System && !options.cue_name.is_empty() {
        let result = (|| -> Result<cpal::Stream> {
            anyhow::ensure!(options.cue_name!=device_name(output),"MASTER and PFL refer to the same output. Use another device or ASIO channel-pair routing.");
            let host = host(Backend::System)?;
            let device = host
                .output_devices()?
                .find(|d| device_name(d) == options.cue_name)
                .context("PFL output unavailable")?;
            cue_stream(&device, cue_rx, config.sample_rate, errors.clone())
        })();
        match result {
            Ok(stream) => {
                cue_enabled.store(true, Ordering::Relaxed);
                Some(stream)
            }
            Err(e) => {
                warnings.push(format!("PFL disabled; MASTER remains active: {e:#}"));
                None
            }
        }
    } else {
        None
    };
    Ok((stream, cue, healthy, frames))
}
#[allow(clippy::too_many_arguments)]
fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    next: impl FnMut() -> (Frame, Frame) + Send + 'static,
    routing: Routing,
    errors: Sender<String>,
    healthy: Arc<AtomicBool>,
    frames: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    macro_rules! typed {
        ($t:ty) => {
            build_typed::<$t, _>(device, config, next, routing, errors, healthy, frames)
        };
    }
    Ok(match format {
        cpal::SampleFormat::F32 => typed!(f32)?,
        cpal::SampleFormat::F64 => typed!(f64)?,
        cpal::SampleFormat::I8 => typed!(i8)?,
        cpal::SampleFormat::I16 => typed!(i16)?,
        cpal::SampleFormat::I24 => typed!(cpal::I24)?,
        cpal::SampleFormat::I32 => typed!(i32)?,
        cpal::SampleFormat::I64 => typed!(i64)?,
        cpal::SampleFormat::U8 => typed!(u8)?,
        cpal::SampleFormat::U16 => typed!(u16)?,
        cpal::SampleFormat::U24 => typed!(cpal::U24)?,
        cpal::SampleFormat::U32 => typed!(u32)?,
        cpal::SampleFormat::U64 => typed!(u64)?,
        other => bail!("Unsupported PCM output format: {other:?}"),
    })
}
#[allow(clippy::too_many_arguments)]
fn build_typed<T, F>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut next: F,
    routing: Routing,
    errors: Sender<String>,
    healthy: Arc<AtomicBool>,
    frames: Arc<AtomicU32>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
    F: FnMut() -> (Frame, Frame) + Send + 'static,
{
    let channels = config.channels as usize;
    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            frames.store((data.len() / channels) as u32, Ordering::Relaxed);
            fill_output(data, channels, routing, &mut next);
        },
        move |e| {
            healthy.store(false, Ordering::Relaxed);
            let _ = errors.try_send(format!("Audio stream stopped: {e}"));
        },
        None,
    )?)
}
/// One engine render per hardware frame, independent of channel count.
pub fn fill_output<T: cpal::Sample + cpal::FromSample<f32>>(
    data: &mut [T],
    channels: usize,
    routing: Routing,
    next: &mut impl FnMut() -> (Frame, Frame),
) {
    if channels == 0 {
        return;
    }
    for samples in data.chunks_mut(channels) {
        let (master, cue) = next();
        for (c, sample) in samples.iter_mut().enumerate() {
            let x = if channels == 1 {
                (master[0] + master[1]) * 0.5
            } else if c == routing.master as usize {
                master[0]
            } else if c == routing.master as usize + 1 {
                master[1]
            } else if routing.cue.is_some_and(|n| c == n as usize) {
                cue[0]
            } else if routing.cue.is_some_and(|n| c == n as usize + 1) {
                cue[1]
            } else {
                0.
            };
            *sample = T::from_sample(if x.is_finite() { x.clamp(-1., 1.) } else { 0. });
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
    let mut config: cpal::StreamConfig = supported.clone().into();
    config.buffer_size = cpal::BufferSize::Default;
    let ratio = source_rate as f64 / config.sample_rate as f64;
    let mut phase = 0.;
    let mut previous = [0.; 2];
    let mut next_frame = [0.; 2];
    let next = move || {
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
        let frame = [
            previous[0] * (1. - phase as f32) + next_frame[0] * phase as f32,
            previous[1] * (1. - phase as f32) + next_frame[1] * phase as f32,
        ];
        (frame, [0.; 2])
    };
    let stream = build_stream(
        device,
        &config,
        supported.sample_format(),
        next,
        Routing::default(),
        errors,
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicU32::new(0)),
    )?;
    stream.play()?;
    Ok(stream)
}

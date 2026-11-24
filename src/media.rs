use anyhow::{anyhow, bail, Context, Result};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    tag::Accessor,
};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
    io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

pub type Frame = [f32; 2];
pub const MAX_FRAMES: usize = 64 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct WaveBin {
    pub level: f32,
    pub bands: [f32; 3],
}

#[derive(Debug)]
pub struct Track {
    pub id: String,
    pub file_bytes: u64,
    pub path: Option<PathBuf>,
    pub title: String,
    pub artist: String,
    pub frames: Vec<Frame>,
    pub sample_rate: u32,
    pub bpm: f64,
    /// Position of the first beat-grid line in seconds, used for phase SYNC.
    pub beat_offset: f64,
    pub peaks: Vec<f32>,
    pub bands: Vec<[f32; 3]>,
    pub detail: Vec<WaveBin>,
    pub detail_rate: f64,
    pub cover: Option<Vec<u8>>,
}
impl Track {
    pub fn duration(&self) -> f64 {
        self.frames.len() as f64 / self.sample_rate as f64
    }
    pub fn from_frames(title: String, frames: Vec<Frame>, sample_rate: u32) -> Self {
        let hop = (sample_rate as usize / 200).max(1);
        let sr = sample_rate.max(1) as f32;
        let low_alpha = 1. - (-std::f32::consts::TAU * 200. / sr).exp();
        let mid_alpha = 1. - (-std::f32::consts::TAU * 2000. / sr).exp();
        let (mut low, mut mid) = (0f32, 0f32);
        let detail: Vec<_> = frames
            .chunks(hop)
            .map(|chunk| {
                let mut bin = WaveBin::default();
                for frame in chunk {
                    let x = frame[0];
                    low += low_alpha * (x - low);
                    mid += mid_alpha * (x - mid);
                    let bands = [low, mid - low, x - mid];
                    for (c, band) in bands.iter().enumerate() {
                        bin.bands[c] = bin.bands[c].max(band.abs());
                    }
                    bin.level = bin.level.max(frame[0].abs().max(frame[1].abs()));
                }
                bin
            })
            .collect();
        let step = detail.len().div_ceil(1200).max(1);
        let peaks = detail
            .chunks(step)
            .map(|bins| bins.iter().fold(0f32, |a, b| a.max(b.level)))
            .collect();
        let bands = detail
            .chunks(step)
            .map(|bins| std::array::from_fn(|c| bins.iter().fold(0f32, |a, b| a.max(b.bands[c]))))
            .collect();
        use sha2::{Digest, Sha256};
        let mut identity = Sha256::new();
        identity.update(b"prospekt-source-pcm-v1");
        identity.update(sample_rate.to_le_bytes());
        for frame in &frames {
            for channel in frame {
                identity.update(channel.to_le_bytes());
            }
        }
        let id = format!("pcm1:{:x}", identity.finalize());
        Self {
            id,
            file_bytes: 0,
            path: None,
            title,
            artist: String::new(),
            frames,
            sample_rate,
            bpm: 120.,
            beat_offset: 0.,
            peaks,
            bands,
            detail,
            detail_rate: sample_rate as f64 / hop as f64,
            cover: None,
        }
    }
    pub fn sample(&self, pos: f64) -> Frame {
        if pos < 0. || pos >= self.frames.len() as f64 {
            return [0.; 2];
        }
        let a = pos as usize;
        let b = (a + 1).min(self.frames.len() - 1);
        let fraction = (pos - a as f64) as f32;
        [
            self.frames[a][0] * (1. - fraction) + self.frames[b][0] * fraction,
            self.frames[a][1] * (1. - fraction) + self.frames[b][1] * fraction,
        ]
    }
}

pub fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "mp3"
                | "wav"
                | "wave"
                | "flac"
                | "ogg"
                | "opus"
                | "aif"
                | "aiff"
                | "m4a"
                | "aac"
                | "webm"
        )
    })
}

pub fn load(path: &Path, auto_bpm: bool) -> Result<Arc<Track>> {
    let metadata = std::fs::metadata(path).context("Nie można otworzyć pliku")?;
    // Uncompressed WAV/AIFF sets of 10+ minutes exceed 100 MiB; memory is bounded by MAX_FRAMES.
    if metadata.len() > MAX_FILE_BYTES {
        bail!("Plik przekracza limit 1 GiB");
    }
    let (frames, sr) = decode(path)
        .or_else(|native| decode_ffmpeg(path).with_context(|| format!("Dekoder Rust: {native}")))?;
    if frames.is_empty() {
        bail!("Plik nie zawiera próbek audio");
    }
    let title = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut track = Track::from_frames(title, frames, sr);
    track.path = Some(path.to_path_buf());
    track.file_bytes = metadata.len();
    let mut declared_bpm = None;
    if let Ok(tagged) = lofty::read_from_path(path) {
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(title) = tag.title() {
                track.title = title.into_owned();
            }
            if let Some(artist) = tag.artist() {
                track.artist = artist.into_owned();
            }
            track.cover = tag
                .pictures()
                .first()
                .filter(|p| p.data().len() <= 8 * 1024 * 1024)
                .map(|p| p.data().to_vec());
            declared_bpm = tag
                .get_string(&lofty::tag::ItemKey::Bpm)
                .or_else(|| tag.get_string(&lofty::tag::ItemKey::IntegerBpm))
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| (40.0..=300.).contains(v));
        }
        let _ = tagged.properties();
    }
    if auto_bpm {
        if let Some(bpm) = estimate_bpm(&track) {
            track.bpm = bpm;
        }
    } else if let Some(bpm) = declared_bpm {
        track.bpm = bpm;
    }
    track.beat_offset = estimate_beat_offset(&track, track.bpm);
    Ok(Arc::new(track))
}

fn decode(path: &Path) -> Result<(Vec<Frame>, u32)> {
    let source = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }
    let probe = symphonia::default::get_probe().format(
        &hint,
        source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probe.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("Brak ścieżki audio"))?;
    let id = track.id;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
    let mut sr = track.codec_params.sample_rate.unwrap_or(48000);
    let mut frames = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(audio) => audio,
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };
        if !frames.is_empty() && sr != decoded.spec().rate {
            bail!("Zmiana częstotliwości próbkowania wewnątrz pliku");
        }
        sr = decoded.spec().rate;
        let channels = decoded.spec().channels.count();
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        buffer.copy_interleaved_ref(decoded);
        for frame in buffer.samples().chunks(channels) {
            // Stereo/mono parity with the supplied HTML. Extra channels fold down safely.
            let left = frame[0];
            let right = *frame.get(1).unwrap_or(&left);
            frames.push([finite(left), finite(right)]);
        }
        if frames.len() > MAX_FRAMES {
            bail!("Zdekodowany utwór przekracza limit pamięci 512 MiB");
        }
    }
    Ok((frames, sr))
}
fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-4., 4.)
    } else {
        0.
    }
}

pub fn ffmpeg_path() -> PathBuf {
    if let Some(path) = std::env::var_os("PROSPEKT_FFMPEG") {
        return path.into();
    }
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.with_file_name(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        });
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("ffmpeg")
}
pub fn ffmpeg_command() -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(ffmpeg_path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}
fn decode_ffmpeg(path: &Path) -> Result<(Vec<Frame>, u32)> {
    let mut child = ffmpeg_command()
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-f", "f32le", "-ac", "2", "-ar", "48000", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Ten kodek wymaga FFmpeg w PATH lub obok programu")?;
    let mut data = Vec::new();
    let read = child
        .stdout
        .take()
        .unwrap()
        .take((MAX_FRAMES * 8 + 1) as u64)
        .read_to_end(&mut data);
    if read.is_err() || data.len() > MAX_FRAMES * 8 {
        let _ = child.kill();
        let _ = child.wait();
        bail!("Przekroczono limit dekodowania");
    }
    if !child.wait()?.success() {
        bail!("FFmpeg nie odczytał pliku");
    }
    let frames = data
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| {
            [
                finite(f32::from_le_bytes(b[0..4].try_into().unwrap())),
                finite(f32::from_le_bytes(b[4..8].try_into().unwrap())),
            ]
        })
        .collect();
    Ok((frames, 48000))
}

/// Spectral-flux style onset envelope (positive RMS differences) at about 200 Hz.
fn onset_envelope(track: &Track, seconds: usize) -> (Vec<f64>, f64) {
    let hop = (track.sample_rate as usize / 200).max(1);
    let max = track.frames.len().min(track.sample_rate as usize * seconds);
    let mut previous = 0.;
    let envelope: Vec<f64> = track.frames[..max]
        .chunks(hop)
        .map(|chunk| {
            let rms = (chunk
                .iter()
                .map(|f| ((f[0] + f[1]) as f64 * 0.5).powi(2))
                .sum::<f64>()
                / chunk.len() as f64)
                .sqrt();
            let flux = (rms - previous).max(0.);
            previous = rms;
            flux
        })
        .collect();
    (envelope, track.sample_rate as f64 / hop as f64)
}

/// Finds the beat phase that best lines up with onsets for a known tempo.
/// Returns the first beat position in seconds, within one beat period.
pub fn estimate_beat_offset(track: &Track, bpm: f64) -> f64 {
    if !(bpm.is_finite() && bpm > 0.) || track.sample_rate == 0 {
        return 0.;
    }
    let (envelope, hz) = onset_envelope(track, 60);
    let period = hz * 60. / bpm;
    if envelope.len() < (period * 4.) as usize || period < 2. {
        return 0.;
    }
    let steps = period.floor() as usize;
    let mut best = (0usize, f64::MIN);
    for phase in 0..steps {
        let mut score = 0.;
        let mut at = phase as f64;
        while (at as usize) < envelope.len() {
            let i = (at.round() as usize).min(envelope.len() - 1);
            // Neighbours count half, absorbing tempo drift without shifting the peak.
            let side =
                envelope.get(i.wrapping_sub(1)).unwrap_or(&0.) + envelope.get(i + 1).unwrap_or(&0.);
            score += envelope[i] + side * 0.5;
            at += period;
        }
        if score > best.1 {
            best = (phase, score);
        }
    }
    best.0 as f64 / hz
}

pub fn estimate_bpm(track: &Track) -> Option<f64> {
    let (mut envelope, hz) = onset_envelope(track, 90);
    if envelope.len() < (hz * 4.) as usize {
        return None;
    }
    let mean = envelope.iter().sum::<f64>() / envelope.len() as f64;
    for e in &mut envelope {
        *e = (*e - mean * 0.7).max(0.);
    }
    let energy = envelope.iter().map(|e| e * e).sum::<f64>();
    if energy < 1e-7 {
        return None;
    }
    let min = (hz * 60. / 180.) as usize;
    let max = (hz * 60. / 70.).ceil() as usize;
    let scores: Vec<f64> = (0..=max)
        .map(|lag| {
            if lag < min {
                0.
            } else {
                envelope[lag..]
                    .iter()
                    .zip(&envelope)
                    .map(|(a, b)| a * b)
                    .sum::<f64>()
                    / energy
            }
        })
        .collect();
    let lag = (min..=max).max_by(|&a, &b| scores[a].total_cmp(&scores[b]))?;
    let left = *scores.get(lag - 1).unwrap_or(&scores[lag]);
    let right = *scores.get(lag + 1).unwrap_or(&scores[lag]);
    let den = left - 2. * scores[lag] + right;
    let shift = if den.abs() > 1e-10 {
        (0.5 * (left - right) / den).clamp(-0.5, 0.5)
    } else {
        0.
    };
    Some((60. * hz / (lag as f64 + shift) * 10.).round() / 10.)
}

pub fn demo(deck: usize) -> Arc<Track> {
    let sr = 48000;
    let mut noise = 0x82f63b78u32 ^ (deck as u32 + 1);
    let frames = (0..sr * 16)
        .map(|i| {
            let t = i as f32 / sr as f32;
            let p = t % 0.5;
            let kick = (std::f32::consts::TAU * (48. * p + 2.8 * (1. - (-p * 35.).exp()))).sin()
                * (-p * 16.).exp();
            let bass = (std::f32::consts::TAU * if deck == 0 { 55. } else { 82.407 } * t).sin()
                * (-p * 7.).exp()
                * 0.2;
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            let hat =
                (noise as f32 / u32::MAX as f32 * 2. - 1.) * (-(t % 0.25) * 140.).exp() * 0.10;
            let value = kick * 0.55 + bass + hat;
            [value, value]
        })
        .collect();
    Arc::new(Track::from_frames(
        format!(
            "DEMO / {}",
            if deck == 0 { "Pulse A" } else { "Bassline B" }
        ),
        frames,
        sr,
    ))
}

use crate::media::{ffmpeg_command, Frame};
use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub const BLOCK_SIZE: usize = 512;
#[derive(Clone)]
pub struct Block {
    pub frames: [Frame; BLOCK_SIZE],
    pub len: usize,
}
impl Default for Block {
    fn default() -> Self {
        Self {
            frames: [[0.; 2]; BLOCK_SIZE],
            len: 0,
        }
    }
}
#[derive(Clone)]
pub struct CaptureSink {
    pub sender: Sender<Block>,
    pub overflow: Arc<AtomicBool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Format {
    #[default]
    Wav,
    Mp3,
    Flac,
    Ogg,
    Opus,
    Aiff,
}
impl Format {
    pub const ALL: [Self; 6] = [
        Self::Wav,
        Self::Mp3,
        Self::Flac,
        Self::Ogg,
        Self::Opus,
        Self::Aiff,
    ];
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Mp3 => "mp3",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::Opus => "opus",
            Self::Aiff => "aiff",
        }
    }
}
pub fn start(
    path: PathBuf,
    format: Format,
    sample_rate: u32,
) -> Result<(CaptureSink, Receiver<Result<PathBuf, String>>)> {
    let wav = if format == Format::Wav {
        path.clone()
    } else {
        PathBuf::from(format!("{}.recording.wav", path.display()))
    };
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).context("Nie można zapisać nagrania")?;
    let (tx, rx) = bounded::<Block>(256);
    let (result_tx, result_rx) = bounded(1);
    let overflow = Arc::new(AtomicBool::new(false));
    let fault = overflow.clone();
    std::thread::spawn(move || {
        let work = || -> Result<PathBuf> {
            for block in rx {
                for frame in &block.frames[..block.len] {
                    for &sample in frame {
                        writer.write_sample((sample.clamp(-1., 1.) * 32767.) as i16)?;
                    }
                }
            }
            writer.finalize()?;
            if fault.load(Ordering::Relaxed) {
                bail!(
                    "Dysk nie nadążał; przerwano nagrywanie. Zachowano WAV: {}",
                    wav.display()
                );
            }
            if format != Format::Wav {
                export_wav(&wav, &path, format)?;
                std::fs::remove_file(&wav)?;
            }
            Ok(path)
        };
        let _ = result_tx.send(work().map_err(|e| format!("{e:#}")));
    });
    Ok((
        CaptureSink {
            sender: tx,
            overflow,
        },
        result_rx,
    ))
}

pub fn export_wav(wav: &Path, output: &Path, format: Format) -> Result<()> {
    if format == Format::Wav {
        std::fs::copy(wav, output)?;
        return Ok(());
    }
    let temp = PathBuf::from(format!(
        "{}.encoding.{}",
        output.display(),
        format.extension()
    ));
    let result = if format == Format::Aiff {
        write_aiff(wav, &temp)
    } else {
        let mut command = ffmpeg_command();
        command
            .args(["-v", "error", "-nostdin", "-y", "-i"])
            .arg(wav);
        match format {
            Format::Mp3 => {
                command.args(["-c:a", "libmp3lame", "-b:a", "192k"]);
            }
            Format::Flac => {
                command.args(["-c:a", "flac"]);
            }
            Format::Ogg | Format::Opus => {
                command.args(["-c:a", "libopus", "-b:a", "160k", "-f", "ogg"]);
            }
            _ => unreachable!(),
        }
        match command.arg(&temp).output() {
            Ok(result) if result.status.success() => Ok(()),
            Ok(result) => Err(anyhow::anyhow!(
                "Błąd FFmpeg: {}",
                String::from_utf8_lossy(&result.stderr)
            )),
            Err(error) => Err(error).context("Eksport wymaga FFmpeg w PATH lub obok programu"),
        }
    };
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(error).with_context(|| format!("Zachowano nagranie WAV: {}", wav.display()));
    }
    // On Windows rename does not overwrite an existing destination.
    if output.exists() {
        std::fs::remove_file(output)?;
    }
    std::fs::rename(temp, output)?;
    Ok(())
}
fn write_aiff(wav: &Path, output: &Path) -> Result<()> {
    let mut reader = hound::WavReader::open(wav)?;
    let spec = reader.spec();
    let count = reader.len();
    let bytes = count
        .checked_mul(2)
        .context("Nagranie zbyt duże dla AIFF")?;
    let mut w = BufWriter::new(File::create(output)?);
    w.write_all(b"FORM")?;
    w.write_all(&(46 + bytes).to_be_bytes())?;
    w.write_all(b"AIFFCOMM")?;
    w.write_all(&18u32.to_be_bytes())?;
    w.write_all(&spec.channels.to_be_bytes())?;
    w.write_all(&(count / spec.channels as u32).to_be_bytes())?;
    w.write_all(&16u16.to_be_bytes())?;
    let exponent = (spec.sample_rate as f64).log2().floor() as u16;
    let mantissa = (spec.sample_rate as u64) << (63 - exponent);
    w.write_all(&(16383 + exponent).to_be_bytes())?;
    w.write_all(&mantissa.to_be_bytes())?;
    w.write_all(b"SSND")?;
    w.write_all(&(bytes + 8).to_be_bytes())?;
    w.write_all(&[0; 8])?;
    for sample in reader.samples::<i16>() {
        w.write_all(&sample?.to_be_bytes())?;
    }
    w.flush()?;
    Ok(())
}

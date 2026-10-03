use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::Deserialize;
use thiserror::Error;

use crate::domain::detection::MotionSample;

pub const SAMPLE_RATE: usize = 5;
const FRAME_WIDTH: usize = 160;
const FRAME_HEIGHT: usize = 90;
const FRAME_BYTES: usize = FRAME_WIDTH * FRAME_HEIGHT;
const PIXEL_DELTA: u8 = 20;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("cannot inspect input {path}: {reason}")]
    Probe { path: PathBuf, reason: String },
    #[error("invalid MP4 {path}: {reason}")]
    InvalidInput { path: PathBuf, reason: String },
    #[error("cannot decode input {path}: {reason}")]
    Decode { path: PathBuf, reason: String },
    #[error("cannot create event clip {path}: {reason}")]
    Clip { path: PathBuf, reason: String },
}

#[derive(Debug)]
pub struct VideoInfo {
    pub duration_seconds: f64,
}

#[derive(Deserialize)]
struct ProbeData {
    streams: Vec<ProbeStream>,
    format: ProbeFormat,
}

#[derive(Deserialize)]
struct ProbeStream {
    width: Option<usize>,
    height: Option<usize>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
    format_name: String,
}

pub fn probe(path: &Path) -> Result<VideoInfo, MediaError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height:format=duration,format_name",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| MediaError::Probe {
            path: path.to_path_buf(),
            reason: format!("could not run ffprobe: {error}"),
        })?;
    if !output.status.success() {
        return Err(MediaError::Probe {
            path: path.to_path_buf(),
            reason: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let data: ProbeData =
        serde_json::from_slice(&output.stdout).map_err(|error| MediaError::InvalidInput {
            path: path.to_path_buf(),
            reason: format!("ffprobe returned invalid metadata: {error}"),
        })?;
    if !data.format.format_name.split(',').any(|name| name == "mp4") {
        return Err(MediaError::InvalidInput {
            path: path.to_path_buf(),
            reason: "input is not an MP4 container".to_owned(),
        });
    }
    if !data
        .streams
        .iter()
        .any(|stream| stream.width.unwrap_or(0) > 0 && stream.height.unwrap_or(0) > 0)
    {
        return Err(MediaError::InvalidInput {
            path: path.to_path_buf(),
            reason: "no readable video stream".to_owned(),
        });
    }
    let duration = data
        .format
        .duration
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| MediaError::InvalidInput {
            path: path.to_path_buf(),
            reason: "video duration is missing or invalid".to_owned(),
        })?;
    Ok(VideoInfo {
        duration_seconds: duration,
    })
}

pub fn motion_samples(path: &Path) -> Result<Vec<MotionSample>, MediaError> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-xerror",
            "-nostdin",
            "-i",
        ])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-vf",
            "fps=5,scale=160:90:flags=area,format=gray",
            "-an",
            "-sn",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "gray",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| MediaError::Decode {
            path: path.to_path_buf(),
            reason: format!("could not run ffmpeg: {error}"),
        })?;
    let mut stdout = child.stdout.take().ok_or_else(|| MediaError::Decode {
        path: path.to_path_buf(),
        reason: "ffmpeg stdout is unavailable".to_owned(),
    })?;
    let mut stderr = child.stderr.take().ok_or_else(|| MediaError::Decode {
        path: path.to_path_buf(),
        reason: "ffmpeg stderr is unavailable".to_owned(),
    })?;
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        stderr.read_to_end(&mut buffer).map(|_| buffer)
    });
    let mut previous = [0_u8; FRAME_BYTES];
    let mut current = [0_u8; FRAME_BYTES];
    let mut frame_index = 0_usize;
    let mut samples = Vec::new();

    let read_result: Result<(), String> = (|| {
        loop {
            let mut filled = 0;
            while filled < FRAME_BYTES {
                let count = stdout
                    .read(&mut current[filled..])
                    .map_err(|error| error.to_string())?;
                if count == 0 {
                    if filled > 0 {
                        return Err("ffmpeg produced an incomplete video frame".to_owned());
                    }
                    return Ok(());
                }
                filled += count;
            }

            if frame_index > 0 {
                let changed = previous
                    .iter()
                    .zip(&current)
                    .filter(|(before, after)| before.abs_diff(**after) >= PIXEL_DELTA)
                    .count();
                samples.push(MotionSample {
                    end_seconds: frame_index as f64 / SAMPLE_RATE as f64,
                    change_ratio: changed as f64 / FRAME_BYTES as f64,
                });
            }
            std::mem::swap(&mut previous, &mut current);
            frame_index += 1;
        }
    })();
    if read_result.is_err() {
        let _ = child.kill();
    }
    drop(stdout);
    let status = child.wait().map_err(|error| MediaError::Decode {
        path: path.to_path_buf(),
        reason: format!("could not wait for ffmpeg: {error}"),
    })?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| MediaError::Decode {
            path: path.to_path_buf(),
            reason: "ffmpeg error reader stopped unexpectedly".to_owned(),
        })?
        .map_err(|error| MediaError::Decode {
            path: path.to_path_buf(),
            reason: format!("could not read ffmpeg errors: {error}"),
        })?;
    if let Err(reason) = read_result {
        return Err(MediaError::Decode {
            path: path.to_path_buf(),
            reason,
        });
    }
    if !status.success() {
        let message = String::from_utf8_lossy(&stderr).trim().to_owned();
        return Err(MediaError::Decode {
            path: path.to_path_buf(),
            reason: if message.is_empty() {
                format!("ffmpeg exited with {status}")
            } else {
                message
            },
        });
    }
    if frame_index == 0 {
        return Err(MediaError::InvalidInput {
            path: path.to_path_buf(),
            reason: "video contains no decodable frames".to_owned(),
        });
    }
    Ok(samples)
}

pub fn create_clip(input: &Path, output: &Path, start: f64, end: f64) -> Result<(), MediaError> {
    let result = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-ss",
        ])
        .arg(format!("{start:.3}"))
        .arg("-i")
        .arg(input)
        .arg("-t")
        .arg(format!("{:.3}", end - start))
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-sn",
            "-vf",
            "pad=ceil(iw/2)*2:ceil(ih/2)*2",
            "-pix_fmt",
            "yuv420p",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-movflags",
            "+faststart",
        ])
        .arg(output)
        .output()
        .map_err(|error| MediaError::Clip {
            path: output.to_path_buf(),
            reason: format!("could not run ffmpeg: {error}"),
        })?;
    if !result.status.success() {
        return Err(MediaError::Clip {
            path: output.to_path_buf(),
            reason: String::from_utf8_lossy(&result.stderr).trim().to_owned(),
        });
    }
    Ok(())
}

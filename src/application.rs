use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::Serialize;
use thiserror::Error;

use crate::{
    domain::{
        detection::{ConfigError, DetectionConfig, detect_events},
        highlight::plan_highlight,
    },
    infra::ffmpeg::{self, HIGHLIGHT_FRAME_RATE, MediaError, SAMPLE_RATE},
};

const CLIP_PADDING_SECONDS: f64 = 1.0;
const RESULT_DIRECTORY_NAME: &str = "video-cue-engine-output";

#[derive(Debug)]
pub struct RunOptions {
    pub input: PathBuf,
    pub output: PathBuf,
    pub threshold: f64,
    pub min_duration: f64,
    pub merge_gap: f64,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Media(#[from] MediaError),
    #[error("input is not a readable file: {0}")]
    MissingInput(PathBuf),
    #[error("output directory already contains files: {0}")]
    OutputNotEmpty(PathBuf),
    #[error("I/O error at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("cannot serialize analysis: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Serialize)]
struct Analysis {
    schema_version: u32,
    input: AnalysisInput,
    highlight_path: Option<String>,
    events: Vec<AnalysisEvent>,
}

#[derive(Serialize)]
struct AnalysisInput {
    path: String,
    duration_seconds: f64,
}

#[derive(Serialize)]
struct AnalysisEvent {
    start_seconds: f64,
    end_seconds: f64,
    peak_change_ratio: f64,
    clip_path: String,
    highlight_start_seconds: f64,
}

pub fn run(options: RunOptions) -> Result<(), RunError> {
    let config = DetectionConfig::new(options.threshold, options.min_duration, options.merge_gap)?;
    if !options.input.is_file() {
        return Err(RunError::MissingInput(options.input));
    }
    let input = options
        .input
        .canonicalize()
        .map_err(|source| RunError::Io {
            path: options.input.clone(),
            source,
        })?;
    let video = ffmpeg::probe(&input)?;
    let samples = ffmpeg::motion_samples(&input)?;
    let events = detect_events(
        &samples,
        1.0 / SAMPLE_RATE as f64,
        video.duration_seconds,
        config,
    );
    let highlight = plan_highlight(
        &events,
        video.duration_seconds,
        CLIP_PADDING_SECONDS,
        HIGHLIGHT_FRAME_RATE,
    );

    let output = options.output.join(RESULT_DIRECTORY_NAME);
    prepare_output(&output)?;
    let mut analysis_events = Vec::with_capacity(events.len());
    if !events.is_empty() {
        ffmpeg::create_highlight(&input, &output.join("highlights.mp4"), &highlight.segments)?;
        create_directory(&output.join("events"))?;
    }
    for (index, (event, highlight_start_seconds)) in events
        .iter()
        .zip(&highlight.event_start_seconds)
        .enumerate()
    {
        let clip_path = format!("events/event-{:03}.mp4", index + 1);
        let clip_start = (event.start_seconds - CLIP_PADDING_SECONDS).max(0.0);
        let clip_end = (event.end_seconds + CLIP_PADDING_SECONDS).min(video.duration_seconds);
        ffmpeg::create_clip(&input, &output.join(&clip_path), clip_start, clip_end)?;
        analysis_events.push(AnalysisEvent {
            start_seconds: event.start_seconds,
            end_seconds: event.end_seconds,
            peak_change_ratio: event.peak_change_ratio,
            clip_path,
            highlight_start_seconds: *highlight_start_seconds,
        });
    }

    let analysis = Analysis {
        schema_version: 2,
        input: AnalysisInput {
            path: portable_input_path(&input),
            duration_seconds: video.duration_seconds,
        },
        highlight_path: (!events.is_empty()).then(|| "highlights.mp4".to_owned()),
        events: analysis_events,
    };
    let mut json = serde_json::to_vec_pretty(&analysis)?;
    json.push(b'\n');
    let analysis_path = output.join("analysis.json");
    fs::write(&analysis_path, json).map_err(|source| RunError::Io {
        path: analysis_path,
        source,
    })?;
    Ok(())
}

fn prepare_output(path: &Path) -> Result<(), RunError> {
    if path.exists() {
        let mut entries = fs::read_dir(path).map_err(|source| RunError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if let Some(entry) = entries.next() {
            entry.map_err(|source| RunError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            return Err(RunError::OutputNotEmpty(path.to_path_buf()));
        }
        Ok(())
    } else {
        create_directory(path)
    }
}

fn create_directory(path: &Path) -> Result<(), RunError> {
    fs::create_dir_all(path).map_err(|source| RunError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn portable_input_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        if let Some(local) = path.strip_prefix(r"\\?\") {
            return local.to_owned();
        }
    }
    path.into_owned()
}

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use thiserror::Error;

use super::{RunError, RunOptions, run_to_output};
use crate::infra::{ffmpeg, folders};

#[derive(Debug, Error)]
pub enum BatchError {
    #[error("入力フォルダを読み取れません: {path}。存在とアクセス権を確認してください。({source})")]
    Input { path: PathBuf, source: io::Error },
    #[error("フォルダ直下に MP4 がありません: {0}。MP4 のあるフォルダを選んでください。")]
    NoVideos(PathBuf),
    #[error(
        "出力フォルダを使用できません: {path}。読み書きできる既存のフォルダを選んでください。({source})"
    )]
    Output { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Media(#[from] ffmpeg::MediaError),
}

/// A snapshot shown to the user before starting; files added later require a rescan.
#[derive(Debug)]
pub struct BatchPlan {
    files: Vec<PathBuf>,
}

impl BatchPlan {
    pub fn scan(input: &Path) -> Result<Self, BatchError> {
        let input = input.canonicalize().map_err(|source| BatchError::Input {
            path: input.to_owned(),
            source,
        })?;
        let files = folders::find_mp4s(&input).map_err(|source| BatchError::Input {
            path: input.clone(),
            source,
        })?;
        if files.is_empty() {
            return Err(BatchError::NoVideos(input));
        }
        Ok(Self { files })
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
}

#[derive(Debug)]
pub enum BatchEvent {
    Prepared {
        output: PathBuf,
    },
    Started {
        index: usize,
        input: PathBuf,
        output: PathBuf,
    },
    Finished {
        index: usize,
        result: Result<(), RunError>,
    },
}

#[derive(Debug)]
pub struct BatchSummary {
    pub output: PathBuf,
    pub succeeded: usize,
    pub failed: usize,
}

/// Synchronous, UI-independent batch use case. Call on a worker thread for interactive use.
pub fn run_batch(
    plan: &BatchPlan,
    output: &Path,
    mut notify: impl FnMut(BatchEvent),
) -> Result<BatchSummary, BatchError> {
    folders::check_output(output).map_err(|source| BatchError::Output {
        path: output.to_owned(),
        source,
    })?;
    ffmpeg::check_available()?;
    let directory =
        folders::create_batch_directory(output).map_err(|source| BatchError::Output {
            path: output.to_owned(),
            source,
        })?;
    notify(BatchEvent::Prepared {
        output: directory.clone(),
    });
    let mut summary = BatchSummary {
        output: directory,
        succeeded: 0,
        failed: 0,
    };
    for (index, input) in plan.files.iter().enumerate() {
        // Numeric names are bounded and cannot collide on case-insensitive output filesystems.
        let destination = summary.output.join(format!("{:04}", index + 1));
        notify(BatchEvent::Started {
            index,
            input: input.clone(),
            output: destination.clone(),
        });
        let result = fs::create_dir(&destination)
            .map_err(|source| RunError::Io {
                path: destination.clone(),
                source,
            })
            .and_then(|()| {
                run_to_output(
                    RunOptions {
                        input: input.clone(),
                        output: destination.clone(),
                        threshold: 0.005,
                        min_duration: 0.4,
                        merge_gap: 0.6,
                    },
                    &destination,
                )
            });
        if result.is_ok() {
            summary.succeeded += 1;
        } else {
            summary.failed += 1;
        }
        notify(BatchEvent::Finished { index, result });
    }
    Ok(summary)
}

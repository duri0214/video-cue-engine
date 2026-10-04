mod application;
pub mod domain;
mod infra;

pub use application::batch::{
    BatchError, BatchEvent, BatchPlan, BatchSummary, run_batch, run_batch_with_upload,
};
pub use application::{RunError, RunOptions, run};
#[cfg(feature = "gui")]
pub use infra::desktop::{downloads_directory, open_directory};
pub use infra::ffmpeg::MediaError;
pub use infra::upload::{UploadConfig, UploadError, UploadReceipt, upload_result};

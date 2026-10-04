use std::path::{Path, PathBuf};

use clap::Parser;
use video_cue_engine::{RunOptions, UploadConfig, run, upload_result};

#[derive(Parser)]
#[command(
    about = "Detect motion in a fixed-camera MP4 and write a highlight video and event clips"
)]
struct Cli {
    /// Input MP4 file
    #[arg(
        long,
        required_unless_present = "upload_existing",
        conflicts_with = "upload_existing"
    )]
    input: Option<PathBuf>,

    /// Parent directory; video-cue-engine-output is created inside it
    #[arg(
        long,
        required_unless_present = "upload_existing",
        conflicts_with = "upload_existing"
    )]
    output: Option<PathBuf>,

    /// Upload the completed result after local analysis (requires upload environment variables)
    #[arg(long, conflicts_with = "upload_existing")]
    upload: bool,

    /// Retry an upload from an existing result directory without analysing again
    #[arg(long)]
    upload_existing: Option<PathBuf>,

    /// Minimum fraction of changed pixels in a sampled frame (0 < value <= 1)
    #[arg(long, default_value_t = 0.005)]
    threshold: f64,

    /// Minimum duration of a motion event in seconds
    #[arg(long, default_value_t = 0.4)]
    min_duration: f64,

    /// Maximum quiet gap to merge between motion intervals in seconds
    #[arg(long, default_value_t = 0.6)]
    merge_gap: f64,
}

fn main() {
    let cli = Cli::parse();
    if let Some(directory) = cli.upload_existing {
        send_result(&upload_config_or_exit(), &directory);
        return;
    }
    let upload = cli.upload.then(upload_config_or_exit);
    let (Some(input), Some(output)) = (cli.input, cli.output) else {
        eprintln!("error: --input and --output are required");
        std::process::exit(2);
    };
    let options = RunOptions {
        input,
        output: output.clone(),
        threshold: cli.threshold,
        min_duration: cli.min_duration,
        merge_gap: cli.merge_gap,
    };
    if let Err(error) = run(options) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
    if let Some(config) = upload.as_ref() {
        send_result(config, &output.join("video-cue-engine-output"));
    }
}

fn upload_config_or_exit() -> UploadConfig {
    match UploadConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}

fn send_result(config: &UploadConfig, directory: &Path) {
    match upload_result(config, directory) {
        Ok(receipt) => {
            let action = if receipt.created {
                "登録"
            } else {
                "再送を確認"
            };
            println!("送信成功: {action}（動画 ID: {}）", receipt.key);
        }
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}

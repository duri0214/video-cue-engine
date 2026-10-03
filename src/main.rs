use std::path::PathBuf;

use clap::Parser;
use video_cue_engine::{RunOptions, run};

#[derive(Parser)]
#[command(
    about = "Detect motion in a fixed-camera MP4 and write a highlight video and event clips"
)]
struct Cli {
    /// Input MP4 file
    #[arg(long)]
    input: PathBuf,

    /// Parent directory; video-cue-engine-output is created inside it
    #[arg(long)]
    output: PathBuf,

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
    let options = RunOptions {
        input: cli.input,
        output: cli.output,
        threshold: cli.threshold,
        min_duration: cli.min_duration,
        merge_gap: cli.merge_gap,
    };
    if let Err(error) = run(options) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

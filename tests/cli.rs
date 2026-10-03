use std::{fs, path::Path, process::Command};

use serde_json::Value;
use tempfile::tempdir;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn run_cli(input: &Path, output: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_video-cue-engine"))
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(output)
        .output()
        .unwrap()
}

#[test]
fn cli_detects_known_motion_and_creates_playable_videos() {
    let temp = tempdir().unwrap();
    let output_dir = temp.path().join("result");
    let result = run_cli(&fixture("scene-motion.mp4"), &output_dir);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let analysis: Value =
        serde_json::from_slice(&fs::read(output_dir.join("analysis.json")).unwrap()).unwrap();
    let truth: Value =
        serde_json::from_slice(&fs::read(fixture("ground-truth.json")).unwrap()).unwrap();
    assert_eq!(analysis["schema_version"], 2);
    assert_eq!(analysis["input"]["duration_seconds"], 14.0);
    assert!(Path::new(analysis["input"]["path"].as_str().unwrap()).is_file());
    assert_eq!(analysis["highlight_path"], "highlights.mp4");
    let highlight = output_dir.join(analysis["highlight_path"].as_str().unwrap());
    assert!(highlight.is_file());
    let highlight_probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=nb_frames,width,height",
            "-of",
            "json",
        ])
        .arg(&highlight)
        .output()
        .unwrap();
    assert!(highlight_probe.status.success(), "invalid highlight");
    let highlight_info: Value = serde_json::from_slice(&highlight_probe.stdout).unwrap();
    let highlight_duration: f64 = highlight_info["format"]["duration"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((10.8..11.6).contains(&highlight_duration));
    assert!(
        highlight_info["streams"][0]["nb_frames"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 150
    );
    let events = analysis["events"].as_array().unwrap();
    let expected = truth["events"].as_array().unwrap();
    assert_eq!(events.len(), expected.len());
    for (event, target_position) in events.iter().zip([1.0, 5.0, 8.6]) {
        let position = event["highlight_start_seconds"].as_f64().unwrap();
        assert!(
            (position - target_position).abs() < 0.3,
            "unexpected highlight position: {position}"
        );
    }

    for (event, expected) in events.iter().zip(expected) {
        for field in ["start_seconds", "end_seconds"] {
            let actual = event[field].as_f64().unwrap();
            let target = expected[field].as_f64().unwrap();
            assert!(
                (actual - target).abs() <= 0.6,
                "{}: {field} was {actual}, expected {target}",
                expected["name"]
            );
        }
        assert!(event["peak_change_ratio"].as_f64().unwrap() > 0.005);
        let clip = output_dir.join(event["clip_path"].as_str().unwrap());
        assert!(clip.is_file(), "missing clip {}", clip.display());
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "json",
            ])
            .arg(&clip)
            .output()
            .unwrap();
        assert!(probe.status.success(), "invalid clip {}", clip.display());
        let clip_info: Value = serde_json::from_slice(&probe.stdout).unwrap();
        let clip_duration: f64 = clip_info["format"]["duration"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let expected_duration = (event["end_seconds"].as_f64().unwrap() + 1.0).min(14.0)
            - (event["start_seconds"].as_f64().unwrap() - 1.0).max(0.0);
        assert!(
            (clip_duration - expected_duration).abs() <= 0.25,
            "clip {} lasted {clip_duration}s, expected {expected_duration}s",
            clip.display()
        );
    }
}

#[test]
fn cli_writes_no_events_for_a_still_video() {
    let temp = tempdir().unwrap();
    let output_dir = temp.path().join("result");
    let result = run_cli(&fixture("scene-still.mp4"), &output_dir);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let analysis: Value =
        serde_json::from_slice(&fs::read(output_dir.join("analysis.json")).unwrap()).unwrap();
    assert!(analysis["events"].as_array().unwrap().is_empty());
    assert!(analysis["highlight_path"].is_null());
    assert!(!output_dir.join("highlights.mp4").exists());
}

#[test]
fn cli_explains_a_broken_mp4_and_does_not_create_an_analysis() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("broken.mp4");
    fs::write(&input, b"not a valid MP4").unwrap();
    let output_dir = temp.path().join("result");

    let result = run_cli(&input, &output_dir);

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot inspect input"));
    assert!(!output_dir.join("analysis.json").exists());
}

#[test]
fn cli_rejects_an_mp4_truncated_after_its_header() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("truncated.mp4");
    let fixture_bytes = fs::read(fixture("scene-motion.mp4")).unwrap();
    fs::write(&input, &fixture_bytes[..fixture_bytes.len() / 2]).unwrap();
    let output_dir = temp.path().join("result");

    let result = run_cli(&input, &output_dir);

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot decode input"));
    assert!(!output_dir.join("analysis.json").exists());
}

#[test]
fn cli_reports_a_missing_input_file() {
    let temp = tempdir().unwrap();
    let output_dir = temp.path().join("result");

    let result = run_cli(&temp.path().join("missing.mp4"), &output_dir);

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("input is not a readable file"));
    assert!(!output_dir.exists());
}

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::Value;
use tempfile::tempdir;
use video_cue_engine::{BatchError, BatchEvent, BatchPlan, MediaError, run_batch};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

#[test]
fn scan_only_includes_immediate_regular_mp4s_case_insensitively() {
    let input = tempdir().unwrap();
    fs::write(input.path().join("b.MP4"), b"preview only").unwrap();
    fs::write(input.path().join("a.mp4"), b"preview only").unwrap();
    fs::write(input.path().join("notes.txt"), b"ignore").unwrap();
    fs::create_dir(input.path().join("directory.mp4")).unwrap();
    fs::write(input.path().join("directory.mp4/nested.mp4"), b"ignore").unwrap();

    let plan = BatchPlan::scan(input.path()).unwrap();

    let names: Vec<_> = plan
        .files()
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, ["a.mp4", "b.MP4"]);
}

#[test]
fn scan_explains_empty_and_missing_input_folders() {
    let input = tempdir().unwrap();
    assert!(matches!(
        BatchPlan::scan(input.path()),
        Err(BatchError::NoVideos(_))
    ));
    assert!(matches!(
        BatchPlan::scan(&input.path().join("missing")),
        Err(BatchError::Input { .. })
    ));
    fs::write(input.path().join("file"), b"not a directory").unwrap();
    assert!(matches!(
        BatchPlan::scan(&input.path().join("file")),
        Err(BatchError::Input { .. })
    ));
}

#[test]
fn invalid_output_is_rejected_before_any_video_starts_and_is_never_created() {
    let temp = tempdir().unwrap();
    let plan = BatchPlan::scan(fixture("scene-still.mp4").parent().unwrap()).unwrap();
    let missing = temp.path().join("missing-downloads");
    let mut notifications = Vec::new();

    let result = run_batch(&plan, &missing, |event| notifications.push(event));

    assert!(matches!(result, Err(BatchError::Output { .. })));
    assert!(notifications.is_empty());
    assert!(!missing.exists());
    let file = temp.path().join("not-a-directory");
    fs::write(&file, b"keep me").unwrap();
    assert!(matches!(
        run_batch(&plan, &file, |_| panic!("must not start")),
        Err(BatchError::Output { .. })
    ));
    assert_eq!(fs::read(file).unwrap(), b"keep me");
}

#[test]
fn batch_continues_after_failure_and_preserves_results_on_repeated_runs() {
    let input = tempdir().unwrap();
    let output = tempdir().unwrap();
    fs::write(input.path().join("00-broken.mp4"), b"broken").unwrap();
    fs::copy(
        fixture("scene-motion.mp4"),
        input.path().join("01-動き.MP4"),
    )
    .unwrap();
    fs::copy(
        fixture("scene-still.mp4"),
        input.path().join("02-still.mp4"),
    )
    .unwrap();
    fs::write(output.path().join("existing.txt"), b"keep me").unwrap();
    let plan = BatchPlan::scan(input.path()).unwrap();
    // The files confirmed by the user define the batch, even if another file arrives.
    fs::copy(
        fixture("scene-still.mp4"),
        input.path().join("03-later.mp4"),
    )
    .unwrap();
    let mut notifications = Vec::new();

    let first = run_batch(&plan, output.path(), |event| notifications.push(event)).unwrap();

    assert_eq!((first.succeeded, first.failed), (2, 1));
    assert_eq!(notifications.len(), 7);
    assert!(
        matches!(&notifications[0], BatchEvent::Prepared { output } if *output == first.output)
    );
    for index in 0..3 {
        assert!(
            matches!(&notifications[index * 2 + 1], BatchEvent::Started { index: actual, .. } if *actual == index)
        );
        assert!(
            matches!(&notifications[index * 2 + 2], BatchEvent::Finished { index: actual, result } if *actual == index && result.is_ok() == (index != 0))
        );
    }
    assert!(!first.output.join("0001/analysis.json").exists());
    let analysis_path = first.output.join("0002/analysis.json");
    let original_json = fs::read(&analysis_path).unwrap();
    let analysis: Value = serde_json::from_slice(&original_json).unwrap();
    assert!(
        analysis["input"]["path"]
            .as_str()
            .unwrap()
            .ends_with("01-動き.MP4")
    );
    assert!(first.output.join("0002/highlights.mp4").is_file());
    assert!(first.output.join("0002/events/event-001.mp4").is_file());
    let still: Value =
        serde_json::from_slice(&fs::read(first.output.join("0003/analysis.json")).unwrap())
            .unwrap();
    assert!(still["events"].as_array().unwrap().is_empty());
    assert!(still["highlight_path"].is_null());
    assert!(!first.output.join("0004").exists());

    let second = run_batch(&plan, output.path(), |_| {}).unwrap();

    assert_ne!(first.output, second.output);
    assert_eq!((second.succeeded, second.failed), (2, 1));
    assert_eq!(fs::read(analysis_path).unwrap(), original_json);
    assert_eq!(
        fs::read(output.path().join("existing.txt")).unwrap(),
        b"keep me"
    );
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 3);
}

#[test]
fn missing_ffmpeg_is_reported_before_creating_results() {
    const CHILD: &str = "VIDEO_CUE_TEST_WITHOUT_FFMPEG";
    if std::env::var_os(CHILD).is_some() {
        let output = tempdir().unwrap();
        let plan = BatchPlan::scan(fixture("scene-still.mp4").parent().unwrap()).unwrap();
        let result = run_batch(&plan, output.path(), |_| panic!("must not start"));
        assert!(
            matches!(&result, Err(BatchError::Media(MediaError::Unavailable { tool, .. })) if tool == "ffmpeg")
        );
        let message = result.unwrap_err().to_string();
        assert!(message.contains("ffmpeg") && message.contains("PATH"));
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
        return;
    }
    // A child process avoids mutating global PATH while other tests invoke FFmpeg.
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "missing_ffmpeg_is_reported_before_creating_results",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn a_video_removed_after_preview_fails_without_stopping_later_videos() {
    let input = tempdir().unwrap();
    let output = tempdir().unwrap();
    let removed = input.path().join("a.mp4");
    fs::copy(fixture("scene-still.mp4"), &removed).unwrap();
    fs::copy(fixture("scene-still.mp4"), input.path().join("b.mp4")).unwrap();
    let plan = BatchPlan::scan(input.path()).unwrap();
    fs::remove_file(removed).unwrap();

    let summary = run_batch(&plan, output.path(), |_| {}).unwrap();

    assert_eq!((summary.succeeded, summary.failed), (1, 1));
    assert!(!summary.output.join("0001/analysis.json").exists());
    assert!(summary.output.join("0002/analysis.json").is_file());
}

#[cfg(windows)]
#[test]
fn denied_input_read_and_output_write_permissions_are_reported_before_start() {
    let input = tempdir().unwrap();
    let output = tempdir().unwrap();
    fs::copy(fixture("scene-still.mp4"), input.path().join("still.mp4")).unwrap();
    let plan = BatchPlan::scan(input.path()).unwrap();

    let read_guard = DeniedAccess::new(input.path(), "ReadAndExecute");
    let scan_result = BatchPlan::scan(input.path());
    drop(read_guard);
    assert!(matches!(scan_result, Err(BatchError::Input { .. })));

    let write_guard = DeniedAccess::new(output.path(), "Write");
    let mut events = Vec::new();
    let run_result = run_batch(&plan, output.path(), |event| events.push(event));
    drop(write_guard);
    assert!(matches!(run_result, Err(BatchError::Output { .. })));
    assert!(events.is_empty());
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
}

#[cfg(windows)]
struct DeniedAccess {
    path: PathBuf,
    rights: &'static str,
}

#[cfg(windows)]
impl DeniedAccess {
    fn new(path: &Path, rights: &'static str) -> Self {
        let guard = Self {
            path: path.to_owned(),
            rights,
        };
        guard.change("add");
        guard
    }

    fn change(&self, action: &str) {
        // Only the test's fresh temporary directory is modified. Drop removes this exact rule.
        let result = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", r#"
$ErrorActionPreference = 'Stop'
$acl = Get-Acl -LiteralPath $env:VIDEO_CUE_ACL_PATH
$sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$rule = New-Object System.Security.AccessControl.FileSystemAccessRule($sid, $env:VIDEO_CUE_ACL_RIGHTS, 'Deny')
if ($env:VIDEO_CUE_ACL_ACTION -eq 'add') { $acl.AddAccessRule($rule) } else { $acl.RemoveAccessRuleSpecific($rule) }
Set-Acl -LiteralPath $env:VIDEO_CUE_ACL_PATH -AclObject $acl
"#])
            .env("VIDEO_CUE_ACL_PATH", &self.path)
            .env("VIDEO_CUE_ACL_RIGHTS", self.rights)
            .env("VIDEO_CUE_ACL_ACTION", action)
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[cfg(windows)]
impl Drop for DeniedAccess {
    fn drop(&mut self) {
        self.change("remove");
    }
}

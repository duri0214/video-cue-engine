use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use serde_json::json;
use tempfile::tempdir;
use video_cue_engine::{
    BatchEvent, BatchPlan, UploadConfig, UploadError, run_batch_with_upload, upload_result,
};

struct Request {
    path: String,
    authorization: String,
    body: Vec<u8>,
}

fn serve(statuses: Vec<u16>) -> (String, thread::JoinHandle<Vec<Request>>) {
    serve_responses(statuses.into_iter().map(|status| (status, None)).collect())
}

fn serve_responses(
    responses: Vec<(u16, Option<String>)>,
) -> (String, thread::JoinHandle<Vec<Request>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/video_cue/api/results/",
        listener.local_addr().unwrap()
    );
    let worker = thread::spawn(move || {
        responses
            .into_iter()
            .map(|(status, response_body)| {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
                let request = read_request(&mut stream);
                let key = request.path.trim_end_matches('/').rsplit('/').next().unwrap();
                let created = status == 201;
                let body = response_body
                    .unwrap_or_else(|| json!({"key": key, "created": created}).to_string());
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                request
            })
            .collect()
    });
    (url, worker)
}

fn read_request(stream: &mut TcpStream) -> Request {
    let mut data = Vec::new();
    let mut buffer = [0; 8192];
    let header_end = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "request closed before headers");
        data.extend_from_slice(&buffer[..count]);
        if let Some(position) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = String::from_utf8_lossy(&data[..header_end]);
    let mut lines = headers.lines();
    let path = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    let mut authorization = String::new();
    let mut content_length = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("authorization") {
            authorization = value.trim().to_owned();
        }
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let content_length = content_length.expect("multipart file request has a known length");
    while data.len() - header_end < content_length {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "request closed before body");
        data.extend_from_slice(&buffer[..count]);
    }
    Request {
        path,
        authorization,
        body: data[header_end..header_end + content_length].to_vec(),
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn result(directory: &Path, events: bool) {
    let analysis = json!({
        "schema_version": 2,
        "input": {"path": "private/source.mp4", "duration_seconds": 14.0},
        "highlight_path": if events { Some("highlights.mp4") } else { None },
        "events": if events {
            vec![json!({
                "start_seconds": 2.0,
                "end_seconds": 4.0,
                "peak_change_ratio": 0.2,
                "clip_path": "events/event-001.mp4",
                "highlight_start_seconds": 1.0
            })]
        } else {
            Vec::new()
        }
    });
    fs::write(directory.join("analysis.json"), analysis.to_string()).unwrap();
    if events {
        fs::write(directory.join("highlights.mp4"), b"\0\0\0\x18ftypisomvideo").unwrap();
        fs::create_dir(directory.join("events")).unwrap();
        fs::write(
            directory.join("events/event-001.mp4"),
            b"PRIVATE_CLIP_BYTES",
        )
        .unwrap();
    }
}

#[test]
fn completed_result_retries_with_same_key_and_only_required_files() {
    let output = tempdir().unwrap();
    result(output.path(), true);
    let (url, worker) = serve(vec![201, 200]);
    let config = UploadConfig::new(&url, "test-secret").unwrap();

    let created = upload_result(&config, output.path()).unwrap();
    let replay = upload_result(&config, output.path()).unwrap();

    assert!(created.created);
    assert!(!replay.created);
    assert_eq!(created.key, replay.key);
    let requests = worker.join().unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert_eq!(request.authorization, "Bearer test-secret");
        assert_eq!(
            request.path,
            format!("/video_cue/api/results/{}/", created.key)
        );
        assert!(
            request
                .body
                .windows(b"name=\"analysis\"".len())
                .any(|part| part == b"name=\"analysis\"")
        );
        assert!(
            request
                .body
                .windows(b"name=\"highlight\"".len())
                .any(|part| part == b"name=\"highlight\"")
        );
        assert!(request.body.windows(8).any(|part| part == b"ftypisom"));
        assert!(
            !request
                .body
                .windows(18)
                .any(|part| part == b"PRIVATE_CLIP_BYTES")
        );
    }
}

#[test]
fn zero_events_uploads_json_without_highlight() {
    let output = tempdir().unwrap();
    result(output.path(), false);
    let (url, worker) = serve(vec![201]);
    let config = UploadConfig::new(&url, "test-secret").unwrap();

    assert!(upload_result(&config, output.path()).unwrap().created);

    let request = worker.join().unwrap().remove(0);
    let body = String::from_utf8_lossy(&request.body);
    assert!(body.contains("name=\"analysis\""));
    assert!(!body.contains("name=\"highlight\""));
    assert!(body.contains("\"events\":[]"));
}

#[test]
fn failed_upload_keeps_results_and_reports_each_video() {
    let output = tempdir().unwrap();
    let plan = BatchPlan::scan(fixture("scene-motion.mp4").parent().unwrap()).unwrap();
    let (url, worker) = serve(vec![201, 409]);
    let config = UploadConfig::new(&url, "test-secret").unwrap();
    let mut events = Vec::new();

    let summary = run_batch_with_upload(&plan, output.path(), Some(&config), |event| {
        events.push(event)
    })
    .unwrap();

    assert_eq!((summary.succeeded, summary.failed), (2, 0));
    assert_eq!((summary.uploaded, summary.upload_failed), (1, 1));
    assert!(summary.output.join("0001/analysis.json").is_file());
    assert!(summary.output.join("0002/analysis.json").is_file());
    assert!(events.iter().any(|event| matches!(
        event,
        BatchEvent::UploadFinished {
            result: Err(UploadError::Conflict),
            ..
        }
    )));
    assert_eq!(worker.join().unwrap().len(), 2);
}

#[test]
fn server_json_error_is_reported_without_exposing_token() {
    let output = tempdir().unwrap();
    result(output.path(), true);
    let (url, worker) = serve_responses(vec![(
        500,
        Some(
            json!({
                "error": "保存に失敗しました。private-token\r\n"
            })
            .to_string(),
        ),
    )]);
    let config = UploadConfig::new(&url, "private-token").unwrap();

    let error = upload_result(&config, output.path()).unwrap_err();
    let message = error.to_string();

    assert!(matches!(
        error,
        UploadError::HttpMessage { status: 500, .. }
    ));
    assert!(message.contains("保存に失敗しました。"));
    assert!(message.contains("[REDACTED]"));
    assert!(!message.contains("private-token"));
    assert!(!message.contains('\n'));
    assert_eq!(worker.join().unwrap().len(), 1);
}

#[test]
fn unavailable_destination_leaves_local_result_for_retry_without_exposing_token() {
    let output = tempdir().unwrap();
    result(output.path(), true);
    let analysis = fs::read(output.path().join("analysis.json")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/video_cue/api/results/",
        listener.local_addr().unwrap()
    );
    drop(listener);
    let config = UploadConfig::new(&url, "private-token").unwrap();

    let error = upload_result(&config, output.path()).unwrap_err();

    assert!(matches!(error, UploadError::Network | UploadError::Timeout));
    assert!(!error.to_string().contains("private-token"));
    assert_eq!(
        fs::read(output.path().join("analysis.json")).unwrap(),
        analysis
    );
    assert!(output.path().join("highlights.mp4").is_file());
}

#[test]
fn cli_upload_existing_sends_saved_result_without_reanalysis() {
    let output = tempdir().unwrap();
    result(output.path(), true);
    let before = fs::read(output.path().join("analysis.json")).unwrap();
    let (url, worker) = serve(vec![201]);

    let command = Command::new(env!("CARGO_BIN_EXE_video-cue-engine"))
        .arg("--upload-existing")
        .arg(output.path())
        .env("VIDEO_CUE_UPLOAD_URL", url)
        .env("VIDEO_CUE_UPLOAD_TOKEN", "test-secret")
        .output()
        .unwrap();

    assert!(
        command.status.success(),
        "{}",
        String::from_utf8_lossy(&command.stderr)
    );
    assert!(String::from_utf8_lossy(&command.stdout).contains("送信成功"));
    assert_eq!(
        fs::read(output.path().join("analysis.json")).unwrap(),
        before
    );
    assert_eq!(worker.join().unwrap().len(), 1);
}

#[test]
fn cli_upload_flag_sends_only_after_successful_analysis() {
    let output = tempdir().unwrap();
    let (url, worker) = serve(vec![201]);

    let command = Command::new(env!("CARGO_BIN_EXE_video-cue-engine"))
        .arg("--input")
        .arg(fixture("scene-motion.mp4"))
        .arg("--output")
        .arg(output.path())
        .arg("--upload")
        .env("VIDEO_CUE_UPLOAD_URL", url)
        .env("VIDEO_CUE_UPLOAD_TOKEN", "test-secret")
        .output()
        .unwrap();

    assert!(
        command.status.success(),
        "{}",
        String::from_utf8_lossy(&command.stderr)
    );
    assert!(
        output
            .path()
            .join("video-cue-engine-output/analysis.json")
            .is_file()
    );
    assert!(
        output
            .path()
            .join("video-cue-engine-output/highlights.mp4")
            .is_file()
    );
    assert_eq!(worker.join().unwrap().len(), 1);
}

#[test]
fn cli_without_upload_flag_never_contacts_configured_destination() {
    let output = tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/video_cue/api/results/",
        listener.local_addr().unwrap()
    );
    listener.set_nonblocking(true).unwrap();

    let command = Command::new(env!("CARGO_BIN_EXE_video-cue-engine"))
        .arg("--input")
        .arg(fixture("scene-still.mp4"))
        .arg("--output")
        .arg(output.path())
        .env("VIDEO_CUE_UPLOAD_URL", url)
        .env("VIDEO_CUE_UPLOAD_TOKEN", "test-secret")
        .output()
        .unwrap();

    assert!(
        command.status.success(),
        "{}",
        String::from_utf8_lossy(&command.stderr)
    );
    assert!(
        output
            .path()
            .join("video-cue-engine-output/analysis.json")
            .is_file()
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn configuration_rejects_credentials_in_url_and_plain_http_to_remote_host() {
    assert!(matches!(
        UploadConfig::new("http://example.com/video_cue/api/results/", "token"),
        Err(UploadError::InvalidUrl)
    ));
    assert!(matches!(
        UploadConfig::new(
            "https://user:secret@example.com/video_cue/api/results/",
            "token"
        ),
        Err(UploadError::InvalidUrl)
    ));
    assert!(matches!(
        UploadConfig::new("https://example.com/video_cue/api/results/", "bad token"),
        Err(UploadError::InvalidToken)
    ));
}

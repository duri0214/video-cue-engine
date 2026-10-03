use std::{
    env, fs,
    io::{self, Read},
    path::Path,
    time::Duration,
};

use reqwest::{
    Url,
    blocking::{Client, multipart},
    redirect::Policy,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_ANALYSIS_BYTES: u64 = 8 * 1024 * 1024;
const MAX_HIGHLIGHT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone)]
pub struct UploadConfig {
    endpoint: Url,
    token: String,
    client: Client,
}

impl UploadConfig {
    pub fn from_env() -> Result<Self, UploadError> {
        let endpoint = env::var("VIDEO_CUE_UPLOAD_URL").map_err(|_| UploadError::MissingUrl)?;
        let token = env::var("VIDEO_CUE_UPLOAD_TOKEN").map_err(|_| UploadError::MissingToken)?;
        Self::new(&endpoint, &token)
    }

    pub fn new(endpoint: &str, token: &str) -> Result<Self, UploadError> {
        let endpoint = Url::parse(endpoint).map_err(|_| UploadError::InvalidUrl)?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || endpoint.host().is_none()
            || (endpoint.scheme() == "http"
                && !matches!(
                    endpoint.host_str(),
                    Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
                ))
            || !endpoint.path().ends_with("/video_cue/api/results/")
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(UploadError::InvalidUrl);
        }
        if token.is_empty()
            || !token.is_ascii()
            || token.bytes().any(|byte| byte <= b' ' || byte == 127)
        {
            return Err(UploadError::InvalidToken);
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .build()
            .map_err(|_| UploadError::Client)?;
        Ok(Self {
            endpoint,
            token: token.to_owned(),
            client,
        })
    }
}

#[derive(Debug, Error)]
pub enum UploadError {
    #[error("VIDEO_CUE_UPLOAD_URL を設定してください。")]
    MissingUrl,
    #[error("VIDEO_CUE_UPLOAD_TOKEN を設定してください。")]
    MissingToken,
    #[error(
        "VIDEO_CUE_UPLOAD_URL は /video_cue/api/results/ で終わる HTTPS URL（ローカルでは HTTP も可）にしてください。"
    )]
    InvalidUrl,
    #[error("VIDEO_CUE_UPLOAD_TOKEN の形式が不正です。")]
    InvalidToken,
    #[error("送信クライアントを準備できませんでした。")]
    Client,
    #[error("analysis.json がありません。完成した結果フォルダを指定してください。")]
    MissingAnalysis,
    #[error("analysis.json が8 MiBの上限を超えています。")]
    AnalysisTooLarge,
    #[error("analysis.json の形式が送信契約と一致しません。")]
    InvalidAnalysis,
    #[error("イベントのある結果に highlights.mp4 がありません。")]
    MissingHighlight,
    #[error("highlights.mp4 が128 MiBの上限を超えています。")]
    HighlightTooLarge,
    #[error("ローカルの成果物を読み取れませんでした。")]
    LocalRead,
    #[error("送信がタイムアウトしました。ローカル成果物から再送できます。")]
    Timeout,
    #[error("送信先に接続・転送できませんでした。ローカル成果物から再送できます。")]
    Network,
    #[error("認証に失敗しました（HTTP 401）。VIDEO_CUE_UPLOAD_TOKEN を確認してください。")]
    Unauthorized,
    #[error("同じ動画 ID に異なる成果物が登録済みです（HTTP 409）。")]
    Conflict,
    #[error("送信先が容量超過を返しました（HTTP 413）。")]
    ServerSizeLimit,
    #[error("送信先が成果物を拒否しました（HTTP {0}）。")]
    Http(u16),
    #[error("送信先の成功応答を確認できませんでした。ローカル成果物から再送してください。")]
    InvalidResponse,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UploadReceipt {
    pub key: String,
    pub created: bool,
}

#[derive(Deserialize)]
struct AnalysisForUpload {
    schema_version: u32,
    highlight_path: Option<String>,
    events: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct ServerReceipt {
    key: String,
    created: bool,
}

/// Upload a completed result directory. The content-derived key is stable across retries.
pub fn upload_result(
    config: &UploadConfig,
    directory: &Path,
) -> Result<UploadReceipt, UploadError> {
    let analysis_path = directory.join("analysis.json");
    let analysis_size = match fs::metadata(&analysis_path) {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        Ok(_) => return Err(UploadError::MissingAnalysis),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(UploadError::MissingAnalysis);
        }
        Err(_) => return Err(UploadError::LocalRead),
    };
    if analysis_size > MAX_ANALYSIS_BYTES {
        return Err(UploadError::AnalysisTooLarge);
    }
    let analysis_bytes = fs::read(&analysis_path).map_err(|_| UploadError::LocalRead)?;
    if analysis_bytes.len() as u64 > MAX_ANALYSIS_BYTES {
        return Err(UploadError::AnalysisTooLarge);
    }
    let analysis: AnalysisForUpload =
        serde_json::from_slice(&analysis_bytes).map_err(|_| UploadError::InvalidAnalysis)?;
    let has_events = !analysis.events.is_empty();
    if analysis.schema_version != 2
        || analysis.highlight_path.as_deref() != has_events.then_some("highlights.mp4")
    {
        return Err(UploadError::InvalidAnalysis);
    }

    let highlight_path = directory.join("highlights.mp4");
    if has_events {
        let metadata = match fs::metadata(&highlight_path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return Err(UploadError::MissingHighlight),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(UploadError::MissingHighlight);
            }
            Err(_) => return Err(UploadError::LocalRead),
        };
        if metadata.len() > MAX_HIGHLIGHT_BYTES {
            return Err(UploadError::HighlightTooLarge);
        }
    }

    let mut hash = Sha256::new();
    hash.update(b"video-cue-engine-upload-v1\0");
    hash.update(&analysis_bytes);
    if has_events {
        hash.update(b"\0highlight\0");
        let mut file = fs::File::open(&highlight_path).map_err(|_| UploadError::LocalRead)?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|_| UploadError::LocalRead)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
    }
    let key = format!("vce-{:x}", hash.finalize());
    let url = config
        .endpoint
        .join(&format!("{key}/"))
        .map_err(|_| UploadError::InvalidUrl)?;
    let mut form = multipart::Form::new()
        .file("analysis", &analysis_path)
        .map_err(|_| UploadError::LocalRead)?;
    if has_events {
        form = form
            .file("highlight", &highlight_path)
            .map_err(|_| UploadError::LocalRead)?;
    }
    let response = config
        .client
        .post(url)
        .bearer_auth(&config.token)
        .multipart(form)
        .send()
        .map_err(|error| {
            if error.is_timeout() {
                UploadError::Timeout
            } else {
                UploadError::Network
            }
        })?;
    let status = response.status().as_u16();
    match status {
        200 | 201 => {
            let mut body = Vec::new();
            response
                .take(4097)
                .read_to_end(&mut body)
                .map_err(|_| UploadError::InvalidResponse)?;
            if body.len() > 4096 {
                return Err(UploadError::InvalidResponse);
            }
            let receipt: ServerReceipt =
                serde_json::from_slice(&body).map_err(|_| UploadError::InvalidResponse)?;
            if receipt.key != key || receipt.created != (status == 201) {
                return Err(UploadError::InvalidResponse);
            }
            Ok(UploadReceipt {
                key,
                created: receipt.created,
            })
        }
        401 => Err(UploadError::Unauthorized),
        409 => Err(UploadError::Conflict),
        413 => Err(UploadError::ServerSizeLimit),
        _ => Err(UploadError::Http(status)),
    }
}

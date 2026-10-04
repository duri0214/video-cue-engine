use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
};

use eframe::egui::{self, RichText};
use video_cue_engine::{
    BatchError, BatchEvent, BatchPlan, BatchSummary, UploadConfig, UploadError, UploadReceipt,
    downloads_directory, open_directory, run_batch_with_upload, upload_result,
};

use super::theme;

#[cfg(test)]
mod tests;

enum Message {
    Scanned(Option<(PathBuf, Result<BatchPlan, BatchError>)>),
    OutputPicked(Option<PathBuf>),
    Progress(BatchEvent),
    Completed(Result<BatchSummary, BatchError>),
    Opened(Result<(), std::io::Error>),
    RetryFinished {
        index: usize,
        result: Result<UploadReceipt, UploadError>,
    },
}

enum VideoStatus {
    Waiting,
    Running,
    Succeeded,
    Failed(String),
    Uploading,
    Uploaded(UploadReceipt),
    UploadFailed(String),
}

struct VideoRow {
    input: PathBuf,
    output: Option<PathBuf>,
    status: VideoStatus,
}

pub struct BatchApp {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    plan: Option<Arc<BatchPlan>>,
    rows: Vec<VideoRow>,
    receiver: Option<Receiver<Message>>,
    status: String,
    error: Option<String>,
    result_directory: Option<PathBuf>,
    succeeded: usize,
    failed: usize,
    upload_after_analysis: bool,
    uploaded: usize,
    upload_failed: usize,
}

impl BatchApp {
    pub fn new() -> Self {
        let (output, error) = match downloads_directory() {
            Ok(path) => (Some(path), None),
            Err(error) => (
                None,
                Some(format!(
                    "OS からダウンロードフォルダを取得できません。出力フォルダを選んでください。({error})"
                )),
            ),
        };
        Self {
            input: None,
            output,
            plan: None,
            rows: Vec::new(),
            receiver: None,
            status: "入力フォルダを選んでください".into(),
            error,
            result_directory: None,
            succeeded: 0,
            failed: 0,
            upload_after_analysis: false,
            uploaded: 0,
            upload_failed: 0,
        }
    }

    fn spawn(
        &mut self,
        ctx: &egui::Context,
        task: impl FnOnce(Sender<Message>, egui::Context) + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        let context = ctx.clone();
        match std::thread::Builder::new()
            .name("video-cue-worker".into())
            .spawn(move || task(sender, context))
        {
            Ok(_) => self.receiver = Some(receiver),
            Err(error) => {
                self.status = "処理を開始できませんでした".into();
                self.error = Some(format!("作業スレッドを起動できません: {error}"));
            }
        }
    }

    fn scan(&mut self, ctx: &egui::Context, select_folder: bool) {
        let current = self.input.clone();
        self.status = "入力フォルダを確認中…".into();
        self.spawn(ctx, move |sender, context| {
            let folder = if select_folder {
                rfd::FileDialog::new()
                    .set_title("解析する MP4 のフォルダを選択")
                    .pick_folder()
            } else {
                current
            };
            let result = folder.map(|folder| {
                let plan = BatchPlan::scan(&folder);
                (folder, plan)
            });
            send(&sender, &context, Message::Scanned(result));
        });
    }

    fn choose_output(&mut self, ctx: &egui::Context) {
        let current = self.output.clone();
        self.spawn(ctx, move |sender, context| {
            let mut dialog = rfd::FileDialog::new().set_title("結果を保存する既存のフォルダを選択");
            if let Some(path) = current {
                dialog = dialog.set_directory(path);
            }
            send(
                &sender,
                &context,
                Message::OutputPicked(dialog.pick_folder()),
            );
        });
    }

    fn start(&mut self, ctx: &egui::Context) {
        let (Some(plan), Some(output)) = (&self.plan, &self.output) else {
            return;
        };
        let plan = Arc::clone(plan);
        let output = output.clone();
        let upload = if self.upload_after_analysis {
            match UploadConfig::from_env() {
                Ok(config) => Some(config),
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            }
        } else {
            None
        };
        self.error = None;
        self.result_directory = None;
        self.succeeded = 0;
        self.failed = 0;
        self.uploaded = 0;
        self.upload_failed = 0;
        for row in &mut self.rows {
            row.status = VideoStatus::Waiting;
            row.output = None;
        }
        self.status = "出力先と FFmpeg を確認中…".into();
        self.spawn(ctx, move |sender, context| {
            let result = run_batch_with_upload(&plan, &output, upload.as_ref(), |event| {
                send(&sender, &context, Message::Progress(event));
            });
            send(&sender, &context, Message::Completed(result));
        });
    }

    fn retry_upload(&mut self, ctx: &egui::Context, index: usize) {
        let Some(directory) = self.rows[index].output.clone() else {
            return;
        };
        let config = match UploadConfig::from_env() {
            Ok(config) => config,
            Err(error) => {
                self.rows[index].status = VideoStatus::UploadFailed(error.to_string());
                return;
            }
        };
        self.rows[index].status = VideoStatus::Uploading;
        self.error = None;
        self.status = format!("再送中: {}", file_name(&self.rows[index].input));
        self.spawn(ctx, move |sender, context| {
            let result = upload_result(&config, &directory);
            send(&sender, &context, Message::RetryFinished { index, result });
        });
        if self.receiver.is_none() {
            self.rows[index].status =
                VideoStatus::UploadFailed("再送の作業スレッドを起動できませんでした。".into());
        }
    }

    fn poll(&mut self) {
        while let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(Message::Progress(event)) => match event {
                    BatchEvent::Prepared { output } => self.result_directory = Some(output),
                    BatchEvent::Started {
                        index,
                        input,
                        output,
                    } => {
                        self.status = format!("解析中: {}", file_name(&input));
                        self.rows[index].status = VideoStatus::Running;
                        self.rows[index].output = Some(output);
                    }
                    BatchEvent::Finished { index, result } => match result {
                        Ok(()) => {
                            self.succeeded += 1;
                            self.rows[index].status = VideoStatus::Succeeded;
                        }
                        Err(error) => {
                            self.failed += 1;
                            self.rows[index].status = VideoStatus::Failed(format!(
                                "解析できませんでした。元動画の再生可否と出力先の空き容量・アクセス権を確認してください。\n{error}"
                            ));
                        }
                    },
                    BatchEvent::UploadStarted { index } => {
                        self.status = format!("送信中: {}", file_name(&self.rows[index].input));
                        self.rows[index].status = VideoStatus::Uploading;
                    }
                    BatchEvent::UploadFinished { index, result } => match result {
                        Ok(receipt) => {
                            self.uploaded += 1;
                            self.rows[index].status = VideoStatus::Uploaded(receipt);
                        }
                        Err(error) => {
                            self.upload_failed += 1;
                            self.rows[index].status = VideoStatus::UploadFailed(error.to_string());
                        }
                    },
                },
                Ok(message) => {
                    self.receiver = None;
                    match message {
                        Message::Scanned(result) => {
                            if let Some((folder, result)) = result {
                                self.input = Some(folder);
                                self.plan = None;
                                self.rows.clear();
                                self.result_directory = None;
                                self.succeeded = 0;
                                self.failed = 0;
                                self.uploaded = 0;
                                self.upload_failed = 0;
                                self.error = None;
                                match result {
                                    Ok(plan) => {
                                        self.rows = plan
                                            .files()
                                            .iter()
                                            .map(|input| VideoRow {
                                                input: input.clone(),
                                                output: None,
                                                status: VideoStatus::Waiting,
                                            })
                                            .collect();
                                        self.plan = Some(Arc::new(plan));
                                    }
                                    Err(error) => self.error = Some(error.to_string()),
                                }
                            }
                            self.status = if self.plan.is_some() {
                                "対象を確認して解析を開始できます"
                            } else {
                                "入力フォルダを選んでください"
                            }
                            .into();
                        }
                        Message::OutputPicked(Some(path)) => {
                            self.output = Some(path);
                            self.error = None;
                        }
                        Message::OutputPicked(None) => {}
                        Message::Completed(result) => match result {
                            Ok(summary) => {
                                self.succeeded = summary.succeeded;
                                self.failed = summary.failed;
                                self.uploaded = summary.uploaded;
                                self.upload_failed = summary.upload_failed;
                                self.result_directory = Some(summary.output);
                                self.status = "すべての動画の処理が終了しました".into();
                            }
                            Err(error) => {
                                self.status = "解析を開始できませんでした".into();
                                self.error = Some(error.to_string());
                            }
                        },
                        Message::Opened(result) => {
                            if let Err(error) = result {
                                self.error = Some(format!("出力フォルダを開けません: {error}"));
                            }
                        }
                        Message::RetryFinished { index, result } => match result {
                            Ok(receipt) => {
                                self.upload_failed -= 1;
                                self.uploaded += 1;
                                self.rows[index].status = VideoStatus::Uploaded(receipt);
                                self.status = "再送が完了しました".into();
                            }
                            Err(error) => {
                                self.rows[index].status =
                                    VideoStatus::UploadFailed(error.to_string());
                                self.status = "再送に失敗しました".into();
                            }
                        },
                        Message::Progress(_) => unreachable!(),
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.status = "処理が中断されました".into();
                    self.error = Some("作業スレッドが予期せず終了しました。出力先を確認してから再実行してください。".into());
                    for row in &mut self.rows {
                        if matches!(row.status, VideoStatus::Uploading) {
                            row.status = VideoStatus::UploadFailed(
                                "送信が中断されました。ローカル成果物から再送できます。".into(),
                            );
                            self.upload_failed += 1;
                        }
                    }
                }
            }
        }
    }
}

impl eframe::App for BatchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        let busy = self.receiver.is_some();
        let mut retry_index = None;
        if busy && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.error = Some("処理中です。完了してからウィンドウを閉じてください。".into());
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(24))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(
                        RichText::new("VIDEO CUE ENGINE")
                            .size(12.0)
                            .strong()
                            .color(theme::ACCENT),
                    );
                    ui.heading(RichText::new("録画から、動きのある瞬間を。").size(27.0));
                    ui.label(
                        RichText::new("フォルダを選ぶだけで、MP4 をまとめて解析します。")
                            .color(theme::MUTED),
                    );
                    ui.add_space(12.0);
                    theme::panel().show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(RichText::new("01  入力フォルダ").strong());
                        folder_label(ui, self.input.as_deref(), "未選択");
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!busy, egui::Button::new("入力フォルダを選ぶ"))
                                .clicked()
                            {
                                self.scan(ctx, true);
                            }
                            if ui
                                .add_enabled(
                                    !busy && self.input.is_some(),
                                    egui::Button::new("再読み込み"),
                                )
                                .clicked()
                            {
                                self.scan(ctx, false);
                            }
                            ui.label(
                                RichText::new(format!("対象 {} 件", self.rows.len()))
                                    .color(theme::ACCENT),
                            );
                        });
                        ui.small(
                            "選択フォルダ直下の .mp4 / .MP4 が対象です。サブフォルダは含みません。",
                        );
                    });
                    ui.add_space(4.0);
                    theme::panel().show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(RichText::new("02  出力フォルダ").strong());
                        folder_label(
                            ui,
                            self.output.as_deref(),
                            "既存の出力フォルダを選んでください",
                        );
                        if ui
                            .add_enabled(!busy, egui::Button::new("出力フォルダを変更"))
                            .clicked()
                        {
                            self.choose_output(ctx);
                        }
                        ui.small(
                            "初期値は OS のダウンロードフォルダ。毎回新しいフォルダに保存します。",
                        );
                    });
                    ui.add_space(4.0);
                    theme::panel().show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(RichText::new("03  Django へ送信").strong());
                        ui.add_enabled(
                            !busy,
                            egui::Checkbox::new(
                                &mut self.upload_after_analysis,
                                "処理完了後に Django へ転送",
                            ),
                        );
                        ui.small("送信先とトークンは VIDEO_CUE_UPLOAD_URL / VIDEO_CUE_UPLOAD_TOKEN で設定します。ローカル成果物は保存されます。");
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let button = egui::Button::new(
                            RichText::new("一括解析を開始")
                                .strong()
                                .color(theme::BACKGROUND),
                        )
                        .fill(theme::ACCENT)
                        .min_size(egui::vec2(180.0, 42.0));
                        if ui
                            .add_enabled(
                                !busy && self.plan.is_some() && self.output.is_some(),
                                button,
                            )
                            .clicked()
                        {
                            self.start(ctx);
                        }
                        if busy {
                            ui.spinner();
                        }
                        ui.label(&self.status);
                    });
                    if let Some(error) = &self.error {
                        ui.colored_label(theme::RED, error);
                    }
                    ui.add_space(4.0);
                    let finished = self.succeeded + self.failed;
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("完了 {}", self.succeeded)).color(theme::ACCENT),
                        );
                        ui.label(RichText::new(format!("失敗 {}", self.failed)).color(
                            if self.failed > 0 {
                                theme::RED
                            } else {
                                theme::MUTED
                            },
                        ));
                        ui.label(format!("/ 全 {} 件", self.rows.len()));
                    });
                    if self.upload_after_analysis || self.uploaded + self.upload_failed > 0 {
                        ui.horizontal(|ui| {
                            ui.label(format!("送信成功 {}", self.uploaded));
                            ui.label(
                                RichText::new(format!("送信失敗 {}", self.upload_failed)).color(
                                    if self.upload_failed > 0 {
                                        theme::RED
                                    } else {
                                        theme::MUTED
                                    },
                                ),
                            );
                        });
                    }
                    ui.add(
                        egui::ProgressBar::new(if self.rows.is_empty() {
                            0.0
                        } else {
                            finished as f32 / self.rows.len() as f32
                        })
                        .fill(theme::ACCENT)
                        .text(format!(
                            "{} / {} 件を処理",
                            finished,
                            self.rows.len()
                        )),
                    );
                    if let Some(path) = self.result_directory.clone() {
                        if ui
                            .add_enabled(!busy, egui::Button::new("今回の出力フォルダを開く"))
                            .clicked()
                        {
                            self.spawn(ctx, move |sender, context| {
                                send(&sender, &context, Message::Opened(open_directory(&path)));
                            });
                        }
                        folder_label(ui, self.result_directory.as_deref(), "");
                    }
                    ui.separator();
                    for (index, row) in self.rows.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.horizontal(|ui| {
                                let (label, color) = match &row.status {
                                    VideoStatus::Waiting => ("待機", theme::MUTED),
                                    VideoStatus::Running => ("解析中", theme::ACCENT),
                                    VideoStatus::Succeeded => ("完了", theme::ACCENT),
                                    VideoStatus::Failed(_) => ("失敗", theme::RED),
                                    VideoStatus::Uploading => ("送信中", theme::ACCENT),
                                    VideoStatus::Uploaded(receipt) => {
                                        if receipt.created {
                                            ("送信済み", theme::ACCENT)
                                        } else {
                                            ("再送確認済み", theme::ACCENT)
                                        }
                                    }
                                    VideoStatus::UploadFailed(_) => ("送信失敗", theme::RED),
                                };
                                ui.colored_label(color, label);
                                ui.label(format!("{:04}  {}", index + 1, file_name(&row.input)));
                            });
                            if let VideoStatus::Failed(error) = &row.status {
                                ui.colored_label(theme::RED, error);
                            }
                            if let VideoStatus::UploadFailed(error) = &row.status {
                                ui.colored_label(theme::RED, error);
                                if ui
                                    .add_enabled(!busy, egui::Button::new("ローカル成果物から再送"))
                                    .clicked()
                                {
                                    retry_index = Some(index);
                                }
                            }
                            if let VideoStatus::Uploaded(receipt) = &row.status {
                                ui.small(format!("動画 ID: {}", receipt.key));
                            }
                            if let Some(output) = &row.output {
                                ui.small(format!("保存先: {}", output.display()));
                            }
                            ui.separator();
                        });
                    }
                });
            });
        if let Some(index) = retry_index {
            self.retry_upload(ctx, index);
        }
    }
}

fn send(sender: &Sender<Message>, context: &egui::Context, message: Message) {
    let _ = sender.send(message);
    context.request_repaint();
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn folder_label(ui: &mut egui::Ui, path: Option<&Path>, empty: &str) {
    ui.add(
        egui::Label::new(path.map_or_else(|| empty.into(), |path| path.display().to_string()))
            .wrap(),
    );
}

# video-cue-engine

固定視点の録画済み MP4 を解析し、動きのあった区間を `analysis.json`、1本のハイライト動画、確認用の短い MP4 クリップに出力する Rust アプリです。フォルダ単位のローカル GUI と、単一 MP4 用の CLI を利用できます。人物や物体の種類は識別しません。

## 必要なもの

- Rust / Cargo
- `ffmpeg` と `ffprobe` が `PATH` 上にあること。動画作成には `libx264` と `drawtext` 対応の FFmpeg が必要です。タイムコードの描画には、Windows では Consolas または Arial、Linux では DejaVu Sans Mono または Liberation Mono、macOS では Menlo のシステムフォントを使用します。

## ローカル画面から一括解析（Windows）

開発者は次のコマンドで GUI の実行ファイルをビルドします。

```powershell
cargo build --release --features gui --bin video-cue-engine-gui
```

`target\release\video-cue-engine-gui.exe` をダブルクリックすると起動します。ビルド済みの実行ファイルを利用する人には Rust、Django、ブラウザは不要です。FFmpeg / ffprobe は利用者の PC にインストールし、`PATH` に追加してください。現段階ではインストーラーや FFmpeg の同梱は行いません。

1. 「入力フォルダを選ぶ」で録画フォルダを選びます。直下にある `.mp4` / `.MP4` の通常ファイルだけを、ファイル名順に一覧表示します。サブフォルダとシンボリックリンクは対象外です。
2. 対象件数と一覧を確認します。ファイルを追加・削除した場合は「再読み込み」で一覧を更新します。
3. 出力先を確認します。Windows の初期値は Known Folder API が示す現在の「ダウンロード」です。移動済みの場所にも対応します。「出力フォルダを変更」で別の既存フォルダを選べます。
4. 「一括解析を開始」を押します。出力先の存在、読み取り、子フォルダ作成・書き込み、FFmpeg / ffprobe の起動を確認してから、表示した動画を順番に解析します。存在しない出力先は作成せず、理由を画面に表示します。
5. 動画ごとの状態、完了・失敗件数を確認します。失敗した動画の理由を表示し、次の動画へ進みます。終了後は「今回の出力フォルダを開く」で結果を確認できます。

配色は `vj-copilot` と同じ暗い背景・ミント色です。日本語表示には Windows のメイリオを使用します。解析中もスクロールと画面更新は継続します。処理の途中終了には対応していないため、実行中のフォルダ変更・再実行・ウィンドウ終了は抑止します。

実行ごとに `video-cue-engine-batch-` で始まる一意のフォルダを作り、その下の `0001`、`0002` … に一覧と同じ順番で保存します。長いファイル名や大文字・小文字の違いによる出力先の衝突を避けるため、動画の保存先は番号にしています。元動画は画面の一覧と各 `analysis.json` の `input.path` で確認できます。再実行時は別のフォルダになり、以前の結果を上書きしません。

```text
選択した出力フォルダ/
└── video-cue-engine-batch-ランダムな識別子/
    ├── 0001/
    │   ├── analysis.json
    │   ├── highlights.mp4
    │   └── events/event-001.mp4
    └── 0002/
        └── analysis.json
```

動きのない動画は `analysis.json` だけを作成します。失敗時は途中の出力を残す場合があるため、画面の成功・失敗を確認してください。解析設定は CLI の初期値と同じです。

GUI は `gui` feature と専用バイナリに分離しています。検出・出力規則と順次処理はライブラリの `BatchPlan::scan` / `run_batch` に置き、GUI は別スレッドからの通知を表示します。CLI のみなら GUI 依存のビルドは不要です。GUI の動作確認対象は Windows です。

## CLI から単一 MP4 を解析

```sh
cargo run --release -- --input /path/to/recording.mp4 --output /path/to/result-folder
```

`--output` には、解析結果を置く親ディレクトリを指定します。指定先の下に `video-cue-engine-output` を自動作成し、その中へ解析結果を保存します。`video-cue-engine-output` がすでに存在して中身がある場合は、既存の解析結果を誤って上書きしないため拒否します。指定した親ディレクトリには、他のファイルやフォルダがあっても構いません。

上の例では、解析結果は `/path/to/result-folder/video-cue-engine-output` に保存されます。Windows PowerShell では、たとえば `--output "$env:USERPROFILE\Downloads"` と指定すると `$env:USERPROFILE\Downloads\video-cue-engine-output` に保存されます。

| オプション | 初期値 | 意味 |
| --- | ---: | --- |
| `--threshold` | `0.005` | 差分がある画素の割合。小さいほど弱い動きも拾う |
| `--min-duration` | `0.4` | イベントとみなす最短の秒数 |
| `--merge-gap` | `0.6` | 動きの間に挟まる静止区間を結合する最大秒数 |

解析では毎秒 5 フレームを 160 × 90 のグレースケールに縮小し、前フレームから輝度が 20 以上変わった画素の割合を測ります。しきい値を超えた区間を結合してから最短時間を適用します。カメラの揺れや照明の変化も動きとして検出されるため、実録画では `--threshold` を素材に合わせて調整してください。

出力例:

```text
result/
└── video-cue-engine-output/
    ├── analysis.json
    ├── highlights.mp4
    └── events/
        ├── event-001.mp4
        └── event-002.mp4
```

```json
{
  "schema_version": 2,
  "input": {
    "path": "/absolute/path/to/recording.mp4",
    "duration_seconds": 14.0
  },
  "highlight_path": "highlights.mp4",
  "events": [
    {
      "start_seconds": 2.0,
      "end_seconds": 4.0,
      "peak_change_ratio": 0.02,
      "clip_path": "events/event-001.mp4",
      "highlight_start_seconds": 1.0
    }
  ]
}
```

`start_seconds` と `end_seconds` は入力動画の先頭からの秒数、`highlight_start_seconds` はハイライト動画の先頭からイベントを頭出しする秒数です。`peak_change_ratio` はイベント中に観測した最大の変化画素割合です。クリップとハイライトにはイベントの前後に各 1 秒の余白を付け、動画の先頭・末尾で切り詰めます。ハイライトでは重なる余白を結合して同じ映像を重複させず、静止区間を飛ばします。映像内の `HH:MM:SS.mmm` は入力動画の先頭からの経過時刻です。音声は含めません。

ハイライトは H.264 / MP4、15 fps、横幅 320～960 px（縦横比を維持）、`libx264` の `veryfast`・CRF 28、`yuv420p` で出力します。確認用の文字と動きを読める解像度を確保しつつ、長尺の元動画を転送せずに済む設定です。イベントごとのクリップは従来どおり CRF 23 で出力します。

形式バージョン 2 では既存の `input` とイベントの時刻・変化量・`clip_path` を維持し、`highlight_path` と `highlight_start_seconds` を追加しました。イベントが 0 件なら `events` は空配列、`highlight_path` は `null` で、ハイライトとイベントクリップは作成しません。FFmpeg によるハイライトの書き出しが失敗した場合は原因と出力先を示して異常終了し、`analysis.json` は作成しません。

## テスト素材と検証

[`fixtures/`](fixtures/README.md) に短い定点カメラ風 MP4、正解時刻、再生成スクリプトがあります。再生成には Python 3 と FFmpeg を使います。

```sh
python fixtures/generate.py
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

GUI を含むビルド・テストは `cargo test --all-targets --all-features` で確認できます。一括処理の統合テストでは、直下の MP4 検出、出力先エラー、FFmpeg 不在、壊れた動画の後の継続、プレビュー後のファイル削除、再実行時の既存結果保護を確認します。Windows では一時フォルダに読み取り・書き込み拒否の ACL を設定するテストも実行し、終了時に解除します（Windows PowerShell を使用）。GUI のテストはネイティブのフォルダ選択結果を差し替え、実際に描画した開始ボタンへのクリック、出力先エラー後の再実行、ワーカーからの完了表示までを画面なしで検証します。

Windows で画面を確認する場合は、リポジトリ直下の PowerShell で次を実行します。

```powershell
cargo build --features gui --bin video-cue-engine-gui
$batchCheckOutput = Join-Path $env:TEMP ('video-cue-ui-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $batchCheckOutput
(Resolve-Path .\fixtures).Path
$batchCheckOutput
.\target\debug\video-cue-engine-gui.exe
```

表示した `fixtures` の絶対パスを入力フォルダに、新規作成した一時フォルダを出力先に選びます。対象 2 件、完了 2 件・失敗 0 件となり、`scene-motion.mp4` の結果にはハイライトとイベント動画、`scene-still.mp4` の結果には空イベントの JSON があることを確認してください。

## 開発ルールとスキル

開発時は [AGENTS.md](AGENTS.md) を起点に、`.codex/rules/` の設計・Rust・テスト規約と、作業に応じたスキルを参照する。

| スキル | 用途 |
| --- | --- |
| [ticket](.codex/skills/ticket/SKILL.md) | 日本語の GitHub Issue 作成 |
| [review](.codex/skills/review/SKILL.md) | PR レビュー、コメント確認、依頼された指摘の修正 |
| [cleanup-branch](.codex/skills/cleanup-branch/SKILL.md) | マージ後のローカルブランチ整理 |

Issue 番号付きブランチで作業し、変更の検証後にコミット・push・PR 作成へ進む。詳細は `AGENTS.md` の常用フローに従う。

Rust コード・依存関係の変更では、`AGENTS.md` に記載した `cargo fmt`・`cargo test`・`cargo clippy` を実行する。

移植元: [duri0214/vj-copilot（1342e882）](https://github.com/duri0214/vj-copilot/tree/1342e882e0753f72c14d06a9f3c46095e5e9ba9f)。プロジェクト名、スキルへのリンク、文書のみの検証、レビューと修正の適用条件を本リポジトリ向けに調整している。

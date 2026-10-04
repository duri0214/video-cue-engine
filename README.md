# video-cue-engine

固定視点の録画済み MP4 を解析し、動きのあった区間を `analysis.json`、1本のハイライト動画、確認用の短い MP4 クリップに出力する Rust アプリです。フォルダ単位のローカル GUI と、単一 MP4 用の CLI を利用できます。人物や物体の種類は識別しません。

## 必要なもの

- Rust / Cargo
- `ffmpeg` と `ffprobe` が `PATH` 上にあること。動画作成には `libx264` と `drawtext` 対応の FFmpeg が必要です。

## GUI をビルドして起動する（Windows）

開発者は次のコマンドで GUI の実行ファイルをビルドします。

```powershell
cargo build --release --features gui --bin video-cue-engine-gui
```

ビルドが成功すると、リポジトリ直下の `target\release\video-cue-engine-gui.exe` に実行ファイルが作られます。PowerShell で生成先を確認して起動する場合は次を実行します。

```powershell
$guiExe = (Resolve-Path .\target\release\video-cue-engine-gui.exe).Path
Write-Output $guiExe
& $guiExe
```

ローカル解析だけなら、エクスプローラーでこの `.exe` をダブルクリックしても起動できます。ビルド済みの実行ファイルでローカル解析する場合、Rust、Django、ブラウザは不要です。FFmpeg / ffprobe は利用者の PC にインストールし、`PATH` に追加してください。

## ビルドしたアプリケーションを使う（Windows）

ビルドした GUI でローカル解析を行います。Django に転送する場合は、[Django ビューアへの送信設定](#django-ビューアへの送信設定)を先に済ませ、環境変数を設定した同じ PowerShell から GUI を起動してください。Django への送信設定は GUI と CLI に共通です。

1. 「入力フォルダを選ぶ」で録画フォルダを選びます。直下にある `.mp4` / `.MP4` の通常ファイルだけを、ファイル名順に一覧表示します。サブフォルダとシンボリックリンクは対象外です。
2. 対象件数と一覧を確認します。ファイルを追加・削除した場合は「再読み込み」で一覧を更新します。
3. 出力先を確認します。初期値は現在の「ダウンロード」フォルダです。「出力フォルダを変更」で別の既存フォルダを選べます。
4. 「一括解析を開始」を押します。出力先や FFmpeg / ffprobe に問題がある場合は、画面に理由を表示します。
5. 動画ごとの状態、完了・失敗件数を確認します。失敗した動画の理由を表示し、次の動画へ進みます。終了後は「今回の出力フォルダを開く」で結果を確認できます。

結果は、実行ごとに作る `video-cue-engine-batch-` フォルダ内の `0001`、`0002` … に保存します。元動画は画面の一覧か各 `analysis.json` の `input.path` で確認できます。再実行すると別の結果フォルダを作ります。

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

動きのない動画は `analysis.json` だけを作成します。失敗時は途中の出力を残す場合があるため、画面の成功・失敗を確認してください。

## Django ビューアへの送信設定

この設定は GUI と CLI に共通です。Django 側でサーバーを起動し、Django の環境変数 `VIDEO_CUE_UPLOAD_TOKEN` にトークンを設定します。engine 側の PowerShell にも、同じ値を環境変数 `VIDEO_CUE_UPLOAD_TOKEN` として設定してください。送信先 API は `/video_cue/api/results/` です。トークンを URL、`analysis.json`、ソースコード、リポジトリに書かないでください。

### ローカル開発環境の設定

engine 側の PowerShell で、Django 側と同じトークンとローカル用 URL を環境変数に設定します。

```powershell
$env:VIDEO_CUE_UPLOAD_URL = 'http://127.0.0.1:8000/video_cue/api/results/'
$env:VIDEO_CUE_UPLOAD_TOKEN = Read-Host 'Django と engine に共通のトークン'
```

### 本番環境の設定

engine 側の PowerShell で、本番用の HTTPS URL と環境変数を設定します。

```powershell
$env:VIDEO_CUE_UPLOAD_URL = 'https://www.henojiya.net/video_cue/api/results/'
$env:VIDEO_CUE_UPLOAD_TOKEN = Read-Host 'Django と engine に共通のトークン'
```

`Read-Host` で入力した値は、その PowerShell セッションの環境変数としてだけ有効で、そこから起動した子プロセスに引き継がれます。PowerShell を開き直したら、engine 側で再設定してください。

### GUI から送信する

GUI で解析・送信する場合、CLI の `--input`、`--output`、`--upload` は入力しません。環境変数を設定した同じ PowerShell から、上の「GUI をビルドして起動する」の手順で GUI を起動します。PowerShell の環境変数は、エクスプローラーから起動した GUI には渡りません。

GUI で入力フォルダと出力先を選び、「処理完了後に Django へ転送」をオンにしてから「一括解析を開始」を押します。このチェックは初期状態でオフです。オンにした回だけ、解析に成功した各動画の `analysis.json` と、イベントがある場合の `highlights.mp4` を送ります。イベント0件では JSON だけを送ります。元 MP4 と `events/` の個別クリップは送信しません。

`VIDEO_CUE_UPLOAD_URL` または `VIDEO_CUE_UPLOAD_TOKEN` が未設定のまま送信を開始すると、エラーが表示されます。設定後に同じ PowerShell から再実行してください。

送信成功時は動画 ID と、新規登録か再送確認かを表示します。送信が失敗してもローカル成果物は残ります。GUI では動画ごとに送信結果と理由が表示され、失敗した行の「ローカル成果物から再送」で再解析せずに送れます。容量超過などで送信できない場合も、理由が表示されます。

### CLI から送信する（任意）

GUI を使わずに送信する場合は、ローカル開発環境または本番環境の設定を済ませた engine 側 PowerShell から実行します。新しく解析して送る場合は `--upload`、保存済み成果物を再送する場合は `--upload-existing` を使います。

```powershell
$videoCueOutput = Join-Path $env:TEMP ('video-cue-upload-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $videoCueOutput | Out-Null
cargo run --release -- --input .\fixtures\scene-motion.mp4 --output $videoCueOutput --upload
cargo run --release -- --upload-existing (Join-Path $videoCueOutput 'video-cue-engine-output')
```

`--input` / `--output` だけの CLI 実行は送信しません。`--upload` が失敗しても成果物は残るため、`--upload-existing` で再送できます。

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

カメラの揺れや照明の変化も動きとして検出されるため、実録画では `--threshold` を素材に合わせて調整してください。

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

## テスト素材と検証

[`fixtures/`](fixtures/README.md) に短い定点カメラ風 MP4、正解時刻、再生成スクリプトがあります。再生成には Python 3 と FFmpeg を使います。

```sh
python fixtures/generate.py
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

GUI を含むテストは `cargo test --all-targets --all-features` で実行できます。

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

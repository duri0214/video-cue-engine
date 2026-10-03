# video-cue-engine

固定視点の録画済み MP4 を解析し、動きのあった区間を `analysis.json`、1本のハイライト動画、確認用の短い MP4 クリップに出力する Rust CLI です。人物や物体の種類は識別しません。

## 必要なもの

- Rust / Cargo
- `ffmpeg` と `ffprobe` が `PATH` 上にあること。動画作成には `libx264` と `drawtext` 対応の FFmpeg が必要です。タイムコードの描画には、Windows では Consolas または Arial、Linux では DejaVu Sans Mono または Liberation Mono、macOS では Menlo のシステムフォントを使用します。

## 実行

```sh
cargo run --release -- --input recording.mp4 --output result
```

`--output` には、解析結果を置く親ディレクトリを指定します。指定先の下に `video-cue-engine-output` を自動作成し、その中へ解析結果を保存します。`video-cue-engine-output` がすでに存在して中身がある場合は、既存の解析結果を誤って上書きしないため拒否します。指定した親ディレクトリには、他のファイルやフォルダがあっても構いません。

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

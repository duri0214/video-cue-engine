# video-cue-engine

## 開発ルールとスキル

開発時は [AGENTS.md](AGENTS.md) を起点に、`.codex/rules/` の設計・Rust・テスト規約と、作業に応じたスキルを参照する。

| スキル | 用途 |
| --- | --- |
| [ticket](.codex/skills/ticket/SKILL.md) | 日本語の GitHub Issue 作成 |
| [review](.codex/skills/review/SKILL.md) | PR レビュー、コメント確認、依頼された指摘の修正 |
| [cleanup-branch](.codex/skills/cleanup-branch/SKILL.md) | マージ後のローカルブランチ整理 |

Issue 番号付きブランチで作業し、変更の検証後にコミット・push・PR 作成へ進む。詳細は `AGENTS.md` の常用フローに従う。

現在は Rust 実装前で `Cargo.toml` がないため、ルール・文書の変更は `git diff --check` と参照先の存在確認で検証する。Rust 導入後のコード・依存関係の変更では、`AGENTS.md` に記載した `cargo fmt`・`cargo test`・`cargo clippy` を実行する。

移植元: [duri0214/vj-copilot（1342e882）](https://github.com/duri0214/vj-copilot/tree/1342e882e0753f72c14d06a9f3c46095e5e9ba9f)。プロジェクト名、スキルへのリンク、文書のみの検証、レビューと修正の適用条件を本リポジトリ向けに調整している。

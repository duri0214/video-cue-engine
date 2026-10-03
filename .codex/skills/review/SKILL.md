---
name: review
description: Rust リポジトリの PR レビュー、レビューコメント確認、指摘対応を行う。DDD の責務分離と Rust の品質確認を含む。
---

# Review

## 依頼の範囲

- レビューやコメント確認だけの依頼では、調査結果を報告する。ファイル変更、コミット、push、GitHub への投稿は行わない。
- 指摘の修正を依頼された場合に、下記の修正・検証・コミット・push の手順を実行する。GitHub への返信は、返信も依頼されている場合に実行する。

## 手順

1. PR 番号または URL を特定する。指定がなければ現在のブランチから `gh pr view --json number,url` で確認する。
2. `gh pr view <番号> --json title,body,state,baseRefName,headRefName,url` で PR を確認する。
3. レビュー本文とインラインコメントを別々に取得する。

   ```bash
   gh api repos/<owner>/<repo>/pulls/<番号>/reviews --paginate
   gh api repos/<owner>/<repo>/pulls/<番号>/comments --paginate
   ```

   `state` だけでコメントの有無を判断せず、`id`、`in_reply_to_id`、`body`、`path`、`line`、`html_url` を確認する。
4. 未解決で内容が明確な実行可能コメントを重大度順に整理する。情報共有、重複、解決済みのコメントは対応対象から外す。
5. 変更を確認し、Issue の要件、KISS、YAGNI、MECE、DDD の責務分離、Rust の所有権・エラー処理を確認する。
6. 修正依頼がある場合は、実行可能な指摘を追加確認なしにローカルで修正する。関係ないリファクタリングへ広げない。レビューのみなら、ここでレビュー結果を報告する。
7. Rust のコード・依存関係の変更では `cargo fmt --all -- --check`、対象テスト、`cargo clippy --all-targets --all-features -- -D warnings` を実行する。ルール・ドキュメントだけの変更では `git diff --check` と参照先の存在確認を行う。`Cargo.toml` がない場合は Cargo の検証を実行できないことを報告する。
8. 指摘対応をコミットして push する。
9. 返信も依頼されている場合は、対応したインラインコメントへ、対応内容とコミットを `gh api` で返信する。返信後、コメント一覧で `in_reply_to_id` と本文を確認する。

## DDD と Rust の確認点

- Domain が DB、HTTP、ファイル、フレームワーク、非同期ランタイムへ直接依存していないか。
- Entity / Aggregate が自身の不変条件を守っているか。
- Value Object が意味のある型と生成時検証を持つか。
- Application Service が処理順を調整し、業務ルールを重複実装していないか。
- Repository trait が必要な操作だけを表し、具体的な永続化が infrastructure に隠れているか。
- `clone`、`unwrap`、`expect`、`panic!` が設計上の問題を隠していないか。

## レビュー結果

レビュー結果は重大度順に、ファイルと行番号、挙動上の根拠を添えて報告する。問題がなければ明確にそう書き、残るテスト不足やリスクだけを短く補足する。

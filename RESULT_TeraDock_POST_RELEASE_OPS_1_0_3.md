# TeraDock 1.0.3 Post-Release Operations Result

## 変更内容

- 1.0.3公開後の運用状態を調査し、`docs/post-release-audit-1.0.3.md` に記録した。
- GitHub Issue templatesを追加した。
- Pull Request templateを追加した。
- `CONTRIBUTING.md` を追加した。
- `SECURITY.md` を追加した。
- `ROADMAP.md` を追加した。
- READMEに1.0.3の現在版、報告、貢献、セキュリティ、ロードマップ、既知制約への導線を追加した。

## 変更ファイル

- `.github/ISSUE_TEMPLATE/bug_report.yml`
- `.github/ISSUE_TEMPLATE/documentation.yml`
- `.github/ISSUE_TEMPLATE/feature_request.yml`
- `.github/pull_request_template.md`
- `CONTRIBUTING.md`
- `SECURITY.md`
- `ROADMAP.md`
- `README.md`
- `docs/post-release-audit-1.0.3.md`
- `RESULT_TeraDock_POST_RELEASE_OPS_1_0_3.md`

## 1.0.3 post-release audit結果

- Local `HEAD` は tag `v1.0.3` と一致している。
- GitHub Releaseの最新は `v1.0.3`。draft/prereleaseではなく、公開済み。
- Release assetsは `td-1.0.3-windows-x86_64-setup.exe`、`td-1.0.3-linux-x86_64.tar.gz`、checksums、deb/rpm。
- deb/rpmはCargo package metadata由来で `0.1.0` 名のまま出ている。
- workspaceのCargo package versionは `td`、`tdcore`、`tui`、`common` ともに `0.1.0`。
- README、CHANGELOG、RELEASE_NOTES、RELEASE_CHECKLIST、release artifact validation docsには `0.1.0` / `v0.1.0` 表記が残っている。
- 主要機能のREADME記載と実装は概ね整合している。CLI/TUIにはprofile、CommandSet、TUI、interactive SSH session、recent、secrets、transfer、tunnel、import/export、doctorなどの導線が存在する。

## 1.0.3時点の主要機能

- SSH/Telnet/Serial profile管理。
- danger levelとcritical profileのtyped confirmation。
- SSH connect/exec/run。
- `td init --with-samples` によるread-only sample CommandSet導入。
- TUIでの検索、絞り込み、mark、CommandSet実行、結果タブ、`s` によるinteractive SSH session。
- `td recent` / `td recent --json`。
- master password配下のsecret保存。
- config/env/configset/client override/SSH auth order。
- SCP/SFTP/明示許可FTPによるtransfer。
- SSH tunnel。
- import/export。
- doctor。

## README/CHANGELOG/Release notesのズレ

- READMEは今回 `Current stable version: 1.0.3` と1.0.3 asset名を追記した。
- CHANGELOGはまだ `0.1.0` のみ。
- `RELEASE_NOTES_0.1.0.md` は残っており、1.0.3専用のchecked-in release notesはない。
- `RELEASE_CHECKLIST.md` と `docs/release-artifact-validation.md` は `v0.1.0` 前提のまま。
- Cargo package versionが `0.1.0` のため、deb/rpm asset名も1.0.3と揃っていない。

## Issue template追加内容

- `bug_report.yml`
  - TeraDock version、OS、install method、command executed、expected/actual behavior、logs/output、secret除去確認、reproduction steps。
  - secret/password/token/private keyを貼らないこと、SSH host/userをmaskすることを明記。
- `feature_request.yml`
  - problem、proposed solution、alternatives、target user、priority、v1.x/future major versionの区分。
  - secret類やmaskなしSSH情報を貼らない注意を追加。
- `documentation.yml`
  - affected document、confusing section、expected clarification、suggested wording。
  - host/user/log/command exampleのsanitize確認を追加。

## PR template追加内容

- Summary、Scope、Related issue、Test results。
- Security/logging impact。
- Documentation updated。
- Breaking change。
- Checklistとして `cargo fmt --check`、`cargo test`、`cargo clippy --all-targets --all-features -- -D warnings`、docs更新、secret非ログ化、TUI manual smoke、release notes確認を追加。

## CONTRIBUTING.md要約

- Rust stable前提の開発環境。
- build/test/clippy/release前チェックコマンド。
- CLI/TUI smoke check方針。
- Issue/PR方針。
- security-sensitive dataを貼らない注意。
- 1.0.xは安定化優先、機能拡張は1.1以降で扱う方針。

## SECURITY.md要約

- Supported versionsは `1.0.x`。
- security issueは公開Issueに詳細を書かず、まずminimal issueでprivate coordinationを依頼する方針。
- secret/password/token/private key/full SSH auth args/unmasked host userを公開しない注意。
- logsに含めてよい情報/いけない情報。
- SSH session/oplogは小さく安全なmetadataに留める方針。
- FTPはinsecure扱いで、SCP/SFTPを推奨。

## ROADMAP.md要約

- Current stableは `1.0.3`。
- 1.0.xはbug/docs/packaging/regression fix中心で、大きな機能追加はしない。
- 1.1候補としてTUI recent pane、terminal emulator launch configuration、tmux integration design、transfer/tunnel SSH invocation cleanup、CommandSet runner boundary cleanup、smoke test script、screenshots/GIF documentationを整理。
- 1.1でやらないものとしてWeb UI、cloud sync、remote daemon、Ansible replacement、credential sharing serviceを明記。

## README更新点

- Current stable versionとして `1.0.3` を明記。
- install artifact例を1.0.3のWindows/Linux assetsに更新。
- GitHub Releases配布でありcrates.io公開ではないことを維持。
- Project Operations sectionを追加し、bug report、feature request、security policy、contributing、roadmapへの導線を追加。
- More Documentationに `SECURITY.md`、`CONTRIBUTING.md`、`ROADMAP.md`、post-release auditを追加。

## 実行したテスト

```bash
git diff --check
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## テスト結果

- `git diff --check`: 成功。
- `cargo fmt --check`: 成功。
- `cargo test`: 成功。
  - common: 5 passed。
  - td CLI: 14 passed。
  - tdcore: 37 passed。
  - tui lib: 12 passed。
  - doctests: passed。
- `cargo clippy --all-targets --all-features -- -D warnings`: 成功。

## 未対応事項

- v1.1候補機能は実装していない。
- terminal emulator launch、tmux integration、TUI recent paneは実装していない。
- Cargo package version `0.1.0` と public release `1.0.3` の不一致は修正していない。
- `CHANGELOG.md`、`RELEASE_NOTES_0.1.0.md`、`RELEASE_CHECKLIST.md`、`docs/release-artifact-validation.md` の1.0.x向け全面更新は今回の対象外として監査に記録した。
- 実サーバ接続前提の自動テストは追加していない。

## 次にやるべきこと

- Cargo package versionとpublic release versionを揃えるか、配布戦略として分けるかを決める。
- 1.0.x向けのrelease checklistとartifact validation docsへ更新する。
- 次回patch release用のrelease notes作成ルールを決める。
- 実サーバ不要のsmoke test scriptを設計する。
- 1.1候補はROADMAP上で優先順位を確定してから着手する。

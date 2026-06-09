# TeraDock SSH Session Hardening Result

## 変更内容

- TUI interactive SSH sessionの終了結果を既存 `op_logs` に `op = ssh_session` として記録するようにした。
- SSH process launch失敗もTUIへ復帰したうえで `ssh_session` 失敗ログとして記録するようにした。
- `ssh_session` ログ用metadataに安全なprofile情報だけを入れ、SSH auth args、password、secret、full command stringは記録しないようにした。
- `td recent` / `td recent --limit N` / `td recent --json` を追加し、既存 `op_logs` から最近のinteractive SSH session profileを参照できるようにした。
- `td ui` 起動時にstdin/stdoutがTTYでない場合、raw modeへ入る前に明確なエラーで終了するようにした。
- critical confirmationのキャンセル時にstatus messageを出すようにした。
- README、`docs/tui.md`、`docs/security.md` にTUI SSH session、TTY要件、oplog、recent、secret非記録方針を追記した。

## 変更ファイル

- `Cargo.lock`
- `README.md`
- `crates/cli/Cargo.toml`
- `crates/cli/src/main.rs`
- `crates/core/src/oplog.rs`
- `crates/tui/src/app.rs`
- `crates/tui/src/state.rs`
- `docs/security.md`
- `docs/tui.md`
- `RESULT_TeraDock_SSH_SESSION_HARDENING.md`

## 追加/変更したCLI

- `td recent`
- `td recent --limit 10`
- `td recent --json`

`td recent` は `op_logs` の `op = ssh_session` をprofileごとに最新1件へ集約し、最新順で表示する。schema変更はしていない。

## 操作方法

1. `td ui` を起動する。
2. ProfilesペインでSSH profileを選択する。
3. `s` を押す。
4. critical profileの場合は表示されたprofile idを入力してEnterで確認する。
5. TUIが一時停止し、同じターミナル上でSSH sessionが開く。
6. SSH終了後、TUIへ戻りstatusに終了結果が表示される。
7. CLIで `td recent` を実行すると最近のinteractive SSH session profileを確認できる。

## oplog仕様

- `op`: `ssh_session`
- `profile_id`: 対象profile id
- `client_used`: 解決されたSSH client path
- `ok`: SSH processの成功/失敗
- `exit_code`: 終了コード。取得できない場合やlaunch failureではNULL
- `duration_ms`: process起動待ちから終了/launch failureまでの時間
- `meta_json`: `mode`, `source`, `host`, `port`, `user`, `profile_type`

launch failureの場合は `meta_json.launch_error` も記録する。password、secret、token、SSH auth args、full command stringは記録しない。

## recent仕様

表示内容:

- profile_id
- name
- user@host:port
- profile type
- danger level
- last connected time
- last exit code / status

JSON出力では `client_used` と `duration_ms` も含む。

## 実行したテスト

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo run -p td -- ui
cargo run -p td -- recent --limit 1 --json
```

## テスト結果

- `cargo fmt --check`: 成功。
- `cargo test`: 成功。
- `cargo clippy --all-targets --all-features -- -D warnings`: 成功。
- `cargo run -p td -- ui`: 非TTY環境で `td ui requires an interactive TTY; interactive SSH sessions require a TTY` と表示して終了することを確認。
- `cargo run -p td -- recent --limit 1 --json`: 現在のDBでは `[]` を返すことを確認。

## 追加/変更したテスト

- `ssh_session` recent queryがprofileごとに最新順で集約すること。
- `recent --limit` が件数制限すること。
- `td recent --limit 5 --json` のCLI parse。
- TUI SSH session requestが安全なprofile metadataを保持すること。
- SSH session結果ログで `op = ssh_session`、`exit_code = NULL`、metadata、`last_used_at` を記録すること。
- SSH launch failureを `exit_code = NULL` の失敗ログとして記録すること。
- critical confirmation cancel時にstatus messageを出すこと。

## 未対応事項

- TUI上のrecent一覧ペインは未実装。今回はCLI recentまで。
- 新規ターミナルウィンドウ起動、tmux連携、自前pseudo terminalは未実装。
- 実サーバへのSSH接続を使うテストは追加していない。
- `op_logs` は既存どおりlocal DB内の操作履歴で、export/import対象にはしていない。

## 次にやるべきこと

- TUIにrecent一覧を追加する場合は、CLIと同じ `tdcore::oplog::recent_ssh_sessions` を使う。
- SSH auth option生成がCLI/TUIで重複しているため、次の整理ではcoreへ寄せる。
- CommandSet timeoutやSSH launch failureなど、他の操作でも失敗oplogの粒度を揃える。

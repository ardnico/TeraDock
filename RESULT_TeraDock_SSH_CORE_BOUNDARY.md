# RESULT: TeraDock SSH Core Boundary

## 調査結果

SSH関連処理の重複は主に以下にありました。

- `crates/cli/src/main.rs`: `connect`、`exec`、`run`、`test`、`tunnel`、`push/pull/xfer`、`config apply` がそれぞれSSH client解決とauth args生成を呼び出していました。`exec` と `connect` は `-p <port>` と `user@host` の組み立てもCLI内で持っていました。
- `crates/tui/src/state.rs`: TUI interactive SSH session用に、SSH profile検証、client解決、auth order読み込み、auth args生成、safe metadata生成をCLIとは別実装で持っていました。
- `crates/core/src/cmdset_runner.rs`: CommandSet実行のspawnとoplog記録はcoreにありましたが、SSH client/auth argsは呼び出し側から渡される境界でした。

coreへ寄せるべき処理は、SSH profile検証、SSH client解決、auth order検証、auth args生成、共通SSH引数構築、safe metadata生成です。

CLI/TUIに残すべき処理は、表示、確認、process spawn、stdout/stderr処理、TUI terminal suspend/resumeです。

今回はtransfer/tunnelのフル引数構築、terminal emulator launch、tmux integration、大規模なCommandSet runner改修は触らない方針にしました。

## 変更内容

- `tdcore::ssh` を追加し、SSH invocation構築の共通基盤を作成しました。
- TUI interactive SSH session request生成を `tdcore::ssh::build_ssh_invocation` 利用へ変更しました。
- CLI `connect`、`exec`、`run` のSSH client/auth/基本引数構築をcore共通処理へ寄せました。
- CLI/TUIで重複していたSSH auth order解析とauth args生成をcoreへ移しました。
- TUI `ssh_session` metadataはcore生成のsafe metadataを使い、launch failure時だけ `launch_error` を追加する形にしました。
- transfer/tunnel/test/config applyは、既存構造を保ちながらcoreのauth/client resolution helperを使う状態にしました。
- SSH invocation責務境界の内部ドキュメントを追加しました。

## 変更ファイル

- `crates/core/src/ssh.rs`
- `crates/core/src/lib.rs`
- `crates/cli/src/main.rs`
- `crates/tui/src/state.rs`
- `docs/internal/ssh-invocation-boundary.md`
- `docs/internal/commandset-execution-boundary.md`
- `docs/tui.md`
- `docs/security.md`
- `README.md`

## coreへ移した責務

- profile idからSSH profileを取得、検証する処理
- 非SSH profileの拒否
- SSH client path解決
- `ssh_auth_order` 読み込み、検証、整形
- SSH auth args生成
- `-p <port>` と `user@host` の基本引数構築
- caller指定の `source` / `mode` を含むsafe metadata生成

## CLI/TUIに残した責務

- CLI argument parsing、表示、JSON/text出力
- CLI critical confirmation
- CLI/TUIのprocess spawn
- CLI auth hint/warning表示
- TUI selection、status message、confirmation state
- TUI terminal suspend/resume
- TUI SSH session result/launch failureのoplog記録

## 既存機能への影響

- TUIの `s` キーによるinteractive SSH sessionは同じspawn方式のままです。
- CLI `connect` / `exec` / `run` はSSH引数構築だけ共通化し、外部process実行と出力処理はCLI側に残しています。
- transfer/tunnelは大きな構造変更を避け、今回の主目的外としてフル共通化していません。
- 新しい実サーバ接続前提のテストは追加していません。

## oplog/recentへの影響

- TUI SSH sessionの `op = ssh_session` は維持しました。
- success/failure、exit_code、duration、client_usedの扱いは維持しました。
- launch failureは従来通り `ok = false`、`exit_code = NULL` で記録します。
- safe metadataはcore生成に変わりましたが、内容は `mode`、`source`、`host`、`port`、`user`、`profile_type` の安全な項目に限定しています。
- `td recent` は既存の `op = ssh_session` 集約ロジックを変更していません。

## 実行したテスト

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

追加で実装中に以下のfocused test/checkも実行しました。

```bash
cargo check
cargo test -p tdcore ssh::tests
cargo test -p tui ssh_session
```

## テスト結果

- `cargo fmt --check`: 成功
- `cargo test`: 成功
- `cargo clippy --all-targets --all-features -- -D warnings`: 成功
- focused tests/checks: 成功

## 未対応事項

- `tdcore::cmdset_runner` はまだ `ssh` pathとauth argsを個別に受け取ります。
- transfer/tunnelはSSH invocation全体ではなく、core auth/client helperの利用に留めました。
- terminal emulator launchとtmux integrationは未実装、未整理です。

## 次にやるべきこと

- CommandSet runnerに渡すSSH境界を `SshInvocation` または専用の軽量構造へ寄せる。
- transfer/tunnel向けに、それぞれのコマンド形状に合うcore helperを設計する。
- terminal emulator launch/tmux integrationを追加する場合は、core invocationを入力にしたcaller-owned launch strategyとして設計する。

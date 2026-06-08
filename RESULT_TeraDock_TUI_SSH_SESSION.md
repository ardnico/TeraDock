# TeraDock TUI SSH Session Result

## 変更内容

- TUIのProfilesペインで選択中のSSH profileに対し、`s` キーで対話SSHセッションを開けるようにした。
- SSH起動前にraw modeを解除し、alternate screenとmouse captureを解除し、cursorを表示するようにした。
- SSH終了後にraw mode、alternate screen、mouse captureを復帰し、TUIを再描画できる状態へ戻すようにした。
- SSH終了ステータスをstatus messageへ反映するようにした。
- 選択profileなし、SSH以外のprofile、SSH client未解決時はSSHを起動せずstatus messageを表示するようにした。
- critical profileでは、既存の安全モデルに合わせてprofile id入力確認後にSSH sessionを開くようにした。
- TUI helpと下部action hint、README、docsに `s` の操作説明を追加した。

## 変更ファイル

- `crates/tui/src/app.rs`
- `crates/tui/src/state.rs`
- `crates/tui/src/ui.rs`
- `README.md`
- `docs/tui.md`
- `RESULT_TeraDock_TUI_SSH_SESSION.md`

## 操作方法

1. `td ui` を起動する。
2. ProfilesペインでSSH profileを選択する。
3. `s` を押す。
4. critical profileの場合はprofile idを入力してEnterで確認する。
5. TUIが一時的に通常端末へ戻り、同じターミナルでSSHセッションが開始される。
6. SSHセッションを終了するとTUIへ復帰し、statusに終了結果が表示される。

## 実行したテスト

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## テスト結果

- `cargo fmt --check`: 成功。
- `cargo test`: 成功。workspace全体で既存テストと追加したTUI state/appテストが成功。
- `cargo clippy --all-targets --all-features -- -D warnings`: 成功。

## 未対応事項

- OS別に新規ターミナルウィンドウを開く方式は実装していない。
- terminal emulator選択設定は実装していない。
- profileごとのterminal command overrideは実装していない。
- tmux pane/window連携は実装していない。
- SSH session履歴、最近接続したprofile一覧は実装していない。

## 将来拡張案

- 新規ターミナルウィンドウで開く。
- terminal emulatorを設定で選択できるようにする。
- profileごとにterminal command overrideを設定できるようにする。
- tmux pane/windowと連携する。
- SSH session履歴を記録する。
- 最近接続したprofile一覧をTUIに表示する。

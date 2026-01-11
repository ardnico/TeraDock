# PROJECT_PLAN.md - TeraDock（内部設計 + 実装計画）v0.1

本ドキュメントは「外部設計 v0.3」を実装へ落とすための **内部設計（アーキテクチャ / データ設計 / 暗号設計 / 実行方式 / テスト方針）** と **実装計画（フェーズ）** を定義する。

---

## 0. ゴール / アンチゴール

### ゴール
- Windows / Linux の両方で動作する CLI + TUI の TeraDock を提供する。
- ServerProfile を `profile_id`（小文字正規化・予約語禁止・base32自動生成）で管理し、`connect/exec/run/push/pull/xfer/config apply` を実行できる。
- Secret（パスワード等）を暗号化保存し、`td show` で **userは平文表示**、**パスワードは絶対に表示しない**。
- マスターパスワードを設定した場合、明示コマンドでのみ `secret reveal` を許可する（ログに値を残さない）。
- 依存コマンドの検査と差分吸収（`td doctor` + クライアント指定）を備える。
- v0.1では **SSHはフル対応**（connect/exec/run/transfer/config）、**Telnet/Serialはconnect中心**（初期送信のみ可、対話自動化なし）。

### アンチゴール
- SSH/Telnetプロトコルやターミナルエミュレーションの自前実装（Serialは最低限のパススルーのみ）。
- Expect風の対話自動化（v0.1では一切やらない）。
- FTPをデフォルト有効（危険）。明示設定＋明示フラグが必要。

---

## 1. 全体アーキテクチャ

### 1.1 ワークスペース構成
- `crates/core` : ドメイン・永続化・暗号・コマンド生成・実行・ログ記録
- `crates/cli`  : clapベースのCLI（`td ...`）
- `crates/tui`  : ratatui + crossterm のTUI（`td ui`）
- `crates/common`（任意）: 共有ユーティリティ（id正規化、エラー型、serdeスキーマ）

> 重要：UI（CLI/TUI）は **coreのユースケースAPI** を呼ぶだけにして、仕様逸脱を防ぐ。

### 1.2 層（責務）
- Domain層（core）
  - Profile/Secret/CommandSet/ConfigSet などのモデル
  - ルール：ID正規化、予約語禁止、danger guardなど
- Usecase層（core）
  - `add_profile`, `connect`, `exec`, `run`, `push/pull/xfer`, `config_apply`, `doctor` 等
- Infra層（core）
  - DB（SQLite）
  - 暗号（master password / keyring）
  - 外部コマンド実行（ssh/scp/sftp/telnet）
  - Serial接続（serialport + crossterm raw passthrough）
- Presentation層（cli/tui）
  - 引数解釈、表示、確認プロンプト、TUI操作

---

## 2. データ永続化設計

### 2.1 ストレージ選定
- 実行時DB：SQLite（単一ファイル、原子性、検索性、移行容易）
- Export/Import：JSON（機械向け）＋必要ならTOML（人間向け）を追加

理由：
- プロファイル/コマンドセット/設定配布/ログなどが増えると、TOML単体は更新競合やクエリが厳しい。
- SQLiteにしておけばTUIの検索/絞り込みが速くなる。

### 2.2 DB配置
- Windows: `%APPDATA%/TeraDock/teradock.db`
- Linux: `~/.config/teradock/teradock.db`
- ログ（テキスト）：同ディレクトリ配下 `logs/`
- Exportファイル：ユーザ指定パス

（`directories` crateで決定）

### 2.3 スキーマ（概要）
- `settings`
  - `key` TEXT PRIMARY KEY
  - `value` TEXT
- `profiles`
  - `profile_id` TEXT PRIMARY KEY
  - `name`, `type`(ssh/telnet/serial), `host`, `port`, `user`(平文)
  - `danger_level`, `group`, `tags_json`, `note`
  - `client_overrides_json`（ssh/scp/sftp/telnetの上書き）
  - `created_at`, `updated_at`, `last_used_at`
- `ssh_forwards`
  - `id` INTEGER PRIMARY KEY
  - `profile_id` TEXT FK
  - `name`, `kind`(L/R/D), `listen`, `dest`
- `ssh_jump`
  - `profile_id` TEXT PRIMARY KEY FK
  - `jump_profile_id` TEXT
- `secrets`
  - `secret_id` TEXT PRIMARY KEY
  - `kind` TEXT（password/token/passphrase等）
  - `label` TEXT
  - `ciphertext` BLOB
  - `nonce` BLOB
  - `created_at`, `updated_at`
- `cmdsets`
  - `cmdset_id` TEXT PRIMARY KEY
  - `name`, `vars_json`
- `cmdsteps`
  - `id` INTEGER PRIMARY KEY
  - `cmdset_id` TEXT FK
  - `ord` INTEGER
  - `cmd` TEXT
  - `timeout_ms` INTEGER
  - `on_error` TEXT（stop/continue）
  - `parser_spec` TEXT（raw/json/regex:<id>）
- `parsers`
  - `parser_id` TEXT PRIMARY KEY
  - `type` TEXT（regex）
  - `definition` TEXT（regex本体 + capture定義JSON等）
- `configsets`
  - `config_id` TEXT PRIMARY KEY
  - `name`, `hooks_cmdset_id` TEXT NULL
- `configfiles`
  - `id` INTEGER PRIMARY KEY
  - `config_id` TEXT FK
  - `src` TEXT（ローカルパス or テンプレID）
  - `dest` TEXT（`~/`解釈）
  - `mode` TEXT NULL
  - `when` TEXT（always/missing/changed）
- `op_logs`（監査用：Secret値なし）
  - `id` INTEGER PRIMARY KEY
  - `ts` INTEGER
  - `op` TEXT（connect/exec/run/push/pull/xfer/config_apply/doctor/test）
  - `profile_id` TEXT NULL
  - `client_used` TEXT NULL（実際に使ったssh等）
  - `ok` INTEGER
  - `exit_code` INTEGER NULL
  - `duration_ms` INTEGER NULL
  - `meta_json` TEXT（サイズ、転送方式、parser等）

### 2.4 マイグレーション
- `schema_version` を `settings` で持つか、`refinery` 等を導入して段階的に適用。

---

## 3. ID仕様（内部実装ルール）

### 3.1 正規化・検証
- すべてのID（profile/secret/cmdset/parser/config）は保存前に以下を通す：
  - 小文字化
  - 正規表現 `^[a-z0-9][a-z0-9_-]{2,63}$`
  - 予約語テーブルに含まれていたら拒否

### 3.2 自動生成
- `p_`/`s_`/`c_`/`r_`/`g_` など接頭辞を付け、base32（6〜8桁）生成。
- 生成衝突時は再生成。

---

## 4. Secret暗号設計（マスターパスワード対応）

### 4.1 方針（安全側）
- Secret（password/token/passphrase）は常に暗号化保存する。
- `td show` では表示しない。
- `td secret reveal` は **マスターパスワードが設定されている場合のみ可能**。
  - 未設定なら `set-master` を促して終了（安全側に倒す）。

### 4.2 暗号方式
- 対称暗号：XChaCha20-Poly1305（nonce 24B、AEAD）
- 鍵導出（マスターパスワード）：Argon2id
  - saltはDB `settings` に保存（例：`master_salt`）
  - パラメータは固定し、将来変更に備えて `master_kdf_params` を保存

### 4.3 鍵管理
- マスターパスワード設定時：
  - `master_salt` を生成して保存
  - ユーザ入力から Argon2id で `master_key` を導出
- Secret保存時：
  - `nonce` をランダム生成
  - `ciphertext = AEAD_Encrypt(master_key, nonce, plaintext, aad)`
  - AAD（追加認証データ）に `secret_id` と `kind` を入れて取り違え耐性を上げる

### 4.4 revealの表示制御（外部設計要件）
- `secret reveal` は以下を満たす：
  - マスターパスワード入力必須（stdinから非エコー入力）
  - 表示は最小限
    - 既定は「コピーのみ」でも可（要件次第）
    - 表示する場合は一定時間で自動マスク or 1回限り
  - ログにSecret値を絶対に残さない

---

## 5. 外部コマンド実行設計（SSH/SCP/SFTP/Telnet）

### 5.1 CommandSpec
- `exe: PathBuf`
- `args: Vec<OsString>`
- `env: Vec<(OsString, OsString)>`（基本空）
- `mask_rules`（ログ表示時に隠すトークン）

> 重要：シェルを介さず `Command` の args で渡す（インジェクション回避）。

### 5.2 クライアント解決
- 解決順：
  1) プロファイル `client_overrides`
  2) グローバル設定 `settings.client.*`
  3) PATHから探索（Windowsは`.exe`考慮）
- `td doctor` で探索結果と欠落を表示。

### 5.3 connect（対話）
- `Command` を `stdin/stdout/stderr` 継承で `spawn`。
- exit code を取得し `op_logs` へ記録。
- `danger_level=critical` なら二段階確認。

### 5.4 exec/run（非対話、SSHのみ）
- `Command.output()` で stdout/stderr を回収。
- timeout 対応：
  - `wait_timeout` 相当の実装（platform差を吸収するcrate採用）または `tokio` で管理（内部設計で確定）
- `--format json` のスキーマを必ず満たすように整形して返す：
  - `ok, exit_code, stdout, stderr, duration_ms, parsed`

---

## 6. SSH機能設計

### 6.1 接続
- 生成するコマンドは以下の概念を満たす：
  - `ssh user@host -p port`
  - `ProxyJump`：`-J jumpuser@jumphost:port`（jump_profile参照）
  - forward適用：`-L/-R/-D`
  - keepalive/timeout：`-o ServerAliveInterval=...` 等（必要なら）

### 6.2 `dest` の `~/` 解釈（ConfigSet/transfer）
- scp/sftpのリモートパスは `user@host:~/path` 形式を使い、ホーム展開をリモート側に任せる。
- plan/backup時のリモート操作は `ssh` で `sh -lc` を使って `~` を確実に解釈させる（ログにはコマンドをマスク/簡略化）。

---

## 7. Telnet/Serialのスコープ（v0.1実装線）

### 7.1 Telnet（v0.1）
- `connect` のみ。
- 対話自動化はしない。
- 初期送信文字列（任意）を1回送るだけは許容。
  - 実装：telnet起動後に短い遅延→指定文字列送出（ただしOS/クライアント差があるので最初は控えめ）

### 7.2 Serial（v0.1）
- `connect` のみ。
- 端末エミュレーションはしない（raw passthrough）。
- 実装案：
  - `serialport` crate でポートを開く
  - `crossterm` raw mode で stdin を読み、serialへ書く
  - serialからの受信を stdout へ書く
- 初期送信文字列（任意）を開始時に1回送る。

---

## 8. ファイル転送設計

### 8.1 push/pull（scp/sftp）
- `push`: `scp local remote` または `sftp` バッチ
- `pull`: `scp remote local` など
- progress表示は将来（v0.1は最低限でOK）

### 8.2 xfer（サーバ↔サーバ、ローカル中継）
- 1) `pull` を temp dir へ
- 2) `push` を dest へ
- tempは `directories::BaseDirs::temp_dir()` 相当を使用し、終了後削除。

### 8.3 FTP（安全装置）
- 設定 `allow_insecure_transfers=true` が無い限り ftp は拒否。
- 実行時に `--i-know-its-insecure` が無い限り拒否。
- ログに明示的に「insecure transfer」を記録。

---

## 9. ConfigSet（設定配布）設計

### 9.1 applyの手順（SSH前提）
ファイル1件ごとに以下を行う（`--plan` は実行せず差分判定だけ）：

1) dest解釈：
   - `~/` はリモートホーム
2) リモートの存在/ハッシュ確認（when=changedの場合）：
   - `ssh` で `test -f` と `sha256sum`（無ければmissing扱い）
3) バックアップ（--backup）：
   - `cp dest dest.bak.<timestamp>`（存在する場合のみ）
4) 転送：
   - 一旦 `dest.tmp.<timestamp>` に upload
5) 反映：
   - `mv tmp dest`
6) mode設定（指定があれば）：
   - `chmod <mode> dest`

> 重要：planは「何が変わるか」を出すだけで副作用ゼロ。

---

## 10. CommandSet / Parser 実装設計

### 10.1 CommandSet実行
- `run(profile, cmdset)` は `steps` を ord順に実行する。
- stepごとに：
  - timeout適用
  - exit_code非0時：
    - `on_error=stop` なら打ち切り
    - `continue` なら続行
  - parser適用して `parsed` を生成（stepごとにも保持）

### 10.2 Parser
- v0.1は以下のみ：
  - `raw`
  - `json`（stdoutがJSONとしてparseできた場合のみ `parsed` をJSON化）
  - `regex`（定義は `parsers.definition` に保持）
- regex抽出結果は `parsed` に統一フォーマットで格納：
  - `parsed` は JSON object または JSON array（parser定義で指定）

### 10.3 JSON出力スキーマ（固定）
- `exec/run` の `--format json` は必ず以下の形にする：

```json
{
  "ok": true,
  "exit_code": 0,
  "stdout": "...",
  "stderr": "...",
  "duration_ms": 1234,
  "parsed": {}
}
````

---

## 11. TUI（ratatui）設計

### 11.1 画面

* Profiles一覧（検索、type/tag/group/danger絞り込み）
* Detailsペイン（show相当、ただしSecret値は表示しない）
* Actions（connect/exec/run/push/pull/config apply）
* Secrets一覧（revealはマスターパスワード必須）

### 11.2 操作

* インクリメンタル検索（絞り込みが最優先機能）
* `danger_level=critical` は二段階確認
* 実行前に CommandSpec 表示（Secretはマスク）

---

## 12. ログ設計

* 永続ログ：SQLite `op_logs`（Secretなし）
* ファイルログ：tracingで `logs/teradock.log`（Secretなし、コマンドはマスク）
* すべての実行パスで「Secretがログに出ない」ことをテストする（後述）。

---

## 13. テスト方針（壊れやすい所を優先）

### 13.1 ユニットテスト

* ID正規化・予約語拒否
* export/importのスキーマ互換
* CommandSpec生成（ssh/scp/sftp/telnet）で「引数が正しい」「Secretが混入しない」
* Secret暗号：

  * encrypt→decrypt一致
  * AAD違いで復号失敗
* FTPガード：

  * 設定なしで拒否
  * 設定ありでもフラグなしで拒否

### 13.2 結合テスト（任意）

* `td doctor` が PATH差分を正しく検出する（OS別）
* `--format json` の出力スキーマ検証

### 13.3 手動試験項目（v0.1で必須）

* SSH connect/exec/run（実機 or ローカルVM）
* ConfigSet apply（backup/plan含む）
* TUI操作（検索/confirm/実行）

---

## 14. 配布（Windows/Linux）

* Windows：

  * Inno Setup でインストーラー生成（`td.exe`、設定ディレクトリ作成、アンインストール）
* Linux：

  * `cargo-deb` で `.deb`（Ubuntu想定）
  * 併せて `tar.gz` のポータブル配布（依存が少ない前提）

CI（GitHub Actions）：

* matrix：windows-latest / ubuntu-latest
* `cargo fmt`, `cargo clippy`, `cargo test`
* リリース成果物生成（tag pushで配布物作成）

---

## 15. 実装計画（フェーズ）

外部仕様の「優先順位」を実装順に落とす。

### Phase 0: リポジトリ整備（足場）

* workspace分割（core/cli/tui）
* エラー型・ログ基盤（tracing）
* 設定ディレクトリ決定（directories）
* SQLite導入 + migration基盤

成果物：

* `td --version` がWindows/Linuxで動く
* 空DB生成・マイグレーションが走る

### Phase 1: IDルール確定（仕様の核）

* profile_id/secret_id等の正規化・検証・予約語拒否
* 自動生成（base32）

成果物：

* ID周りのユニットテスト完備

### Phase 2: Profiles CRUD + list/show（Secret非表示）

* profilesテーブル操作
* `td add/edit/rm/list/show`
* `td show` で user平文、password表示なし

成果物：

* CLIでプロファイル管理が成立

### Phase 3: Secret暗号（マスターパスワード）

* `set-master`（仮）で master_salt生成・保存
* `secret add/edit/rm/list`
* `secret reveal`（master必須、ログ漏れ防止）

成果物：

* 暗号化保存でき、revealはmaster必須で動く

### Phase 4: doctor（OS差分吸収の核）

* `td doctor` 実装（ssh/scp/sftp/telnet検出、TUI可否）
* client override（global/profile）を設定に反映
* 実行ログに client_used を記録

成果物：

* 環境差による「動かない」を早期に潰せる

### Phase 5: 本番ガード（最優先機能 #1）

* danger_level=critical 二段階確認（CLI/TUI共通）
* 適用範囲：connect/exec/run/push/pull/xfer/config apply

成果物：

* critical操作が確実に止まる

### Phase 6: SSH connect（最初の価値）

* `td connect <profile_id>`（inherit stdio）
* jump / forward（最低限 -L/-D）
* last_used更新

成果物：

* SSH接続がプロファイル指定でできる

### Phase 7: SSH exec/run + JSON出力（核）

* `exec` 実装（timeout含む）
* `run` 実装（cmdset/steps）
* `--format json` スキーマ固定
* parser（raw/json/regex）実装

成果物：

* ツール連携可能なJSONが安定して出る

### Phase 8: 検索/絞り込み（最優先機能 #2）

* `list` の `--tag/--group/--query/--type/--danger` 実装
* TUIのインクリメンタル検索基盤

成果物：

* 運用での体感速度が出る

### Phase 9: 転送（scp/sftp）+ xfer（ローカル中継）

* push/pull（scp/sftp）
* xfer（temp中継）

成果物：

* 基本転送が成立（FTPは未対応でOK）

### Phase 10: ConfigSet（apply/backup/plan）

* configset登録
* apply（backup/plan/when）
* `~/` 解釈ルールの反映

成果物：

* 設定配布が安全に回る（planで事故回避）

### Phase 11: Import/Export（最優先機能 #3）

* `export --include-secrets=no|refs|yes`

  * v0.1は `refs` をまず安定させる
* `import` 実装（ID衝突時の挙動を決める：reject/rename）

成果物：

* バックアップとチーム共有の下地

### Phase 12: 鍵/agent優先（最優先機能 #4）

* SSH authの優先順位を実装に反映
* UI/ヘルプで誘導（passwordは最後の手段）

成果物：

* 運用の安全性が上がる

### Phase 13: 到達性テスト（最優先機能 #5）

* `td test`（DNS/TCP/任意でBatchMode）
* doctorと整合する出力（json対応）

成果物：

* 接続トラブルの切り分けが速くなる

### Phase 14: Telnet connect（v0.1範囲）

* `connect` のみ（doctorで存在チェック）
* 初期送信（任意、最小限）

### Phase 15: Serial connect（v0.1範囲）

* raw passthrough
* 初期送信（任意）

### Phase 16: FTP（安全装置付き）

* 設定 `allow_insecure_transfers=true` がないと拒否
* 実行フラグ `--i-know-its-insecure` がないと拒否
* ログにinsecureを明記

### Phase 17: TUI仕上げ（運用速度）

* Profiles/Secrets/Actions統合
* 二段階confirm、実行前コマンド表示（マスク）
* exec/run結果ビュー（stdout/stderr/parsed）

### Phase 18: 配布・CI

* Windows installer
* Linux deb/tar
* GitHub Actionsのリリース生成

---

## 16. リスクと対策（先に書いて潰す）

* OS差分でtelnetが無い：

  * `doctor` と client override で吸収。無ければ機能を明確に使えないと出す。
* Secret漏洩（最悪）：

  * ログ・dry-run・エラー経路すべてでSecret値を出さないテストを用意。
  * revealはmaster必須、表示最小限。
* Serialの入出力が崩れる：

  * v0.1はraw passthroughのみ、期待値を上げない。
* Config applyの事故：

  * `--plan` と `--backup` を標準動作に近づける（バックアップはデフォONでも良い）

---

## 17. Done条件（v0.1の最低ライン）

* Windows/Linuxでインストールまたは単体実行できる
* `doctor` が依存不足を正しく言える
* SSH：

  * connect / exec / run / forwards（最低限） / transfer（scp/sftp） / config apply（backup/plan）
* Secret：

  * 暗号化保存
  * showで非表示
  * master設定済みなら reveal可能（ログ漏れなし）
* TUI：

  * 検索してプロファイル選択→connect/exec/runができる
* Export/Import（refs）でバックアップ可能

---
# PROJECT_PLAN.md 追記 - 設定強化 / TUI操作強化 / SSH転送・SSH-Agentサポート（v0.1追補）

本追記は「PROJECT_PLAN.md - TeraDock（内部設計 + 実装計画）v0.1」に、追加要件（設定機能・TUI・SSH転送設定・SSH-Agentサポート）を **追記統合**するための差分仕様である。  
このファイル単体でも読めるよう、追加分の要件・内部設計・タスク・Done条件をまとめる。

---

## A. 追加ゴール（この追記で増える価値）

- 設定が増えても「何が効いてるか」迷子にならない（Resolved View）。
- `td config set` 実行時に **設定可能な値（許容値）をヘルプ等で表示**できる（型付き設定 + スキーマ）。
- TUIは「速さ」と「事故防止」を最優先にし、普段の5手がキーボードのみで完結する。
- SSH転送（forward）の定義ルールを固定し、必要に応じて「トンネル専用」も扱える（セッション管理）。
- SSH-Agent利用は “動いてるか/鍵が入ってるか” を可視化し、必要な範囲で支援コマンドを用意する。
- 既存の危険設定（FTP等）に加え、known_hosts/StrictHostKeyChecking等も安全側で扱う。

---

## B. 外部I/F 追加・更新（コマンド仕様の追記）

### B-1. 設定（型付き・許容値ヘルプ）
- `td config get <key> [--resolved] [--format text|json]`
- `td config set <key> <value> [--scope global|env:<name>|profile:<id>]`
- `td config keys [--query <q>]`  
  設定キー一覧（説明付き）
- `td config schema [<key>] [--format text|json]`  
  設定スキーマ（型・許容値・デフォルト・例）を表示
- `td config set <key> --help`  
  **そのキーで設定可能な値（許容値）を表示**（本要件の主対象）

> 要件：`set` でエラーになった場合も、許容値と例を必ず提示する。

### B-2. 環境プリセット（設定プロファイル）
- `td env list`
- `td env use <name>`
- `td env show <name>`
- `td env set <name>.<key> <value>`（内部的には scope=env）

### B-3. Resolved View（最終適用値）
- CLI: `td show <profile_id> --resolved`
- TUI: 詳細ペインで「生値 + グローバル + env + profile override + コマンド一時指定」を合成した **最終適用値**を表示

### B-4. SSH-Agent支援
- `td agent status [--format text|json]`
- `td agent list`（可能なら `ssh-add -l` を要約）
- `td agent add <key_path>`（勝手にやらない／必ずユーザ操作）
- `td agent clear`（危険操作：二段階確認）

### B-5. Forward / tunnel / sessions
- `td tunnel start <profile_id> [--forward <name>...]`
- `td tunnel status [--format text|json]`
- `td tunnel stop <session_id>`
- `td sessions list`
- `td sessions stop <session_id>`

---

## C. 内部設計 追加（重要部分のみ）

### C-1. 設定システム（Settings Registry + 型 + スキーマ）

#### C-1-1. 設定のスコープと優先順位
- スコープ：
  - `global`
  - `env:<name>`
  - `profile:<id>`
  - `command`（一時オプション：CLI引数/TUI実行時）
- 優先順位：
  - `command` > `profile` > `env` > `global`

#### C-1-2. 設定レジストリ（コンパイル時定義）
- `SettingKey` を列挙し、各キーに以下を持たせる：
  - `key`（例：`ssh.use_agent`）
  - `type`（bool/int/enum/duration/path/string）
  - `default`
  - `description`
  - `allowed_values`（enumの場合は必須、範囲制約もここで）
  - `examples`（1〜3個）
  - `dangerous` フラグ（trueの場合は二重ロック対象）

> ここが「`td config set --help` で許容値を表示する」ための根幹。

#### C-1-3. 設定保存
- DBテーブル（既存 `settings`）を拡張してスコープを持つ：
  - `settings(scope TEXT, key TEXT, value TEXT, PRIMARY KEY(scope,key))`
- `env` 管理：
  - `current_env` を `settings(global, "env.current", "<name>")` で保持
  - `env` 自体は scope=`env:<name>` に格納

#### C-1-4. バリデーション・エラーUX
- `set` は必ず型変換と制約チェックを行う。
- 失敗時は以下を出す（text/json両対応）：
  - エラー理由（例：enumに存在しない）
  - 許容値一覧（allowed_values）
  - 例（examples）
  - 参照：`td config schema <key>`

#### C-1-5. Resolved View 実装
- `ResolvedConfig` を生成する関数を core に置く：
  - inputs: global/env/profile/command overrides
  - outputs: 最終適用値（どのスコープが勝ったかの由来も保持）

---

### C-2. SSH forward（仕様固定 + 健全性チェック）

#### C-2-1. forward のルール
- forward は `name` 必須かつ profile内でユニーク
- `listen` が portのみなら `127.0.0.1:<port>` に解釈（安全側）
- `dest` の host省略は禁止（明示必須）
- `D` は `dest` なし

#### C-2-2. forward の既定有効
- `enabled_by_default` を持たせる（connect時に自動適用できる）

#### C-2-3. 健全性チェック（doctor/test連携）
- listenポート使用中（ローカル側）を検知して警告
- Linuxの低番ポート権限警告
- jump経由の場合の到達性ヒント（可能な範囲）

---

### C-3. セッション管理（tunnel/sessions）
- `sessions` テーブル（または `op_logs` 拡張）に以下を保持：
  - `session_id`（自動生成）
  - `kind`（connect/tunnel）
  - `profile_id`
  - `pid`（可能なら）
  - `started_at`
  - `forwards_applied[]`
- stop は pid kill を基本（OS差分注意）
- status は “生存確認” を行い、死んでるセッションは掃除する

---

### C-4. SSH-Agentサポート

#### C-4-1. 対象を明確化
- v0.1は OpenSSH の `ssh-agent` を対象
- 将来：
  - Pageant / 1Password / Bitwarden などは “対象外” と明記し、後で拡張

#### C-4-2. `agent status` の中身
- `SSH_AUTH_SOCK` の有無（Linux）
- Windowsの場合は `ssh-agent` サービス/プロセスの検知（可能な範囲）
- `ssh-add -l` を実行できれば鍵本数等を要約

#### C-4-3. known_hosts / StrictHostKeyChecking
- デフォルトは安全側（OpenSSHの通常挙動に準拠）
- これを緩める設定は “危険設定” として二重ロック対象にする
  - 設定で許可
  - 実行時フラグで許可
  - ログに insecure を明記

---

### C-5. TUI操作強化（速度 + 事故防止）

#### C-5-1. キーバインド（案）
- `/` 検索
- `Enter` connect（デフォ）
- `e` exec
- `r` run（cmdset選択）
- `t` transfer（push/pull/xfer選択）
- `c` config apply
- `d` details（Resolved View）
- `?` help
- `Space` マーク（複数選択）
- `R` 一括 run（マーク対象にcmdset実行）

#### C-5-2. 二段階確認（critical）
- TUIは “Enter連打事故” が起きるため、criticalは文字入力確認を採用（例：profile_idをタイプ）
- “一定時間の緩和” は設定で可能（例：10分）だがデフォはOFF推奨

#### C-5-3. 実行キュー（複数対象）
- マークした複数プロファイルに `run` を実行
- 結果サマリ（成功/失敗/失敗理由）を一覧化
- execは将来（v0.1はrun優先）

---

## D. 既存実装計画への追記（フェーズ追加・修正）

既存の Phase 0〜18 に以下を追記する（順序は外部設計の優先順位に合わせる）。

### 追加 Phase S1: Settings Registry + `config set` 許容値ヘルプ（最優先の土台）
- SettingKeyレジストリ（型、default、allowed_values、examples、dangerous）
- `td config schema`, `td config keys`
- `td config set <key> --help` で許容値表示
- バリデーションとエラー時の許容値提示
- scope（global/env/profile）保存と解決（ResolvedConfig）

完了条件：
- 代表キーで `set --help` が許容値/例を表示できる
- 不正値入力時に許容値が必ず提示される
- `--resolved` で最終値が出る

### 追加 Phase S2: envプリセット（work/home等）
- `env list/use/show/set`
- current_envの切替
- ResolvedConfigへ統合

完了条件：
- env切替でsshクライアント等が変わる

### Phase 4（doctor）を拡張
- 依存コマンド検出に加え
  - 設定矛盾検出（use_agent=true だがagent不在、鍵パス不存在等）
  - known_hosts/StrictHostKeyCheckingの危険設定警告
- json出力対応

完了条件：
- “動かない理由” が doctor で分かる

### Phase 6（SSH connect）を拡張：forwardルール固定 + forward健全性
- forward name必須/ユニークチェック
- listen portのみの解釈固定
- port競合警告（可能なら）

### 追加 Phase S3: tunnel/sessions（必要性が出る前に最小実装）
- `tunnel start/stop/status`
- `sessions list/stop`
- PID管理（可能な範囲）
- ログにsession_idを残す

完了条件：
- トンネルを張って状況が追える

### 追加 Phase S4: SSH-Agent支援
- `agent status/list/add/clear`
- doctor連携
- プロファイル単位 `use_agent=true/false/auto` を ResolvedConfigで決定

完了条件：
- agentが“動いてるか/鍵があるか”が可視化される

### Phase 17（TUI）を拡張：操作系・Resolved View・キュー実行
- キーバインド採用
- Resolved View表示
- critical文字入力確認
- 複数選択→runの一括実行と結果サマリ

完了条件：
- 普段の運用がTUIで完結し、事故が減る

---

## E. 追加設定キー（例：スキーマに載せる候補）

以下は Settings Registry に登録する候補（例）。許容値表示の対象にもなる。

- `ssh.client`（path|string）
- `scp.client`（path|string）
- `sftp.client`（path|string）
- `telnet.client`（path|string）
- `ssh.use_agent`（bool）
- `ssh.agent.mode`（enum: `openssh`, `custom`）
- `ssh.strict_host_key_checking`（enum: `default`, `yes`, `no`） ※危険設定（二重ロック対象）
- `transfer.default_via`（enum: `scp`, `sftp`）
- `transfer.allow_insecure_transfers`（bool） ※危険設定（二重ロック対象）
- `danger.confirm_mode`（enum: `double_enter`, `type_profile_id`）
- `danger.relax_window_minutes`（int, default 0）
- `ui.default_action`（enum: `connect`, `details`）
- `ui.search_mode`（enum: `contains`, `prefix`, `fuzzy`）

---

## F. Done条件（追記分）

- `td config set <key> --help` が **許容値・デフォルト・例** を表示できる
- 不正値指定時は必ず許容値を提示する
- `td show <profile> --resolved` と TUI details で最終適用値が確認できる
- `td doctor` が依存不足 + 設定矛盾（agent等）を検知できる
- forwardの解釈ルールが固定され、衝突や危険を警告できる
- `td agent status` で “agentと鍵” の状態が見える
- TUIで検索→connect/exec/run/transfer/config apply が高速に行え、criticalは文字入力確認で事故が減る
- ftp等の危険設定は設定＋実行フラグの二重ロックで防げる

---

# TeraDock 外部設計（External Design） v0.3

## 1. 目的

TeraDock は、Windows / Linux の両方で動作するターミナル中心のツールとして以下を提供する。

- 接続先（サーバ/機器）のプロファイル管理
- ユニーク識別子指定による接続（SSH/Telnet/Serial）
- SSH転送（ポートフォワード）
- 非対話のリモートコマンド実行（SSHのみ）
- コマンドセット登録と実行、出力の構造化（raw/regex/json）
- ファイル転送（scp/sftpを基本、ftpは安全装置付き）
- 設定ファイル配布（ConfigSet）
- シークレット（パスワード等）の暗号化保存と参照管理
- CLI と TUI（CUIだがグラフィカルなメニュー操作モード）

---

## 2. 対応OS・実行方針

### 2.1 対応OS
- Windows 10/11
- Linux（ディストリ非依存、主にUbuntu系を想定）

### 2.2 実行方針（接続・転送）
- TeraDock はターミナルエミュレータやプロトコルの自前実装は行わない。
- 原則として OS にある外部コマンドを起動して接続・転送を実現する。
  - SSH: `ssh`
  - SCP/SFTP: `scp`, `sftp`
  - Telnet: `telnet`（環境差あり）
  - Serial: 内蔵実装または外部コマンド（実装方針は内部設計で確定）

---

## 3. 依存チェック・差分吸収（OS差分対策）

### 3.1 `td doctor`
- `td doctor` は、必要コマンド・環境が揃っているか検査し、欠けていれば代替候補と設定誘導を提示する。
- 検査項目（例）
  - `ssh`, `scp`, `sftp` の存在
  - `telnet` の存在
  - TUI実行可能性（端末機能）
  - 設定保存先の書き込み可否
- `doctor` は以下を出力できる：
  - テキスト（人間向け）
  - JSON（自動化向け）

### 3.2 クライアント指定（差し替え）
- グローバル設定およびプロファイルごとに、使用するクライアントコマンドを上書きできる。
  - 例：`client.ssh = "ssh"`（デフォルト）
  - 例：`client.ssh = "C:\\path\\to\\ssh.exe"`
- 実行ログには「どのクライアントを使ったか」を記録する。

---

## 4. 識別子仕様

### 4.1 `profile_id`（ServerProfile識別子）
- `profile_id` はユニークであること。
- 生成方法：
  1) ユーザ定義
  2) 自動生成
- 正規化：
  - 保存時に小文字化する（ユーザ入力でも内部保存は小文字）
- 形式（正規表現）：
  - `[a-z0-9][a-z0-9_-]{2,63}`
- 予約語禁止：
  - コマンド名やサブコマンドと衝突する ID を禁止（例：`list`, `add`, `rm`, `connect`, `exec`, `run`, `doctor`, `secret` 等）
- 自動生成形式：
  - `p_<6〜8桁 base32>`（例：`p_k3a9m1`）
- 表示用の「番号」は入力仕様にしない（必要なら `list` の表示インデックスのみ）。

### 4.2 `secret_id`（Secret識別子）
- `secret_id` はユニークであること。
- 形式は `profile_id` と同様のルールに従う。
- 自動生成形式：
  - `s_<6〜8桁 base32>`

### 4.3 `cmdset_id`, `parser_id`, `config_id`
- それぞれユニークであること。
- 形式・正規化・予約語禁止は `profile_id` に準拠。

---

## 5. シークレット仕様

### 5.1 Secret対象
- 暗号化保存する対象：
  - パスワード、トークン、鍵パスフレーズ等
- 暗号化しない対象：
  - ユーザー名（平文保存）

### 5.2 `td show` の表示仕様
- user は平文表示してよい。
- パスワード等のSecret値は **絶対に表示しない**。

### 5.3 TeraDock用マスターパスワード（確認用）
- TeraDock は「マスターパスワード（TeraDock Password）」を設定できる。
- マスターパスワードが設定されている場合に限り、ユーザが明示的に要求したときのみ、保存済みSecret値の確認を許可できる。
  - コマンド例：`td secret reveal <secret_id>`（仮）
- `reveal` は以下を満たす：
  - マスターパスワード入力を必須
  - 表示は最小限（一定時間で自動マスク、または明示的にコピーのみ）
  - 実行ログにSecret値を残さない

### 5.4 ログ・dry-runでの扱い
- ログ、`--dry-run`、エラー出力に Secret値を出さない（マスクする）。
- Secretが混入しうる引数（例：`sshpass` 等）を使用する場合は仕様として禁止または強警告を行う。

---

## 6. 接続方式とスコープ

### 6.1 対応方式
- SSH
- Telnet
- Serial

### 6.2 方式別スコープ（段階的対応）

#### SSH
- `connect`：対話セッション
- `exec/run`：非対話コマンド実行（結果回収）
- `forward`：L/R/D の適用

#### Telnet
- v0.1：`connect`（対話セッション）のみ
- v0.1：対話自動化（expect風）は対応しない
- v0.1：任意で「初期送信文字列」を許す（例：改行送信、簡単な初期コマンド）
- `exec/run` は将来対応枠（初期は対象外）

#### Serial
- v0.1：`connect`（対話セッション）のみ
- v0.1：対話自動化は対応しない
- v0.1：任意で「初期送信文字列」を許す
- `exec/run` は将来対応枠（初期は対象外）

---

## 7. ファイル転送仕様

### 7.1 転送方式
- 優先：`scp` / `sftp`
- FTP：オプション（安全装置付き）

### 7.2 FTPの安全装置
- FTP はデフォルト無効。
- 有効化には明示設定が必要：
  - 例：`td config set allow_insecure_transfers=true`
- 実行時にも明示フラグが必要（例：`--i-know-its-insecure`）

### 7.3 コマンド
- ローカル ↔ サーバ
  - `td push <profile_id> <local_path> <remote_path> [--via scp|sftp|ftp]`
  - `td pull <profile_id> <remote_path> <local_path> [--via scp|sftp|ftp]`
- サーバ ↔ サーバ（ローカル中継）
  - `td xfer <src_profile_id> <src_path> <dst_profile_id> <dst_path> [--via scp|sftp|ftp]`
  - 初期実装は pull→push のローカル中継で成立させる。

---

## 8. 設定配布（ConfigSet）

### 8.1 目的
- `.inputrc`, `.bashrc`, `.profile` 等の設定ファイルを所定のサーバへ配布する。

### 8.2 配布の解釈ルール
- `dest` の解釈：
  - `~/` はリモート側ホームディレクトリを指す
  - それ以外は絶対パス扱い

### 8.3 コマンド
- 登録：
  - `td config add <config_id>`
- 適用：
  - `td config apply <profile_id> <config_id> [--dry-run] [--backup] [--plan]`
- オプション：
  - `--backup`：既存ファイル退避（標準搭載）
  - `--plan`：差分予定を表示（より賢い dry-run）

---

## 9. コマンド実行・パーサー仕様

### 9.1 実行モード
- `exec`：単一コマンドを非対話で実行（SSHのみ）
  - `td exec <profile_id> "<cmd>" [--timeout 10s] [--parser raw|json|regex:<parser_id>] [--format text|json]`
- `run`：コマンドセットを実行（SSHのみ）
  - `td run <profile_id> <cmdset_id> [--format text|json]`

### 9.2 パーサー種別
- `raw`：そのまま表示
- `regex`：抽出（key/valueまたは配列）
- `json`：stdoutがJSONの場合にJSONとして整形・返却

### 9.3 `--format json` の戻り値スキーマ
- `exec/run` の JSON 出力は以下を必ず含む：

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

* `parsed` は parser が指定された場合に設定される（未指定なら `null` または空）。

### 9.4 CommandSet

* 登録：

  * `td cmdset add <cmdset_id>`
* 実行：

  * `td run <profile_id> <cmdset_id>`
* Step仕様：

  * `cmd`（文字列）
  * `timeout`
  * `on_error: stop|continue`
  * `parser: raw|json|regex:<parser_id>`

---

## 10. SSH転送（ポートフォワード）

* SSHプロファイルは複数の forward を保持できる。
* 種別：

  * Local Forward（-L）
  * Remote Forward（-R）
  * Dynamic Forward（-D）
* `connect` 時に forward を選択して適用できる：

  * `td connect <profile_id> --with-forward <forward_name>...`

---

## 11. TUIモード

* `td ui` で TUI を起動する。
* 可能な操作：

  * Profilesの検索/絞り込み
  * connect（方式に応じて起動）
  * SSHの場合：exec/run、push/pull、config apply
* 実行前に「実際に走るコマンド」を確認できる（Secretはマスク）。
* `danger_level=critical` は二段階確認。

---

## 12. 本番ガード・到達性テスト・検索・export/import・鍵/agent優先（採用）

### 12.1 優先順位（実装順）

1. 本番ガード（danger_level）
2. 検索/絞り込み（list/TUI）
3. Import/Export（まずはSecret参照のみ）
4. 鍵/agent優先（仕様とUIで誘導）
5. 到達性テスト（doctor/testへ統合）

### 12.2 本番ガード

* `danger_level=critical` のプロファイルに対して以下は確認必須：

  * `connect`, `exec`, `run`, `push`, `pull`, `xfer`, `config apply`
* 確認文には `profile_id`, `host`, `type`, `group` を必ず含める。

### 12.3 到達性テスト

* `td test <profile_id>`

  * DNS解決
  * TCP接続（host:port）
  * SSHは任意で認証可否も判定（BatchMode等）

### 12.4 検索/絞り込み

* `td list --tag <t> --group <g> --query <q> --type <t>`
* TUI内でインクリメンタル検索。

### 12.5 Export/Import

* `td export --include-secrets=no|refs|yes`

  * `no`：Secret含めない
  * `refs`：Secret ID参照のみ（優先）
  * `yes`：暗号化Secretも含む（移行用途）
* `td import <file>`

### 12.6 鍵/agent優先（SSH認証）

* SSHの推奨優先順位：

  1. agent
  2. key_path
  3. password（最後の手段）

---

## 13. ログ仕様

* ログは Secret値を絶対に含まない。
* 実行ログに含めるべき情報：

  * 実行種別（connect/exec/run/push/pull/xfer/config apply）
  * profile_id / type / host:port
  * 実際に使用したクライアント（ssh/scp等のパスまたはコマンド名）
  * 結果（ok/exit_code/duration）
* `--dry-run` はコマンドを表示するが Secretはマスクする。

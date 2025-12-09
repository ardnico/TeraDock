# PLAN.md - TeraDock (SSH 直接版)

## 0. 概要

TeraDock を「Tera Term 専用ランチャ」から拡張し、  
**Windows 標準の OpenSSH（ssh.exe）や Windows Terminal を使って SSH 接続する汎用 SSH ランチャ**として再定義する。

- 実行環境: Windows 10/11
- 依存ツール:
  - OpenSSH for Windows (`ssh.exe`)  … できれば OS 標準
  - 任意: Windows Terminal (`wt.exe`) が入っていれば優先利用
- 配布形態: インストーラー付き Windows アプリ
- 役割:
  - 接続プロファイル管理
  - コマンドライン生成
  - 安全装置付きの SSH 接続起動

**Tera Term は一切前提としない。**

---

## 1. ゴール / アンチゴール

### 1.1 ゴール

- プロファイルを選ぶだけで **`ssh user@host -p port`** を実行できるランチャにする。
- Windows Terminal や既定コンソール上で SSH セッションを開く。
- `dev / stg / prod` などの環境ごとにプロファイル管理・色分け・安全確認を行う。
- TeraDock 自身は **SSH プロセスの起動と管理に専念し、ターミナルエミュレータは外部ツールに任せる。**

### 1.2 アンチゴール

- SSH プロトコルの自前実装（libssh2 直叩きなど）はしない（トラブルの沼）。
- ターミナルエミュレータ（VT100/ANSI エスケープの解釈など）を自前実装しない。
- SFTP / SCP GUI クライアントを内包しない。
- 多プラットフォーム対応（Linux/macOS）は scope 外。

---

## 2. 想定構成（バックエンド切り替え）

TeraDock の役割を「**SSH クライアントの起動フロントエンド**」と定義し直す。  
接続先プロファイルごと、またはアプリ全体の設定で、以下のような「クライアント種別」を選べるようにする。

- `ClientKind::WindowsTerminalSsh`
  - `wt.exe` を起動し、その中で `ssh user@host -p 22` を実行
- `ClientKind::PlainSsh`
  - 既定コンソール（`cmd.exe` / `powershell.exe`）から `ssh ...` を起動
- （将来拡張）`ClientKind::TeraTerm`, `ClientKind::PuTTY` などを追加する余地は残すが、**今回のプランでは使わない。**

---

## 3. 技術選定

### 3.1 言語 / ライブラリ

- 言語: Rust（現状の TeraDock と同じ）
- 構成: `core` / `gui` / `cli` のワークスペース構成は維持
- プロセス起動: `std::process::Command`
- 設定ファイル: TOML (`serde` + `toml`)
- GUI: `egui` + `eframe`（既存の TeraDock 設計を流用）
- ロギング: `tracing` + `tracing-subscriber`
- インストーラー: Inno Setup（現行の `installer` ディレクトリを流用・修正）

### 3.2 SSH クライアント

- **優先**: Windows 標準の OpenSSH (`ssh.exe`)
- 任意: Windows Terminal (`wt.exe`) がある場合はそれを使って新規タブで起動

理由：

- OpenSSH は実績充分で、鍵周り・暗号スイートなどを自前で追随しなくて済む。
- TeraDock は「プロファイル管理＋コマンド生成」に集中できる。
- ターミナルの描画や入力処理は Windows Terminal / コンソールに全振り。

---

## 4. 全体アーキテクチャ（SSH 直接版）

### 4.1 コンポーネント

- `core`:
  - プロファイル管理
  - 接続コマンド生成（`ssh.exe` / `wt.exe` 用）
  - バックエンド種別 (`ClientKind`) 判定
  - ログ記録
- `gui`:
  - プロファイル一覧 UI
  - 検索・タグ・グループ・危険フラグ
  - 「接続」ボタン押下 → `core` に接続要求
- `cli`:
  - `teradock list`
  - `teradock connect <profile-id>`
- `installer`:
  - `gui.exe` / `cli.exe` をまとめる Inno Setup

### 4.2 プロファイルモデル（SSH 想定）

```rust
enum ClientKind {
    WindowsTerminalSsh,
    PlainSsh,
    // TeraTerm, Putty などは将来追加用
}

struct Profile {
    id: String,
    name: String,
    host: String,
    port: u16,
    user: Option<String>,
    group: Option<String>,         // dev/stg/prod...
    tags: Vec<String>,
    danger_level: DangerLevel,     // Normal/Warn/Critical
    client_kind: ClientKind,
    pinned: bool,
    last_used_at: Option<DateTime<Utc>>,
    // 省略：色設定などは現状と同様
}
````

---

## 5. 機能一覧（SSH 直接版）

### 5.1 P0（必須）

* プロファイル管理（追加/編集/削除/保存）
* プロファイル検索・フィルタ・タグ
* dev/stg/prod グルーピング & 色分け
* 危険接続（`prod` など）時の確認ダイアログ
* 接続コマンド生成:

  * `WindowsTerminalSsh`:

    * `wt.exe new-tab ssh user@host -p 22`
  * `PlainSsh`:

    * `cmd.exe /c start "" ssh user@host -p 22`
* 接続履歴ログ：

  * datetime, profile_id, host, user, client_kind, result
* CLI:

  * `teradock list`
  * `teradock connect <profile-id>`

### 5.2 P1（余裕が出てきたら）

* Pre/Post Hook（`ssh` 起動前に VPN チェックなど）
* プロファイル Import/Export (TOML/JSON)
* プロファイル継承（共通設定の親を作る）
* Windows Terminal のプロファイル名指定（`wt.exe -p "MyProfile" ssh ...`）

---

## 6. フェーズ分け

### フェーズ 0: 既存コードの棚卸し

* 現行 TeraDock の `core` / `gui` / `cli` 構成を確認。
* `ttermpro.exe` 前提になっている箇所を洗い出し、**「クライアント依存ロジック」を 1 箇所に集約**するためのインターフェースを定義。

完了条件:

* `core` に `ClientKind` と `build_command(&Profile)` 的な関数の骨組みが入る。
* 既存の TeraTerm 用コマンド生成は暫定的に `ClientKind::TeraTerm` 実装として分離（将来的に消してもいい）。

---

### フェーズ 1: SSH 直接版バックエンドの実装

* `ClientKind::WindowsTerminalSsh` / `ClientKind::PlainSsh` を実装。
* `build_command(profile: &Profile) -> CommandSpec` を実装：

  * `CommandSpec` は `.exe パス + 引数 Vec<String>` を持つ構造体。
* 実際の起動は `std::process::Command` に委譲。

完了条件:

* `core` のユニットテストで、`Profile` から期待される `CommandSpec` が生成される。

---

### フェーズ 2: CLI 統合

* `cli` crate で `teradock connect <id>` を実装。
* 指定プロファイルを読み込み → `build_command` → `Command::spawn()` → エラー時は標準エラーにメッセージ。

完了条件:

* コマンドプロンプトから `teradock connect dev-01` 実行で Windows Terminal or ssh が立ち上がり、接続できる。

---

### フェーズ 3: GUI 統合

* `gui` crate で:

  * プロファイル編集 UI に `ClientKind` を設定するコンボボックスを追加。
  * 「接続ボタン」押下で `core` の接続 API を叩く。
  * 危険接続の確認ダイアログ実装。
* 既存の TeraTerm 前提の UI 文字列・設定項目を削るか、「クライアント種別に応じて表示切替」。

完了条件:

* GUI から P0 機能一式が使える。
* TeraTerm がインストールされていない環境でも、OpenSSH と Windows Terminal だけで動作する。

---

### フェーズ 4: インストーラー更新

* Inno Setup の `setup.iss` を更新:

  * 説明文から「Tera Term 前提」の文言を削除。
  * 必須コンポーネントとして「OpenSSH for Windows」の存在をチェック（存在しない場合は警告メッセージ）。
* `dist/` にビルド＋インストーラ生成をまとめたバッチ/PS スクリプトを追加。

完了条件:

* クリーンな Windows 環境に対し `setup.exe` → TeraDock インストール → SSH 接続が行える。

---

## 7. リスクと対策

* **OpenSSH 未インストール環境**

  * 対策: 起動時にパス検出し、見つからなければ「設定画面で ssh.exe のパスを指定させる」＋ GUI 上で明示的に警告。
* **Windows Terminal 未インストール**

  * 対策: `WindowsTerminalSsh` が使えない場合は自動的に `PlainSsh` にフォールバック。
* **社内ポリシーで SSH クライアントが固定されている場合**

  * 対策: 将来的に `ClientKind::Custom` を追加し、任意の `.exe` とテンプレを設定できる余地を残す。

---

## 8. やらないこと（SSH 直接版）

* Tera Term 特有のマクロ機能の代替は提供しない。
* ウィンドウ内に独自のターミナルを埋め込まない（egui 内ターミナルは scope 外）。
* SFTP/SCP やファイル転送 UI は別途ツール（WinSCP など）に任せる。

---

## 9. まとめ

* **SSH 接続自体に Tera Term は不要**なので、TeraDock を「SSH クライアントランチャ & プロファイルマネージャ」として再設計する。
* 技術的には **外部クライアントとして OpenSSH (`ssh.exe`) と Windows Terminal を叩く形が、実装コストと安全性のバランスが最良。**
* 既存の Rust ベースの構成を生かしつつ、`ClientKind` 抽象を入れてバックエンド差し替えできる設計にするのが現実的な落とし所。

# PLAN.md

## 0. 概要

Windows 上で Tera Term（ttermpro.exe）を起動するためのランチャアプリケーションを実装する。  
主目的は「**よく使う接続先に素早く・安全に・ミスなく接続するための入口**」を提供すること。

- 実行環境: Windows 10/11
- 配布形態: 単一インストーラー（.exe）で配布
- 実行形式: GUI ランチャ + CLI モード両対応

---

## 1. ゴール / アンチゴール

### 1.1 ゴール

- Tera Term をコマンドラインで起動する処理をラップし、**プロファイル選択だけで接続**できるようにする。
- 接続先プロファイル（環境・タグ・色・マクロ等）を管理できる UI を提供する。
- **本番環境など危険な接続に対して安全装置を提供**し、操作ミスを減らす。
- Windows 向けに **インストーラーを提供**し、非エンジニアでも導入可能な形にする。
- CLI からも同じプロファイルを使い回しできるようにし、バッチや他ツールからも利用可能にする。

### 1.2 アンチゴール（やらないこと）

- SSH クライアントそのものの実装（= Tera Term を置き換えること）はしない。
- SSH 鍵管理ツールや VPN クライアントなどの「周辺ツール」本体は作らない（必要ならフックで呼び出す）。
- 多プラットフォーム（Linux / macOS）対応は scope 外（Windows 専用で割り切る）。
- 高機能な設定同期（クラウド・オンライン同期など）は v1.0 ではやらない。
- 自動アップデート機構は初期バージョンでは対応しない（後続フェーズ候補）。

---

## 2. 想定環境 / 前提条件

- ユーザーが Windows 10/11 上に Tera Term（ttermpro.exe）をすでにインストール済みであること。
- 権限周りは標準ユーザーで利用可能（インストール時に管理者権限が必要な場合あり）。
- プロファイル定義はローカルファイル（TOML）で管理し、Git 等でバージョン管理可能な形を想定。
- オフライン環境でも問題なく動作すること（外部ネット依存なし）。

---

## 3. 技術選定

### 3.1 言語・フレームワーク

- **アプリ本体 & CLI & ロジック**: Rust (stable 1.8x 付近)
- **GUI**:  
  - `egui` + `eframe`（ネイティブウィンドウ / WebView 依存なし）
- **設定ファイルフォーマット**: TOML
  - ライブラリ: `toml`, `serde`
- **ロギング**: `tracing` + `tracing-subscriber`
- **ビルド / 配布**:
  - ビルド: `cargo`（Windows x86_64 / aarch64 対応）
  - インストーラー: **Inno Setup**（.iss スクリプトで exe, ショートカット, アンインストーラを生成）

#### Rust + egui を選ぶ理由

- 単一バイナリで配布しやすく、ランタイム依存が少ない（.NET ランタイム不要）。
- CLI と GUI を **同一コードベースのコアロジック**から呼び出せる。
- egui は比較的シンプルな GUI を素早く構築でき、Windows でも安定して動く。
- ユーザーが Rust に慣れているので、メンテしやすい。

#### 代替案（あえて書いておく）

- C# + WPF / WinForms + WiX / Squirrel など
  - Windows ネイティブ感は高いが、.NET ランタイム依存や将来の移植性を考えると今回は採用しない。

---

## 4. 全体アーキテクチャ

### 4.1 コンポーネント構成

- `core`（ライブラリ crate）
  - プロファイル管理
  - コマンドライン生成ロジック
  - ログ/履歴保存
  - 危険接続判定
- `cli`（bin crate）
  - サブコマンド: `list`, `connect`, `export`, `import` etc.
- `gui`（bin crate）
  - egui ベースのランチャ UI
- `installer`（ツールではなく Inno Setup 設定ファイル）
  - `setup.iss` により `gui.exe` / `cli.exe` / 設定フォルダ等をまとめてインストール

### 4.2 ディレクトリ構成（案）

```text
project-root/
  Cargo.toml
  crates/
    core/
      src/...
    cli/
      src/main.rs
    gui/
      src/main.rs
  config/
    default_profiles.toml
  installer/
    setup.iss      # Inno Setup スクリプト
  dist/
    # ビルド成果物、インストーラ出力先
  docs/
    PLAN.md
    AGENTS.md (必要なら後で)
````

---

## 5. 機能一覧と優先度

### 5.1 P0（MVP 必須）

1. **プロファイル管理（ホスト一覧）**

   * フィールド例：

     * `id`, `name`, `host`, `port`, `protocol`(ssh/telnet), `user`
     * `group`（dev/stg/prod etc.）
     * `tags`（文字列リスト）
     * `danger_level`（normal/warn/critical）
     * `macro_path`（任意）
     * `window_color` / `title_suffix` など
   * 操作:

     * 追加 / 編集 / 削除
     * 永続化: `profiles.toml`

2. **コマンドライン自動生成 & 起動**

   * Tera Term 実行ファイルパス設定（手動指定 + 設定ファイル保存）
   * 例: `ttermpro.exe /ssh host:22 /auth=user /F=profile.ini /W="title"`
   * 生成されるコマンドラインをログに残す（デバッグオプション有り）

3. **プロファイル検索・フィルタ・タグ**

   * 名前 / ホスト / タグ / グループ でのフィルタリング
   * egui のテキストボックス + リストでインクリメンタルフィルタ

4. **最近使った接続 & ピン留め**

   * 接続実行時に「最終接続時刻」を記録し、ソート表示
   * プロファイルに「pinned: bool」フラグ

5. **環境ごとのグルーピング & 色分け**

   * `group` ごとにセクション表示
   * `danger_level` に応じて GUI 上の色を変える
   * Tera Term 起動時に `title_suffix` や背景色設定（可能な範囲で）

6. **危険接続の安全装置**

   * `danger_level == critical` のプロファイルは接続前に確認ダイアログ

     * 「本番環境です。本当に接続しますか？」「今日は二度と聞かない」チェックボックス

7. **ログ／履歴**

   * ファイル形式: ローテートするテキスト / JSON Lines / SQLite のいずれか

     * MVP では JSONL（1行1イベント）で十分
   * 記録内容: datetime, profile_id, host, user, result(success/fail), duration 等

8. **シンプル GUI ランチャ**

   * メイン画面要素:

     * 検索ボックス
     * プロファイル一覧（グループごと）
     * 接続ボタン
     * プロファイル編集ボタン
     * 設定ボタン（Tera Term パスなど）
   * UI は egui で構築

### 5.2 P1（完成度アップ用 / v0.2 以降）

1. 接続前後フック（Pre/Post Hook）
2. Tera Term マクロ (.ttl) の紐付け & オプション実行
3. テンプレート & プレースホルダ
4. プロファイル設定の Import/Export（TOML/JSON）
5. CLI モード:

   * `ttlaunch list`
   * `ttlaunch connect <profile-id>`
6. プロファイル階層構造 & 継承（共通設定親プロファイル）

### 5.3 P2（遊び・余裕あれば）

1. 使用統計ダッシュボード
2. 実績・バッジシステム
3. 簡易トポロジビュー
4. 組織向け共通プロファイル配布機能
5. コマンドパレット風 UI（Ctrl+P ライク）

### 5.4 新規拡張候補

- Windows Credential Manager / DPAPI を利用したパスワード保管バックエンドの選択肢追加（ローカル鍵ファイル方式との切り替え）。
- SSH ポートフォワーディングのプリセット管理（よく使う転送設定をテンプレート化して適用）。
- テーマ設定のエクスポート/インポートと職場向けデザインプリセット配布。

---

## 6. データモデル（概要）

### 6.1 Profile

```rust
struct Profile {
    id: String,
    name: String,
    host: String,
    port: u16,
    protocol: Protocol,        // Ssh, Telnet, Serial, etc. (MVPはSsh優先)
    user: Option<String>,
    group: Option<String>,     // "dev", "stg", "prod"...
    tags: Vec<String>,
    danger_level: DangerLevel, // Normal, Warn, Critical
    macro_path: Option<PathBuf>,
    window_color: Option<WindowColor>,
    title_suffix: Option<String>,
    pinned: bool,
    last_used_at: Option<DateTime<Utc>>,
}
```

付帯する enum / 型:

```rust
enum Protocol { Ssh, Telnet }
enum DangerLevel { Normal, Warn, Critical }

struct WindowColor { r: u8, g: u8, b: u8 }
```

### 6.2 Config

```rust
struct AppConfig {
    tera_term_path: PathBuf,
    profiles_path: PathBuf,
    logs_path: PathBuf,
    ui: UiConfig,
}
```

補助構造体:

```rust
struct UiConfig {
    theme: UiTheme,                 // Light/Dark/System
    require_force_for_prod: bool,   // prod 接続時の強制確認
    recent_limit: usize,            // 最近使った接続の保持件数
}

struct HistoryEntry {
    profile_id: String,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    result: ConnectResult,          // Success/Failed + エラー理由
    command: Vec<String>,           // 実行した完全な引数リスト
    log_path: PathBuf,              // 生成されたログファイルパス
}
```

### 6.3 コマンド生成結果

`core` は実行前に副作用無しでコマンドを構築し、GUI/CLI に渡す。戻り値の想定:

```rust
struct LaunchCommand {
    program: PathBuf,          // ttermpro.exe のパス
    args: Vec<OsString>,       // /ssh host:22 ... などの引数
    log_path: PathBuf,         // 実行後に生成されるログパス（core が事前に決定）
    needs_confirmation: bool,  // 危険接続なら true
}
```

---

## 7. フェーズ分け / マイルストーン

### フェーズ 0: プロジェクトセットアップ

* Rust ワークスペース構成 (`core`, `cli`, `gui`) を作成
* 基本的な `Profile` / `AppConfig` 型定義
* TOML 設定の読み書き実装
* ロギング基盤（`tracing`）導入

**完了条件:**

* `cargo test` が通る
* `profiles.toml` の読み書きが行えるユニットテストがある

---

### フェーズ 1: コアロジック（P0 機能の「中身」）

* `core` に以下を実装:

  * プロファイル管理（追加/編集/削除/検索）
  * コマンドライン生成
  * 危険接続判定
  * ログ／履歴記録

**完了条件:**

* CLI テスト用コードから `core` を呼び出し、指定プロファイルのコマンドライン文字列が得られる。
* テスト実行時にログファイルが意図通り出力される。

---

### フェーズ 2: CLI インターフェース（最小版）

* `cli` crate でサブコマンド実装:

  * `list`
  * `connect <profile-id>`
* `connect` 実行時に:

  * `core` でコマンドライン文字列生成
  * `std::process::Command` で `ttermpro.exe` を起動

**完了条件:**

* コマンドプロンプトから `ttlaunch connect dev1` などで実際に Tera Term が起動する。

---

### フェーズ 3: GUI ランチャ（P0 UI）

* `gui` crate で egui ベースのウィンドウアプリを作成

  * プロファイル一覧表示
  * 検索ボックス
  * グルーピング & 色分け
  * 接続ボタン
  * 編集ダイアログ
  * 設定ダイアログ（Tera Term パス等）
* 危険接続時の確認ダイアログ実装

**完了条件:**

* GUI アプリから P0 機能一通りが操作できる。
* プロファイル編集内容が TOML に保存され、再起動時も反映される。

---

### フェーズ 4: インストーラー

* `installer/setup.iss` を作成

  * `gui.exe`, `cli.exe` を `Program Files\TeraTermLauncher` 等に配置
  * スタートメニュー / デスクトップショートカット作成（任意）
  * アンインストーラ登録
* ビルドスクリプト（PowerShell / batch）で:

  * `cargo build --release`
  * 成果物を `dist/` にコピー
  * Inno Setup コマンドラインで `setup.exe` を生成

**完了条件:**

* クリーンな Windows 環境に `setup.exe` を配布 → インストール → 起動 → 接続まで確認。

---

### フェーズ 5: P1 機能の追加（必要なものから順に）

優先度高め順:

1. 接続前後フック（`hooks.toml` / プロファイルごとの設定）
2. マクロ紐付け＆自動実行
3. Import/Export（`profiles.toml` の CLI での操作）
4. プロファイル継承（共通設定の親プロファイル機構）

フェーズ 5 以降は、**実際の運用で不足を感じたものだけ選んで実装**する。

---

## 8. プロファイル仕様と設定ファイル

### 8.1 プロファイルの TOML フォーマット

`config/profiles.toml` に保存する。すべてのフィールドを明示し、GUI/CLI で同じ構造を読み書きする。

```toml
version = 1

[[profiles]]
id = "dev1"
name = "Dev Server 1"
host = "192.168.1.10"
port = 22
protocol = "ssh"       # ssh | telnet
user = "deploy"
group = "dev"
tags = ["team-a", "app"]
color = "#3b82f6"      # GUI ラベル用
macro = "macros/init.ttl"
danger = false          # true のとき接続前ダイアログ表示
note = "用途や注意事項を自由記述"

[[profiles.extra_shortcuts]]
label = "Jump to bastion"
args = ["/ssh", "bastion:22"]

[profiles.hooks]
pre = ["scripts/pre.bat"]
post = ["scripts/post.bat"]

[profiles.tera_term]
path = "C:/Program Files/Tera Term/ttermpro.exe"
extra_args = ["/ssh"]
```

### 8.2 設定ファイルのパスと扱い

- 既定値: `%APPDATA%/TeraTermLauncher/profiles.toml`。無ければ `config/default_profiles.toml` を初回コピー。
- CLI 引数 `--config <path>` で上書き可能。GUI からも設定ダイアログで変更し、次回起動時に反映。
- `config/settings.toml` にアプリ共通設定（Tera Term パス、ログディレクトリ、テーマなど）を保存。
- プロファイルと設定の schema バージョンを `version = 1` で明示し、後方互換性のために migration 関数を `core::config::migrate(v: u32)` に用意。
- ファイル監視は行わず、起動時読み込み + 保存時上書きの単純モデルにする。

### 8.3 コマンドライン生成規約

- `core` はプロファイルと Tera Term のパスから **完全な引数リスト**を返す関数を提供する。
  - 例: `ttermpro.exe /ssh host:22 /user="deploy" /log="<path>" /MACRO="macros/init.ttl"`
- 引数生成は副作用なし。実行は `cli`/`gui` 側で `std::process::Command` に渡す。
- ログファイルは `logs/YYYYMMDD/HHMMSS_profileid.log` 形式で作成し、パスは `core` が返す構造体に含める。
- 生成規則:
  - `protocol == ssh` の場合 `/ssh host:port` を必須。`telnet` の場合 `/telnet host:port`。
  - `user` がある場合 `/user="<user>"` を付与。
  - `macro_path` がある場合 `/MACRO="<path>"` を最後尾に追加。
  - `extra_shortcuts.args` が指定されている場合、GUI のボタンから同じ組み立て処理を再利用する。
  - 危険接続 (`danger_level == Critical`) は `/W="[PROD] <title>"` のようにウィンドウタイトルに接頭辞を付ける。

### 8.4 安全装置の基本仕様

- `danger = true` または `group = "prod"` の場合、GUI/CLI 両方で確認ダイアログまたは `--force` 要求。
- 確認内容: プロファイル名/host/ユーザー/マクロ有無を表示し、「はい」でのみ実行。
- 実行履歴は `logs/history.jsonl` に JSON Lines 形式で追記し、GUI で参照可能にする。
- 履歴 UI: `gui` で「最近の接続」タブを用意し、`HistoryEntry` を時系列で表示。失敗時は赤色で理由を表示。
- CLI では `ttlaunch history --limit 20` で最新イベントを表示（MVP では JSONL をそのまま整形）。
- `--force` を指定した場合は履歴に `forced=true` を記録しておく。

### 8.5 GUI の最小仕様

- 画面構成: 左に検索 + フィルタ、右に詳細 + 接続ボタンの 2 カラム。
- ショートカットキー: `Ctrl+F` 検索フォーカス、`Enter` で選択接続、`Ctrl+E` で編集ダイアログ。
- 危険接続時は接続ボタンを赤色にし、ダイアログで 3 秒遅延後に「実行」ボタンを活性化する。
- プロファイル編集はモーダルで、必須項目未入力時は保存ボタンを無効化。
- 設定ダイアログで Tera Term パスを検証し、存在しない場合はエラー表示。

### 8.6 CLI インターフェース（詳細）

- コマンド:
  - `ttlaunch list [--json]` : プロファイル ID, 名前, 危険度, 最終使用時刻を表示。`--json` で JSON 出力。
  - `ttlaunch connect <profile-id> [--force] [--dry-run]` : コマンド生成のみ/実行。`--dry-run` でコマンドだけ表示。
  - `ttlaunch history [--limit N]` : 最新 N 件を表示。デフォルト 20。
- exit code 規約: 実行成功 0, プロファイル不存在 2, コマンド実行失敗 3, 入力エラー 64。
- CLI は `core` に依存するのみで、設定ファイルの書き換えは行わない（編集は GUI 専用）。

---

## 9. 開発プロセス / テスト

### 9.1 コーディング規約

- Rust stable 1.8x をターゲット。`cargo fmt` / `cargo clippy -- -D warnings` を必須とする。
- モジュールは crate 単位で分離し、共通ロジックは `crates/core` に集約。bin crate は薄く保つ。
- エラーハンドリングは `anyhow::Result` + `thiserror` でドメインエラーを明示する。

### 9.2 テストと検証

- `crates/core` には TOML 読み書きとコマンド生成のユニットテストを用意。サンプルプロファイルを fixture として保持。
- CLI は `cargo test -p cli -- --ignored` で E2E 風の統合テストを作成し、Windows 環境でのみ動くテストは `#[cfg(windows)]` でガード。
- GUI はスモークテストとして起動・終了のテストを最小限に置く。UI 動作はマニュアル検証手順を `docs/validation.md` に残す。

### 9.3 手動検証シナリオ

1. `config/default_profiles.toml` をコピーしたクリーン環境で GUI を起動し、デフォルトプロファイルで接続（`--dry-run` ログ確認のみでも可）。
2. 危険フラグ付きプロファイルで接続し、確認ダイアログ・赤色ボタン・3 秒遅延が機能することを確認。
3. プロファイル編集で `host` を変更→保存→再起動後も反映されることを確認。
4. CLI `ttlaunch list --json` が JSON を返し、`connect --dry-run` が実行せずにコマンドを表示することを確認。
5. 履歴タブで直近の接続が新しい順に表示されること、失敗時のメッセージが赤で表示されることを確認。

### 9.4 ビルド / 配布フロー

1. `cargo build --release` で `target/release/ttlaunch-{cli,gui}.exe` を生成。
2. `dist/` に成果物と `config/default_profiles.toml` をコピー。
3. `installer/setup.iss` を Inno Setup CLI でビルドし、`dist/setup.exe` を得る。
4. クリーンな Windows VM でインストール→接続確認を行い、ログ・設定の書き込みを検証。

### 9.5 リリース判定基準

- P0 機能すべてが GUI/CLI から操作可能で、危険接続の安全装置が動作していること。
- `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` が Windows 環境で通ること。
- 手動検証シナリオ 5 点が Windows 10/11 で再現済みであることを `docs/validation.md` に記録。
- インストーラーでのクリーンインストール→アンインストールが警告なしで完了すること。

---

## 10. リスク / 前提条件 / 対策

* **Tera Term のパス検出が環境依存**

  * 対策: 自動検出（レジストリ or 既知のパス） + 手動設定 UI の併用。
* **プロファイル定義が増えすぎると UI が煩雑になる**

  * 対策: タグ / グループ / 検索前提の設計。P2 のトポロジビューなどは後回し。
* **egui の UI 表現力限界**

  * 対策: まずはシンプルで済ませる。凝った UI は scope 外とする。
* **インストーラー環境ごとの差異**

  * 対策: Inno Setup のテンプレをシンプルに保ち、配布前に複数 Windows バージョンでテスト。

---

## 11. やらないことリスト（明示）

* クロスプラットフォーム（Linux/macOS）対応
* 自動アップデータの実装
* SSH 鍵管理ツール機能の内包
* VPN 接続制御の実装（必要なら Pre Hook で外部ツールを叩く）
* 派手なテーマ切り替え・スキン機能

---

## 12. ざっくりロードマップ

1. **v0.1 (MVP)**

   * フェーズ 0〜4 完了
   * P0 機能のみ実装
   * 自分用 / 社内限定配布

2. **v0.2**

   * フェーズ 5 で P1 機能を 2〜3 個追加
   * ドキュメント整理（README, プロファイルサンプル）

3. **v0.3 以降**

   * 実際の利用状況から必要な P1/P2 を選定
   * 必要なら AGENTS.md / 自動テスト拡充 / CI 導入

---

### まとめ

* 今のボトルネックは「どの機能から作るか」ではなく、**P0 のセットを v0.1 として切り出して一気に作り切ること**。
* 技術的には Rust + egui + Inno Setup で、**単一インストーラーの Windows ランチャ**として実装するのが現実的な落とし所。

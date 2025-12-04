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

### 6.2 Config

```rust
struct AppConfig {
    tera_term_path: PathBuf,
    profiles_path: PathBuf,
    logs_path: PathBuf,
    ui: UiConfig,
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

## 8. リスク / 前提条件 / 対策

* **Tera Term のパス検出が環境依存**

  * 対策: 自動検出（レジストリ or 既知のパス） + 手動設定 UI の併用。
* **プロファイル定義が増えすぎると UI が煩雑になる**

  * 対策: タグ / グループ / 検索前提の設計。P2 のトポロジビューなどは後回し。
* **egui の UI 表現力限界**

  * 対策: まずはシンプルで済ませる。凝った UI は scope 外とする。
* **インストーラー環境ごとの差異**

  * 対策: Inno Setup のテンプレをシンプルに保ち、配布前に複数 Windows バージョンでテスト。

---

## 9. やらないことリスト（明示）

* クロスプラットフォーム（Linux/macOS）対応
* 自動アップデータの実装
* SSH 鍵管理ツール機能の内包
* VPN 接続制御の実装（必要なら Pre Hook で外部ツールを叩く）
* 派手なテーマ切り替え・スキン機能

---

## 10. ざっくりロードマップ

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

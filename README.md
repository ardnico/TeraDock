# TeraDock

TeraDock は、SSH/Telnet/Serial などの接続先プロファイルや CommandSet を一元管理し、CLI と TUI の両方から操作できるツールです。

## 特徴

- プロファイル管理（SSH/Telnet/Serial）
- CommandSet の実行と結果表示
- 設定スコープ（global/env/profile/command）による上書き
- SSH トンネルとフォワード管理
- CLI と TUI の両方を提供

## 使い方（CLI）

```bash
# ヘルプ
cargo run -p td -- --help

# プロファイル一覧
cargo run -p td -- profile list

# TUI 起動
cargo run -p td -- ui
```

## 使い方（TUI）

TUI は `td ui` で起動します。キーボード操作でプロファイルの選択、CommandSet の実行、結果表示、詳細表示ができます。

```bash
cargo run -p td -- ui
```

## ビルド

```bash
# CLI ビルド
cargo build -p td

# TUI ビルド
cargo build -p tui
```

## リリース

`v*` タグを push すると GitHub Actions がリリースを作成し、以下の成果物を付与します。

- Windows インストーラ
- Debian パッケージ（.deb）
- RPM パッケージ（.rpm）

## 開発メモ

- 仕様・設計は `PROJECT_PLAN.md` と `EXTERNAL_DESIGN.md` を参照してください。
- 実装計画は `execplans/` にあります。

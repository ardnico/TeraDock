# PROJECT_PLAN.md - TeraDock secrets.rs エラー修正計画

## 0. 概要

発生しているビルドエラーは、`windows` クレートの `LocalFree` / `HLOCAL` の **モジュールパスの誤り** と **二重インポート** が原因。

実際の定義は `windows::Win32::Foundation::{LocalFree, HLOCAL}` にあり、`System::Memory` からは参照できない。:contentReference[oaicite:0]{index=0}  

本プロジェクトでは、以下を行う：

- `secrets.rs` 内の Windows API 呼び出し部分を整理
- `LocalFree` / `HLOCAL` のインポート元を一本化
- Windows 以外の環境でもコンパイルが通るように `cfg(windows)` 周りを明確化
- 既存の暗号処理ロジックの挙動は変えずに **ビルドエラーのみ解消** する

---

## 1. ゴール / アンチゴール

### 1.1 ゴール

- `ttcore` クレートが **Windows / 非 Windows の両方でビルド成功** すること。
- `secrets.rs` の Windows API 呼び出し（DPAPI / Credential Manager）の部分から
  - `LocalFree` / `HLOCAL` の未解決インポートエラーを解消する。
  - 不要な/矛盾した `use` を排除し、コードを読みやすくする。
- 将来、`LocalFree` 周りを拡張する際も迷わないように **ラッパ関数 or ヘルパー**に切り出しておく。

### 1.2 アンチゴール

- DPAPI / Credential Manager の仕様変更や暗号設計そのものは変えない。
- `windows` クレートを `windows-sys` 等に置き換える大規模変更はしない。
- 新たなバックエンド（Linux 用 secret backend 等）の追加は本計画では扱わない。

---

## 2. 現状整理

### 2.1 症状

コンパイルエラー抜粋：

```text
error[E0432]: unresolved imports `windows::Win32::System::Memory::LocalFree`, `windows::Win32::System::Memory::HLOCAL`
   --> crates\core\src\secrets.rs:110:46
    |
110 |         use windows::Win32::System::Memory::{LocalFree, HLOCAL};
    |                                              ^^^^^^^^^  ^^^^^^ no `HLOCAL` in `Win32::System::Memory`
    |                                              |
    |                                              no `LocalFree` in `Win32::System::Memory`

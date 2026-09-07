# Codex Usage Monitor

Windowsのタスクバーに、Codexの5時間枠・週次枠の残量とリセットまでの時間を常時表示する軽量モニターです。

![Windows](https://img.shields.io/badge/platform-Windows-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> [!IMPORTANT]
> 非公式のコミュニティ製アプリです。OpenAIによる提供、承認、提携を受けた製品ではありません。

## 特長

- 5時間枠と7日枠をタスクバー内に2行で常時表示
- ゲージと数値は使用量ではなく、バッテリー表示のように残量（100 − 使用量％）を表示
- 各枠のリセットまでの残り時間を表示
- Windowsの表示言語に合わせた自動言語選択（日本語を含む）
- 複数モニターから表示先を選択し、画面復帰後も選択したモニターを維持
- 最近の消費ペースを基に、使い切る可能性が高い場合はゲージ色で警告
- 定期更新、手動更新、Windows起動時の自動起動
- 管理者権限、ブラウザのCookie、独自のログイン操作は不要

## 必要環境

- Windows 10またはWindows 11
- Codex CLIまたはCodexアプリがインストール済みで、サインイン済みであること

本アプリはCodexがローカルに保存している既存の認証情報を利用してOpenAIから使用量を取得します。認証情報を独自に保存したり、OpenAI以外の独自サーバーへ送信したりしません。

## インストール

### インストーラーを使う

[最新のRelease](https://github.com/riyonasan/codex-usage-monitor/releases/latest)から `install.ps1` をダウンロードし、PowerShellで実行します。

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

`%LOCALAPPDATA%\Programs\CodexUsage` にユーザー単位でインストールされ、スタートメニューと「インストールされているアプリ」に登録されます。管理者権限は不要です。

### ポータブル版を使う

[最新のRelease](https://github.com/riyonasan/codex-usage-monitor/releases/latest)から `codex-usage.exe` をダウンロードし、任意の書き込み可能なフォルダーから起動します。

Releaseには改ざん確認用の `codex-usage.exe.sha256` も添付されます。

## 操作

通知領域のアイコンを右クリックすると、更新、表示するサービスや行、更新間隔、言語、表示モニター、自動起動などを設定できます。左クリックではタスクバー表示の表示・非表示を切り替えます。

### 消費ペースの目安

- 5h：リセットまでの時間に対して残量が少ない場合、黄色または赤で警告します。
- 7d：残量とリセットまでの日数から1日あたりの予算を求め、最近の実消費が予算を超える場合に警告します。
- 最初の24時間は履歴不足のため予測せず、通常色で表示します。
- 右クリックの「消費ペース・1日の目安」から詳細を確認できます。

履歴は `%LOCALAPPDATA%\CodexUsage\pace-history.json` に日時、使用率、リセット日時だけを保存します。認証情報は含みません。

## 更新

GitHub Releasesを使用して更新を確認します。ポータブル版と本インストーラー版は、このリポジトリのReleaseから更新されます。上流版のWinGetパッケージとは別系統です。

## アンインストール

Windowsの「設定」→「アプリ」→「インストールされているアプリ」から **Codex Usage** をアンインストールできます。

設定も削除する場合は次を実行します。

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA\Programs\CodexUsage\uninstall.ps1" -RemoveSettings
```

## ビルド

[Rust](https://www.rust-lang.org/tools/install)を導入したWindows環境で実行します。

```powershell
cargo test
cargo build --release
```

ビルド結果は `target\release\codex-usage.exe` に生成されます。

## 公開Releaseの作成

バージョンを更新して `v` で始まるタグ（例: `v1.10.0`）をpushすると、GitHub ActionsがWindows版exeをビルドし、exe、SHA256、インストール・アンインストールスクリプト、ライセンス類をReleaseへ添付します。

## プライバシーとセキュリティ

- Codexの既存認証情報を使用し、独自の認証情報は保存しません。
- 認証トークン、Cookie、Authorizationヘッダーをログへ出力しません。
- ブラウザスクレイピングやテレメトリーは使用しません。
- アップデート時はReleaseに添付されたSHA256を検証します。

## 上流プロジェクトとライセンス

このプロジェクトは [upstream-ray/codex-usage-monitor](https://github.com/upstream-ray/codex-usage-monitor) を基にした派生版です。元プロジェクトおよび本プロジェクトのコードは[MIT License](LICENSE)で公開されています。

OpenAIおよびBlossomロゴはOpenAIの商標です。詳細は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) と [OpenAI Brand Guidelines](https://openai.com/brand/) を参照してください。

## 既知の制約

- Codex側の非公開APIや認証保存形式が変更された場合、使用量を取得できなくなる可能性があります。
- Windowsのタスクバー構成やExplorerの更新によっては、表示位置の再調整が必要になる場合があります。
- 取得できない利用枠は0%ではなく、利用不可として扱います。

---

簡体字中国語版は [README.zh-CN.md](README.zh-CN.md) を参照してください。

# astrid-capsule-identity

[English](README.md) · [简体中文](README.zh-CN.md) · **日本語**

[![ライセンス：MIT または Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![MSRV：1.94](https://img.shields.io/badge/MSRV-1.94-blue)](https://www.rust-lang.org)

**[Astrid OS](https://github.com/unicity-astrid/astrid) 向けの、アイデンティティおよびシステムプロンプトビルダーです。**

OS モデルでは、この capsule は `/etc/profile` に相当します。エージェントの spark アイデンティティを管理し、capsule の状態として永続化して、アイデンティティと環境コンテキストをシステムプロンプトに組み立てます。

## 仕組み

`spark.v1.request.build` イベントを受信すると：

1. 永続化された spark アイデンティティを capsule の KV 状態から読み込みます。
2. KV 状態が空の場合は `home://.config/spark.toml` を自動検出します。
3. 環境コンテキスト（作業ディレクトリとプラットフォーム）を追加します。
4. 組み立てたプロンプトを `spark.v1.response.ready` に発行します。

アイデンティティがまだ存在しない場合、プロンプトにはオンボーディング手順が含まれます。オンボーディング後、LLM は `save_identity` を呼び出し、選択した callsign、class、aura、signal、コアディレクティブを capsule の状態へ保存し、復旧用コピーとして `home://.config/spark.toml` を書き込みます。

状態は Astrid の capsule KV 分離により、呼び出し元プリンシパルの範囲に限定されます。react ループで関連付けられるよう、セッション ID はそのまま返されます。

## IPC プロトコル

| 方向 | トピック |
|---|---|
| 購読 | `spark.v1.request.build` |
| 購読 | `tool.v1.execute.save_identity` |
| 購読 | `tool.v1.request.describe` |
| 購読 | `cli.v1.command.execute` |
| 発行 | `spark.v1.response.ready` |
| 発行 | `tool.v1.execute.*.result` |
| 発行 | `tool.v1.response.describe.*` |
| 発行 | `agent.v1.response` |

## 開発

```bash
cargo build
cargo test
```

## ライセンス

[MIT](LICENSE-MIT) および [Apache 2.0](LICENSE-APACHE) のデュアルライセンスです。

Copyright © 2025–2026 Joshua J. Bouw and Unicity Labs.

# astrid-capsule-identity

[English](README.md) · **简体中文** · [日本語](README.ja.md)

[![许可证：MIT 或 Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![MSRV：1.94](https://img.shields.io/badge/MSRV-1.94-blue)](https://www.rust-lang.org)

**面向 [Astrid OS](https://github.com/unicity-astrid/astrid) 的身份与系统提示词构建器。**

在操作系统模型中，该 capsule 相当于 `/etc/profile`。它管理智能体的 spark 身份，将其作为 capsule 状态持久保存，并把身份与环境上下文组合成系统提示词。

## 工作原理

收到 `spark.v1.request.build` 事件时：

1. 从 capsule KV 状态加载已持久保存的 spark 身份。
2. 如果 KV 状态为空，则自动检测 `home://.config/spark.toml`。
3. 添加环境上下文（工作目录、平台）。
4. 在 `spark.v1.response.ready` 上发布组装后的提示词。

如果尚无身份，提示词会包含引导说明。完成引导后，LLM 会调用 `save_identity`，将所选的 callsign、class、aura、signal 和核心指令保存到 capsule 状态，并把 `home://.config/spark.toml` 写作恢复副本。

状态按照 Astrid 的 capsule KV 隔离机制限定在调用主体范围内。系统会原样返回会话 ID，以供 react 循环关联。

## IPC 协议

| 方向 | 主题 |
|---|---|
| 订阅 | `spark.v1.request.build` |
| 订阅 | `tool.v1.execute.save_identity` |
| 订阅 | `tool.v1.request.describe` |
| 订阅 | `cli.v1.command.execute` |
| 发布 | `spark.v1.response.ready` |
| 发布 | `tool.v1.execute.*.result` |
| 发布 | `tool.v1.response.describe.*` |
| 发布 | `agent.v1.response` |

## 开发

```bash
cargo build
cargo test
```

## 许可证

本项目采用 [MIT](LICENSE-MIT) 与 [Apache 2.0](LICENSE-APACHE) 双重许可。

版权所有 © 2025–2026 Joshua J. Bouw 与 Unicity Labs。

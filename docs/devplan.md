# `graftail` 多波次开发计划 (Development Plan)

> **关联文档**: [软件定义文档](./software-definition.md) | [项目提案](./proposol.md)

---

## 总体策略

按依赖关系自底向上分 6 个波次推进。每个波次产出可编译、可测试的独立模块。

```
Wave 1 ──► Wave 2 ──► Wave 3 ──► Wave 4 ──► Wave 5 ──► Wave 6
(基础)    (配置)     (API)      (展示)     (集成)     (测试)
```

波次 2/3/4 的模块之间无强依赖，可以在各自波次内并行开发。

---

## Wave 1: 项目脚手架 + 核心类型

**目标**: 可编译的空骨架，所有模块文件就位，核心类型定义完成。

**依赖**: 无

**任务清单**:

| # | 任务 | 产出文件 |
|---|------|----------|
| 1.1 | `cargo init` + 写 `Cargo.toml` | `Cargo.toml` |
| 1.2 | 错误类型定义 | `src/error.rs` |
| 1.3 | CLI 参数定义 (clap derive) | `src/cli.rs` |
| 1.4 | 数据模型 (Loki API 结构体 + 应用类型) | 分散在各模块 |
| 1.5 | `main.rs` 骨架 (打印版本/帮助) | `src/main.rs` |
| 1.6 | 目录结构创建 | `mkdir -p src/api src/stream src/output tests/integration tests/fixtures` |

**验证**: `cargo build` 成功，`cargo run -- --help` 输出帮助信息。

---

## Wave 2: 配置 + 认证模块

**目标**: 完整的配置加载链和认证方案。

**依赖**: Wave 1 (Cli struct, error types)

**任务清单**:

| # | 任务 | 产出文件 |
|---|------|----------|
| 2.1 | `config.rs` — 加载与合并 (cli > env > config file) | `src/config.rs` |
| 2.2 | `auth.rs` — AuthMethod 枚举 + apply/apply_to_ws | `src/auth.rs` |
| 2.3 | AppConfig 构建逻辑 | `src/config.rs` |

**验证**: 
- `cargo test` 配置模块通过
- 手动测试: 从环境变量和配置文件分别加载配置

---

## Wave 3: API 层

**目标**: Grafana Proxy URL 构建 + 历史查询 + WebSocket Tail 连接。

**依赖**: Wave 2 (AppConfig, AuthMethod)

**任务清单**:

| # | 任务 | 产出文件 |
|---|------|----------|
| 3.1 | `grafana_proxy.rs` — URL 构建 (http→ws 转换) | `src/api/grafana_proxy.rs` |
| 3.2 | `query_range.rs` — HTTP GET 历史日志 | `src/api/query_range.rs` |
| 3.3 | `tail.rs` — WebSocket 连接 + 接收循环 + 重连 | `src/api/tail.rs` |
| 3.4 | `api/mod.rs` — re-export | `src/api/mod.rs` |
| 3.5 | WebSocket 重连逻辑 (指数退避 + jitter) | `src/api/tail.rs` |

**验证**:
- `cargo test` API 模块通过 (含 wiremock 集成测试)
- 若有真实 Grafana+Loki 环境，手动验证 WebSocket 连接

---

## Wave 4: 流解析 + 输出格式化

**目标**: JSON 反序列化 → 日志条目 → 终端彩色输出。

**依赖**: Wave 3 (接收 WS 帧), Wave 1 (数据类型)

**任务清单**:

| # | 任务 | 产出文件 |
|---|------|----------|
| 4.1 | `parser.rs` — Loki 响应反序列化 → `LogEntry` | `src/stream/parser.rs` |
| 4.2 | `stream/mod.rs` — re-export | `src/stream/mod.rs` |
| 4.3 | `formatter.rs` — 时间戳转换 + 格式化 | `src/output/formatter.rs` |
| 4.4 | `color.rs` — 级别着色 + 标签颜色分配 | `src/output/color.rs` |
| 4.5 | `screen.rs` — 冻结屏幕交互 (AtomicBool + crossterm) | `src/output/screen.rs` |
| 4.6 | `output/mod.rs` — 统一 OutputHandler | `src/output/mod.rs` |

**验证**:
- `cargo test` 解析 + 格式化 + 颜色模块通过
- 手动测试: 用样本 JSON 验证输出格式

---

## Wave 5: 集成编排 + 生命周期

**目标**: `main.rs` 完整编排，信号处理，优雅退出。

**依赖**: Wave 2, 3, 4 (所有核心模块)

**任务清单**:

| # | 任务 | 产出文件 |
|---|------|----------|
| 5.1 | `main.rs` — 完整流程编排 (config → auth → history → tail → output) | `src/main.rs` |
| 5.2 | 信号处理 (SIGINT/SIGTERM → CancellationToken) | `src/main.rs` (或独立模块) |
| 5.3 | 优雅退出流程 (关闭 WS, 恢复终端) | `src/main.rs` |
| 5.4 | 冻结屏幕 Task (crossterm event 轮询) | 集成到 `src/output/screen.rs` + `main.rs` |

**验证**:
- `cargo build --release` 成功
- 手动端到端测试 (若有真实环境)
- `cargo build --release` 验证 release profile

---

## Wave 6: 测试 + 工程化

**目标**: 完整测试覆盖，构建脚本，README。

**依赖**: Wave 5 (功能完整)

**任务清单**:

| # | 任务 | 产出文件 |
|---|------|----------|
| 6.1 | 测试 fixtures (tail_response.json, query_range_response.json) | `tests/fixtures/` |
| 6.2 | 单元测试补全 (各模块 `#[cfg(test)]`) | 各 `src/**/*.rs` |
| 6.3 | 集成测试 (wiremock 模拟 Grafana+Loki) | `tests/integration/e2e.rs` |
| 6.4 | `cargo test` 全绿 | - |
| 6.5 | 构建脚本 + release 流程 | `scripts/build.sh` |
| 6.6 | README 编写 | `README.md` |

**验证**: `cargo test` 全部通过，`cargo build --release` 产出可执行二进制。

---

## 波次依赖图

```
Wave 1 (脚手架)
  │
  ▼
Wave 2 (配置+认证) ──────────────────┐
  │                                    │
  ▼                                    │
Wave 3 (API 层) ────────────────────┐ │
  │                                  │ │
  ▼                                  │ │
Wave 4 (流解析+输出) ◄───────────────┘ │
  │         (可并行 Wave 3+4)          │
  ▼                                    │
Wave 5 (集成编排) ◄───────────────────┘
  │
  ▼
Wave 6 (测试+工程化)
```

---

## 文件创建顺序总览

```
Wave 1: Cargo.toml, error.rs, cli.rs, main.rs (skeleton)
Wave 2: config.rs, auth.rs
Wave 3: api/mod.rs, api/grafana_proxy.rs, api/query_range.rs, api/tail.rs
Wave 4: stream/mod.rs, stream/parser.rs
        output/mod.rs, output/formatter.rs, output/color.rs, output/screen.rs
Wave 5: main.rs (full), cleanup
Wave 6: tests/, scripts/, README.md
```

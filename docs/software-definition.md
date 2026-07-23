# `graftail` 软件定义文档 (Software Definition Document)

> **版本**: v1.0  
> **状态**: 初始定义  
> **关联文档**: [项目提案](./proposol.md)

---

## 目录

1. [项目定义与范围](#1-项目定义与范围)
2. [系统架构](#2-系统架构)
3. [模块设计](#3-模块设计)
4. [数据结构定义](#4-数据结构定义)
5. [API 集成规范](#5-api-集成规范)
6. [CLI 接口定义](#6-cli-接口定义)
7. [配置文件规范](#7-配置文件规范)
8. [认证与安全设计](#8-认证与安全设计)
9. [终端输出与格式化规范](#9-终端输出与格式化规范)
10. [错误处理设计](#10-错误处理设计)
11. [生命周期与状态管理](#11-生命周期与状态管理)
12. [构建与发布](#12-构建与发布)
13. [测试策略](#13-测试策略)

---

## 1. 项目定义与范围

### 1.1 项目标识

| 属性 | 值 |
|------|-----|
| 项目名称 | `graftail` (Grafana + Tail) |
| 语言 | Rust (Edition 2021) |
| 类型 | CLI 工具 |
| 许可 | 待定 |

### 1.2 项目定位

`graftail` 是一个高性能命令行日志追踪工具。它以 Grafana Data Source Proxy API 作为安全网关，通过 WebSocket 对接 Loki Tail 接口，为开发者与运维人员提供类似 `tail -f` 的实时日志流式追踪体验。

### 1.3 核心价值主张

1. **安全合规**: 无需在本地配置底层数据源直连凭据，完全复用 Grafana RBAC 和 API Token。
2. **极致性能**: Rust 实现，低延迟、低内存占用的日志流处理。
3. **单文件分发**: 编译为单一静态链接二进制文件，无需运行时依赖。
4. **丝滑体验**: WebSocket 实时推送，告别轮询延迟；LogQL 语法高亮与终端彩色输出。

### 1.4 范围边界

**在范围内**:
- 通过 Grafana Proxy 以 WebSocket 实时接收并展示 Loki 日志
- 支持 LogQL 查询、历史上下文回溯
- 终端彩色输出与标签/级别高亮
- Token 与 Basic Auth 双认证模式
- 断线自动重连、优雅退出
- 冻结屏幕交互

**不在范围内**:
- Dashboard 渲染、图表生成
- 直接连接底层数据源 (Loki / Elasticsearch)
- 日志的写入、修改或删除操作
- GUI 界面

---

## 2. 系统架构

### 2.1 架构概览

```
┌─────────────────────────────────────────────────────────┐
│                       graftail CLI                       │
├─────────────────────────────────────────────────────────┤
│  CLI Layer        │  clap (参数解析 + 配置合并)           │
├───────────────────┼──────────────────────────────────────┤
│  Config Layer     │  config crate (文件/环境变量/参数)    │
├───────────────────┼──────────────────────────────────────┤
│  Auth Layer       │  Token / Basic Auth / 交互式输入      │
├───────────────────┼──────────────────────────────────────┤
│  API Layer        │  reqwest (HTTP)  │  tokio-tungstenite │
│                   │  历史查询        │  WebSocket Tail    │
├───────────────────┼──────────────────┴────────────────────┤
│  Stream Layer     │  tokio 异步流处理 + JSON 反序列化      │
├───────────────────┼───────────────────────────────────────┤
│  Output Layer     │  owo-colors 着色 + crossterm 终端控制  │
├───────────────────┼───────────────────────────────────────┤
│  Signal Layer     │  tokio::signal (SIGINT/SIGTERM)       │
└───────────────────┴───────────────────────────────────────┘
         │                              │
         ▼                              ▼
┌─────────────────┐          ┌──────────────────┐
│  Grafana Server  │          │  Loki (后端存储)  │
│  (RBAC + Proxy)  │◄────────►│                   │
└─────────────────┘          └──────────────────┘
```

### 2.2 核心数据流

```
用户输入 (CLI Args)
    │
    ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ 1. 参数解析   │───►│ 2. 配置合并   │───►│ 3. 认证构建   │
│  (clap)      │    │  (config)    │    │  (Auth Header)│
└──────────────┘    └──────────────┘    └──────┬───────┘
                                               │
                    ┌───────────────────────────┤
                    ▼                           ▼
           ┌──────────────┐           ┌──────────────────┐
           │ 4a. 历史查询   │           │ 4b. WebSocket    │
           │  (HTTP GET)   │           │    连接建立        │
           │  query_range  │           │    (Upgrade)      │
           └──────┬───────┘           └────────┬─────────┘
                  │                            │
                  ▼                            ▼
           ┌──────────────┐           ┌──────────────────┐
           │ 输出历史日志   │           │ 5. 流式接收帧      │
           │ (一次性)      │           │   tokio async     │
           └──────────────┘           └────────┬─────────┘
                                               │
                                               ▼
                                       ┌──────────────────┐
                                       │ 6. JSON 反序列化  │
                                       │   serde_json      │
                                       └────────┬─────────┘
                                                │
                                                ▼
                                       ┌──────────────────┐
                                       │ 7. 格式化 + 着色  │
                                       │   owo-colors      │
                                       └────────┬─────────┘
                                                │
                                                ▼
                                       ┌──────────────────┐
                                       │ 8. 终端输出       │
                                       │   stdout/stderr   │
                                       └──────────────────┘
```

### 2.3 运行时并发模型

```
Main Task (tokio::main)
├── Signal Watcher Task
│   └── 监听 SIGINT / SIGTERM → 触发全局取消令牌
│
├── WebSocket Read Task
│   ├── 接收 WS 帧 → JSON 解包 → 格式化 → stdout
│   └── 检测连接断开 → 触发重连逻辑
│
├── WebSocket Ping/Pong Task (可选的 Keep-Alive)
│   └── 定期发送 Ping 帧维持连接
│
└── Input Watcher Task (冻结屏幕功能)
    └── 监听键盘 'h' 键 → 切换冻结/恢复状态
```

---

## 3. 模块设计

### 3.1 Crate 结构

```
graftail/
├── Cargo.toml
├── src/
│   ├── main.rs              # 入口点，组装各模块
│   ├── cli.rs                # clap 命令行参数定义
│   ├── config.rs             # 配置加载、合并与校验
│   ├── auth.rs               # 认证信息构建 (Token / Basic Auth)
│   ├── api/
│   │   ├── mod.rs            # API 模块入口
│   │   ├── grafana_proxy.rs  # Grafana Proxy URL 构建
│   │   ├── query_range.rs    # 历史日志查询
│   │   └── tail.rs           # WebSocket Tail 连接与流处理
│   ├── stream/
│   │   ├── mod.rs            # 流处理模块入口
│   │   └── parser.rs         # Loki JSON 响应解析
│   ├── output/
│   │   ├── mod.rs            # 输出模块入口
│   │   ├── formatter.rs      # 时间戳/标签/级别格式化
│   │   ├── color.rs          # 终端颜色方案
│   │   └── screen.rs         # 冻结屏幕交互管理
│   └── error.rs              # 统一错误类型定义
└── tests/
    ├── integration/
    │   └── e2e.rs            # 端到端集成测试
    └── fixtures/             # 测试用 JSON 响应样本
```

### 3.2 模块职责

#### 3.2.1 `cli` — 命令行接口定义

- 使用 `clap` v4 Derive 模式定义所有参数
- 定义子命令 (如有): `graftail`, `graftail config`
- 提供 `--help` 自动生成

**关键结构体**:

```rust
#[derive(Parser)]
#[command(name = "graftail", version, about = "Real-time Loki log tailing via Grafana Proxy")]
pub struct Cli {
    /// Grafana base URL
    #[arg(long, env = "GRAFTAIL_URL")]
    pub grafana_url: Option<String>,

    /// Loki datasource UID in Grafana
    #[arg(long, env = "GRAFTAIL_DATASOURCE_UID")]
    pub datasource_uid: Option<String>,

    /// LogQL query string
    #[arg(short = 'q', long)]
    pub query: String,

    /// Grafana API Token (Service Account)
    #[arg(long, env = "GRAFTAIL_TOKEN")]
    pub token: Option<String>,

    /// Grafana username (for Basic Auth)
    #[arg(long, env = "GRAFTAIL_USER")]
    pub user: Option<String>,

    /// Grafana password (for Basic Auth) — prefer env var or prompt
    #[arg(long, env = "GRAFTAIL_PASSWORD", hide_env_values = true)]
    pub password: Option<String>,

    /// Number of historical log lines to fetch before tailing
    #[arg(long, default_value_t = 0)]
    pub last: usize,

    /// Start time for tail (e.g., "1h", "30m")
    #[arg(long)]
    pub since: Option<String>,

    /// Output format: "pretty" (default) or "json"
    #[arg(long, default_value = "pretty")]
    pub output: OutputFormat,

    /// Path to config file
    #[arg(long, default_value = "~/.config/graftail/config.toml")]
    pub config: PathBuf,
}
```

#### 3.2.2 `config` — 配置管理

- 加载优先级: CLI 参数 > 环境变量 > 配置文件 > 默认值
- 配置文件路径: `~/.config/graftail/config.toml`
- 配置校验: URL 格式、必填项检查

**配置文件结构 (TOML)**:

```toml
[graftail]
grafana_url = "https://grafana.example.com"
datasource_uid = "loki-uid-here"

[auth]
token = ""       # 不建议在此写明文
user = ""
# password 不应写入配置文件
```

#### 3.2.3 `auth` — 认证模块

负责构建 HTTP 认证头，支持三种模式（按优先级）:

1. **Service Account Token** (`--token` / `GRAFTAIL_TOKEN` 环境变量)
   - Header: `Authorization: Bearer <token>`

2. **Basic Authentication** (`--user` + `--password` / 环境变量)
   - Header: `Authorization: Basic <base64(user:password)>`

3. **交互式密码输入**
   - 当 Token 和密码都未提供时，使用 `rpassword` 隐藏回显输入密码

**输出**: 统一的 `AuthMethod` 枚举，提供 `apply_to_request` 方法。

```rust
pub enum AuthMethod {
    Bearer(String),         // Service Account Token
    Basic(String, String),  // username, password
}

impl AuthMethod {
    pub fn apply(&self, builder: RequestBuilder) -> RequestBuilder {
        match self {
            AuthMethod::Bearer(token) => builder.header("Authorization", format!("Bearer {}", token)),
            AuthMethod::Basic(user, pass) => builder.basic_auth(user, Some(pass)),
        }
    }
    
    /// 对 tokio-tungstenite 的 Request，手动注入 Header
    pub fn apply_to_ws(&self, builder: http::request::Builder) -> http::request::Builder {
        match self {
            AuthMethod::Bearer(token) => builder.header("Authorization", format!("Bearer {}", token)),
            AuthMethod::Basic(user, pass) => {
                let creds = base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", user, pass));
                builder.header("Authorization", format!("Basic {}", creds))
            }
        }
    }
}
```

#### 3.2.4 `api::grafana_proxy` — Grafana Proxy URL 构建

- 根据 Grafana base URL + datasource UID + Loki API 路径拼接完整 URL
- 自动处理 `http` → `ws` 的协议转换 (用于 WebSocket)

```rust
/// 构建 Grafana Data Source Proxy URL
///
/// 模板: {grafana_url}/api/datasources/proxy/uid/{uid}/{loki_api_path}
pub fn build_proxy_url(grafana_url: &str, datasource_uid: &str, loki_path: &str) -> String;

/// 构建 WebSocket 版本的 Proxy URL
/// 将 http/https 替换为 ws/wss
pub fn build_proxy_ws_url(grafana_url: &str, datasource_uid: &str, loki_path: &str) -> String;
```

#### 3.2.5 `api::query_range` — 历史日志查询

- 调用 Loki `query_range` API (通过 Grafana Proxy)
- 参数: `query`, `limit`, `start`, `end`, `direction=backward`
- 返回解析后的日志条目列表

#### 3.2.6 `api::tail` — WebSocket Tail 连接

核心模块，负责:

1. 建立 WebSocket 连接 (HTTP Upgrade)
2. 循环接收 WS 消息帧 (Text 帧)
3. 将 JSON 帧传递给 `stream::parser` 解析
4. 将解析结果传递给 `output` 模块格式化输出
5. 检测连接断开并触发重连
6. 响应全局取消令牌实现优雅退出

```rust
pub struct TailSession {
    pub config: Arc<TailConfig>,
    pub auth: AuthMethod,
    pub cancel_token: CancellationToken,
}

impl TailSession {
    /// 启动 Tail 主循环
    pub async fn run(&self) -> Result<()>;
    
    /// 建立 WebSocket 连接
    async fn connect(&self) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>>;
    
    /// 消息接收循环
    async fn receive_loop(&self, ws: &mut WebSocketStream<...>) -> Result<()>;
    
    /// 断线重连 (指数退避)
    async fn reconnect(&self) -> Result<WebSocketStream<...>>;
}
```

#### 3.2.7 `stream::parser` — 流式 JSON 解析

- 使用 `serde_json` 反序列化 Loki Tail 响应
- 处理 Loki 返回的嵌套 `streams[].values[]` 结构
- 提取: 标签集合 (stream labels) + 时间戳 + 日志行

#### 3.2.8 `output` — 输出与格式化

子模块:

- **`formatter`**: 时间戳转换 (纳秒 → 人类可读)、标签格式化
- **`color`**: 颜色方案定义、标签颜色分配、日志级别着色
- **`screen`**: 冻结屏幕交互 (`h` 键切换)

---

## 4. 数据结构定义

### 4.1 Loki API 响应结构

```rust
use serde::{Deserialize, Serialize};

/// Loki Tail WebSocket 消息的顶层结构
#[derive(Debug, Deserialize)]
pub struct LokiTailResponse {
    pub streams: Vec<LokiStream>,
}

/// 单个日志流 (按标签分组)
#[derive(Debug, Deserialize)]
pub struct LokiStream {
    /// 标签键值对，如 {"app": "nginx", "pod": "nginx-7d4f8b9c-abcde"}
    pub stream: std::collections::HashMap<String, String>,
    
    /// 日志条目数组: [[纳秒时间戳, 日志内容], ...]
    pub values: Vec<[String; 2]>,
}

/// 解析后的单条日志
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// 标签集合
    pub labels: std::collections::HashMap<String, String>,
    
    /// 时间戳 (纳秒，从 Loki 返回)
    pub timestamp_ns: i64,
    
    /// 日志行内容
    pub line: String,
}

/// Loki Query Range API 响应
#[derive(Debug, Deserialize)]
pub struct LokiQueryRangeResponse {
    pub status: String,
    pub data: LokiQueryRangeData,
}

#[derive(Debug, Deserialize)]
pub struct LokiQueryRangeData {
    pub resultType: String,
    pub result: Vec<LokiStream>,
}
```

### 4.2 应用内部数据结构

```rust
/// 输出格式
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Pretty,   // 彩色终端输出
    Json,     // 机器可读 JSON
    Plain,    // 纯文本 (无颜色)
}

/// 日志级别枚举 (用于着色)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Unknown,
}

impl LogLevel {
    /// 从日志行内容中自动检测级别
    pub fn detect(line: &str) -> Self;
    
    /// 获取对应颜色
    pub fn color(&self) -> owo_colors::Style;
}

/// 标签颜色映射 (LRU 策略)
pub struct LabelColorMap {
    /// 已分配的标签颜色
    mapping: HashMap<String, Color>,
    /// 可用颜色池
    color_pool: Vec<Color>,
}

/// 应用程序运行时的统一配置
pub struct AppConfig {
    pub grafana_url: String,
    pub datasource_uid: String,
    pub query: String,
    pub auth: AuthMethod,
    pub last: usize,
    pub since: Option<chrono::Duration>,
    pub output: OutputFormat,
}
```

---

## 5. API 集成规范

### 5.1 Grafana Data Source Proxy API

**基础 URL 模板**:

```
{grafana_base_url}/api/datasources/proxy/uid/{datasource_uid}/{loki_api_path}
```

**认证头**:

```
Authorization: Bearer {grafana_api_token}
-- 或 --
Authorization: Basic {base64(user:password)}
```

**关键行为**:
- Grafana Proxy 是无状态的转发层
- 支持 HTTP 请求升级为 WebSocket (条件: 正确的 `Upgrade` / `Connection` 头)
- 所有 Loki 返回的状态码和错误体透传

### 5.2 Loki Tail WebSocket API

**端点**: `loki/api/v1/tail`

**查询参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `query` | string | 是 | LogQL 查询语句 (需 URL 编码) |
| `start` | int | 否 | 开始追踪的纳秒时间戳，默认当前时间 |
| `limit` | int | 否 | 每次返回的最大 stream 数，默认 100 |
| `delay_for` | int | 否 | 延迟查询秒数，默认 0 |

**WebSocket 消息格式** (Text 帧, JSON):

```json
{
  "streams": [
    {
      "stream": {
        "filename": "/var/log/syslog",
        "job": "varlogs"
      },
      "values": [
        ["1698386400000000000", "This is a log line"]
      ]
    }
  ]
}
```

**连接维护**:
- 服务端可能发送 Ping 帧
- 客户端应回复 Pong 帧 (tokio-tungstenite 自动处理)
- 服务端在日志无更新时可能长时间不发送数据帧，不应超时断开

### 5.3 Loki Query Range API (历史查询)

**端点**: `loki/api/v1/query_range` (GET)

**查询参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `query` | string | 是 | LogQL 查询语句 |
| `limit` | int | 否 | 返回条数，推荐 100 |
| `start` | int | 否 | 起始纳秒时间戳 |
| `end` | int | 否 | 结束纳秒时间戳 |
| `direction` | string | 否 | `backward` 获取最新日志 |

**响应结构**: 见 4.1 节 `LokiQueryRangeResponse`

### 5.4 时间戳计算

Loki 返回纳秒时间戳字符串。与 Rust 标准库时间的转换:

```rust
fn nanos_to_datetime(nanos_str: &str) -> Result<DateTime<Utc>> {
    let nanos: i64 = nanos_str.parse()
        .context("Failed to parse nanosecond timestamp")?;
    let secs = nanos / 1_000_000_000;
    let nsecs = (nanos % 1_000_000_000) as u32;
    Ok(Utc.timestamp_opt(secs, nsecs).single()
        .context("Invalid timestamp")?)
}
```

---

## 6. CLI 接口定义

### 6.1 命令语法

```
graftail [OPTIONS] --query <LogQL>

graftail [OPTIONS] -q <LogQL>
```

### 6.2 参数表

| 参数 | 短标志 | 环境变量 | 默认值 | 说明 |
|------|--------|----------|--------|------|
| `--grafana-url` | - | `GRAFTAIL_URL` | 配置文件 | Grafana 服务地址 (含协议) |
| `--datasource-uid` | - | `GRAFTAIL_DATASOURCE_UID` | 配置文件 | Loki 数据源 UID |
| `--query` | `-q` | `GRAFTAIL_QUERY` | (必填) | LogQL 查询语句 |
| `--token` | - | `GRAFTAIL_TOKEN` | - | Grafana Service Account Token |
| `--user` | - | `GRAFTAIL_USER` | - | Grafana 用户名 (Basic Auth) |
| `--password` | - | `GRAFTAIL_PASSWORD` | - | Grafana 密码 (Basic Auth) |
| `--last` | - | - | `0` | Tail 前先拉取最近 N 条历史日志 |
| `--since` | - | - | - | 从指定历史时间开始 Tail (如 `1h`, `30m`) |
| `--output` | - | - | `pretty` | 输出格式: `pretty`, `json`, `plain` |
| `--config` | - | - | `~/.config/graftail/config.toml` | 配置文件路径 |

### 6.3 使用示例

```bash
# 基础用法: 实时追踪 nginx 错误日志
graftail -q '{app="nginx"} |= "error"'

# 使用 Token 认证
graftail --token "glsa_xxx" -q '{namespace="prod"}'

# 使用用户名密码 + 环境变量
export GRAFTAIL_USER="admin"
export GRAFTAIL_PASSWORD="secret"
graftail -q '{job="varlogs"}'

# 带历史上下文: 先拉最近 100 条, 再实时追踪
graftail -q '{app="api"}' --last 100

# 从 1 小时前开始追踪
graftail -q '{app="api"}' --since 1h

# JSON 输出 (适合管道传递给 jq)
graftail -q '{app="api"}' --output json | jq .

# 指定 Grafana 地址和数据源
graftail --grafana-url https://monitoring.example.com \
         --datasource-uid "abc123" \
         -q '{app="api"}'
```

### 6.4 退出码

| 退出码 | 含义 |
|--------|------|
| `0` | 正常退出 (SIGINT 或自然结束) |
| `1` | 配置错误 (参数缺失、URL 格式错误等) |
| `2` | 认证失败 (401 / 403) |
| `3` | 连接失败 (网络不可达、超时) |
| `4` | WebSocket 协议错误 (非预期响应) |
| `5` | 运行时错误 (解析失败等) |

---

## 7. 配置文件规范

### 7.1 文件位置

- **Linux / macOS**: `$HOME/.config/graftail/config.toml`
- **Windows**: `%APPDATA%\graftail\config.toml`

### 7.2 文件格式 (TOML)

```toml
# ~/.config/graftail/config.toml

[graftail]
# Grafana 服务地址 (必填)
grafana_url = "https://grafana.example.com"

# Loki 数据源 UID (必填)
datasource_uid = "loki-xxxxxxxx"

# 默认 LogQL 查询 (可选; CLI 参数会覆盖)
# default_query = '{app="myapp"}'

# 默认输出格式
# default_output = "pretty"

[auth]
# Service Account Token (⚠️ 不建议在此写明文; 推荐使用环境变量)
# token = "glsa_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"

# 用户名 (Basic Auth — ⚠️ 不建议在此写明文)
# user = "admin"

# 密码不应写入配置文件; 始终通过环境变量或交互式输入提供
# password = ""  ← 不要这样做
```

### 7.3 配置优先级

```
CLI 参数 > 环境变量 > 配置文件 > 硬编码默认值
```

具体规则:
1. 若 CLI 提供了 `--grafana-url`，忽略环境变量和配置文件中的值
2. 若 CLI 未提供但环境变量存在，使用环境变量
3. 以上均无，读取配置文件；配置文件也无，报错退出

---

## 8. 认证与安全设计

### 8.1 认证优先级

```
Service Account Token (--token / GRAFTAIL_TOKEN)
    │
    ├── 有? → Header: Authorization: Bearer <token>
    │
    └── 无?
        │
        ├── 有 --user + --password (或环境变量)?
        │   └── Header: Authorization: Basic <base64(user:pass)>
        │
        └── 有 --user 但无 password?
            └── 交互式输入密码 (rpassword, 不回显)
```

### 8.2 交互式密码输入

当仅提供了用户名但未提供密码时:

```rust
use std::io::{self, Write};
use rpassword::read_password;

fn prompt_credentials() -> Result<AuthMethod> {
    print!("Grafana username: ");
    io::stdout().flush()?;
    let mut user = String::new();
    io::stdin().read_line(&mut user)?;
    let user = user.trim().to_string();
    
    let password = read_password()?;
    
    Ok(AuthMethod::Basic(user, password))
}
```

### 8.3 安全约束

| 约束 | 说明 |
|------|------|
| 禁止 CLI 明文密码 | `--password` 参数使用 `hide_env_values = true`，不接受明文管道传入 |
| 内存清理 | Token/密码使用后通过 Rust `Drop` 确保敏感数据被零化 (如使用 `zeroize` crate) |
| 日志安全 | 错误输出中不包含 Token 或密码内容; 使用 `#[serde(skip_serializing)]` 标记敏感字段 |
| 配置警告 | 若在配置文件中检测到明文 `password` 字段，打印警告 |
| HTTPS 强制 | 若 Grafana URL 使用 HTTP (非 HTTPS)，打印安全警告 (但允许继续) |

---

## 9. 终端输出与格式化规范

### 9.1 输出格式

#### Pretty 模式 (默认)

每行日志格式: `<时间戳> <标签颜色块> <日志行>`

```
2023-10-27 14:32:01.123 [app:nginx] [pod:nginx-7d4f] 192.168.1.1 - GET /api/health 200
2023-10-27 14:32:01.456 [app:nginx] [pod:nginx-7d4f] ERROR: connection refused to backend
```

#### JSON 模式

```json
{
  "timestamp": "2023-10-27T14:32:01.123Z",
  "timestamp_ns": "1698386400000000000",
  "labels": {"app": "nginx", "pod": "nginx-7d4f"},
  "level": "INFO",
  "line": "192.168.1.1 - GET /api/health 200"
}
```

#### Plain 模式

与 Pretty 格式相同但不含 ANSI 转义色码，适合管道重定向到文件。

### 9.2 时间戳格式化

| 设置 | 格式示例 | 说明 |
|------|----------|------|
| 默认 (本地时间) | `2023-10-27 14:32:01.123` | ISO 8601 风格, 含毫秒 |
| UTC | `2023-10-27 06:32:01.123Z` | 若配置或环境变量要求 UTC |

### 9.3 日志级别着色规则

| 级别 | 颜色 | 匹配关键字 (大小写不敏感) |
|------|------|--------------------------|
| `TRACE` | 暗灰色 | `trace` |
| `DEBUG` | 青色 | `debug` |
| `INFO` | 绿色 | `info`, `notice` |
| `WARN` | 黄色 | `warn`, `warning` |
| `ERROR` | 红色 | `error`, `err`, `fatal` |
| `FATAL` | 红底白字 | `fatal`, `critical`, `panic` |
| 未知 | 白色/默认 | - |

检测逻辑: 在日志行中搜索 `level=ERROR`, `"ERROR"`, `[ERROR]`, `ERROR:` 等常见模式。

### 9.4 标签着色方案

使用预设颜色池，按首次遇到的标签自动分配颜色:

```
颜色池 (共 8 种):
  亮蓝、品红、青色、亮绿、亮黄、亮红、亮白、深灰

分配策略:
  标签 "app" → 亮蓝 (第 1 个)
  标签 "pod" → 品红 (第 2 个)
  标签 "namespace" → 青色 (第 3 个)
  ...
  标签池耗尽后复用 (使用 LRU 策略)
```

### 9.5 冻结屏幕交互

- **触发键**: `h` (小写)
- **行为**:
  - 第 1 次按 `h`: 停止自动滚动，日志继续后台接收但不更新屏幕; 终端底部显示 `[PAUSED] 按 h 继续`
  - 第 2 次按 `h`: 恢复自动滚动，直接跳转到最新日志; 移除 `[PAUSED]` 提示

- **实现方案**: 使用 `crossterm::event::poll` 的非阻塞键盘监听 + `AtomicBool` 状态标志

```rust
pub struct ScreenState {
    pub frozen: AtomicBool,
}

impl ScreenState {
    pub fn toggle(&self) -> bool {
        // 原子翻转，返回新状态
        let was = self.frozen.fetch_xor(true, Ordering::SeqCst);
        !was
    }
}
```

- **可复制性**: 使用 `crossterm` 的 `DisableMouseCapture` / `EnableMouseCapture` 配置，确保鼠标选中为终端原生行为，不被 CLI 拦截。日志输出仅写入 stdout，不影响终端对鼠标事件的处理。

### 9.6 重连提示

WebSocket 断线重连时，在 stderr 输出:

```
[graftail] Connection lost. Reconnecting in 2s... (attempt 1/10)
[graftail] Reconnected successfully.
```

不影响 stdout 的日志输出流。

---

## 10. 错误处理设计

### 10.1 错误类型定义

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GraftailError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Invalid timestamp: {0}")]
    Timestamp(String),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection lost after {0} retries")]
    MaxRetriesExceeded(usize),

    #[error("Interrupted by signal")]
    Interrupted,
}
```

### 10.2 重连策略

```
初始退避: 1 秒
最大退避: 60 秒
退避算法: 指数退避 + 随机抖动 (Exponential Backoff with Jitter)
最大重试: 无限 (直到 SIGINT)

公式:
  delay = min(base_delay * 2^attempt + random_jitter, max_delay)
  random_jitter ∈ [0, delay * 0.5]

输出:
  第 1 次: 1s ~ 1.5s
  第 2 次: 2s ~ 3s
  第 3 次: 4s ~ 6s
  ...
  第 10 次+: 60s ~ 90s
```

### 10.3 错误处理策略

| 场景 | 策略 |
|------|------|
| 配置缺失 | 立即退出 (exit code 1)，提示缺失项 |
| 认证失败 (401/403) | 立即退出 (exit code 2)，提示检查凭据 |
| 网络不可达 | 立即退出 (exit code 3) |
| WebSocket 连接断开 | 自动重连 (见 10.2) |
| JSON 解析失败 (单帧) | 跳过该帧，打印 warning 到 stderr，继续运行 |
| 单条日志格式化失败 | 跳过该条目，继续处理后续 |
| SIGINT / SIGTERM | 优雅关闭连接，正常退出 (exit code 0) |

---

## 11. 生命周期与状态管理

### 11.1 状态机

```
                    ┌──────────┐
                    │  START   │
                    └────┬─────┘
                         │
                    ┌────▼─────┐
               ┌────│  CONFIG  │────┐
               │    └────┬─────┘    │
               │   (配置错误)        │ (配置成功)
               │         │          │
          ┌────▼──┐ ┌────▼─────┐    │
          │ ERROR │ │ HIST_QRY │    │ (--last 未指定)
          └───────┘ └────┬─────┘    │
                         │          │
                    ┌────▼─────┐    │
                    │   TAIL   │◄───┘
                    └────┬─────┘
                         │
              ┌──────────┼──────────┐
              │          │          │
         ┌────▼──┐ ┌─────▼────┐ ┌──▼──────┐
         │PAUSED │ │RECONNECT │ │ SHUTDOWN │
         └───┬───┘ └────┬─────┘ └─────────┘
             │          │
             │ (恢复)    │ (重连成功)
             └──────────┘
                ┌────▼─────┐
                │   TAIL   │
                └──────────┘
```

### 11.2 信号处理

```rust
use tokio::signal;
use tokio_util::sync::CancellationToken;

async fn watch_signals(cancel: CancellationToken) {
    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
        .expect("Failed to register SIGINT handler");
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
        .expect("Failed to register SIGTERM handler");

    tokio::select! {
        _ = sigint.recv() => {
            eprintln!("\n[graftail] Received SIGINT, shutting down...");
        }
        _ = sigterm.recv() => {
            eprintln!("\n[graftail] Received SIGTERM, shutting down...");
        }
    }

    cancel.cancel();
}

// Windows 兼容: 使用 ctrl_c handler
#[cfg(windows)]
async fn watch_signals(cancel: CancellationToken) {
    tokio::signal::ctrl_c().await
        .expect("Failed to register Ctrl+C handler");
    eprintln!("\n[graftail] Received Ctrl+C, shutting down...");
    cancel.cancel();
}
```

### 11.3 关闭流程

```
1. 收到退出信号
2. 设置全局 CancellationToken → 触发所有 Task 取消
3. 发送 WebSocket Close 帧 (正常关闭握手)
4. 等待 WS 连接关闭 (timeout: 3 秒)
5. 恢复终端状态 (crossterm::terminal::disable_raw_mode)
6. 清空缓冲区, 释放资源
7. 进程退出 (exit code 0)
```

---

## 12. 构建与发布

### 12.1 依赖清单 (Cargo.toml)

```toml
[package]
name = "graftail"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "graftail"
path = "src/main.rs"

[dependencies]
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive", "env"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
tokio-tungstenite = { version = "0.24", features = ["rustls-tls"] }
tungstenite = { version = "0.24" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
crossterm = "0.28"
owo-colors = { version = "4", features = ["supports-colors"] }
config = "0.14"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
thiserror = "2"
rpassword = "7"
base64 = "0.22"
url = "2"
tokio-util = { version = "0.7", features = ["rt"] }
humantime = "2"
zeroize = { version = "1", features = ["derive"] }
futures-util = "0.3"

[dev-dependencies]
tokio-test = "0.4"
wiremock = "0.6"
pretty_assertions = "1"

[profile.release]
lto = true          # 链接时优化
codegen-units = 1   # 更好的优化
strip = true        # 去除符号表
opt-level = 3       # 最大优化
panic = "abort"     # 移除 panic 展开代码减小体积
```

### 12.2 Rust 版本要求

- MSRV (Minimum Supported Rust Version): **1.75.0**
- Edition: **2021**

### 12.3 跨平台编译

```bash
# Linux (x86_64)
cargo build --release --target x86_64-unknown-linux-gnu

# Linux (aarch64) — 使用 cross 或 cargo-zigbuild
cross build --release --target aarch64-unknown-linux-gnu

# macOS (x86_64 + aarch64 universal binary)
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
lipo -create -output graftail target/{x86_64,aarch64}-apple-darwin/release/graftail

# Windows (x86_64)
cross build --release --target x86_64-pc-windows-msvc
```

### 12.4 目标平台

| 平台 | 架构 | 说明 |
|------|------|------|
| Linux | x86_64 | glibc 2.17+ (兼容主流发行版) |
| Linux | aarch64 | ARM64 服务器 / Apple Silicon Linux |
| macOS | x86_64 | Intel Mac |
| macOS | aarch64 | Apple Silicon (M1/M2/M3) |
| Windows | x86_64 | Windows 10+ |

---

## 13. 测试策略

### 13.1 测试层级

| 层级 | 范围 | 工具 | 目标 |
|------|------|------|------|
| 单元测试 | 每个模块的函数、方法 | `cargo test` | ≥80% 行覆盖率 |
| 集成测试 | API 调用、端到端流程 | `cargo test --test integration` | 覆盖核心路径 |
| Mock 测试 | 外部 API 模拟 | `wiremock` | 离线验证 API 交互 |

### 13.2 关键测试用例

#### 配置模块
- [ ] 默认配置加载成功
- [ ] CLI 参数覆盖配置文件
- [ ] 环境变量覆盖配置文件
- [ ] 缺少必填项时正确报错

#### 认证模块
- [ ] Bearer Token 生成正确 Header
- [ ] Basic Auth 生成正确 Base64 Header
- [ ] 认证优先级: Token > Basic Auth > 交互式
- [ ] 密码不通过日志泄露

#### API 模块
- [ ] Grafana Proxy URL 构建正确 (含 http → ws 转换)
- [ ] Query Range 请求参数拼接正确
- [ ] WebSocket 连接成功 (Mock)
- [ ] WebSocket 断线自动重连 (Mock)
- [ ] 重连达到最大次数后退出

#### 解析模块
- [ ] Loki Tail 响应反序列化成功
- [ ] 纳秒时间戳转换正确
- [ ] 含多个 streams 的响应正确拆分
- [ ] 畸形 JSON 被安全跳过 (不 panic)

#### 输出模块
- [ ] 时间戳格式化 (本地时间 / UTC)
- [ ] 日志级别正确识别并着色
- [ ] 标签颜色分配与复用
- [ ] JSON 输出格式正确
- [ ] Plain 模式无 ANSI 转义码

#### 生命周期
- [ ] SIGINT 优雅退出
- [ ] 退出前发送 WebSocket Close 帧
- [ ] 终端状态恢复
- [ ] 冻结/恢复切换正常

### 13.3 Mock 测试示例

```rust
#[cfg(test)]
mod tests {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path, header};

    #[tokio::test]
    async fn test_query_range_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/datasources/proxy/uid/test-uid/loki/api/v1/query_range"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "status": "success",
                    "data": {
                        "resultType": "streams",
                        "result": []
                    }
                })))
            .mount(&mock_server)
            .await;

        // ... 调用 query_range 并验证结果
    }
}
```

---

## 附录 A: LogQL 语法参考

`graftail` 透传 LogQL 查询到 Loki，不做解析。以下为常用模式:

```logql
# 标签选择器
{app="nginx"}
{app=~"nginx|apache"}

# 行过滤
{app="nginx"} |= "error"
{app="nginx"} != "debug"
{app="nginx"} |~ "(?i)error|fail"

# JSON 解析
{app="api"} | json

# 日志级别过滤
{app="api"} | json | level = "error"
```

## 附录 B: 项目目录结构 (完整)

```
graftail/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
├── .gitignore
├── docs/
│   ├── proposol.md
│   └── software-definition.md          ← 本文档
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── auth.rs
│   ├── error.rs
│   ├── api/
│   │   ├── mod.rs
│   │   ├── grafana_proxy.rs
│   │   ├── query_range.rs
│   │   └── tail.rs
│   ├── stream/
│   │   ├── mod.rs
│   │   └── parser.rs
│   └── output/
│       ├── mod.rs
│       ├── formatter.rs
│       ├── color.rs
│       └── screen.rs
├── tests/
│   ├── integration/
│   │   └── e2e.rs
│   └── fixtures/
│       ├── tail_response.json
│       └── query_range_response.json
└── scripts/
    ├── build.sh
    └── release.sh
```

## 附录 C: 术语表

| 术语 | 说明 |
|------|------|
| Grafana Proxy | Grafana 的数据源代理 API，作为安全网关转发请求到后端数据源 |
| Loki | Grafana Labs 开发的日志聚合系统 |
| LogQL | Loki 的查询语言，类似 PromQL |
| WebSocket | 全双工通信协议，用于实时推送日志 |
| Data Source UID | Grafana 中数据源的唯一标识符 |
| Service Account | Grafana 中用于 API 调用的机器账户 |
| RBAC | 基于角色的访问控制 |
| CancellationToken | tokio 中用于跨任务传播取消信号的同步原语 |

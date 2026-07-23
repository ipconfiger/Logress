这是一份为基于 Rust 开发的、通过 Grafana Proxy 对接 Loki WebSocket 的实时日志追踪 CLI 工具量身定制的**项目定义与需求文档 (PRD)**。

我们将该项目暂定名为 **`graftail`** (Grafana + Tail)。

---

# `graftail` 项目定义与需求文档 (PRD)

## 一、 项目定义 (Project Definition)

### 1.1 项目概述
`graftail` 是一个用 Rust 编写的高性能命令行工具。它通过 Grafana 的数据源代理 API (Data Source Proxy API) 作为安全网关，底层利用 Loki 的 WebSocket Tail 接口，为开发者提供类似 `tail -f` 的实时日志流式追踪体验。

### 1.2 核心目标
1.  **安全合规**：无需在本地 CLI 配置底层 Loki/Elasticsearch 的直连账号密码，完全复用 Grafana 的 RBAC（基于角色的访问控制）和 API Token。
2.  **极致性能**：利用 Rust 的内存安全和零成本抽象，实现低延迟、低内存占用的日志流处理。
3.  **单文件分发**：编译为单一静态链接二进制文件，无需安装运行时环境，方便在各类 CI/CD 管道和开发者本地机器中分发。
4.  **丝滑体验**：提供真正的 WebSocket 实时推送，告别 HTTP 轮询带来的延迟和卡顿；支持 LogQL 语法高亮和终端彩色输出。

### 1.3 目标用户
*   **后端/云原生开发者**：需要频繁排查 Kubernetes 集群或微服务日志，但受限于安全策略无法直接访问底层日志存储的开发人员。
*   **SRE/运维工程师**：需要在生产环境进行故障排查，需要安全、受控的日志查看工具。
*   **DevOps 工程师**：希望将日志追踪集成到自动化脚本或 CI/CD 流水线中。

### 1.4 项目边界 (Non-Goals)
*   **不做**：不实现复杂的 Dashboard 渲染或图表生成。
*   **不做**：不直接连接底层数据源（如直接连 Loki 或 ES），必须且仅通过 Grafana Proxy 进行通信。
*   **不做**：不实现日志的写入、修改或删除操作（纯只读工具）。

---

## 二、 需求文档 (Requirements Document)

### 2.1 功能需求 (Functional Requirements)

#### 2.1.1 核心追踪功能 (Live Tail)
*   **实时流式输出**：建立 WebSocket 连接，实时接收并打印 Loki 推送的新增日志。
*   **LogQL 支持**：支持传入完整的 LogQL 查询语句（如 `{app="nginx"} |= "error"`）。
*   **时间窗口控制**：
    *   支持从“当前时间”开始 Tail（默认）。
    *   支持从“指定历史时间”开始 Tail（如 `--since=1h`）。
*   **历史上下文 (Context)**：支持在开始实时 Tail 前，先拉取最近的 N 条历史日志（类似 `tail -n 100 -f`），然后无缝切换到实时流。

#### 2.1.2 认证与配置管理
*   **Token 认证**：支持通过命令行参数 `--token`、环境变量 `GRAFTAIL_TOKEN` 或配置文件传入 Grafana API Token / Service Account Token。
*   **配置持久化**：支持 `~/.config/graftail/config.toml`，可预设常用的 Grafana 地址、Loki 数据源 UID 和默认查询语句。

#### 2.1.3 终端输出与格式化 (Formatting)
*   **时间戳格式化**：将 Loki 返回的纳秒时间戳转换为人类可读的本地时间或 UTC 时间（如 `2023-10-27 10:00:00.123`）。
*   **标签高亮**：为不同的 Label 键值对（如 `pod="xxx"`, `namespace="yyy"`）分配不同的终端颜色，便于视觉区分。
*   **日志级别着色**：自动识别日志内容中的 `INFO`, `WARN`, `ERROR`, `DEBUG` 等关键字，并进行对应的颜色高亮。
*   **输出格式控制**：支持纯文本输出（默认，适合管道重定向）和 JSON 格式输出（`--output=json`，适合机器解析）。

#### 2.1.4 交互与生命周期
*   **优雅退出**：捕获 `SIGINT` (Ctrl+C) 和 `SIGTERM` 信号，安全关闭 WebSocket 连接并恢复终端状态。
*   **断线重连**：网络波动导致 WebSocket 断开时，自动进行指数退避重连，并在终端输出重连提示，不中断整体运行。

#### 2.1.5 操作上的需求
*  **冻结屏幕**：在按下键盘h键的时候，停止界面的滚动，将日志冻结到当前状态，再按一次h恢复滚动，并直接定位到最新的日志
*  **可复制性**：鼠标选中的部分文本可以直接复制到剪贴板，或者不能影响在终端被复制

### 2.2 非功能需求 (Non-Functional Requirements)

*   **性能**：在每秒处理 10,000 条日志的峰值下，CLI 进程的 CPU 占用率不超过 15%，内存占用不超过 50MB。
*   **兼容性**：支持 Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)。
*   **安全性**：Token 等敏感信息在内存中处理完毕后应及时清理（Rust 的 `Drop` 特性可辅助），不在日志或错误堆栈中泄露 Token。

---

## 三、 技术架构与 Rust 选型

### 3.1 整体工作流
1.  **解析参数**：`clap` 解析 CLI 参数，合并配置文件。
2.  **历史查询 (可选)**：如果指定了 `--last`，通过 HTTP GET 调用 Grafana Proxy 的 Loki `query_range` 接口获取历史数据，格式化输出。
3.  **建立长连接**：通过 HTTP Upgrade 调用 Grafana Proxy 的 Loki `tail` WebSocket 接口。
4.  **流式处理**：`tokio` 异步接收 WebSocket 帧 -> `serde_json` 反序列化 -> 提取 Label 和 Log Line -> 终端着色 -> 打印。
5.  **信号处理**：`tokio::signal` 监听退出信号，触发清理逻辑。

### 3.2 核心 Rust 依赖选型

| 模块 | 推荐 Crate | 选型理由 |
| :--- | :--- | :--- |
| **异步运行时** | `tokio` | Rust 生态事实标准，高性能异步 I/O。 |
| **CLI 解析** | `clap` (v4) | 功能强大，支持 derive 宏，自动生成帮助文档。 |
| **HTTP 客户端** | `reqwest` | 用于初始的历史日志查询和配置校验，支持 rustls。 |
| **WebSocket** | `tokio-tungstenite` | 基于 tokio 的轻量级 WebSocket 实现，支持自定义 HTTP Upgrade 头（传 Token）。 |
| **序列化** | `serde`, `serde_json` | 处理 Loki 返回的 JSON 数据流。 |
| **终端控制/着色**| `crossterm`, `owo-colors`| `crossterm` 处理跨平台终端信号和状态；`owo-colors` 提供零分配、高性能的终端颜色渲染。 |
| **配置管理** | `config`, `toml` | 优雅地合并命令行参数、环境变量和配置文件。 |
| **错误处理** | `anyhow`, `thiserror` | `anyhow` 用于应用级错误包装，`thiserror` 用于库级自定义错误。 |
| **时间处理** | `chrono` | 处理 Loki 的纳秒时间戳与本地时间的转换。 |

---

## 四、 关键技术资料与 API 引用

在开发过程中，开发者需要频繁查阅以下官方文档：

### 4.1 Grafana Data Source Proxy API
*   **作用**：作为安全网关，转发请求到 Loki。
*   **文档链接**：[Grafana HTTP API - Data Source Proxy](https://grafana.com/docs/grafana/latest/developers/http_api/datasource_proxy/)
*   **核心知识点**：
    *   URL 模板：`/api/datasources/proxy/uid/<datasource_uid>/<loki_api_path>`
    *   认证方式：Header `Authorization: Bearer <grafana_api_token>`
    *   **WebSocket 支持**：Grafana Proxy 支持将 HTTP 请求升级为 WebSocket，前提是请求头包含正确的 `Upgrade: websocket` 和 `Connection: Upgrade`（`tokio-tungstenite` 会自动处理）。

### 4.2 Loki Tail WebSocket API
*   **作用**：提供实时的日志流推送。
*   **文档链接**：[Loki HTTP API - Tail](https://grafana.com/docs/loki/latest/reference/loki-http-api/#tail-logs) (注意：需结合 Grafana Proxy 路径使用)
*   **核心参数 (Query Parameters)**：
    *   `query` (string, 必填): LogQL 查询语句。需进行 URL 编码。
    *   `start` (int, 可选): 开始追踪的纳秒时间戳。如果不传，默认从连接建立时开始。
    *   `limit` (int, 可选): 每次返回的最大 stream 数量（默认 100）。
    *   `delay_for` (int, 可选): 延迟查询的秒数（用于等待延迟到达的日志，默认 0）。
*   **响应数据结构 (JSON)**：
    ```json
    {
      "streams": [
        {
          "stream": {
            "filename": "/var/log/syslog",
            "job": "varlogs"
          },
          "values": [
            [
              "1698386400000000000", // 纳秒时间戳 (字符串格式)
              "This is a log line"   // 日志内容
            ]
          ]
        }
      ]
    }
    ```

### 4.3 Loki Query Range API (用于历史上下文)
*   **作用**：在建立 WebSocket Tail 之前，拉取最近的历史日志。
*   **文档链接**：[Loki HTTP API - Query](https://grafana.com/docs/loki/latest/reference/loki-http-api/#query-logs-at-a-single-timestamp) / [Query Range](https://grafana.com/docs/loki/latest/reference/loki-http-api/#query-logs-within-a-range-of-time)
*   **核心参数**：`query`, `limit`, `start`, `end`, `direction` (设为 `backward` 以获取最新的 N 条)。

### 4.4 Rust 生态参考
*   **tokio-tungstenite 自定义 Header 示例**：[GitHub - tokio-tungstenite examples](https://github.com/snapview/tokio-tungstenite/blob/master/examples/autobahn-client.rs) (重点看如何传入 `Authorization` header)。
*   **owo-colors 终端着色**：[Docs.rs - owo-colors](https://docs.rs/owo-colors/latest/owo_colors/)
*   **clap v4 Derive 模式**：[Docs.rs - clap](https://docs.rs/clap/latest/clap/_derive/index.html)

---

## 五、 开发里程碑建议 (Milestones)

1.  **M1: 基础骨架与认证 (Week 1)**
    *   实现 `clap` 参数解析和 `config` 配置读取。
    *   实现通过 `reqwest` 调用 Grafana Proxy 验证 Token 有效性及获取数据源信息。
2.  **M2: 历史日志查询 (Week 2)**
    *   实现调用 Loki `query_range` API。
    *   实现基础的时间戳转换和终端纯文本输出。
3.  **M3: 核心 WebSocket Tail (Week 3)**
    *   使用 `tokio-tungstenite` 建立 Grafana Proxy WebSocket 连接。
    *   实现 JSON 流式解析和实时打印。
    *   实现断线重连机制。
4.  **M4: 终端美化与工程化 (Week 4)**
    *   引入 `owo-colors` 实现 Label 和日志级别高亮。
    *   实现 `SIGINT` 优雅退出。
    *   编写 README，提供跨平台编译脚本 (使用 `cross` 或 `cargo-zigbuild`)。


**完全可以。** 

Grafana 的 HTTP API（包括 Data Source Proxy API 和底层的 WebSocket 升级请求）原生支持 **HTTP Basic Authentication（基本认证）**。

如果你不想或无法在 Grafana 中创建 API Token / Service Account，你可以直接使用 Grafana 的**本地登录用户名和密码**（例如默认的 `admin` / `admin`，或者你个人的账号）来调用接口。

以下是关于在 `graftail` (Rust CLI) 项目中使用用户名密码的详细技术指南和注意事项：

---

### 一、 技术实现：如何在 Rust 中传递 Basic Auth

HTTP Basic Auth 的原理是将 `username:password` 进行 Base64 编码，然后放入 HTTP Header 的 `Authorization: Basic <base64_string>` 中。

在 Rust 中，你不需要手动去算 Base64，主流的 HTTP 和 WebSocket 库都提供了优雅的封装。

#### 1. 对于 HTTP 请求 (如查询历史日志 `query_range`)
使用 `reqwest` 时，直接调用 `.basic_auth()` 方法即可：

```rust
use reqwest;

async fn fetch_history_logs(grafana_url: &str, loki_uid: &str, user: &str, password: &str) -> Result<String, reqwest::Error> {
    let url = format!("{}/api/datasources/proxy/uid/{}/loki/api/v1/query_range?query=...", grafana_url, loki_uid);
    
    let client = reqwest::Client::new();
    let res = client
        .get(&url)
        // 核心：直接传入用户名和密码，reqwest 会自动处理 Base64 和 Header
        .basic_auth(user, Some(password)) 
        .send()
        .await?;
        
    Ok(res.text().await?)
}
```

#### 2. 对于 WebSocket 连接 (核心 Tail 功能)
使用 `tokio-tungstenite` 时，你需要在构建 WebSocket 升级请求时手动注入 Header。

```rust
use tokio_tungstenite::{connect_async, tungstenite::http::Request};
use url::Url;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine}; // 需要引入 base64 crate

async fn tail_logs_ws(grafana_url: &str, loki_uid: &str, user: &str, password: &str) {
    // 1. 构造 WebSocket URL
    let ws_url = format!("{}/api/datasources/proxy/uid/{}/loki/api/v1/tail?query=...", 
                         grafana_url.replace("http", "ws"), loki_uid);
    let url = Url::parse(&ws_url).unwrap();

    // 2. 手动构造 Basic Auth 字符串
    let credentials = format!("{}:{}", user, password);
    let auth_header_value = format!("Basic {}", BASE64.encode(credentials));

    // 3. 构建带有自定义 Header 的 HTTP 请求
    let request = Request::builder()
        .uri(url.as_str())
        .header("Authorization", auth_header_value)
        // tungstenite 需要这些 header 来发起 WebSocket 升级
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", tungstenite::handshake::client::generate_key())
        .body(())
        .unwrap();

    // 4. 发起连接
    match connect_async(request).await {
        Ok((mut ws_stream, _)) => {
            println!("WebSocket 连接成功，开始接收日志...");
            // 后续处理 ws_stream...
        }
        Err(e) => {
            eprintln!("WebSocket 连接失败: {}", e);
        }
    }
}
```

---

### 二、 使用用户名密码的优缺点分析

虽然技术上完全可行，但在工程实践中，使用密码和使用 Token 有明显的区别：

#### 优点：
1. **开箱即用**：不需要去 Grafana UI 里找菜单创建 Service Account，只要你有能登录 Grafana 的账号密码就能直接跑 CLI。
2. **权限等同**：Basic Auth 使用的账号拥有什么权限（Admin/Editor/Viewer），CLI 就拥有什么权限。

#### 缺点与风险（需要特别注意）：
1. **密码明文暴露风险（最大痛点）**：
   * 如果你通过命令行参数传密码（如 `graftail -u admin -p mypassword`），密码会留在 Shell 的历史记录中（如 `~/.bash_history`），极易泄露。
   * **缓解方案**：强制要求通过环境变量（如 `GRAFTAIL_PASSWORD`）或交互式提示（如 `rpassword` crate 隐藏输入）来传递密码，**绝对不要**在 CLI 参数中直接写明文密码。
2. **SSO / LDAP / MFA 冲突**：
   * 如果你的 Grafana 接入了公司统一的 OAuth/OIDC（如飞书、钉钉、Okta 登录），或者开启了强制两步验证（2FA），**Basic Auth 可能会失效**，或者只能使用特定的“应用密码”。
   * Token (Service Account) 则不受 SSO 登录流程的影响，专门用于 API 调用。
3. **审计追踪粒度**：
   * 使用个人账号密码，Grafana 审计日志会记录“用户张三”在查询。
   * 使用 Service Account Token，可以记录为“自动化脚本/CLI工具”在查询，便于区分人工操作和机器操作。

---

### 三、 给 `graftail` 项目的架构建议

为了让你的 CLI 工具既灵活又安全，建议在需求文档中做如下兼容设计：

1. **认证方式优先级**：
   在 CLI 设计时，支持多种认证方式，并设定优先级：
   * **优先级 1**：Service Account Token (通过 `--token` 或 `GRAFTAIL_TOKEN` 环境变量)。
   * **优先级 2**：Basic Auth (通过 `--user` / `--password` 或 `GRAFTAIL_USER` / `GRAFTAIL_PASSWORD` 环境变量)。

2. **交互式密码输入（强烈推荐加入）**：
   如果用户没有配置环境变量，且没有传 Token，CLI 应该提示用户输入密码，并且**输入时不回显（隐藏字符）**。
   * **Rust 实现**：使用 `rpassword` crate。
   ```rust
   // 伪代码
   if token.is_none() && password.is_none() {
       let user = prompt("Enter Grafana username: ");
       let pass = rpassword::prompt_password("Enter Grafana password: ").unwrap();
       // 使用 user 和 pass 进行 Basic Auth
   }
   ```

3. **配置文件支持**：
   允许在 `~/.config/graftail/config.toml` 中保存配置，但**强烈建议在文档中警告用户不要将明文密码写入配置文件**，或者在写入配置文件时进行简单的加密/混淆（虽然防君子不防小人，但比纯明文好）。

### 总结

**可以用用户名密码，Grafana 完美支持 Basic Auth。** 
在 Rust 中实现起来也非常简单（`reqwest` 和 `tokio-tungstenite` 都支持）。但为了 CLI 工具的安全性和企业级兼容性（应对 SSO/MFA），**首选依然是 Service Account Token**，将用户名密码作为备选方案，并务必做好密码的隐藏输入和环境变量读取。
Vertical 2:3 Instagram travel poster for [LOCATION], part of a collectible handcrafted travel-art series. No title, headline, slogan, or caption anywhere in the image.

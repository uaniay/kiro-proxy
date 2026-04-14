# kiro-proxy

[English](README.md)

轻量级 Kiro API 代理，兼容 OpenAI 和 Anthropic 接口格式。支持单用户代理模式和多用户模式（Web UI、用户独立 Kiro Token 绑定、管理员 Token Pool 负载均衡）。

## 功能特性

- **双 API 格式** — 兼容 OpenAI (`/v1/chat/completions`) 和 Anthropic (`/v1/messages`)
- **流式响应** — Server-Sent Events (SSE) 实时输出
- **多用户模式** — SQLite 数据库 + Web UI（React + shadcn/ui）
- **用户独立 API Key** — 每个用户创建自己的 Key，带用量统计（请求数、Token 用量）
- **Kiro Token 绑定** — 用户通过 AWS SSO Device Code Flow 绑定自己的凭证
- **管理员 Token Pool** — 多账号轮询负载均衡
- **自动 Token 刷新** — 后台每 5 分钟刷新即将过期的 Token
- **重试机制** — 429/5xx 错误指数退避重试
- **截断恢复** — 检测并恢复被截断的 API 响应
- **向后兼容** — 不配置数据库时作为简单的单用户代理运行

## 架构

```
                          ┌─────────────────────────────────┐
                          │         kiro-proxy              │
                          │                                 │
  客户端 (curl/SDK)        │  ┌──────────┐  ┌────────────┐  │
  ───────────────────────►│  │ 中间件    │  │  格式转换   │  │     Kiro API
  Authorization: Bearer   │  │ (认证)    │─►│ OpenAI/    │──│────► (AWS)
  sk-xxx / PROXY_KEY      │  │          │  │ Anthropic  │  │
                          │  └──────────┘  │  → Kiro     │  │
                          │       │        └────────────┘  │
  浏览器                   │       ▼                        │
  ───────────────────────►│  ┌──────────┐                  │
  /_ui/                   │  │  SQLite  │                  │
                          │  │ (用户、   │                  │
                          │  │  密钥、   │                  │
                          │  │  Token)  │                  │
                          │  └──────────┘                  │
                          └─────────────────────────────────┘
```

**请求路由优先级：**
1. 用户自己的 Kiro Token（如已绑定）
2. 管理员 Token Pool（轮询）
3. 全局 `PROXY_API_KEY` 兜底

## 快速开始

### 环境要求

- [Rust](https://rustup.rs/) 1.75+
- [Node.js](https://nodejs.org/) 18+（仅多用户模式需要，用于构建前端）

### 单用户代理模式（无需数据库）

```bash
git clone https://github.com/uaniay/kiro-proxy.git
cd kiro-proxy

cp .env.example .env
# 编辑 .env：设置 PROXY_API_KEY（至少 16 个字符）
# 可选：设置 KIRO_REFRESH_TOKEN、KIRO_CLIENT_ID、KIRO_CLIENT_SECRET

cargo run --release
# 监听 http://localhost:9199
```

### 多用户模式（带 Web UI）

```bash
git clone https://github.com/uaniay/kiro-proxy.git
cd kiro-proxy

# 构建前端
cd frontend && npm install && npm run build && cd ..

cp .env.example .env
# 编辑 .env：
#   PROXY_API_KEY=your-secure-key-here
#   DATABASE_URL=sqlite:data/kiro-proxy.db?mode=rwc

mkdir -p data
cargo run --release
# API:    http://localhost:9199
# Web UI: http://localhost:9199/_ui/
```

打开 `http://localhost:9199/_ui/`，注册的第一个用户自动成为管理员。

### Docker 部署

```bash
cp .env.example .env
# 编辑 .env
docker compose up -d
# Web UI: http://localhost:9199/_ui/
```

## 接口调用示例

### OpenAI 格式

**非流式请求：**

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "messages": [
      {"role": "system", "content": "你是一个有用的助手。"},
      {"role": "user", "content": "法国的首都是哪里？"}
    ],
    "max_tokens": 1024,
    "stream": false
  }'
```

**流式请求：**

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "messages": [
      {"role": "user", "content": "写一首关于编程的俳句"}
    ],
    "stream": true
  }'
```

**获取模型列表：**

```bash
curl http://localhost:9199/v1/models
```

**Python（OpenAI SDK）：**

```python
from openai import OpenAI

client = OpenAI(
    api_key="YOUR_API_KEY",
    base_url="http://localhost:9199/v1"
)

response = client.chat.completions.create(
    model="claude-sonnet-4",
    messages=[{"role": "user", "content": "你好！"}],
)
print(response.choices[0].message.content)
```

### Anthropic 格式

**非流式请求：**

```bash
curl http://localhost:9199/v1/messages \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "messages": [
      {"role": "user", "content": "用简单的语言解释量子计算。"}
    ],
    "stream": false
  }'
```

**流式请求：**

```bash
curl http://localhost:9199/v1/messages \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "messages": [
      {"role": "user", "content": "写一个关于机器人的短故事。"}
    ],
    "stream": true
  }'
```

**带系统提示和工具调用：**

```bash
curl http://localhost:9199/v1/messages \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "system": "你是一个天气助手。",
    "messages": [
      {"role": "user", "content": "东京的天气怎么样？"}
    ],
    "tools": [
      {
        "name": "get_weather",
        "description": "获取指定地点的当前天气",
        "input_schema": {
          "type": "object",
          "properties": {
            "location": {"type": "string"}
          },
          "required": ["location"]
        }
      }
    ]
  }'
```

**Python（Anthropic SDK）：**

```python
import anthropic

client = anthropic.Anthropic(
    api_key="YOUR_API_KEY",
    base_url="http://localhost:9199"
)

message = client.messages.create(
    model="claude-sonnet-4",
    max_tokens=1024,
    messages=[{"role": "user", "content": "你好！"}],
)
print(message.content[0].text)
```

## 可用模型

| 模型 | 说明 |
|------|------|
| `claude-sonnet-4` | Claude Sonnet 4 |
| `claude-sonnet-4-5` | Claude Sonnet 4.5 |
| `claude-haiku-4` | Claude Haiku 4 |
| `claude-haiku-4-5` | Claude Haiku 4.5 |
| `claude-opus-4` | Claude Opus 4 |
| `claude-opus-4-6` | Claude Opus 4.6 |

## 配置项

| 变量 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `PROXY_API_KEY` | 是 | — | 代理认证密钥（至少 16 个字符） |
| `DATABASE_URL` | 否 | — | SQLite 数据库 URL，设置后启用多用户模式 |
| `KIRO_REFRESH_TOKEN` | 否 | — | AWS SSO 刷新令牌（单用户模式） |
| `KIRO_CLIENT_ID` | 否 | — | AWS SSO OAuth 客户端 ID |
| `KIRO_CLIENT_SECRET` | 否 | — | AWS SSO OAuth 客户端密钥 |
| `KIRO_REGION` | 否 | `us-east-1` | Kiro API 的 AWS 区域 |
| `KIRO_SSO_REGION` | 否 | 同 `KIRO_REGION` | SSO OIDC 端点的 AWS 区域 |
| `SERVER_HOST` | 否 | `0.0.0.0` | 监听地址 |
| `SERVER_PORT` | 否 | `9199` | 监听端口 |
| `LOG_LEVEL` | 否 | `info` | 日志级别（trace/debug/info/warn/error） |

## 多用户模式

设置 `DATABASE_URL` 后，代理启用以下功能：

- **Web UI** — `/_ui/` 路径，注册、登录、管理 API Key、绑定 Kiro Token
- **首个用户 = 管理员** — 第一个注册的用户自动获得管理员权限
- **用户独立 API Key** — `sk-` 前缀的密钥，带用量统计（请求数、输入/输出 Token）
- **Kiro Token 绑定** — 每个用户可通过 AWS SSO Device Code Flow 绑定自己的凭证
- **管理员 Token Pool** — 管理员可添加多个 Kiro 账号，轮询负载均衡
- **后台任务** — Token 刷新（每 5 分钟）、Session 清理（每 1 小时）

### Web UI 接口

| 端点 | 说明 |
|------|------|
| `/_ui/` | Web UI（React SPA） |
| `/_ui/api/auth/register` | 注册新用户 |
| `/_ui/api/auth/login` | 登录 |
| `/_ui/api/auth/me` | 当前用户信息 |
| `/_ui/api/keys` | API Key 管理 |
| `/_ui/api/kiro/setup` | 发起 Kiro Token 绑定 |
| `/_ui/api/kiro/status` | Token 状态 |
| `/_ui/api/admin/users` | 用户管理（管理员） |
| `/_ui/api/admin/pool` | Token Pool 管理（管理员） |
| `/_ui/api/admin/usage` | 用量统计（管理员） |

## 项目结构

```
kiro-proxy/
├── src/
│   ├── main.rs              # 入口
│   ├── config.rs             # 环境变量配置
│   ├── error.rs              # 错误类型
│   ├── db.rs                 # SQLite 数据库层
│   ├── pool.rs               # Token Pool 轮询调度器
│   ├── tasks.rs              # 后台任务（Token 刷新、Session 清理）
│   ├── middleware.rs          # 认证中间件（API Key + 多用户）
│   ├── http_client.rs         # HTTP 客户端（带重试）
│   ├── tokenizer.rs          # Token 计数
│   ├── thinking_parser.rs    # 思考块提取
│   ├── truncation.rs         # 截断检测与恢复
│   ├── auth/                 # AWS SSO 认证
│   ├── models/               # OpenAI/Anthropic/Kiro 数据类型
│   ├── converters/           # 格式转换（OpenAI↔Kiro、Anthropic↔Kiro）
│   ├── streaming/            # AWS Event Stream 解析 + SSE
│   ├── routes/               # API 路由处理
│   └── web_ui/               # Web UI 后端（认证、密钥、Kiro 绑定、管理）
├── frontend/                 # React + Vite + Tailwind + shadcn/ui
├── migrations/               # SQLite 数据库迁移
├── Dockerfile                # 多阶段构建
├── docker-compose.yml
└── .env.example
```

## 贡献

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送分支 (`git push origin feature/amazing-feature`)
5. 发起 Pull Request

## 许可证

[MIT](LICENSE)

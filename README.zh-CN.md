# kiro-proxy

[English](README.md)

轻量级 Kiro API 代理，兼容 OpenAI 和 Anthropic 接口格式。支持单用户代理模式和多用户模式（Web UI、用户独立 Kiro Token 绑定、管理员 Token Pool 负载均衡、统一账号管理）。

## 功能特性

- **双 API 格式** — 兼容 OpenAI (`/v1/chat/completions`) 和 Anthropic (`/v1/messages`)
- **流式响应** — Server-Sent Events (SSE) 实时输出
- **多用户模式** — SQLite 数据库 + Web UI（React + shadcn/ui）
- **用户独立 API Key** — 每个用户创建自己的 Key，带用量统计（请求数、Token 用量）
- **Kiro Token 绑定** — 用户通过 AWS SSO Device Code Flow 绑定自己的凭证
- **管理员 Token Pool** — 多账号轮询负载均衡，通过 Device Code Flow 添加
- **Token 共享** — 管理员可将用户 Token 标记为共享，参与 Pool 轮询分配
- **统一账号管理** — 管理员面板统一查看和控制所有 Kiro 账号（全局、用户、Pool），支持启用/禁用
- **用量追踪** — 按 Key 统计请求数、输入/输出 Token，流式和非流式请求均记录
- **自动 Token 刷新** — 后台每 5 分钟刷新即将过期的 Token
- **用户审批流程** — 新用户注册后需管理员审批才能使用 API
- **重试机制** — 429/5xx 错误指数退避重试
- **截断恢复** — 检测并恢复被截断的 API 响应
- **对话记录** — 可选的异步请求/响应完整记录，带管理员查看界面（对代理转发零延迟影响）
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
1. 用户自己的 Kiro Token（如已绑定且启用）
2. 管理员 Token Pool + 共享用户 Token（轮询）
3. 全局环境变量账号（如启用）

## 快速开始

### Docker 部署（推荐）

```bash
git clone https://github.com/uaniay/kiro-proxy.git
cd kiro-proxy

cp .env.example .env
# 编辑 .env：设置 PROXY_API_KEY（至少 16 个字符）

docker compose up -d
# Web UI: http://localhost:9199/_ui/
```

数据库存储在 Docker 命名卷（`kiro-data`）中，容器重建不会丢失数据。更新方式：

```bash
git pull
docker compose up --build -d    # 数据保留
# docker compose down -v        # 注意：这会删除所有数据！
```

### 从源码构建

#### 环境要求

- [Rust](https://rustup.rs/) 1.75+
- [Node.js](https://nodejs.org/) 18+（用于构建前端）

#### 单用户代理模式（无需数据库）

```bash
git clone https://github.com/uaniay/kiro-proxy.git
cd kiro-proxy

cp .env.example .env
# 编辑 .env：设置 PROXY_API_KEY（至少 16 个字符）
# 可选：设置 KIRO_REFRESH_TOKEN、KIRO_CLIENT_ID、KIRO_CLIENT_SECRET

cargo run --release
# 监听 http://localhost:9199
```

#### 多用户模式（带 Web UI）

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

## 配置项

| 变量 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `PROXY_API_KEY` | 是 | — | 代理认证密钥（至少 16 个字符） |
| `DATABASE_URL` | 否 | — | SQLite 数据库 URL，设置后启用多用户模式（如 `sqlite:/data/kiro-proxy.db?mode=rwc`） |
| `KIRO_REFRESH_TOKEN` | 否 | — | AWS SSO 刷新令牌（单用户模式） |
| `KIRO_CLIENT_ID` | 否 | — | AWS SSO OAuth 客户端 ID |
| `KIRO_CLIENT_SECRET` | 否 | — | AWS SSO OAuth 客户端密钥 |
| `KIRO_REGION` | 否 | `us-east-1` | Kiro API 的 AWS 区域 |
| `KIRO_SSO_REGION` | 否 | 同 `KIRO_REGION` | SSO OIDC 端点的 AWS 区域 |
| `SERVER_HOST` | 否 | `0.0.0.0` | 监听地址 |
| `SERVER_PORT` | 否 | `9199` | 监听端口 |
| `LOG_LEVEL` | 否 | `info` | 日志级别（trace/debug/info/warn/error） |
| `ENABLE_CONVERSATION_LOG` | 否 | `false` | 启用完整请求/响应记录（设为 `true` 启用） |

## 多用户模式

设置 `DATABASE_URL` 后，代理启用完整的多用户功能。

### 初始配置

1. 启动服务，打开 `http://YOUR_HOST:9199/_ui/`
2. 注册第一个账号 — 自动成为管理员
3. 后续用户注册后需等待管理员审批

### 用户操作流程

1. **注册登录** — 在 `/_ui/` 创建账号，等待管理员审批
2. **创建 API Key** — 进入 Profile 页面，创建 API Key（每用户最多 10 个）
3. **绑定 Kiro Token** — 在 Profile 页面点击"绑定 Kiro Token"，按照 Device Code Flow 操作：
   - 页面显示一个授权码和链接
   - 在浏览器中打开链接，输入授权码，用 AWS 账号授权
   - Token 自动保存，后台定时刷新
4. **使用 API** — 用你的 API Key 配合任何 OpenAI/Anthropic 兼容客户端使用

### 管理员操作

管理员面板位于 `/_ui/admin`，包含五个 Tab：

#### Users（用户管理）
- 查看所有注册用户及状态（active/pending/rejected）
- 审批或拒绝待审核用户
- 删除用户（不能删除自己）

#### Usage（用量统计）
- 查看所有用户的 API Key 用量
- 显示用户邮箱、Key 前缀（点击可复制）、请求数、输入/输出 Token、最后使用时间

#### Token Pool（令牌池）
- 查看所有管理员管理的 Pool 条目及启用/禁用状态
- **通过 Device Code Flow 添加 Pool 条目**：输入标签，点击"Authorize"，在浏览器中完成授权
- 启用/禁用或删除 Pool 条目
- Pool 条目参与轮询负载均衡，为没有自己 Token 的用户提供服务

#### Kiro Accounts（Kiro 账号管理）
- 统一查看三种账号类型：Global（环境变量）、User（用户绑定）、Pool（管理员添加）
- **启用/禁用**任意账号 — 禁用的账号在请求路由时被跳过
- **共享用户 Token 到 Pool**：勾选用户，点击"Share to Pool"
  - 共享的 Token 与管理员 Pool 条目一起参与轮询
  - 原用户仍然优先使用自己的 Token
  - 点击"Unshare"取消共享

#### Conversations（对话记录）
- 查看所有 API 请求/响应的完整记录（需设置 `ENABLE_CONVERSATION_LOG=true`）
- 支持按内容关键词搜索或按 API Key ID 筛选
- 分页列表（默认每页 10 条），显示时间、用户、模型、API 类型、流式模式、Token 数、耗时
- 点击任意行展开完整的请求体、响应体和脱敏后的请求头（Authorization、Cookie 等敏感头已自动过滤）
- 支持删除单条对话记录
- 管理员在 Profile 页面的 API Key 列表中可点击"Logs"直接跳转到该 Key 的对话记录

### Token 解析优先级

当请求携带用户的 API Key 时：

1. **用户自己的 Token** — 如果用户已绑定 Kiro Token 且启用，优先使用
2. **Pool 轮询** — 管理员 Pool 条目 + 共享用户 Token，按请求轮询
3. **全局兜底** — 环境变量 `KIRO_REFRESH_TOKEN` 配置的账号（如启用）
4. **错误** — 如果都不可用，返回 `KiroTokenRequired` 错误

## 接口调用示例

### OpenAI 格式

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "messages": [{"role": "user", "content": "你好！"}],
    "stream": true
  }'
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

```bash
curl http://localhost:9199/v1/messages \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "你好！"}],
    "stream": true
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

### 获取模型列表

```bash
curl http://localhost:9199/v1/models
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

## API 端点

### 代理端点

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/v1/models` | 获取可用模型列表 |
| POST | `/v1/chat/completions` | OpenAI 兼容聊天（需 API Key） |
| POST | `/v1/messages` | Anthropic 兼容消息（需 API Key） |
| GET | `/health` | 健康检查 |

### Web UI 端点（`/_ui/api/` 下）

| 方法 | 端点 | 认证 | 说明 |
|------|------|------|------|
| POST | `/auth/register` | 公开 | 注册新用户 |
| POST | `/auth/login` | 公开 | 登录 |
| GET | `/auth/me` | Session | 当前用户信息 |
| POST | `/auth/logout` | Session | 登出 |
| GET | `/keys` | Session | 列出用户的 API Key |
| POST | `/keys` | Session | 创建 API Key |
| DELETE | `/keys/:id` | Session | 删除 API Key |
| POST | `/kiro/setup` | Session | 发起 Device Code Flow |
| POST | `/kiro/poll` | Session | 轮询设备授权 |
| GET | `/kiro/status` | Session | Token 状态 |
| DELETE | `/kiro/token` | Session | 删除 Kiro Token |
| GET | `/admin/users` | Admin | 列出所有用户 |
| DELETE | `/admin/users/:id` | Admin | 删除用户 |
| POST | `/admin/users/:id/approve` | Admin | 审批用户 |
| POST | `/admin/users/:id/reject` | Admin | 拒绝用户 |
| POST | `/admin/users/share` | Admin | 批量共享/取消共享用户 Token |
| GET | `/admin/pool` | Admin | 列出 Token Pool |
| POST | `/admin/pool` | Admin | 添加 Pool 条目（手动） |
| POST | `/admin/pool/setup` | Admin | 发起 Pool Device Code Flow |
| POST | `/admin/pool/poll` | Admin | 轮询 Pool 设备授权 |
| DELETE | `/admin/pool/:id` | Admin | 删除 Pool 条目 |
| PATCH | `/admin/pool/:id` | Admin | 切换 Pool 条目启用/禁用 |
| GET | `/admin/usage` | Admin | 用量统计 |
| GET | `/admin/accounts` | Admin | 列出所有 Kiro 账号 |
| PATCH | `/admin/accounts/:id` | Admin | 切换账号启用/禁用 |
| GET | `/admin/conversations` | Admin | 列出对话记录（支持搜索/分页） |
| GET | `/admin/conversations/:id` | Admin | 获取完整对话详情 |
| DELETE | `/admin/conversations/:id` | Admin | 删除对话记录 |

## 项目结构

```
kiro-proxy/
├── src/
│   ├── main.rs              # 入口
│   ├── config.rs             # 环境变量配置
│   ├── conversation_log.rs   # 对话记录请求头脱敏
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
├── migrations/               # SQLite 数据库迁移（001-007）
├── Dockerfile                # 多阶段构建
├── docker-compose.yml
└── .env.example
```

## 许可证

[MIT](LICENSE)

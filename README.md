# kiro-proxy

[中文文档](README.zh-CN.md)

Lightweight Kiro API proxy with OpenAI and Anthropic compatible endpoints. Supports single-user proxy mode and multi-user mode with Web UI, per-user Kiro token binding, admin token pool load balancing, and unified account management.

## Features

- **Dual API Format** — OpenAI (`/v1/chat/completions`) and Anthropic (`/v1/messages`) compatible
- **Streaming** — Server-Sent Events (SSE) for real-time responses
- **Multi-user Mode** — SQLite-backed user management with Web UI (React + shadcn/ui)
- **Per-user API Keys** — Each user creates their own keys with usage tracking (request count, token usage)
- **Kiro Token Binding** — Users bind their own AWS SSO credentials via device code flow
- **Admin Token Pool** — Round-robin load balancing across multiple Kiro accounts, added via device code flow
- **Token Sharing** — Admin can mark user tokens as shared to participate in pool round-robin
- **Unified Account Management** — Admin dashboard to view and control all Kiro accounts (global, user, pool) with enable/disable toggles
- **Usage Tracking** — Per-key request count, input/output token tracking for both streaming and non-streaming requests
- **Auto Token Refresh** — Background task refreshes expiring tokens every 5 minutes
- **User Approval Workflow** — New users require admin approval before accessing the API
- **Retry Logic** — Exponential backoff on 429/5xx errors
- **Truncation Recovery** — Detects and recovers from truncated API responses
- **Conversation Logging** — Optional async logging of full request/response bodies with admin viewer (zero proxy latency impact)
- **Backward Compatible** — Works as a simple single-user proxy without a database

## Architecture

```
                          ┌─────────────────────────────────┐
                          │         kiro-proxy              │
                          │                                 │
  Client (curl/SDK)       │  ┌──────────┐  ┌────────────┐  │
  ───────────────────────►│  │ Middleware│  │  Converter  │  │     Kiro API
  Authorization: Bearer   │  │ (Auth)   │─►│ OpenAI/    │──│────► (AWS)
  sk-xxx / PROXY_KEY      │  │          │  │ Anthropic  │  │
                          │  └──────────┘  │  → Kiro     │  │
                          │       │        └────────────┘  │
  Browser                 │       ▼                        │
  ───────────────────────►│  ┌──────────┐                  │
  /_ui/                   │  │  SQLite  │                  │
                          │  │ (users,  │                  │
                          │  │  keys,   │                  │
                          │  │  tokens) │                  │
                          │  └──────────┘                  │
                          └─────────────────────────────────┘
```

**Request routing priority:**
1. User's own Kiro token (if bound and enabled)
2. Admin token pool + shared user tokens (round-robin)
3. Global environment variable fallback (if enabled)

## Quick Start

### Docker (Recommended)

```bash
git clone https://github.com/uaniay/kiro-proxy.git
cd kiro-proxy

cp .env.example .env
# Edit .env: set PROXY_API_KEY (min 16 chars)

docker compose up -d
# Web UI: http://localhost:9199/_ui/
```

The database is stored in a Docker named volume (`kiro-data`), so data persists across container rebuilds. To update:

```bash
git pull
docker compose up --build -d    # data is preserved
# docker compose down -v        # WARNING: this deletes all data
```

### From Source

#### Prerequisites

- [Rust](https://rustup.rs/) 1.75+
- [Node.js](https://nodejs.org/) 18+ (for frontend build)

#### Proxy-only Mode (single user, no database)

```bash
git clone https://github.com/uaniay/kiro-proxy.git
cd kiro-proxy

cp .env.example .env
# Edit .env: set PROXY_API_KEY (min 16 chars)
# Optionally set KIRO_REFRESH_TOKEN, KIRO_CLIENT_ID, KIRO_CLIENT_SECRET

cargo run --release
# Listening on http://localhost:9199
```

#### Multi-user Mode (with Web UI)

```bash
git clone https://github.com/uaniay/kiro-proxy.git
cd kiro-proxy

# Build frontend
cd frontend && npm install && npm run build && cd ..

cp .env.example .env
# Edit .env:
#   PROXY_API_KEY=your-secure-key-here
#   DATABASE_URL=sqlite:data/kiro-proxy.db?mode=rwc

mkdir -p data
cargo run --release
# API:    http://localhost:9199
# Web UI: http://localhost:9199/_ui/
```

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PROXY_API_KEY` | Yes | — | Shared API key for proxy auth (min 16 chars) |
| `DATABASE_URL` | No | — | SQLite URL to enable multi-user mode (e.g. `sqlite:/data/kiro-proxy.db?mode=rwc`) |
| `KIRO_REFRESH_TOKEN` | No | — | AWS SSO refresh token (proxy-only mode) |
| `KIRO_CLIENT_ID` | No | — | AWS SSO OAuth client ID |
| `KIRO_CLIENT_SECRET` | No | — | AWS SSO OAuth client secret |
| `KIRO_REGION` | No | `us-east-1` | AWS region for Kiro API |
| `KIRO_SSO_REGION` | No | same as `KIRO_REGION` | AWS region for SSO OIDC endpoint |
| `SERVER_HOST` | No | `0.0.0.0` | Listen address |
| `SERVER_PORT` | No | `9199` | Listen port |
| `LOG_LEVEL` | No | `info` | Log level (trace/debug/info/warn/error) |
| `ENABLE_CONVERSATION_LOG` | No | `false` | Enable full request/response logging (set `true` to enable) |

## Multi-user Mode

When `DATABASE_URL` is set, the proxy enables full multi-user functionality.

### Initial Setup

1. Start the service and open `http://YOUR_HOST:9199/_ui/`
2. Register the first account — it automatically becomes admin
3. Subsequent users register and wait for admin approval

### User Workflow

1. **Register & Login** — create account at `/_ui/`, wait for admin approval
2. **Create API Key** — go to Profile page, create an API key (up to 10 per user)
3. **Bind Kiro Token** — click "Bind Kiro Token" on Profile page, follow the device code flow:
   - A code and link are displayed
   - Open the link in browser, enter the code, authorize with your AWS account
   - Token is automatically saved and refreshed in the background
4. **Use the API** — use your API key with any OpenAI/Anthropic compatible client

### Admin Operations

The admin panel is at `/_ui/admin` with five tabs:

#### Users Tab
- View all registered users with status (active/pending/rejected)
- Approve or reject pending users
- Delete users (except yourself)

#### Usage Tab
- View API key usage across all users
- Shows user email, key prefix (click to copy), request count, input/output tokens, last used time

#### Token Pool Tab
- View all admin-managed pool entries with enable/disable status
- **Add pool entry via device code flow**: enter a label, click "Authorize", complete browser authorization
- Enable/disable or delete pool entries
- Pool entries participate in round-robin load balancing for users without their own token

#### Kiro Accounts Tab
- Unified view of all three account types: Global (env), User tokens, Pool entries
- **Enable/Disable** any account — disabled accounts are skipped during request routing
- **Share user tokens to pool**: select users with checkboxes, click "Share to Pool"
  - Shared tokens participate in pool round-robin alongside admin pool entries
  - The original user still uses their own token with highest priority
  - Click "Unshare" to remove from pool

#### Conversations Tab
- View full request/response logs for all API calls (requires `ENABLE_CONVERSATION_LOG=true`)
- Search by content keyword or filter by API Key ID
- Paginated list (default 10 per page) showing time, user, model, API type, stream mode, token counts, duration
- Click any row to expand full request body, response body, and sanitized headers (sensitive headers like Authorization/Cookie are stripped)
- Delete individual conversation logs
- Admin users can also click "Logs" on any API key in the Profile page to jump directly to that key's conversation history

### Token Resolution Priority

When a request comes in with a user's API key:

1. **User's own token** — if the user has bound a Kiro token and it's enabled, use it
2. **Pool (round-robin)** — admin pool entries + shared user tokens, rotated per request
3. **Global fallback** — the environment variable `KIRO_REFRESH_TOKEN` account, if enabled
4. **Error** — if none available, return `KiroTokenRequired` error

## API Usage

### OpenAI Format

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

**Python (OpenAI SDK):**

```python
from openai import OpenAI

client = OpenAI(
    api_key="YOUR_API_KEY",
    base_url="http://localhost:9199/v1"
)

response = client.chat.completions.create(
    model="claude-sonnet-4",
    messages=[{"role": "user", "content": "Hello!"}],
)
print(response.choices[0].message.content)
```

### Anthropic Format

```bash
curl http://localhost:9199/v1/messages \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

**Python (Anthropic SDK):**

```python
import anthropic

client = anthropic.Anthropic(
    api_key="YOUR_API_KEY",
    base_url="http://localhost:9199"
)

message = client.messages.create(
    model="claude-sonnet-4",
    max_tokens=1024,
    messages=[{"role": "user", "content": "Hello!"}],
)
print(message.content[0].text)
```

### List Models

```bash
curl http://localhost:9199/v1/models
```

## Available Models

### Claude Models

| Model | Description |
|-------|-------------|
| `claude-sonnet-4` | Claude Sonnet 4 |
| `claude-sonnet-4-5` | Claude Sonnet 4.5 |
| `claude-sonnet-4-6` | Claude Sonnet 4.6 |
| `claude-haiku-4` | Claude Haiku 4 |
| `claude-haiku-4-5` | Claude Haiku 4.5 |
| `claude-haiku-4-6` | Claude Haiku 4.6 |
| `claude-opus-4` | Claude Opus 4 |
| `claude-opus-4-6` | Claude Opus 4.6 |
| `claude-opus-4-7` | Claude Opus 4.7 |

### Non-Claude Models

| Model | Description | Credit Multiplier |
|-------|-------------|-------------------|
| `deepseek-v3-2` | Experimental preview of DeepSeek V3.2 | 0.25x |
| `minimax-m2-5` | The MiniMax M2.5 model | 0.25x |
| `minimax-m2-1` | Experimental preview of MiniMax M2.1 | 0.15x |
| `glm-5` | The GLM-5 model | 0.5x |
| `qwen3-coder-next` | Experimental preview of Qwen3 Coder Next | 0.05x |

### Model Usage Examples

**Deepseek V3.2:**

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "deepseek-v3-2",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

**MiniMax M2.5:**

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "minimax-m2-5",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

**GLM-5:**

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "glm-5",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

**Qwen3 Coder Next:**

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-coder-next",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

## API Endpoints

### Proxy Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/v1/models` | List available models |
| POST | `/v1/chat/completions` | OpenAI-compatible chat (requires API key) |
| POST | `/v1/messages` | Anthropic-compatible messages (requires API key) |
| GET | `/health` | Health check |

### Web UI Endpoints (under `/_ui/api/`)

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| POST | `/auth/register` | Public | Register new user |
| POST | `/auth/login` | Public | Login |
| GET | `/auth/me` | Session | Current user info |
| POST | `/auth/logout` | Session | Logout |
| GET | `/keys` | Session | List user's API keys |
| POST | `/keys` | Session | Create API key |
| DELETE | `/keys/:id` | Session | Delete API key |
| POST | `/kiro/setup` | Session | Start device code flow |
| POST | `/kiro/poll` | Session | Poll device authorization |
| GET | `/kiro/status` | Session | Token status |
| DELETE | `/kiro/token` | Session | Remove Kiro token |
| GET | `/admin/users` | Admin | List all users |
| DELETE | `/admin/users/:id` | Admin | Delete user |
| POST | `/admin/users/:id/approve` | Admin | Approve pending user |
| POST | `/admin/users/:id/reject` | Admin | Reject pending user |
| POST | `/admin/users/share` | Admin | Batch share/unshare user tokens |
| GET | `/admin/pool` | Admin | List token pool |
| POST | `/admin/pool` | Admin | Add pool entry (manual) |
| POST | `/admin/pool/setup` | Admin | Start device code flow for pool |
| POST | `/admin/pool/poll` | Admin | Poll device authorization for pool |
| DELETE | `/admin/pool/:id` | Admin | Delete pool entry |
| PATCH | `/admin/pool/:id` | Admin | Toggle pool entry enabled/disabled |
| GET | `/admin/usage` | Admin | Usage statistics |
| GET | `/admin/accounts` | Admin | List all Kiro accounts |
| PATCH | `/admin/accounts/:id` | Admin | Toggle account enabled/disabled |
| GET | `/admin/conversations` | Admin | List conversation logs (with search/pagination) |
| GET | `/admin/conversations/:id` | Admin | Get full conversation detail |
| DELETE | `/admin/conversations/:id` | Admin | Delete conversation log |

## Project Structure

```
kiro-proxy/
├── src/
│   ├── main.rs              # Entry point
│   ├── config.rs             # Environment configuration
│   ├── conversation_log.rs   # Header sanitization for conversation logging
│   ├── error.rs              # Error types
│   ├── db.rs                 # SQLite database layer
│   ├── pool.rs               # Token pool round-robin scheduler
│   ├── tasks.rs              # Background tasks (token refresh, cleanup)
│   ├── middleware.rs          # Auth middleware (API key + multi-user)
│   ├── http_client.rs         # HTTP client with retry logic
│   ├── tokenizer.rs          # Token counting
│   ├── thinking_parser.rs    # Thinking block extraction
│   ├── truncation.rs         # Truncation detection & recovery
│   ├── auth/                 # AWS SSO authentication
│   ├── models/               # OpenAI/Anthropic/Kiro data types
│   ├── converters/           # Format conversion (OpenAI↔Kiro, Anthropic↔Kiro)
│   ├── streaming/            # AWS Event Stream parser + SSE
│   ├── routes/               # API route handlers
│   └── web_ui/               # Web UI backend (auth, keys, kiro setup, admin)
├── frontend/                 # React + Vite + Tailwind + shadcn/ui
├── migrations/               # SQLite schema migrations (001-007)
├── Dockerfile                # Multi-stage build
├── docker-compose.yml
└── .env.example
```

## License

[MIT](LICENSE)

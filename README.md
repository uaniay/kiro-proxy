# kiro-proxy

[中文文档](README.zh-CN.md)

Lightweight Kiro API proxy with OpenAI and Anthropic compatible endpoints. Supports single-user proxy mode and multi-user mode with Web UI, per-user Kiro token binding, and admin token pool load balancing.

## Features

- **Dual API Format** — OpenAI (`/v1/chat/completions`) and Anthropic (`/v1/messages`) compatible
- **Streaming** — Server-Sent Events (SSE) for real-time responses
- **Multi-user Mode** — SQLite-backed user management with Web UI (React + shadcn/ui)
- **Per-user API Keys** — Each user creates their own keys with usage tracking (request count, token usage)
- **Kiro Token Binding** — Users bind their own AWS SSO credentials via device code flow
- **Admin Token Pool** — Round-robin load balancing across multiple Kiro accounts
- **Auto Token Refresh** — Background task refreshes expiring tokens every 5 minutes
- **Retry Logic** — Exponential backoff on 429/5xx errors
- **Truncation Recovery** — Detects and recovers from truncated API responses
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
1. User's own Kiro token (if bound)
2. Admin token pool (round-robin)
3. Global `PROXY_API_KEY` fallback

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) 1.75+
- [Node.js](https://nodejs.org/) 18+ (for frontend build, multi-user mode only)

### Proxy-only Mode (single user, no database)

```bash
git clone https://github.com/uaniay/kiro-proxy.git
cd kiro-proxy

cp .env.example .env
# Edit .env: set PROXY_API_KEY (min 16 chars)
# Optionally set KIRO_REFRESH_TOKEN, KIRO_CLIENT_ID, KIRO_CLIENT_SECRET

cargo run --release
# Listening on http://localhost:9199
```

### Multi-user Mode (with Web UI)

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

Open `http://localhost:9199/_ui/` in your browser. The first registered user automatically becomes admin.

### Docker

```bash
cp .env.example .env
# Edit .env with your settings
docker compose up -d
# Web UI at http://localhost:9199/_ui/
```

## API Usage

### OpenAI Format

**Non-streaming:**

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "What is the capital of France?"}
    ],
    "max_tokens": 1024,
    "stream": false
  }'
```

**Streaming:**

```bash
curl http://localhost:9199/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "messages": [
      {"role": "user", "content": "Write a haiku about programming"}
    ],
    "stream": true
  }'
```

**List models:**

```bash
curl http://localhost:9199/v1/models
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

**Non-streaming:**

```bash
curl http://localhost:9199/v1/messages \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "messages": [
      {"role": "user", "content": "Explain quantum computing in simple terms."}
    ],
    "stream": false
  }'
```

**Streaming:**

```bash
curl http://localhost:9199/v1/messages \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "messages": [
      {"role": "user", "content": "Write a short story about a robot."}
    ],
    "stream": true
  }'
```

**With system prompt and tools:**

```bash
curl http://localhost:9199/v1/messages \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4",
    "max_tokens": 1024,
    "system": "You are a weather assistant.",
    "messages": [
      {"role": "user", "content": "What is the weather in Tokyo?"}
    ],
    "tools": [
      {
        "name": "get_weather",
        "description": "Get current weather for a location",
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

## Available Models

| Model | Description |
|-------|-------------|
| `claude-sonnet-4` | Claude Sonnet 4 |
| `claude-sonnet-4-5` | Claude Sonnet 4.5 |
| `claude-haiku-4` | Claude Haiku 4 |
| `claude-haiku-4-5` | Claude Haiku 4.5 |
| `claude-opus-4` | Claude Opus 4 |
| `claude-opus-4-6` | Claude Opus 4.6 |

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PROXY_API_KEY` | Yes | — | Shared API key for proxy auth (min 16 chars) |
| `DATABASE_URL` | No | — | SQLite URL to enable multi-user mode |
| `KIRO_REFRESH_TOKEN` | No | — | AWS SSO refresh token (proxy-only mode) |
| `KIRO_CLIENT_ID` | No | — | AWS SSO OAuth client ID |
| `KIRO_CLIENT_SECRET` | No | — | AWS SSO OAuth client secret |
| `KIRO_REGION` | No | `us-east-1` | AWS region for Kiro API |
| `KIRO_SSO_REGION` | No | same as `KIRO_REGION` | AWS region for SSO OIDC endpoint |
| `SERVER_HOST` | No | `0.0.0.0` | Listen address |
| `SERVER_PORT` | No | `9199` | Listen port |
| `LOG_LEVEL` | No | `info` | Log level (trace/debug/info/warn/error) |

## Multi-user Mode

When `DATABASE_URL` is set, the proxy enables:

- **Web UI** at `/_ui/` — register, login, manage API keys, bind Kiro tokens
- **First user = admin** — first registered user automatically gets admin role
- **Per-user API keys** — `sk-` prefixed keys with usage tracking (requests, input/output tokens)
- **Kiro token binding** — each user can bind their own AWS SSO credentials via device code flow
- **Admin token pool** — admin can add multiple Kiro accounts for round-robin load balancing
- **Background tasks** — token refresh (every 5 min), session cleanup (every 1 hr)

### Web UI Endpoints

| Endpoint | Description |
|----------|-------------|
| `/_ui/` | Web UI (React SPA) |
| `/_ui/api/auth/register` | Register new user |
| `/_ui/api/auth/login` | Login |
| `/_ui/api/auth/me` | Current user info |
| `/_ui/api/keys` | API key management |
| `/_ui/api/kiro/setup` | Start Kiro token binding |
| `/_ui/api/kiro/status` | Token status |
| `/_ui/api/admin/users` | User management (admin) |
| `/_ui/api/admin/pool` | Token pool management (admin) |
| `/_ui/api/admin/usage` | Usage statistics (admin) |

## Project Structure

```
kiro-proxy/
├── src/
│   ├── main.rs              # Entry point
│   ├── config.rs             # Environment configuration
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
├── migrations/               # SQLite schema migrations
├── Dockerfile                # Multi-stage build
├── docker-compose.yml
└── .env.example
```

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

[MIT](LICENSE)

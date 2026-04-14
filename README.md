# kiro-proxy

Lightweight Kiro API proxy with OpenAI and Anthropic compatible endpoints. Supports single-user proxy mode and multi-user mode with Web UI, per-user Kiro token binding, and admin token pool load balancing.

## Features

- OpenAI compatible: `POST /v1/chat/completions`, `GET /v1/models`
- Anthropic compatible: `POST /v1/messages`
- Streaming (SSE) and non-streaming responses
- AWS SSO device code flow + pre-configured refresh token authentication
- Multi-user mode with Web UI (SQLite, React)
- Per-user API keys and Kiro token binding
- Admin token pool with round-robin load balancing
- Automatic token refresh with graceful degradation
- Exponential backoff retry on 429/5xx
- Truncation recovery for large tool call responses

## Quick Start

### Proxy-only mode (single user)

```bash
cp .env.example .env
# Set PROXY_API_KEY and optionally KIRO_REFRESH_TOKEN
cargo run
```

### Multi-user mode

```bash
cp .env.example .env
# Set PROXY_API_KEY and DATABASE_URL
# DATABASE_URL=sqlite:data/kiro-proxy.db?mode=rwc
cd frontend && npm install && npm run build && cd ..
cargo run
# Open http://localhost:9199/_ui/ to register and manage users
```

## Docker

```bash
cp .env.example .env
# Edit .env
docker compose up -d
# Web UI at http://localhost:9199/_ui/
```

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PROXY_API_KEY` | Yes | — | Shared API key for proxy auth (min 16 chars) |
| `DATABASE_URL` | No | — | SQLite URL to enable multi-user mode |
| `KIRO_REFRESH_TOKEN` | No | — | AWS SSO refresh token (proxy-only mode) |
| `KIRO_CLIENT_ID` | No | — | AWS SSO OAuth client ID |
| `KIRO_CLIENT_SECRET` | No | — | AWS SSO OAuth client secret |
| `KIRO_REGION` | No | `us-east-1` | AWS region for Kiro API |
| `KIRO_SSO_REGION` | No | same as KIRO_REGION | AWS region for SSO OIDC endpoint |
| `SERVER_HOST` | No | `0.0.0.0` | Listen address |
| `SERVER_PORT` | No | `9199` | Listen port |
| `LOG_LEVEL` | No | `info` | Log level (trace/debug/info/warn/error) |

## Multi-user Mode

When `DATABASE_URL` is set, the proxy enables:

- Web UI at `/_ui/` for user registration, login, API key management, and Kiro token binding
- First registered user becomes admin
- Users can bind their own Kiro token via AWS SSO device code flow
- Admin can manage a token pool for users without their own token
- Requests are routed: user token > admin pool (round-robin) > global PROXY token

## License

[MIT](LICENSE)

# kiro-proxy

Lightweight Kiro API proxy with OpenAI and Anthropic compatible endpoints.

## Features

- OpenAI compatible: `POST /v1/chat/completions`, `GET /v1/models`
- Anthropic compatible: `POST /v1/messages`
- Streaming (SSE) and non-streaming responses
- AWS SSO device code flow + pre-configured refresh token authentication
- Automatic token refresh with graceful degradation
- Exponential backoff retry on 429/5xx
- Truncation recovery for large tool call responses
- Thinking/reasoning block extraction (fake reasoning support)
- Single binary, no database required

## Quick Start

```bash
cp .env.example .env
# Edit .env with your PROXY_API_KEY and Kiro credentials
cargo run
```

## Docker

```bash
cp .env.example .env
# Edit .env
docker compose up -d
```

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PROXY_API_KEY` | Yes | — | API key for proxy authentication (min 16 chars) |
| `KIRO_REFRESH_TOKEN` | No | — | AWS SSO refresh token |
| `KIRO_CLIENT_ID` | No | — | AWS SSO OAuth client ID |
| `KIRO_CLIENT_SECRET` | No | — | AWS SSO OAuth client secret |
| `KIRO_REGION` | No | `us-east-1` | AWS region for Kiro API |
| `KIRO_SSO_REGION` | No | same as KIRO_REGION | AWS region for SSO OIDC endpoint |
| `SERVER_HOST` | No | `0.0.0.0` | Listen address |
| `SERVER_PORT` | No | `8000` | Listen port |
| `LOG_LEVEL` | No | `info` | Log level (trace/debug/info/warn/error) |

## Usage

```bash
# OpenAI format
curl http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer YOUR_PROXY_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4","messages":[{"role":"user","content":"Hello"}],"stream":false}'

# Anthropic format
curl http://localhost:8000/v1/messages \
  -H "Authorization: Bearer YOUR_PROXY_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4","messages":[{"role":"user","content":"Hello"}],"max_tokens":1024,"stream":false}'
```

## License

[MIT](LICENSE)

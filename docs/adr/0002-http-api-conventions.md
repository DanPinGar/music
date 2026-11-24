# 0002. HTTP API conventions

- Status: accepted
- Date: 2026-11-24

## Context

The UI talks to the core over HTTP + WebSocket, on `127.0.0.1` in the desktop app and, later,
to a cloud server. The API is consumed from TypeScript today and possibly by other clients
later, so its conventions must be explicit and stable across Rust, Python and TypeScript.

## Decision

- **Contract first from code:** the OpenAPI 3.1 document is generated from the Rust handlers
  (utoipa); the TypeScript client is generated from it (`pnpm gen:api`) and CI fails if the
  committed client is out of date.
- **JSON:** `snake_case` field names, the same as the Rust and Python models, so no renaming
  layer is needed. Enums are lower-case strings (`"importing"`).
- **Identifiers:** UUIDv7 strings (time-ordered, safe to merge when libraries sync).
- **Times:** RFC 3339 timestamps in UTC; media positions and durations in integer milliseconds
  (`*_ms`), musical note times in seconds as floats inside artifacts.
- **Errors:** RFC 9457 problem details (`application/problem+json`) with `type`, `title`,
  `status` and `detail`; internal errors carry no detail.
- **Authentication:** every route except `/health` needs the session token, as
  `Authorization: Bearer <token>` or, for media elements that cannot send headers, a `token`
  query parameter. Locally the token is random per run and only the webview receives it.
- **Local hardening:** the desktop API binds to `127.0.0.1` on a random port, checks the `Host`
  header against DNS rebinding and limits CORS to the webview origins.

## Consequences

The same API serves the desktop app and the future web version. Generated clients keep the UI in
sync with the core at compile time. `snake_case` reads slightly unusual in TypeScript, which is
accepted in exchange for one naming scheme across the whole system.

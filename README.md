# Drome (M1)

A Rust + GPUI desktop AI client skeleton for `Windows/Linux/macOS`, with:

- Multi-provider runtime (`OpenAI`, `Anthropic`, `Gemini`)
- MCP client runtime (`stdio`, `SSE`, `streamable-http`)
- SQLite persistence for sessions/messages
- Optional local secrets encryption (password-based)
- Local structured logs with daily file rotation
- Bilingual UI resources (`zh-CN` + `en-US`)

## Workspace layout

- `crates/app_desktop` — GPUI desktop entry and basic two-page shell
- `crates/core_types` — stable cross-module interfaces and DTOs
- `crates/core_orchestrator` — model/tool orchestration loop
- `crates/provider_zed` — provider adapter layer (Zed-inspired payload/event mapping)
- `crates/mcp_runtime` — Rust MCP SDK based client runtime
- `crates/storage_sqlite` — SQLite persistence (`sqlx`)
- `crates/secrets` — plaintext + optional encrypted secret store
- `crates/config` — config load/save + schema migration
- `crates/i18n` — i18n dictionary and lookup

## Notes on Zed reuse

This implementation keeps the architecture and payload/event mapping approach aligned with Zed provider modules (`open_ai` / `anthropic` / `google_ai`) while avoiding direct hard coupling to their full workspace dependency graph in this environment.  
The `provider_zed` crate is intentionally named and structured to keep migration to direct upstream crate reuse straightforward.

## Build

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features
```

Run desktop shell:

```bash
cargo run -p app_desktop
```

Linux runtime/linking requires system libraries such as `xcb` and `xkbcommon`.

### Windows runtime note

Windows release binaries are built with static CRT (`crt-static`) so they do not require `VCRUNTIME140.dll` to be preinstalled on the target machine.

## CI outputs

GitHub Actions builds and uploads platform binaries for:

- `drome-linux-x86_64`
- `drome-macos-universal`
- `drome-windows-x86_64`

## License

This repository is configured for `GPL-3.0-or-later`.

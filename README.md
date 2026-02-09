# cherry-studio-rs

Rust + GPUI implementation workspace for Cherry Studio migration.

## Run

```bash
cargo run -p cherry-app
```

## Current interactive controls

- Click route entries to switch page skeleton.
- Press `1-9` to switch common routes quickly.
- In Home page, click quick actions to create conversations, send prompt samples, and view streaming chunks.
- In `Store/Launchpad`, use actions to export/import backups (Local/WebDAV/S3/LAN channels).
- In `Files/Notes/Knowledge`, use panel actions to add/remove sample data and upload local files.
- In `OpenClaw`, use actions to call MCP tools, set tool permissions, and resolve `cherry://` protocol links.
- In `Settings`, cycle 15 sections and apply section-specific actions (display/runtime/data/MCP/API/memory, etc).

## Legacy import (JSON)

```bash
cargo run -p cherry-app --bin migrate_legacy -- ./legacy-export.json
```

Expected JSON top-level fields:
- `settings`
- `providers`
- `conversations` (with `title` and `messages`)
- `notes`
- `files`
- `knowledge_documents`

## Legacy import (directory + sqlite auto-detect)

```bash
cargo run -p cherry-app --bin migrate_legacy_dir -- ./legacy-data-dir
```

## Functional parity matrix

See `docs/FEATURE_PARITY.md`.

## Validate

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

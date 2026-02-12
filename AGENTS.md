# Repository Guidelines

## Project Structure & Module Organization
- `src/renderer/` contains the Vite + React desktop UI. Core areas are `components/`, `pages/`, `services/`, `store/`, and `hooks/`.
- `src-tauri/` contains the Rust/Tauri host app and native commands (see `src-tauri/src/commands/`).
- `packages/` contains shared workspace modules (for example `aiCore`, `ai-sdk-provider`, `extension-table-plus`, `shared`, `mcp-trace`).
- `dist/` is generated output; do not hand-edit.
- `cherry-studio/` is an upstream reference snapshot; only change it when intentionally syncing upstream content.

## Build, Test, and Development Commands
- `pnpm install`: install workspace dependencies (Node `>=22`, pnpm `10.x`).
- `pnpm dev`: run renderer in Vite dev mode.
- `pnpm tauri:dev`: run the full desktop app with Tauri.
- `pnpm build`: build the web renderer bundle.
- `pnpm tauri:build`: build desktop bundles.
- `pnpm typecheck`: strict TypeScript check (`tsc --noEmit`).
- `pnpm -r --if-present test`: run all workspace test scripts that exist.
- `pnpm --filter @cherrystudio/ai-core test`: run package-targeted tests.
- `cargo check --manifest-path src-tauri/Cargo.toml`: validate Rust backend changes.

## Coding Style & Naming Conventions
- TypeScript is `strict` (`tsconfig.json`); keep types explicit on public APIs.
- Follow existing TS style: 2-space indentation, single quotes, minimal semicolon use.
- Naming patterns: React components in `PascalCase` (`ModelSelector.tsx`), hooks as `useXxx.ts`, services as `XxxService.ts`.
- Rust follows standard `rustfmt` conventions and `snake_case` for functions/modules.
- Prefer configured path aliases (for example `@renderer/*`, `@shared/*`) over deep relative imports.

## Testing Guidelines
- Vitest is used in workspace packages (for example `packages/aiCore/vitest.config.ts`).
- Place tests in `__tests__/` with `*.test.ts` or `*.test.tsx`; snapshots belong in `__snapshots__/`.
- Add or update tests near changed logic. For provider/IPC flows, prefer deterministic mocks over live network calls.
- For Rust changes, run `cargo check` at minimum; add unit tests for isolated logic when feasible.

## Commit & Pull Request Guidelines
- Follow the project’s observed commit style: `fix: ...`, `feat: ...`, `chore: ...`, `ci: ...`.
- Use `<type>: <imperative summary>` (example: `fix: build`).
- PRs should include scope, changed paths, validation steps, and UI screenshots for renderer changes.
- Before review, ensure `pnpm typecheck`, relevant tests, and Rust checks (if applicable) pass.

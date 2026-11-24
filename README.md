# Segno

A desktop app for studying Synthesia-style piano videos: download or import them, move around
the song instantly and precisely, mark sections and loops, take notes, transcribe the video into
notes and sheet music, and analyse its harmony.

> **Status:** Phase 0 (foundations). The plan, architecture and roadmap are in
> [docs/PLAN.md](docs/PLAN.md); decisions are recorded in [docs/adr](docs/adr).

## Repository layout

| Path                  | What it is                                                               |
| --------------------- | ------------------------------------------------------------------------ |
| `apps/ui`             | React + TypeScript user interface (Vite, Tailwind, shadcn/ui on Base UI) |
| `apps/desktop`        | Tauri 2 desktop shell; `src-tauri` starts the embedded core API          |
| `crates/domain`       | Rust domain model: entities, rules and ports                             |
| `crates/storage`      | Rust storage adapters (SQLite, with migrations)                          |
| `crates/api`          | Rust HTTP API (Axum + OpenAPI) and the `segno-api` development server    |
| `packages/api-client` | TypeScript client generated from the API's OpenAPI document              |
| `engine`              | Python analysis engine (transcription, scores, harmony), managed with uv |
| `scripts`             | Developer scripts, e.g. fetching ffmpeg into `.tools/`                   |
| `docs`                | Plan and architecture decision records                                   |

## Requirements (Windows)

- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the
  "Desktop development with C++" workload, and WebView2 (included in Windows 11).
- [Rust](https://rustup.rs) via rustup: the toolchain version is pinned in `rust-toolchain.toml`
  and installed automatically.
- Node.js 22.12 or newer and pnpm 11 (the version is pinned in `package.json`).
- [uv](https://docs.astral.sh/uv/) for Python; it creates `engine/.venv` with the right packages.

Everything else stays inside the repository: `node_modules`, `engine/.venv` and `.tools/`.

## Getting started

```sh
pnpm install                              # JavaScript dependencies (also installs the git hooks)
uv sync --directory engine --all-groups   # engine/.venv: engine, dev tools, yt-dlp and Deno
pnpm tools                                # ffmpeg (LGPL build) into .tools/ffmpeg
pnpm dev                                  # run the desktop app with hot reload
```

A plain `uv sync` keeps only the `dev` group and uninstalls yt-dlp and Deno: keep `--all-groups`.

To launch, drive and screenshot the app from scripts or AI agents, see
[.claude/skills/run-segno/SKILL.md](.claude/skills/run-segno/SKILL.md).

To work on the UI in a browser, run the API and the UI separately:

```sh
pnpm dev:api                     # core API on http://127.0.0.1:7700 (token "dev")
pnpm dev:ui                      # UI on http://localhost:5173
```

## Checks

```sh
pnpm check                       # everything CI runs, in one command
pnpm check:ui                    # Biome, type checks, Vitest and the UI build
pnpm check:rust                  # rustfmt, clippy and the Rust tests
pnpm check:engine                # Ruff, Pyright and pytest
```

## Common tasks

| Task                                                             | Command                                                                   |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Regenerate the TypeScript API client after changing the Rust API | `pnpm gen:api`                                                            |
| Build the Windows installer                                      | `pnpm build`                                                              |
| Regenerate the app icons from `apps/desktop/app-icon.svg`        | `pnpm --filter @segno/desktop tauri icon app-icon.svg -o src-tauri/icons` |
| Smoke-test the real app (dev mode, then the built app)           | `node .claude/skills/run-segno/driver.mjs --smoke` and `--smoke prod`     |

## Conventions

- Code, comments, commits and documentation are written in English; the app's interface is in
  Spanish and English.
- [Conventional Commits](https://www.conventionalcommits.org), short-lived branches and pull
  requests; significant decisions get an ADR in `docs/adr`.
- Only dependencies with permissive licences (MIT, BSD, Apache and similar; LGPL as separate
  modules only).

## Licence

Not chosen yet: all rights reserved until the project picks one.

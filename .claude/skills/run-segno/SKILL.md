---
name: run-segno
description: Build, run, launch, drive, screenshot or smoke-test the Segno desktop app (Tauri 2 + React UI in WebView2 + embedded Rust API) on Windows, or run its core API and Python engine on their own. Use when asked to start Segno, check a UI or API change in the real app, take a screenshot of it, or poke the running app or its API.
---

Segno is a Tauri 2 desktop app: a React UI in WebView2 that talks to a Rust core API embedded
in the same process (random port and token on every run), plus a Python engine. Drive it with
`.claude/skills/run-segno/driver.mjs`: it launches the real app with WebView2's DevTools port
open, attaches Playwright over CDP and runs commands read from stdin. Verified on Windows 11,
the only platform set up so far.

All paths are relative to the repository root.

## Prerequisites

Windows 11 with Node 22.12+ and pnpm 11. The rest was installed with:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --source winget --accept-source-agreements --accept-package-agreements --override "--passive --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id Rustlang.Rustup --exact --source winget --accept-source-agreements --accept-package-agreements
winget install --id astral-sh.uv --exact --source winget --accept-source-agreements --accept-package-agreements
```

The Build Tools install asks for administrator rights once (UAC). `rust-toolchain.toml` pins
Rust; rustup installs that version on first use. Restart VS Code and terminals afterwards
(see Gotchas).

## Setup

```bash
pnpm install                              # JS dependencies, playwright-core for the driver, git hooks
uv sync --directory engine --all-groups   # engine/.venv with dev tools, yt-dlp and Deno
pnpm tools                                # ffmpeg into .tools/ffmpeg (not needed to run the app yet)
```

## Run (agent path)

One-shot check, the first thing to run after a change:

```bash
node .claude/skills/run-segno/driver.mjs --smoke        # dev mode: Vite + cargo run, ~15 s once Rust is built
node .claude/skills/run-segno/driver.mjs --smoke prod   # built app: real CSP and origin, ~20 s
```

It prints `PASS`/`FAIL` per check (status bar fed by the API, `Ctrl+K` palette running a
command, `?` dialog, `Ctrl+2`/`Ctrl+1`, language toggle, `GET /videos` with the session token,
no console errors or HTTP errors), saves screenshots, closes the app and exits 0 only if
everything passed.

To drive the app step by step, pipe a script of commands. There is no tmux on Windows; each
line runs after the previous one finishes, and the app is closed when the input ends:

```bash
printf 'launch\napi\nget /system\nget /videos/00000000-0000-0000-0000-000000000000\nss library\ntext footer\npress Control+k\nwait [data-slot=command-input]\ntype player\nss palette\nclick [data-value="nav.player"]\neval location.hash\nconsole\nquit\n' | node .claude/skills/run-segno/driver.mjs
```

Running `node .claude/skills/run-segno/driver.mjs` in a terminal gives a `segno>` prompt for
the same commands. Palette items carry their command id in `data-value` (e.g.
`[data-value="nav.player"]`), which works whatever the UI language; text labels follow the
language the app is set to.

Screenshots go to `target/run-segno/shots/`, the app's output to `target/run-segno/app.log`
and the production build's output to `target/run-segno/build.log`.

| command                             | what it does                                                                                                                  |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `launch [dev\|prod]`                | start the app and attach. `dev` runs `pnpm dev`; `prod` builds with the UI embedded and runs `target/debug/segno-desktop.exe` |
| `ss [name]`                         | screenshot of the window: `target/run-segno/shots/<name>.png`                                                                 |
| `press <key>`                       | one key press, with Playwright names: `Control+k`, `?`, `Control+2`, `Escape`                                                 |
| `type <text>`                       | type into the focused element                                                                                                 |
| `click <css>` / `click-text <text>` | click through the DOM (buttons, links, palette items)                                                                         |
| `wait <css>`                        | wait up to 10 s for an element                                                                                                |
| `text [css]`                        | visible text of an element, or of the whole page                                                                              |
| `eval <js>`                         | evaluate an expression in the page and print it as JSON                                                                       |
| `goto <route>`                      | change the hash route, e.g. `goto /player`                                                                                    |
| `api`                               | `{base_url, token}` of the embedded API, read through Tauri IPC                                                               |
| `get <path>`                        | `GET` the embedded API with the token and print status and body                                                               |
| `console`                           | console errors and warnings, HTTP errors and failed requests since `launch`                                                   |
| `smoke`                             | the `--smoke` checks on the running app                                                                                       |
| `sleep <ms>`                        | pause a piped script                                                                                                          |
| `cleanup`                           | stop app and Vite processes left from earlier runs (also `--cleanup`)                                                         |
| `quit`                              | close the window, stop everything and exit                                                                                    |

## Direct invocation (no desktop app)

For changes that only touch the core API: serve it on `127.0.0.1:7700` with the token `dev`.

```bash
cargo build -p segno-api
./target/debug/segno-api.exe serve --db target/run-segno/api-dev.db &
curl -s --retry 30 --retry-connrefused --retry-delay 1 http://127.0.0.1:7700/health
curl -s -H "Authorization: Bearer dev" http://127.0.0.1:7700/videos
kill %1
```

For the Python engine, talk to the worker over stdin (JSON Lines):

```bash
printf '{"type":"ping","id":"1"}\n{"type":"shutdown"}\n' | uv run --directory engine segno-engine worker --stdio
```

## Run (human path)

```bash
pnpm dev   # builds and opens the window with hot reload; closing the window stops everything
```

## Test

```bash
pnpm check
```

Biome, both type checks, Vitest (21 tests), the UI build, rustfmt, clippy with warnings as
errors, cargo test (19 tests), Ruff, Pyright in strict mode and pytest (7 tests). All pass.

## Gotchas

- **Stopping `pnpm dev` from outside leaves the app running.** Killing its shell (or a
  background task) left `segno-desktop.exe`, its WebView2 (DevTools on port 9222) and Vite
  (port 5173) alive. The driver closes the window instead, which stops the whole tree, and
  `--cleanup` stops leftovers started from this repository.
- **Force-killing the app loses its latest `localStorage` writes.** A language change made just
  before a hard kill was gone on the next launch: WebView2 writes storage lazily. That is why
  `quit` closes the window and only kills what is still running 8 s later.
- **DevTools opens only with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
  in the app's environment.** It passes unchanged through pnpm, the Tauri CLI and `cargo run`.
  `SEGNO_CDP_PORT` changes the port; a second driver refuses to launch while one is running.
- **Dev mode skips the CSP.** Vite serves the UI at `http://localhost:5173`; only the built app
  (`http://tauri.localhost`) enforces the CSP and the production CORS origin. Use
  `--smoke prod` for changes to CSP, CORS, assets or `tauri.conf.json`.
- **A console 404 without a URL is usually the favicon.** Chromium fetches it outside the page's
  network events, so Playwright never sees the response. A missing favicon failed the smoke
  test until `apps/ui/public/favicon.svg` was added.
- **Shells opened before the installs do not find `cargo` or `uv`.** They live in
  `%USERPROFILE%\.cargo\bin` and
  `%LOCALAPPDATA%\Microsoft\WinGet\Packages\astral-sh.uv_Microsoft.Winget.Source_8wekyb3d8bbwe`.
  The driver adds both to the PATH of what it starts; your own terminals need a restart.
- **A plain `uv sync` uninstalls yt-dlp and Deno.** `uv sync` makes the environment match the
  selected groups exactly, and only `dev` is selected by default; it removed the `tools` group.
  Use `--all-groups`. `uv run` never removes packages.
- **The embedded API's port and token change on every launch.** Use `api` or `get` instead of
  hard-coding them.
- **The first `pnpm dev` compiles the whole Rust workspace** (4 min 13 s on this laptop); later
  launches compile in about 5 s. Building for `launch prod` after `dev` took about 30 s.

## Troubleshooting

- **`ERROR: port 9222 already serves DevTools (an earlier run?)`**: another driver or a leftover
  app is running. Quit that driver, or run `node .claude/skills/run-segno/driver.mjs --cleanup`.
- **`the app process exited (code 4294967295)`** in a driver's output: something else killed the
  app (for example a `--cleanup`). Run `launch` again.
- **`pnpm tools` fails with `http.client.RemoteDisconnected: Remote end closed connection without
  response`**: GitHub dropped the download. The script retries three times; run it again if it
  still fails.
- **`uv: command not found` or `cargo: command not found` in Git Bash**: see the PATH gotcha.
- **`pnpm peers check` reports `unmet peer typescript` for `openapi-typescript`**: the client
  generator needs TypeScript 5, so `packages/api-client` pins `typescript@~5.9.3` while the UI
  uses TypeScript 7. Keep that pin until openapi-typescript supports TypeScript 7.

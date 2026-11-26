---
name: run-segno
description: Build, run, launch, drive, screenshot or smoke-test the Segno desktop app (Tauri 2 + React UI in WebView2 + embedded Rust API) on Windows, or run its core API and Python engine on their own. Use when asked to start Segno, check a UI or API change in the real app, import or play a video in it, take a screenshot of it, or poke the running app or its API.
---

Segno is a Tauri 2 desktop app: a React UI in WebView2 that talks to a Rust core API embedded
in the same process (random port and token on every run), plus a Python engine. Drive it with
`.claude/skills/run-segno/driver.mjs`: it launches the real app with WebView2's DevTools port
open, attaches Playwright over CDP and runs commands read from stdin. Every launch uses a fresh
test library in `target/run-segno/library`, never the user's own. Verified on Windows 11, the
only platform set up so far.

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
pnpm tools                                # ffmpeg into .tools/ffmpeg: needed to import videos
```

## Run (agent path)

One-shot check, the first thing to run after a change:

```bash
node .claude/skills/run-segno/driver.mjs --smoke        # dev mode: Vite + cargo run, ~25 s once Rust is built
node .claude/skills/run-segno/driver.mjs --smoke prod   # built app: real CSP and origin, ~25 s
```

It prints `PASS`/`FAIL` for 38 checks:

- the status bar fed by the API, the `Ctrl+K` palette running a command, the `?` dialog,
  `Ctrl+2`/`Ctrl+1` and the language toggle;
- `GET /videos` with the session token and importing a 5-second 60 fps clip made with ffmpeg,
  with a plucked note every half second from 0.5 s to 4.5 s;
- the core preparing it in the background (waveform peaks, attacks, seek proxy, thumbnail
  sprite) and serving the thumbnails and peaks;
- the player switching to the seek proxy and loading it;
- `Space` playing and pausing, `.` stepping exactly one frame, `→` jumping 2 s and `-` slowing
  to 95 %;
- the timeline: waveform drawn and 9 attacks for the 9 notes, `Shift+→`/`Shift+←` landing on
  the notes, a click on the overview jumping there, `Ctrl` + wheel zooming and `Ctrl+0` showing
  the whole clip, and the hover preview showing the exact frame;
- studying:
  - tapping `T` builds a beat grid, and `S` then `→` steps by beat;
  - `G` goes to a typed time;
  - `Alt+←`/`Alt+→` move through the history;
  - `M` adds a marker and `1` jumps to it;
  - `A`/`B` set a loop that wraps round (its wrap gaps are printed);
  - after a reload the video reopens with its loop, grid and markers;
- timeline gestures: Shift + drag makes a loop whose edges drag and snap to the attacks (`Alt`:
  free), a double-click adds a marker, and return-on-pause goes back to where playback started;
- comments: `C` writes one in Markdown at the playhead, `Ctrl+Enter` saves it and gives the keys
  back, a time written in it jumps there, and the hover preview shows it;
- no console errors or HTTP errors.

The timeline's detail canvas exposes its state for tests: `data-start`, `data-span`,
`data-follow`, `data-waveform` (`ready` or `none`), `data-attacks` (how many), `data-grid` (the
beats, in seconds, comma-separated) and `data-loop` (`start-end`). The player video keeps the
last loop wraps' gaps in `data-loop-gaps` (milliseconds).

It saves screenshots, closes the app and exits 0 only if everything passed.

### Measure navigation

`bench` times what the navigation budget in `docs/PLAN.md` (4.2) measures, on the video open in
the player: 10 jumps, 30 frame steps each way and 3 play starts after a jump. It first waits for
the preparation (36 s for the 2:21 1080p60 video below). `bench original` measures the original
file instead, which the driver serves itself (the API serves the proxy once it exists), then
gives the player its source back:

```bash
printf 'launch prod\nimport eval/data/videos/7A12z9S4F8U.mp4\nbench\nbench original\nquit\n' | node .claude/skills/run-segno/driver.mjs
```

```
{"source":"seek proxy","jumps":{"median":18,"p95":35,"max":35},"frameBack":{"median":18,"p95":26,"max":28},"frameForward":{"median":19,"p95":26,"max":27},"playStart":{"median":66,"p95":70,"max":70}}
{"source":"original","jumps":{"median":201,"p95":260,"max":260},"frameBack":{"median":250,"p95":262,"max":263},"frameForward":{"median":249,"p95":263,"max":264},"playStart":{"median":79,"p95":279,"max":279}}
```

Milliseconds; the budget is p95 under 100 ms for jumps and under 50 ms for frame steps. The video
is the reference one, git-ignored; download it once with:

```bash
mkdir -p eval/data/videos
uv run --directory engine yt-dlp --ffmpeg-location ../.tools/ffmpeg/bin -f "bv*[vcodec^=avc1][height<=1080][fps>30]+ba[ext=m4a]/bv*[vcodec^=avc1][height<=1080]+ba[ext=m4a]/b" --merge-output-format mp4 -o "../eval/data/videos/%(id)s.%(ext)s" "https://www.youtube.com/watch?v=7A12z9S4F8U"
```

To drive the app step by step, pipe a script of commands. There is no tmux on Windows; each
line runs after the previous one finishes, and the app is closed when the input ends:

```bash
printf 'launch\napi\nimport test-clip\nwait video\nsleep 1500\npress Space\nsleep 1500\npress Space\ntext [data-testid=player-time]\npress Home\npress .\ntext [data-testid=player-time]\nss player\nget /videos\nconsole\nquit\n' | node .claude/skills/run-segno/driver.mjs
```

Running `node .claude/skills/run-segno/driver.mjs` in a terminal gives a `segno>` prompt for
the same commands. Palette items carry their command id in `data-value` (e.g.
`[data-value="nav.player"]`), which works whatever the UI language; text labels follow the
language the app is set to.

Screenshots go to `target/run-segno/shots/`, the app's output to `target/run-segno/app.log`
and the production build's output to `target/run-segno/build.log`.

| command | what it does |
|---|---|
| `launch [dev\|prod]` | start the app with a fresh test library and attach. `dev` runs `pnpm dev`; `prod` builds with the UI embedded and runs `target/debug/segno-desktop.exe` |
| `import <path\|test-clip>` | import a video through the API and open it in the player; `test-clip` makes a 5 s 1280×720 60 fps clip with ffmpeg |
| `ss [name]` | screenshot of the window: `target/run-segno/shots/<name>.png` |
| `press <key>` | one key press, with Playwright names: `Space`, `Control+k`, `?`, `.`, `ArrowRight`, `Home` |
| `type <text>` | type into the focused element |
| `click <css>` / `click-text <text>` | click through the DOM (buttons, links, palette items) |
| `click-at <css> [fx] [fy]` / `dblclick-at …` | a real mouse click or double-click at fractions of an element's size; unlike `click`, it moves the focus |
| `drag <css> <from> <to> [Shift\|Alt]` | drag across an element between two fractions of its width, holding a modifier: `drag [data-testid=timeline-detail] 0.2 0.4 Shift` makes a loop |
| `move <css> [fx] [fy]` | move the mouse over an element, at fractions of its size (default the centre), e.g. `move [data-testid=timeline-detail] 0.7 0.5` to hover the timeline |
| `wait <css>` | wait up to 10 s for an element |
| `text [css]` | visible text of an element, or of the whole page; the player's clock is `[data-testid=player-time]` |
| `eval <js>` | evaluate an expression in the page and print it as JSON |
| `goto <route>` | change the hash route, e.g. `goto /player` |
| `api` | `{base_url, token}` of the embedded API, read through Tauri IPC |
| `get <path>` | `GET` the embedded API with the token and print status and body |
| `console` | console errors and warnings, HTTP errors and failed requests since `launch` |
| `smoke` | the `--smoke` checks on the running app |
| `bench [now\|original]` | navigation timings of the open video, as JSON, after its preparation; `now` measures whatever is loaded without waiting, `original` the original file |
| `sleep <ms>` | pause a piped script |
| `cleanup` | stop app and Vite processes left from earlier runs (also `--cleanup`) |
| `quit` | close the window, stop everything and exit |

## Direct invocation (no desktop app)

For changes that only touch the core API: serve a library folder on `127.0.0.1:7700` with the
token `dev`. The clip below is the one `import test-clip` or `--smoke` makes.

```bash
cargo build -p segno-api
./target/debug/segno-api.exe serve --library target/run-segno/api-library &
curl -s --retry 30 --retry-connrefused --retry-delay 1 http://127.0.0.1:7700/health
curl -s -H "Authorization: Bearer dev" -H "Content-Type: application/json" -d "{\"path\": \"$(pwd -W)/target/run-segno/test-clip-v2.mp4\"}" http://127.0.0.1:7700/videos/import
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

Biome, both type checks, Vitest (108 tests), the UI build, rustfmt, clippy with warnings as
errors, cargo test (89 tests, including real ffmpeg runs), Ruff, Pyright in strict mode and
pytest (7 tests). All pass. Set `SEGNO_REQUIRE_FFMPEG=1` to make the ffmpeg tests fail, instead
of skipping, when `.tools/ffmpeg` is missing (CI does).

## Gotchas

- **Stopping `pnpm dev` from outside leaves the app running.** Killing its shell (or a
  background task) left `segno-desktop.exe`, its WebView2 (DevTools on port 9222) and Vite
  (port 5173) alive. The driver closes the window instead, which stops the whole tree, and
  `--cleanup` stops leftovers started from this repository.
- **Force-killing the app loses its latest `localStorage` writes.** A language change made just
  before a hard kill was gone on the next launch: WebView2 writes storage lazily. That is why
  `quit` closes the window and only kills what is still running 8 s later.
- **Each `launch` wipes and recreates `target/run-segno/library`.** Set `SEGNO_LIBRARY_DIR` to
  drive the app with another library; the driver then leaves it as it is.
- **DevTools opens only with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
  in the app's environment.** It passes unchanged through pnpm, the Tauri CLI and `cargo run`.
  `SEGNO_CDP_PORT` changes the port; a second driver refuses to launch while one is running.
- **Dev mode does not reproduce the production CSP.** In the built app Tauri adds nonces to the
  CSP, and with a nonce present browsers ignore `'unsafe-inline'`: inline styles (sonner's
  toasts) were blocked only under `--smoke prod`. `tauri.conf.json` sets
  `dangerousDisableAssetCspModification` for `style-src` because of it. Use `--smoke prod` for
  changes to CSP, CORS, assets or `tauri.conf.json`.
- **The first play after opening a video starts about 0.85 s late** (audio device and decoder
  warming up). The smoke test waits for `readyState` 4 and for the clock to advance instead of
  sleeping a fixed time.
- **Seeking cancels the video's in-flight range requests** (`net::ERR_ABORTED`). That is normal;
  the driver ignores it for media requests and reports every other failed request.
- **`bench` varies between runs, and the first run after a preparation is the slowest.** Jumps on
  the same proxy measured a p95 of 47 ms in the first run and 26 ms in the next. Run it at least
  twice before drawing conclusions, and compare files in the same session (`bench` then
  `bench original`).
- **The player page has two `<video>` elements**: the player and the hidden one behind the
  timeline's exact-frame preview. Select the player with `[data-testid=player-video]`, never
  with a bare `video`.
- **Zoom and follow mode outlive the test library.** The timeline keeps them in `localStorage`
  (`segno.timeline`), which `launch` does not wipe, so a new session may open zoomed in. Press
  `Control+0` first when a check depends on the zoom. Each smoke run zoomed in a little more
  until a zoom check failed at the 0.5 s minimum. The side panel (`segno.player.panel`) and
  return-on-pause (`segno.player`) persist the same way. Dev (`localhost:5173`) and prod
  (`tauri.localhost`) are different origins with separate storage. A check passing in one mode
  and failing in the other can come down to this, for example a panel open in one and closed in
  the other.
- **The timeline's width changes what snaps.** Snapping reaches 8 px. The window is 1280 CSS px
  (screenshots are 1600 px at 125 % scaling), and the side panel takes 320 px. So the same drag
  reaches 0.042 s with the panel open and 0.031 s without, on a 5 s view. Place test drags well
  inside that reach (the smoke test presses 15 ms from an attack).
- **Shortcuts are ignored while a dialog is open, opening or closing.** Keys pressed then belong
  to the dialog. A second `G` pressed while the first go-to dialog was still closing once
  swallowed every key that followed, failing six checks at random. Wait for
  `[cmdk-input]:focus` before typing, and for `[data-slot=dialog-content]` to be detached before
  the next key, as the smoke test's `goTo` does.
- **`click` does not move the focus; `click-at` does.** `click` calls `element.click()`, which
  is what most checks want. For focus-related bugs, reproduce with real mouse events.
- **Right after an import, `/media` serves the original.** The player switches to the proxy when
  the preparation finishes (its URL gains `copy=seek-proxy`), keeping the position, speed and play
  state. Anything that measures or screenshots playback should wait for that, as `bench` does.
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

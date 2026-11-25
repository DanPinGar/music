#!/usr/bin/env node
// Driver for the Segno desktop app (Tauri 2 + WebView2) on Windows.
//
// Launches the real app with WebView2's DevTools port open, attaches Playwright to the
// webview over CDP and runs commands read from stdin, typed or piped as a script.
//
//   node .claude/skills/run-segno/driver.mjs               REPL (launch, ss, press, get, quit…)
//   node .claude/skills/run-segno/driver.mjs --smoke       launch in dev mode, smoke flow, quit
//   node .claude/skills/run-segno/driver.mjs --smoke prod  the same against the built app
//   node .claude/skills/run-segno/driver.mjs --cleanup     stop leftovers of earlier runs
//
// Paths are resolved from this file, so it runs from any directory.
// Logs and screenshots go to target/run-segno/ (ignored by git).

// biome-ignore-all lint/performance/useDomNodeTextContent: innerText keeps the breaks between flex items, which the printed text needs.

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import readline from "node:readline";
import { chromium } from "playwright-core";

const ROOT = path.resolve(import.meta.dirname, "../../..");
const CDP_PORT = Number(process.env.SEGNO_CDP_PORT ?? 9222);
const CDP_URL = `http://127.0.0.1:${CDP_PORT}`;
const VITE_PORT = 5173;
const OUT_DIR = path.join(ROOT, "target", "run-segno");
const SHOT_DIR = process.env.SCREENSHOT_DIR ?? path.join(OUT_DIR, "shots");
const APP_LOG = path.join(OUT_DIR, "app.log");
const BUILD_LOG = path.join(OUT_DIR, "build.log");
const DESKTOP_EXE = path.join(ROOT, "target", "debug", "segno-desktop.exe");
// Unless SEGNO_LIBRARY_DIR says otherwise, every launch gets a fresh library here, so agents
// never touch the user's own library.
const TEST_LIBRARY = path.join(OUT_DIR, "library");
const FFMPEG = path.join(ROOT, ".tools", "ffmpeg", "bin", "ffmpeg.exe");
const TEST_CLIP = path.join(OUT_DIR, "test-clip.mp4");
// The first `pnpm dev` compiles the whole Rust workspace (about 4 minutes on a laptop).
const LAUNCH_TIMEOUT_MS = 10 * 60_000;
const APP_PAGE = /localhost:5173|tauri\.localhost/;
const ANSI_ESCAPES = new RegExp(`${String.fromCharCode(27)}\\[[0-9;]*m`, "g");

fs.mkdirSync(SHOT_DIR, { recursive: true });

let child = null; // the process tree we started: `pnpm dev` or the built exe
let launchedHere = false; // only a driver that launched the app cleans up on exit
let browser = null; // Playwright's CDP connection to WebView2
let page = null; // the app's page inside the webview
const problems = []; // console errors and warnings, page errors and failed requests

const log = (...parts) => console.log(...parts);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const rel = (file) => path.relative(ROOT, file);
const squash = (text) => text.replace(/\s+/g, " ").trim();

function need() {
  if (!page) throw new Error('no app attached: run "launch" first');
  return page;
}

function tail(file, lines) {
  try {
    const text = fs.readFileSync(file, "utf8").replace(ANSI_ESCAPES, "");
    return text.split(/\r?\n/).slice(-lines).join("\n");
  } catch {
    return "(no log yet)";
  }
}

/** Child environment with the tools installed in the user profile on PATH, even if this
 *  shell started before they were installed. */
function toolEnv(extra) {
  const toolDirs = [
    path.join(process.env.USERPROFILE ?? "", ".cargo", "bin"),
    path.join(
      process.env.LOCALAPPDATA ?? "",
      "Microsoft",
      "WinGet",
      "Packages",
      "astral-sh.uv_Microsoft.Winget.Source_8wekyb3d8bbwe",
    ),
  ].filter((dir) => fs.existsSync(dir));
  const env = { ...process.env };
  const pathKey = Object.keys(env).find((key) => key.toLowerCase() === "path") ?? "Path";
  const current = env[pathKey] ?? "";
  for (const key of Object.keys(env)) if (key.toLowerCase() === "path") delete env[key];
  env[pathKey] = [...toolDirs, current].join(path.delimiter);
  return { ...env, ...extra };
}

function powershell(script) {
  return spawnSync("powershell", ["-NoProfile", "-NonInteractive", "-Command", script], {
    encoding: "utf8",
    windowsHide: true,
  });
}

/**
 * Stops processes left by earlier runs. Ending `pnpm dev` from outside (closing its shell,
 * stopping a background task) leaves segno-desktop.exe, its WebView2 and Vite running, holding
 * ports 9222 and 5173. Only processes started from this repository are touched.
 */
function cleanupLeftovers() {
  const root = ROOT.replace(/'/g, "''");
  const result = powershell(`
    $root = '${root}'
    $stopped = @()
    foreach ($p in @(Get-Process segno-desktop -ErrorAction SilentlyContinue)) {
      if ($p.Path -like "$root*") { Stop-Process -Id $p.Id -Force; $stopped += "segno-desktop ($($p.Id))" }
    }
    $vite = Get-NetTCPConnection -LocalPort ${VITE_PORT} -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($vite) {
      $proc = Get-CimInstance Win32_Process -Filter "ProcessId=$($vite.OwningProcess)"
      if ($proc.CommandLine -like "*$root*") { Stop-Process -Id $vite.OwningProcess -Force; $stopped += "vite ($($vite.OwningProcess))" }
    }
    if ($stopped.Count -gt 0) { "cleanup: stopped " + ($stopped -join ", ") } else { "cleanup: nothing left running" }
  `);
  log((result.stdout || result.stderr || "").trim());
}

function killTree(pid) {
  spawnSync("taskkill", ["/PID", String(pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
}

function start(commandLine, env) {
  const out = fs.openSync(APP_LOG, "w");
  // A single command line with shell: true runs `pnpm` (a .cmd file) or the exe alike.
  const proc = spawn(commandLine, {
    cwd: ROOT,
    env,
    shell: true,
    stdio: ["ignore", out, out],
    windowsHide: true,
  });
  proc.on("exit", (code) => {
    fs.closeSync(out);
    if (child === proc) child = null;
    if (page) log(`the app process exited (code ${code})`);
  });
  return proc;
}

function buildForProduction(env) {
  const out = fs.openSync(BUILD_LOG, "w");
  const result = spawnSync("pnpm --filter @segno/desktop tauri build --debug --no-bundle", {
    cwd: ROOT,
    env,
    shell: true,
    stdio: ["ignore", out, out],
    windowsHide: true,
  });
  fs.closeSync(out);
  if (result.status !== 0) {
    throw new Error(`build failed; last lines of ${rel(BUILD_LOG)}:\n${tail(BUILD_LOG, 25)}`);
  }
}

async function devToolsUp() {
  try {
    const response = await fetch(`${CDP_URL}/json/version`, { signal: AbortSignal.timeout(1_000) });
    return response.ok;
  } catch {
    return false;
  }
}

async function waitForDevTools() {
  const deadline = Date.now() + LAUNCH_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (await devToolsUp()) return;
    if (!child) {
      throw new Error(
        `the app exited before opening its window; last lines of ${rel(APP_LOG)}:\n${tail(APP_LOG, 25)}`,
      );
    }
    await sleep(500);
  }
  throw new Error(
    `no DevTools endpoint after ${LAUNCH_TIMEOUT_MS / 60_000} min; see ${rel(APP_LOG)}`,
  );
}

async function attach() {
  browser = await chromium.connectOverCDP(CDP_URL);
  for (let attempt = 0; attempt < 60 && !page; attempt += 1) {
    const pages = browser.contexts().flatMap((context) => context.pages());
    page = pages.find((candidate) => APP_PAGE.test(candidate.url())) ?? null;
    if (!page) await sleep(250);
  }
  if (!page) throw new Error("attached to WebView2 but the app page never appeared");
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") {
      problems.push(`${message.type()}: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => problems.push(`page error: ${error.message}`));
  // Console 404 messages carry no URL; the response does.
  page.on("response", (response) => {
    if (response.status() >= 400) problems.push(`HTTP ${response.status()}: ${response.url()}`);
  });
  page.on("requestfailed", (request) => {
    const reason = request.failure()?.errorText ?? "";
    // Chromium cancels in-flight range requests of a <video> whenever it seeks: not a failure.
    if (request.resourceType() === "media" && reason.includes("ERR_ABORTED")) return;
    problems.push(`request failed: ${request.url()} ${reason}`);
  });
  await page.waitForSelector("footer", { timeout: 30_000 });
}

async function launch(mode) {
  if (page) return log("already launched");
  if (mode !== "dev" && mode !== "prod") throw new Error(`unknown mode "${mode}": use dev or prod`);
  if (await devToolsUp()) {
    throw new Error(`port ${CDP_PORT} already serves DevTools (an earlier run?): run "cleanup"`);
  }
  launchedHere = true;
  const library = process.env.SEGNO_LIBRARY_DIR ?? TEST_LIBRARY;
  if (!process.env.SEGNO_LIBRARY_DIR) fs.rmSync(TEST_LIBRARY, { recursive: true, force: true });
  log(`library: ${library}${process.env.SEGNO_LIBRARY_DIR ? "" : " (fresh test library)"}`);
  const env = toolEnv({
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}`,
    SEGNO_LIBRARY_DIR: library,
  });
  if (mode === "prod") {
    log(`building the app with the UI embedded (log: ${rel(BUILD_LOG)})...`);
    buildForProduction(env);
    child = start(`"${DESKTOP_EXE}"`, env);
  } else {
    log(`starting pnpm dev (log: ${rel(APP_LOG)}); a first run compiles Rust for minutes...`);
    child = start("pnpm dev", env);
  }
  await waitForDevTools();
  await attach();
  log(`launched (${mode}): ${page.url()}`);
}

/** Asks our app windows to close, as a user would, so WebView2 can flush its storage. */
function closeAppWindows() {
  const root = ROOT.replace(/'/g, "''");
  powershell(`
    $root = '${root}'
    foreach ($p in @(Get-Process segno-desktop -ErrorAction SilentlyContinue)) {
      if ($p.Path -like "$root*") { [void]$p.CloseMainWindow() }
    }
  `);
}

async function quit() {
  await browser?.close().catch(() => {});
  browser = null;
  page = null;
  if (child) {
    // Close the window first: killing WebView2 outright loses its latest localStorage writes.
    closeAppWindows();
    for (let waited = 0; child && waited < 8_000; waited += 200) await sleep(200);
    if (child?.pid) {
      log("the app did not exit after closing its window: killing its process tree");
      killTree(child.pid);
    } else {
      log("app closed");
    }
  }
  child = null;
  // Another driver's app may be running: only sweep up after our own launch.
  if (launchedHere) cleanupLeftovers();
}

async function apiConfig() {
  // Same IPC call the UI makes at start-up (apps/ui/src/platform/platform.ts).
  return need().evaluate(() => window.__TAURI_INTERNALS__.invoke("api_config"));
}

/** A 5-second 1280×720 clip at 60 fps with a 440 Hz tone, made once with the repo's ffmpeg. */
function makeTestClip() {
  if (fs.existsSync(TEST_CLIP)) return TEST_CLIP;
  if (!fs.existsSync(FFMPEG)) throw new Error("ffmpeg is missing: run `pnpm tools`");
  const result = spawnSync(
    FFMPEG,
    [
      ...["-v", "error", "-y"],
      ...["-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=60:duration=5"],
      ...["-f", "lavfi", "-i", "sine=frequency=440:duration=5"],
      ...["-c:v", "libopenh264", "-c:a", "aac", "-shortest", TEST_CLIP],
    ],
    { encoding: "utf8", windowsHide: true },
  );
  if (result.status !== 0) throw new Error(`ffmpeg could not make the test clip: ${result.stderr}`);
  return TEST_CLIP;
}

/** State of the player's video element. */
function videoState(app) {
  return app.evaluate(() => {
    const video = document.querySelector("video");
    return video
      ? {
          time: video.currentTime,
          paused: video.paused,
          rate: video.playbackRate,
          duration: video.duration,
          ready: video.readyState,
        }
      : null;
  });
}

/** GETs a path of the embedded API as JSON; null on an HTTP error. */
async function apiJson(apiPath) {
  const { base_url: baseUrl, token } = await apiConfig();
  const response = await fetch(`${baseUrl}${apiPath}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  return response.ok ? response.json() : null;
}

/** A video as the embedded API describes it, or null. */
function fetchVideo(id) {
  return apiJson(`/videos/${id}`);
}

/**
 * Serves one file on 127.0.0.1 (a random port) with Range requests. The app's CSP allows media
 * from there, so the player can load files the API does not serve.
 */
function serveFile(file) {
  const size = fs.statSync(file).size;
  const types = {
    ".mp4": "video/mp4",
    ".m4v": "video/mp4",
    ".mov": "video/mp4",
    ".webm": "video/webm",
  };
  const server = http.createServer((request, response) => {
    const range = /^bytes=(\d*)-(\d*)$/.exec(request.headers.range ?? "");
    let [start, end] = [0, size - 1];
    if (range?.[1]) [start, end] = [Number(range[1]), Math.min(Number(range[2] || end), end)];
    else if (range?.[2]) start = size - Number(range[2]);
    response.writeHead(range ? 206 : 200, {
      "Accept-Ranges": "bytes",
      "Content-Length": end - start + 1,
      "Content-Type": types[path.extname(file).toLowerCase()] ?? "application/octet-stream",
      ...(range && { "Content-Range": `bytes ${start}-${end}/${size}` }),
    });
    fs.createReadStream(file, { start, end }).pipe(response);
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

/** Points the player's video at `url` until it can play through; returns the previous source. */
function swapSource(app, url) {
  return app.evaluate(async (source) => {
    const video = document.querySelector("video");
    const previous = video.currentSrc;
    const ready = new Promise((resolve) =>
      video.addEventListener("canplaythrough", resolve, { once: true }),
    );
    video.src = source;
    await ready;
    return previous;
  }, url);
}

/**
 * The same timings on the open video's original file, which the API stops serving once the
 * seek proxy exists: the driver serves it, and the player gets its source back afterwards.
 */
async function benchOriginal(app, id, fps) {
  const system = await apiJson("/system");
  const folder = path.join(system.library_path, "media", id);
  const name = fs.readdirSync(folder).find((file) => file.startsWith("original."));
  if (!name) throw new Error(`no original file in ${folder}`);
  const server = await serveFile(path.join(folder, name));
  const previous = await swapSource(app, `http://127.0.0.1:${server.address().port}/${name}`);
  try {
    return await bench(app, fps);
  } finally {
    await swapSource(app, previous);
    server.close();
    server.closeAllConnections();
  }
}

/** Waits until the API has prepared the video for navigation; returns it, or null. */
async function waitForPreparation(id, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let lastReport = Date.now();
  while (Date.now() < deadline) {
    const video = await fetchVideo(id);
    if (!video?.preparation) return video;
    if (Date.now() - lastReport > 5_000) {
      const { stage, progress } = video.preparation;
      log(`  preparing ${video.title}: ${stage} ${Math.round(progress * 100)} %`);
      lastReport = Date.now();
    }
    await sleep(500);
  }
  return null;
}

/**
 * Times what the navigation budget measures, on the open video: jumps to ten points spread
 * over it, then 30 frame steps back and 30 forward from the middle, enough to cross a whole
 * keyframe interval (milliseconds from setting `currentTime` to the `seeked` event: the new
 * frame is ready to show), then three play starts after a jump (from `play()` to the first
 * new frame on screen).
 */
function bench(app, fps) {
  return app.evaluate(async (framesPerSecond) => {
    const video = document.querySelector("video");
    const seeked = () =>
      new Promise((resolve) => video.addEventListener("seeked", resolve, { once: true }));
    const timed = async (time) => {
      const start = performance.now();
      video.currentTime = time;
      await seeked();
      return Math.round(performance.now() - start);
    };
    const summary = (values) => {
      const sorted = [...values].sort((a, b) => a - b);
      const p95 = sorted[Math.ceil(sorted.length * 0.95) - 1];
      return { median: sorted[Math.floor(sorted.length / 2)], p95, max: sorted.at(-1) };
    };
    video.pause();
    const fractions = [0.71, 0.14, 0.53, 0.04, 0.92, 0.42, 0.24, 0.64, 0.09, 0.84];
    const jumps = [];
    for (const fraction of fractions) jumps.push(await timed(fraction * video.duration));
    await timed(video.duration / 2);
    const back = [];
    for (let i = 0; i < 30; i += 1) back.push(await timed(video.currentTime - 1 / framesPerSecond));
    const forward = [];
    for (let i = 0; i < 30; i += 1)
      forward.push(await timed(video.currentTime + 1 / framesPerSecond));
    const nextFrameShown = () =>
      new Promise((resolve) => {
        const from = video.currentTime;
        const onFrame = (_now, frame) => {
          if (frame.mediaTime > from + 0.001) resolve();
          else video.requestVideoFrameCallback(onFrame);
        };
        video.requestVideoFrameCallback(onFrame);
      });
    const playStarts = [];
    for (const fraction of [0.33, 0.61, 0.87]) {
      await timed(fraction * video.duration);
      const start = performance.now();
      const shown = nextFrameShown();
      await video.play();
      await shown;
      playStarts.push(Math.round(performance.now() - start));
      video.pause();
    }
    return {
      source: video.currentSrc.includes("copy=seek-proxy") ? "seek proxy" : "original",
      jumps: summary(jumps),
      frameBack: summary(back),
      frameForward: summary(forward),
      playStart: summary(playStarts),
    };
  }, fps);
}

/** Id of the video open in the player, from the URL. */
function openVideoId(app) {
  return /#\/videos\/([0-9a-f-]+)/.exec(app.url())?.[1] ?? null;
}

/** The end-to-end flow verified when this skill was written. Returns true when all pass. */
async function smoke() {
  const app = need();
  const results = [];
  const check = (name, ok, detail = "") => {
    results.push(ok);
    log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `  [${detail}]` : ""}`);
  };
  const appears = (selector, timeout = 5_000) =>
    app.waitForSelector(selector, { timeout }).then(
      () => true,
      () => false,
    );

  await app.reload();
  const connected = await appears("footer >> text=/Segno \\d/", 30_000);
  const footer = connected ? squash(await app.locator("footer").innerText()) : "";
  check(
    "status bar shows the core version (the UI reached the API with its token)",
    connected,
    footer,
  );
  const language = await app.evaluate(() => document.documentElement.lang);
  check(
    "library screen renders",
    await appears("h1"),
    `${language}: ${await app.locator("h1").innerText()}`,
  );
  await COMMANDS.ss("smoke-1-library");

  await app.keyboard.press("Control+k");
  check("Ctrl+K opens the command palette", await appears("[data-slot=command-input]"));
  await app.keyboard.type(language === "es" ? "atajo" : "shortcut", { delay: 20 });
  await COMMANDS.ss("smoke-2-palette");
  await app.keyboard.press("Enter");
  check(
    "Enter runs the selected command (shortcuts dialog)",
    await appears("[data-slot=dialog-content]"),
  );
  await COMMANDS.ss("smoke-3-shortcuts");
  await app.keyboard.press("Escape");
  await sleep(300);

  await app.keyboard.press("?");
  check('"?" opens the shortcuts dialog', await appears("[data-slot=dialog-content]"));
  await app.keyboard.press("Escape");
  await sleep(300);

  await app.keyboard.press("Control+2");
  await sleep(300);
  // The Player tab reopens the last video of this library, if any.
  check("Ctrl+2 opens the player", /#\/(player|videos\/)/.test(app.url()), app.url());
  await app.keyboard.press("Control+1");
  await sleep(300);
  check("Ctrl+1 returns to the library", app.url().endsWith("#/"), app.url());

  const toggle = app.getByRole("button", { name: /^(Idioma|Language)$/ });
  await toggle.click();
  await sleep(300);
  const switched = await app.evaluate(() => document.documentElement.lang);
  check(
    "the language button switches language",
    switched !== language,
    `${language} -> ${switched}`,
  );
  await toggle.click();
  await sleep(300);

  const videos = await COMMANDS.get("/videos", { quiet: true });
  check(
    "the embedded API answers GET /videos with the session token",
    videos === 200,
    `HTTP ${videos}`,
  );

  // Import a generated clip through the API and study it in the player.
  const imported = await COMMANDS.import("test-clip", { quiet: true });
  check(
    "POST /videos/import adds a clip made with ffmpeg",
    imported?.status === "ready",
    imported ? `${imported.width}×${imported.height} @ ${imported.fps} fps` : "import failed",
  );
  if (imported) {
    const prepared = await waitForPreparation(imported.id, 120_000);
    check(
      "the core prepares it for instant navigation (seek proxy, thumbnails, waveform)",
      prepared?.seek_proxy === true &&
        prepared.has_peaks === true &&
        prepared.thumbnails !== null &&
        !prepared.preparation_error,
      prepared?.preparation_error ??
        (prepared ? `${prepared.thumbnails?.count} thumbnails` : "not found or timed out"),
    );
    const sprite = await COMMANDS.get(`/videos/${imported.id}/thumbnails`, { quiet: true });
    const peaks = await COMMANDS.get(`/videos/${imported.id}/peaks`, { quiet: true });
    check(
      "the API serves its thumbnails and waveform",
      sprite === 200 && peaks === 200,
      `HTTP ${sprite} / ${peaks}`,
    );
    const switched = await app
      .waitForFunction(
        () => document.querySelector("video")?.currentSrc.includes("copy=seek-proxy") === true,
        null,
        { timeout: 10_000 },
      )
      .then(
        () => true,
        () => false,
      );
    check("the player switches to the seek proxy", switched);
    const loaded = await app
      // HAVE_ENOUGH_DATA: playable through. With only the first frame (2), the first play
      // stalls for a moment while the decoder starts.
      .waitForFunction(() => (document.querySelector("video")?.readyState ?? 0) >= 4, null, {
        timeout: 15_000,
      })
      .then(
        () => true,
        () => false,
      );
    const state = await videoState(app);
    check(
      "the player loads the media from the API",
      loaded,
      state ? `${state.duration.toFixed(2)} s` : "",
    );

    await app.keyboard.press("Space");
    const started = Date.now();
    const playing = await app
      .waitForFunction(
        () => {
          const video = document.querySelector("video");
          return video !== null && !video.paused && video.currentTime > 0.3;
        },
        null,
        { timeout: 3_000 },
      )
      .then(
        () => true,
        () => false,
      );
    let now = await videoState(app);
    check("Space plays", playing, `t = ${now?.time.toFixed(3)} after ${Date.now() - started} ms`);
    await app.keyboard.press("Space");
    await sleep(200);
    now = await videoState(app);
    check("Space pauses", now?.paused === true);

    await app.keyboard.press("Home");
    await app.keyboard.press(".");
    await sleep(200);
    now = await videoState(app);
    check(
      '"." steps exactly one frame at 60 fps',
      Math.abs((now?.time ?? 0) - 1.5 / 60) < 0.001,
      `t = ${now?.time.toFixed(4)}`,
    );
    await app.keyboard.press("ArrowRight");
    await sleep(200);
    now = await videoState(app);
    check(
      '"→" jumps 2 seconds',
      Math.abs((now?.time ?? 0) - (2 + 1.5 / 60)) < 0.001,
      `t = ${now?.time.toFixed(4)}`,
    );
    await app.keyboard.press("-");
    await sleep(100);
    now = await videoState(app);
    check('"-" slows down to 95 %', now?.rate === 0.95, `rate = ${now?.rate}`);
    await app.keyboard.press("+");
    await COMMANDS.ss("smoke-4-player");
  }
  check(
    "no console errors, warnings or failed requests",
    problems.length === 0,
    problems.join(" | "),
  );
  const passed = results.every(Boolean);
  log(passed ? "SMOKE PASSED" : "SMOKE FAILED");
  return passed;
}

const COMMANDS = {
  async launch(mode) {
    await launch(mode || "dev");
  },
  async ss(name) {
    const file = path.join(SHOT_DIR, `${name || `shot-${Date.now()}`}.png`);
    await need().screenshot({ path: file });
    log(`screenshot: ${file}`);
  },
  async press(key) {
    await need().keyboard.press(key);
    log(`pressed ${key}`);
  },
  async type(text) {
    await need().keyboard.type(text, { delay: 20 });
    log(`typed ${JSON.stringify(text)}`);
  },
  async click(selector) {
    const result = await need().evaluate((s) => {
      const element = document.querySelector(s);
      if (!(element instanceof HTMLElement)) return "NOT_FOUND";
      element.click();
      return "OK";
    }, selector);
    log(`click ${selector} -> ${result}`);
  },
  async "click-text"(text) {
    const result = await need().evaluate((t) => {
      const candidates = [
        ...document.querySelectorAll("button, a, [role=button], [role=option], [cmdk-item]"),
      ];
      const element =
        candidates.find((e) => e.textContent?.trim() === t) ??
        candidates.find((e) => e.textContent?.includes(t));
      if (!(element instanceof HTMLElement)) return "NOT_FOUND";
      element.click();
      return `OK (${element.tagName.toLowerCase()})`;
    }, text);
    log(`click-text ${JSON.stringify(text)} -> ${result}`);
  },
  async wait(selector) {
    await need().waitForSelector(selector, { timeout: 10_000 });
    log(`found ${selector}`);
  },
  async text(selector) {
    const content = await need().evaluate(
      (s) => (s ? document.querySelector(s) : document.body)?.innerText ?? "(not found)",
      selector || null,
    );
    log(squash(content));
  },
  async eval(expression) {
    log(JSON.stringify(await need().evaluate(expression)));
  },
  async goto(route) {
    await need().evaluate((hash) => {
      window.location.hash = hash;
    }, route.replace(/^#/, "") || "/");
    await sleep(300);
    log(`at ${need().url()}`);
  },
  async api() {
    log(JSON.stringify(await apiConfig()));
  },
  /** Imports a file (or `test-clip`, generated with ffmpeg) and opens it in the player. */
  async import(filePath, options = {}) {
    const file = filePath === "test-clip" ? makeTestClip() : path.resolve(ROOT, filePath);
    const { base_url: baseUrl, token } = await apiConfig();
    const response = await fetch(`${baseUrl}/videos/import`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify({ path: file }),
    });
    const body = await response.json().catch(() => null);
    if (!options.quiet) log(`HTTP ${response.status} ${JSON.stringify(body)}`);
    if (!response.ok) return null;
    await need().evaluate((id) => {
      window.location.hash = `/videos/${id}`;
    }, body.id);
    return body;
  },
  async get(apiPath, options = {}) {
    const { base_url: baseUrl, token } = await apiConfig();
    const response = await fetch(`${baseUrl}${apiPath || "/system"}`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const body = await response.text();
    if (!options.quiet) log(`HTTP ${response.status} ${body}`);
    return response.status;
  },
  console() {
    log(problems.length ? problems.join("\n") : "no console errors, warnings or failed requests");
  },
  async sleep(ms) {
    await sleep(Number(ms) || 1_000);
  },
  async smoke() {
    await smoke();
  },
  /**
   * Navigation timings of the open video, after waiting for its preparation. `bench now`: of
   * whatever the player has loaded, without waiting. `bench original`: of the original file.
   */
  async bench(mode) {
    const id = openVideoId(need());
    if (!id) throw new Error("open a video first (e.g. import test-clip)");
    const video = mode === "now" ? await fetchVideo(id) : await waitForPreparation(id, 15 * 60_000);
    if (mode === "original") {
      log(JSON.stringify(await benchOriginal(need(), id, video?.fps ?? 30)));
      return;
    }
    if (mode !== "now" && video?.seek_proxy) {
      await need().waitForFunction(
        () => document.querySelector("video")?.currentSrc.includes("copy=seek-proxy") === true,
        null,
        { timeout: 10_000 },
      );
    }
    await need().waitForFunction(() => (document.querySelector("video")?.readyState ?? 0) >= 4);
    log(JSON.stringify(await bench(need(), video?.fps ?? 30)));
  },
  cleanup() {
    cleanupLeftovers();
  },
  help() {
    log(`commands: ${Object.keys(COMMANDS).join(", ")}, quit`);
  },
};

async function repl() {
  const interactive = Boolean(process.stdin.isTTY);
  const lines = readline.createInterface({ input: process.stdin, terminal: interactive });
  const prompt = () => {
    if (interactive) process.stdout.write("segno> ");
  };
  log('Segno driver: "help" lists the commands, "launch" starts the app, "quit" stops it.');
  prompt();
  for await (const line of lines) {
    const [command, ...rest] = line.trim().split(/\s+/);
    if (command === "quit") break;
    if (command) {
      if (!interactive) log(`segno> ${line.trim()}`);
      const handler = COMMANDS[command];
      if (!handler) {
        log(`unknown command "${command}" (try "help")`);
      } else {
        try {
          await handler(rest.join(" "));
        } catch (error) {
          log(`ERROR: ${error.message}`);
        }
      }
    }
    prompt();
  }
  lines.close();
  await quit(); // "quit" or end of input: never leave the app running
}

process.on("SIGINT", () => {
  quit().finally(() => process.exit(130));
});

const [flag, argument] = process.argv.slice(2);
if (flag === "--smoke") {
  let passed = false;
  try {
    await launch(argument ?? "dev");
    passed = await smoke();
  } catch (error) {
    log(`ERROR: ${error.message}`);
  } finally {
    await quit();
  }
  process.exit(passed ? 0 : 1);
} else if (flag === "--cleanup") {
  cleanupLeftovers();
} else if (flag === undefined) {
  await repl();
  process.exit(0);
} else {
  log("usage: driver.mjs [--smoke [dev|prod] | --cleanup]");
  process.exit(2);
}

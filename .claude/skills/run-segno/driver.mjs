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
// The name changes with the recipe, so an old cached clip is never reused.
const TEST_CLIP = path.join(OUT_DIR, "test-clip-v2.mp4");
/** When the test clip's notes start: every half second. */
const TEST_NOTES = [0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5];
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

/**
 * A 5-second 1280×720 clip at 60 fps with a plucked note every half second (`TEST_NOTES`),
 * made once with the repo's ffmpeg.
 */
function makeTestClip() {
  if (fs.existsSync(TEST_CLIP)) return TEST_CLIP;
  if (!fs.existsSync(FFMPEG)) throw new Error("ffmpeg is missing: run `pnpm tools`");
  const notes = TEST_NOTES.map((start, index) => {
    const phase = `PI*${440 + 55 * index}*(t-${start})`;
    return `gte(t,${start})*exp(-(t-${start})*6)*(sin(2*${phase})+0.5*sin(4*${phase}))`;
  }).join("+");
  const result = spawnSync(
    FFMPEG,
    [
      ...["-v", "error", "-y"],
      ...["-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=60:duration=5"],
      ...["-f", "lavfi", "-i", `aevalsrc='0.3*(${notes})':s=48000:d=5`],
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
    const video = document.querySelector("[data-testid=player-video]");
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
    const video = document.querySelector("[data-testid=player-video]");
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
    const video = document.querySelector("[data-testid=player-video]");
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
        () =>
          document
            .querySelector("[data-testid=player-video]")
            ?.currentSrc.includes("copy=seek-proxy") === true,
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
      .waitForFunction(
        () => (document.querySelector("[data-testid=player-video]")?.readyState ?? 0) >= 4,
        null,
        {
          timeout: 15_000,
        },
      )
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
          const video = document.querySelector("[data-testid=player-video]");
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

    // The timeline: waveform and attacks, stepping by attack, jumps, zoom and the preview.
    const detail = app.locator("[data-testid=timeline-detail]");
    const overview = app.locator("[data-testid=timeline-overview]");
    const drawn = await app
      .waitForFunction(
        () => {
          const canvas = document.querySelector("[data-testid=timeline-detail]");
          return canvas?.getAttribute("data-waveform") === "ready" &&
            Number(canvas.getAttribute("data-attacks")) > 0
            ? canvas.getAttribute("data-attacks")
            : null;
        },
        null,
        { timeout: 10_000 },
      )
      .then(
        (handle) => handle.jsonValue(),
        () => null,
      );
    check(
      "the timeline draws the waveform and the attacks found in the audio",
      Number(drawn) === TEST_NOTES.length,
      `${drawn ?? 0} attacks for ${TEST_NOTES.length} notes`,
    );

    await app.keyboard.press("Home");
    await app.keyboard.press("Shift+ArrowRight");
    await app.keyboard.press("Shift+ArrowRight");
    await sleep(200);
    const second = (await videoState(app))?.time ?? 0;
    await app.keyboard.press("Shift+ArrowLeft");
    await sleep(200);
    const first = (await videoState(app))?.time ?? 0;
    check(
      '"Shift+→" and "Shift+←" step from attack to attack',
      Math.abs(second - TEST_NOTES[1]) <= 0.02 && Math.abs(first - TEST_NOTES[0]) <= 0.02,
      `t = ${second.toFixed(3)}, then ${first.toFixed(3)}`,
    );

    const box = await overview.boundingBox();
    if (box) await app.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
    await sleep(200);
    now = await videoState(app);
    check(
      "a click on the overview jumps there",
      Math.abs((now?.time ?? 0) - (now?.duration ?? 0) / 2) < 0.1,
      `t = ${now?.time.toFixed(3)}`,
    );

    const area = await detail.boundingBox();
    const span = async () => Number(await detail.getAttribute("data-span"));
    // The zoom outlives the test library (localStorage): start from the whole clip.
    await app.keyboard.press("Control+0");
    await sleep(100);
    const before = await span();
    if (area) {
      await app.mouse.move(area.x + area.width * 0.3, area.y + area.height / 2);
      await app.keyboard.down("Control");
      await app.mouse.wheel(0, -300);
      await app.keyboard.up("Control");
    }
    await sleep(200);
    const zoomed = await span();
    await app.keyboard.press("Control+0");
    await sleep(100);
    const fitted = await span();
    check(
      "Ctrl + wheel zooms the detail view and Ctrl+0 shows the whole song",
      zoomed < before && Math.abs(fitted - (now?.duration ?? 0)) < 0.1,
      `${before} s -> ${zoomed} s -> ${fitted} s`,
    );

    if (area) await app.mouse.move(area.x + area.width * 0.7, area.y + area.height / 2);
    const exact = await app
      .waitForSelector("[data-testid=timeline-preview][data-exact=true]", { timeout: 5_000 })
      .then(
        () => true,
        () => false,
      );
    check("hovering the detail view previews the exact frame", exact);
    await COMMANDS.ss("smoke-4-player");
    await app.mouse.move(0, 0);

    // Beat grid by tap tempo, stepping by beat, go-to, history, markers, loops, memory.
    const seekTo = (time) =>
      app.evaluate((t) => {
        document.querySelector("[data-testid=player-video]").currentTime = t;
      }, time);
    const timeNow = async () => (await videoState(app))?.time ?? -1;
    const goTo = async (text) => {
      await app.keyboard.press("G");
      await app.waitForSelector("[cmdk-input]:focus", { timeout: 3_000 });
      await app.keyboard.type(text, { delay: 20 });
      await sleep(150);
      await app.keyboard.press("Enter");
      // Until the dialog has gone, keys belong to it.
      await app.waitForSelector("[data-slot=dialog-content]", {
        state: "detached",
        timeout: 3_000,
      });
      await sleep(100);
    };

    await seekTo(0.3);
    await app.evaluate(() => document.querySelector("[data-testid=player-video]").play());
    await sleep(150);
    for (let tap = 0; tap < 4; tap++) {
      await app.keyboard.press("T");
      await sleep(500);
    }
    await app.keyboard.press("Space");
    await sleep(300);
    const beats = ((await detail.getAttribute("data-grid")) ?? "")
      .split(",")
      .filter(Boolean)
      .map(Number);
    const position = squash(await app.locator("[data-testid=grid-position]").innerText());
    check(
      '"T" taps a beat grid while playing',
      beats.length >= 3 && /\d+\.\d/.test(position),
      `${beats.length} beats: ${position}`,
    );

    await app.keyboard.press("S");
    await app.keyboard.press("Home");
    await app.keyboard.press("ArrowRight");
    await sleep(250);
    // The first beat after 0: tapped, or extrapolated before the first tap.
    const period =
      beats.length > 1
        ? (beats[Math.min(4, beats.length - 1)] - beats[0]) / Math.min(4, beats.length - 1)
        : 0.5;
    let firstBeat = beats[0] ?? 0;
    while (firstBeat - period > 0.001) firstBeat -= period;
    const onBeat = await timeNow();
    const unit = squash(await app.locator("[data-testid=step-unit]").innerText());
    check(
      '"S" steps by beat: "→" lands on the first beat',
      Math.abs(onBeat - firstBeat) < 0.002,
      `${unit}, t = ${onBeat.toFixed(3)} for ${firstBeat.toFixed(3)}`,
    );
    await app.keyboard.press("S");
    await app.keyboard.press("S");

    await goTo("3.5");
    const went = await timeNow();
    check('"G" goes to a typed time', Math.abs(went - 3.5) < 0.01, `t = ${went.toFixed(3)}`);

    await goTo("4.5");
    await sleep(1000);
    await app.keyboard.press("Home");
    await sleep(1000);
    await app.keyboard.press("Alt+ArrowLeft");
    await sleep(300);
    const back = await timeNow();
    await app.keyboard.press("Alt+ArrowRight");
    await sleep(300);
    const forward = await timeNow();
    check(
      '"Alt+←" goes back to before the last jump and "Alt+→" returns',
      Math.abs(back - 4.5) < 0.01 && forward < 0.01,
      `t = ${back.toFixed(3)}, then ${forward.toFixed(3)}`,
    );

    await seekTo(1.25);
    await sleep(200);
    await app.keyboard.press("M");
    await app
      .waitForFunction(
        () => document.querySelector("[data-testid=markers-toggle]")?.textContent?.endsWith("1"),
        null,
        { timeout: 5_000 },
      )
      .catch(() => {});
    await app.keyboard.press("Home");
    await app.keyboard.press("1");
    await sleep(250);
    const atMarker = await timeNow();
    check(
      '"M" adds a marker and "1" jumps to it',
      Math.abs(atMarker - 1.25) < 0.01,
      `t = ${atMarker.toFixed(3)}`,
    );

    await seekTo(1);
    await sleep(150);
    await app.keyboard.press("A");
    await seekTo(2);
    await sleep(150);
    await app.keyboard.press("B");
    await sleep(150);
    const loopRange = await detail.getAttribute("data-loop");
    await app.keyboard.press("Home");
    await sleep(200);
    await app.keyboard.press("Space");
    await sleep(2_600);
    const looping = await videoState(app);
    await app.keyboard.press("Space");
    const loopText = squash(await app.locator("[data-testid=loop-state]").innerText());
    const gaps = ((await app.getAttribute("[data-testid=player-video]", "data-loop-gaps")) ?? "")
      .split(",")
      .filter(Boolean)
      .map(Number);
    check(
      '"A" and "B" set a loop that plays round and round',
      loopRange === "1.000-2.000" &&
        /×[3-9]/.test(loopText) &&
        (looping?.time ?? 0) >= 1 &&
        (looping?.time ?? 0) < 2.05,
      `${loopText}, gaps ${gaps.join("/")} ms`,
    );

    await sleep(1_500); // the state is saved a second after the last change
    await app.reload();
    const restored = await app
      .waitForFunction(
        () =>
          document.querySelector("[data-testid=timeline-detail]")?.getAttribute("data-loop") ===
          "1.000-2.000",
        null,
        { timeout: 15_000 },
      )
      .then(
        () => true,
        () => false,
      );
    await sleep(500);
    const reopened = squash(await app.locator("[data-testid=grid-position]").innerText());
    const markersAfter = squash(await app.locator("[data-testid=markers-toggle]").innerText());
    check(
      "the video reopens with its loop, beat grid and markers",
      restored && /\d+\.\d/.test(reopened) && markersAfter.endsWith("1"),
      `${reopened}; ${markersAfter}`,
    );
    await COMMANDS.ss("smoke-5-study");

    // Timeline gestures: Shift + drag, edges that snap (Alt: free), double-click.
    await app.keyboard.press("Shift+R"); // no loop
    await app.keyboard.press("Control+0");
    await sleep(150);
    const loopAttr = () => detail.getAttribute("data-loop");
    await COMMANDS.drag("[data-testid=timeline-detail] 0.19 0.41 Shift");
    const drawn2 = await loopAttr();
    await COMMANDS.drag("[data-testid=timeline-detail] 0.41 0.495");
    const snapped = await loopAttr();
    await COMMANDS.drag("[data-testid=timeline-detail] 0.498 0.495 Alt");
    const free = await loopAttr();
    check(
      "Shift + drag makes a loop whose edges drag and snap to the attacks (Alt: no snapping)",
      drawn2 === "0.990-2.050" && snapped === "0.990-2.490" && free === "0.990-2.475",
      `${drawn2}, ${snapped}, ${free}`,
    );

    const count = async () =>
      Number(
        squash(await app.locator("[data-testid=markers-toggle]").innerText()).match(/\d+$/)?.[0],
      );
    const markersBefore = await count();
    await COMMANDS["dblclick-at"]("[data-testid=timeline-detail] 0.8 0.7");
    await sleep(600);
    check(
      "a double-click on the timeline adds a marker",
      (await count()) === markersBefore + 1,
      `${markersBefore} -> ${await count()}`,
    );

    await app.click("[data-testid=return-on-pause]");
    await seekTo(3);
    await sleep(200);
    await app.keyboard.press("Space");
    await sleep(700);
    await app.keyboard.press("Space");
    await sleep(300);
    const returned = await timeNow();
    await app.click("[data-testid=return-on-pause]");
    check(
      "with return-on-pause, pausing goes back to where playback started",
      Math.abs(returned - 3) < 0.01,
      `t = ${returned.toFixed(3)}`,
    );
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
  /** Moves the mouse over an element, at fractions of its width and height (default: centre). */
  async move(args) {
    const [selector, fx = "0.5", fy = "0.5"] = args.split(" ");
    const box = await need().locator(selector).boundingBox();
    if (!box) throw new Error(`${selector} is not visible`);
    await need().mouse.move(box.x + box.width * Number(fx), box.y + box.height * Number(fy));
    log(`mouse over ${selector} at ${fx}, ${fy}`);
  },
  /** A real mouse click on an element (it moves the focus, unlike `click`). */
  async "click-at"(args) {
    const [selector, fx = "0.5", fy = "0.5"] = args.split(" ");
    const box = await need().locator(selector).boundingBox();
    if (!box) throw new Error(`${selector} is not visible`);
    await need().mouse.click(box.x + box.width * Number(fx), box.y + box.height * Number(fy));
    log(`mouse click on ${selector} at ${fx}, ${fy}`);
  },
  /** A real double-click on an element, at fractions of its size. */
  async "dblclick-at"(args) {
    const [selector, fx = "0.5", fy = "0.5"] = args.split(" ");
    const box = await need().locator(selector).boundingBox();
    if (!box) throw new Error(`${selector} is not visible`);
    await need().mouse.dblclick(box.x + box.width * Number(fx), box.y + box.height * Number(fy));
    log(`double-click on ${selector} at ${fx}, ${fy}`);
  },
  /**
   * Drags across an element from one fraction of its width to another, at half its height,
   * optionally holding Shift or Alt: `drag [data-testid=timeline-detail] 0.2 0.4 Shift`.
   */
  async drag(args) {
    const [selector, from = "0.3", to = "0.6", modifier] = args.split(" ");
    const app = need();
    const box = await app.locator(selector).boundingBox();
    if (!box) throw new Error(`${selector} is not visible`);
    const y = box.y + box.height / 2;
    if (modifier) await app.keyboard.down(modifier);
    await app.mouse.move(box.x + box.width * Number(from), y);
    await app.mouse.down();
    await app.mouse.move(box.x + box.width * Number(to), y, { steps: 12 });
    await app.mouse.up();
    if (modifier) await app.keyboard.up(modifier);
    log(`dragged ${selector} from ${from} to ${to}${modifier ? ` with ${modifier}` : ""}`);
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
        () =>
          document
            .querySelector("[data-testid=player-video]")
            ?.currentSrc.includes("copy=seek-proxy") === true,
        null,
        { timeout: 10_000 },
      );
    }
    await need().waitForFunction(
      () => (document.querySelector("[data-testid=player-video]")?.readyState ?? 0) >= 4,
    );
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

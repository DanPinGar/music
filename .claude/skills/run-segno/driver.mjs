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
  page.on("requestfailed", (request) =>
    problems.push(`request failed: ${request.url()} ${request.failure()?.errorText ?? ""}`),
  );
  await page.waitForSelector("footer", { timeout: 30_000 });
}

async function launch(mode) {
  if (page) return log("already launched");
  if (mode !== "dev" && mode !== "prod") throw new Error(`unknown mode "${mode}": use dev or prod`);
  if (await devToolsUp()) {
    throw new Error(`port ${CDP_PORT} already serves DevTools (an earlier run?): run "cleanup"`);
  }
  launchedHere = true;
  const env = toolEnv({
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}`,
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
  check("Ctrl+2 opens the player", app.url().endsWith("#/player"), app.url());
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

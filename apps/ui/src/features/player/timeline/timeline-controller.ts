import type { Attacks } from "@segno/api-client";
import { firstAtOrAfter } from "../attacks";
import { formatTimecode } from "../navigation";
import {
  clampView,
  DEFAULT_SPAN,
  FOLLOW_MODES,
  type FollowMode,
  fitView,
  followView,
  panView,
  revealView,
  ruler,
  rulerLabel,
  timeAt,
  type View,
  xAt,
  zoomView,
} from "./view";
import { envelope, type Waveform } from "./waveform";

export type SurfaceName = "overview" | "detail";

export interface Hover {
  readonly surface: SurfaceName;
  /** Seconds under the pointer. */
  readonly time: number;
  /** Pointer position from the timeline's left edge, in CSS pixels. */
  readonly x: number;
  /** Width of the timeline, in CSS pixels. */
  readonly width: number;
}

/** What React shows around the canvases (the view itself changes every frame while playing). */
export interface TimelineState {
  readonly follow: FollowMode;
  readonly hover: Hover | null;
}

interface Surface {
  readonly name: SurfaceName;
  readonly canvas: HTMLCanvasElement;
  readonly context: CanvasRenderingContext2D;
  width: number;
  height: number;
  ratio: number;
}

interface Drag {
  readonly surface: SurfaceName;
  readonly pointer: number;
  readonly startX: number;
  readonly wasPlaying: boolean;
  scrubbing: boolean;
}

interface Palette {
  readonly wave: string;
  readonly played: string;
  readonly playhead: string;
  readonly text: string;
  readonly font: string;
}

const RULER_HEIGHT = 18;
/** Ruler labels stay at least this far apart, in pixels. */
const LABEL_SPACING = 72;
/** Frame boundaries are marked on the ruler once frames are this wide, in pixels. */
const FRAME_TICK_SPACING = 6;
const ZOOM_STEP = 1.5;
/** A press that moves further than this, in pixels, scrubs instead of just jumping. */
const DRAG_THRESHOLD = 3;
/** One wheel notch (100 px in Chromium) pans the detail view by a fifth of its span… */
const WHEEL_PAN = 0.002;
/** …and Ctrl + one notch zooms by about 20 %. */
const WHEEL_ZOOM = 0.002;
const ATTACK_COLOR = "oklch(0.8 0.15 75)";
const PREFERENCES_KEY = "segno.timeline";

interface Preferences {
  readonly follow: FollowMode;
  readonly span: number;
}

/**
 * Draws the two-level timeline on its canvases and turns pointer gestures into jumps, scrubs,
 * zooms and pans. It reads the playhead from the video element on every animation frame
 * (`frame`), so playback never goes through React.
 */
export class TimelineController {
  readonly #video: () => HTMLVideoElement | null;
  #duration = 0;
  #fps = 30;
  #view: View;
  #follow: FollowMode;
  /** A manual pan pauses following until the next jump or play. */
  #suspended = false;
  #jumped = false;
  #waveform: Waveform | null = null;
  #attacks: Attacks | null = null;
  #surfaces: Partial<Record<SurfaceName, Surface>> = {};
  #hover: Hover | null = null;
  #drag: Drag | null = null;
  #palette: Palette | null = null;
  #paths = new Map<SurfaceName, { key: string; path: Path2D }>();
  #columns = new Float32Array(0);
  #drawn = "";
  #announced = -1;
  #state: TimelineState;
  readonly #listeners = new Set<() => void>();

  constructor(video: () => HTMLVideoElement | null) {
    this.#video = video;
    const preferences = loadPreferences();
    this.#follow = preferences.follow;
    this.#view = { start: 0, span: preferences.span };
    this.#state = { follow: this.#follow, hover: null };
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  readonly getState = (): TimelineState => this.#state;

  setMedia(duration: number, fps: number): void {
    this.#duration = duration;
    this.#fps = fps > 0 ? fps : 30;
    this.#setView(clampView(this.#view, duration));
    this.#invalidate();
  }

  setWaveform(waveform: Waveform | null): void {
    this.#waveform = waveform;
    this.#surfaces.detail?.canvas.setAttribute("data-waveform", waveform ? "ready" : "none");
    this.#invalidate();
  }

  setAttacks(attacks: Attacks | null): void {
    this.#attacks = attacks;
    this.#surfaces.detail?.canvas.setAttribute("data-attacks", String(attacks?.times.length ?? 0));
    this.#invalidate();
  }

  /** The video started a seek: the next frame brings the playhead into view. */
  noteJump(): void {
    this.#jumped = true;
    this.#suspended = false;
  }

  notePlay(): void {
    this.#suspended = false;
  }

  /** Starts drawing on the canvases and listening to them; returns a function that stops. */
  attach(overview: HTMLCanvasElement, detail: HTMLCanvasElement): () => void {
    const cleanups: (() => void)[] = [];
    for (const [name, canvas] of [
      ["overview", overview],
      ["detail", detail],
    ] as const) {
      const context = canvas.getContext("2d");
      if (!context) continue;
      const surface: Surface = { name, canvas, context, width: 0, height: 0, ratio: 1 };
      this.#surfaces[name] = surface;
      const listen = <K extends keyof HTMLElementEventMap>(
        type: K,
        handler: (event: HTMLElementEventMap[K]) => void,
        options?: AddEventListenerOptions,
      ) => {
        canvas.addEventListener(type, handler, options);
        cleanups.push(() => canvas.removeEventListener(type, handler, options));
      };
      listen("pointerdown", (event) => this.#pointerDown(surface, event));
      listen("pointermove", (event) => this.#pointerMove(surface, event));
      listen("pointerup", (event) => this.#pointerUp(surface, event));
      listen("pointercancel", (event) => this.#pointerUp(surface, event));
      listen("pointerleave", () => {
        if (!this.#drag) this.#setHover(null);
      });
      // Not passive: the wheel must not scroll or zoom the page.
      listen("wheel", (event) => this.#wheel(surface, event), { passive: false });
    }
    const observer = new ResizeObserver(() => this.#resize());
    observer.observe(overview);
    observer.observe(detail);
    cleanups.push(() => observer.disconnect());
    this.#resize();
    this.setWaveform(this.#waveform);
    this.setAttacks(this.#attacks);
    this.#publishView();
    return () => {
      for (const cleanup of cleanups) cleanup();
      this.#surfaces = {};
    };
  }

  /** Follows playback and redraws when anything changed. Call on every animation frame. */
  frame(): void {
    const video = this.#video();
    const time = video?.currentTime ?? 0;
    let view = this.#view;
    if (this.#jumped) {
      this.#jumped = false;
      // Scrubbing in the detail view keeps it still under the pointer.
      if (this.#drag?.surface !== "detail") {
        view = revealView(view, time, this.#follow, this.#duration);
      }
    } else if (video && !video.paused && !this.#suspended && !this.#drag) {
      view = followView(view, time, this.#follow, this.#duration);
    }
    this.#setView(view);
    this.#announce(time);

    const hover = this.#hover;
    const key = `${time}|${view.start}|${view.span}|${hover?.surface}|${hover?.time}`;
    if (key === this.#drawn) return;
    this.#drawn = key;
    this.#palette ??= readPalette(this.#surfaces.detail?.canvas ?? document.documentElement);
    const overview = this.#surfaces.overview;
    const detail = this.#surfaces.detail;
    if (overview && overview.width > 0) this.#drawOverview(overview, this.#palette, time);
    if (detail && detail.width > 0) this.#drawDetail(detail, this.#palette, time);
  }

  readonly zoomIn = (): void => this.#zoomBy(1 / ZOOM_STEP);
  readonly zoomOut = (): void => this.#zoomBy(ZOOM_STEP);

  readonly zoomFit = (): void => {
    this.#setView(fitView(this.#duration));
  };

  readonly cycleFollow = (): void => {
    this.#follow =
      FOLLOW_MODES[(FOLLOW_MODES.indexOf(this.#follow) + 1) % FOLLOW_MODES.length] ?? "page";
    this.#suspended = false;
    this.#surfaces.detail?.canvas.setAttribute("data-follow", this.#follow);
    this.#savePreferences();
    this.#publish();
  };

  #zoomBy(factor: number): void {
    const view = this.#view;
    const time = this.#video()?.currentTime ?? 0;
    const visible = time >= view.start && time <= view.start + view.span;
    const anchor = visible ? time : view.start + view.span / 2;
    this.#setView(zoomView(view, factor, anchor, this.#duration));
    this.#savePreferences();
  }

  #pointerDown(surface: Surface, event: PointerEvent): void {
    const video = this.#video();
    if (event.button !== 0 || !video) return;
    event.preventDefault();
    surface.canvas.setPointerCapture(event.pointerId);
    this.#drag = {
      surface: surface.name,
      pointer: event.pointerId,
      startX: event.clientX,
      wasPlaying: !video.paused,
      scrubbing: false,
    };
    this.#seek(this.#timeAt(surface, this.#xOf(surface, event)));
  }

  #pointerMove(surface: Surface, event: PointerEvent): void {
    const x = this.#xOf(surface, event);
    const time = this.#timeAt(surface, x);
    const drag = this.#drag;
    if (drag && drag.pointer === event.pointerId) {
      if (!drag.scrubbing && Math.abs(event.clientX - drag.startX) >= DRAG_THRESHOLD) {
        drag.scrubbing = true;
        // Scrubbing pauses; the video plays on from the new place on release.
        if (drag.wasPlaying) this.#video()?.pause();
      }
      if (drag.scrubbing) this.#seek(time);
    }
    this.#setHover({ surface: surface.name, time, x, width: surface.width });
  }

  #pointerUp(surface: Surface, event: PointerEvent): void {
    const drag = this.#drag;
    if (!drag || drag.pointer !== event.pointerId) return;
    this.#drag = null;
    if (surface.canvas.hasPointerCapture(event.pointerId)) {
      surface.canvas.releasePointerCapture(event.pointerId);
    }
    if (drag.scrubbing && drag.wasPlaying)
      this.#video()
        ?.play()
        .catch(() => {});
  }

  #wheel(surface: Surface, event: WheelEvent): void {
    event.preventDefault();
    const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? surface.width : 1;
    if (event.ctrlKey || event.metaKey) {
      // Ctrl + wheel, or a pinch on a touchpad: zoom around the pointer.
      const anchor =
        surface.name === "detail"
          ? this.#timeAt(surface, this.#xOf(surface, event))
          : this.#view.start + this.#view.span / 2;
      this.#setView(
        zoomView(this.#view, Math.exp(event.deltaY * unit * WHEEL_ZOOM), anchor, this.#duration),
      );
      this.#savePreferences();
      return;
    }
    const delta =
      (Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY) * unit;
    this.#suspended = true;
    this.#setView(panView(this.#view, delta * WHEEL_PAN * this.#view.span, this.#duration));
  }

  #seek(time: number): void {
    const video = this.#video();
    if (!video) return;
    const end = Number.isFinite(video.duration) ? video.duration : this.#duration;
    video.currentTime = Math.min(Math.max(time, 0), end || 0);
  }

  #xOf(surface: Surface, event: MouseEvent): number {
    const rect = surface.canvas.getBoundingClientRect();
    return Math.min(Math.max(event.clientX - rect.left, 0), rect.width);
  }

  #timeAt(surface: Surface, x: number): number {
    const view = surface.name === "detail" ? this.#view : this.#whole();
    return timeAt(view, x, surface.width || 1);
  }

  #whole(): View {
    return { start: 0, span: this.#duration > 0 ? this.#duration : 1 };
  }

  #setView(view: View): void {
    if (view.start === this.#view.start && view.span === this.#view.span) return;
    this.#view = view;
    this.#publishView();
  }

  #publishView(): void {
    const canvas = this.#surfaces.detail?.canvas;
    if (!canvas) return;
    canvas.setAttribute("data-start", this.#view.start.toFixed(3));
    canvas.setAttribute("data-span", this.#view.span.toFixed(3));
    canvas.setAttribute("data-follow", this.#follow);
  }

  /** Keeps the slider's accessible value current, once a second. */
  #announce(time: number): void {
    const second = Math.floor(time);
    const canvas = this.#surfaces.overview?.canvas;
    if (!canvas || second === this.#announced) return;
    this.#announced = second;
    canvas.setAttribute("aria-valuenow", String(second));
    canvas.setAttribute("aria-valuemax", String(Math.round(this.#duration)));
    canvas.setAttribute("aria-valuetext", formatTimecode(time));
  }

  #setHover(hover: Hover | null): void {
    if (hover?.time === this.#hover?.time && hover?.surface === this.#hover?.surface) return;
    this.#hover = hover;
    this.#publish();
  }

  #publish(): void {
    this.#state = { follow: this.#follow, hover: this.#hover };
    for (const listener of this.#listeners) listener();
  }

  #invalidate(): void {
    this.#paths.clear();
    this.#drawn = "";
  }

  #resize(): void {
    const ratio = window.devicePixelRatio || 1;
    for (const surface of Object.values(this.#surfaces)) {
      const rect = surface.canvas.getBoundingClientRect();
      surface.width = rect.width;
      surface.height = rect.height;
      surface.ratio = ratio;
      surface.canvas.width = Math.round(rect.width * ratio);
      surface.canvas.height = Math.round(rect.height * ratio);
    }
    this.#palette = null;
    this.#invalidate();
  }

  #savePreferences(): void {
    try {
      const preferences: Preferences = { follow: this.#follow, span: this.#view.span };
      localStorage.setItem(PREFERENCES_KEY, JSON.stringify(preferences));
    } catch {
      // Storage unavailable: the preferences last for this session only.
    }
  }

  #drawOverview(surface: Surface, palette: Palette, time: number): void {
    const context = this.#begin(surface);
    const whole = this.#whole();
    const playX = xAt(whole, time, surface.width);
    this.#drawWave(surface, palette, whole, 3, surface.height - 3, playX);

    // The stretch shown in the detail view.
    const left = xAt(whole, this.#view.start, surface.width);
    const width = Math.max(3, xAt(whole, this.#view.start + this.#view.span, surface.width) - left);
    context.fillStyle = palette.played;
    context.globalAlpha = 0.12;
    context.fillRect(left, 0, width, surface.height);
    context.globalAlpha = 0.5;
    context.strokeStyle = palette.played;
    context.lineWidth = 1;
    context.strokeRect(left + 0.5, 0.5, width - 1, surface.height - 1);
    context.globalAlpha = 1;

    this.#drawHover(surface, palette, whole);
    this.#drawPlayhead(surface, palette, playX, false);
  }

  #drawDetail(surface: Surface, palette: Palette, time: number): void {
    const context = this.#begin(surface);
    const view = this.#view;
    const { width } = surface;

    // Ruler: minor ticks, frame boundaries when frames are wide enough, labelled ticks.
    const { step, minor, labels } = ruler(view, width, LABEL_SPACING);
    context.fillStyle = palette.text;
    context.globalAlpha = 0.6;
    for (
      let index = Math.ceil(view.start / minor - 1e-9);
      index * minor <= view.start + view.span;
      index++
    ) {
      context.fillRect(Math.round(xAt(view, index * minor, width)), RULER_HEIGHT - 4, 1, 4);
    }
    if (width / (view.span * this.#fps) >= FRAME_TICK_SPACING) {
      context.globalAlpha = 0.35;
      for (
        let frame = Math.ceil(view.start * this.#fps);
        frame / this.#fps <= view.start + view.span;
        frame++
      ) {
        context.fillRect(Math.round(xAt(view, frame / this.#fps, width)), RULER_HEIGHT - 2, 1, 2);
      }
    }
    context.globalAlpha = 1;
    context.font = palette.font;
    context.textBaseline = "top";
    for (const label of labels) {
      const x = Math.round(xAt(view, label, width));
      context.fillRect(x, RULER_HEIGHT - 9, 1, 9);
      context.fillText(rulerLabel(label, step), x + 3, 3);
    }
    context.globalAlpha = 0.3;
    context.fillRect(0, RULER_HEIGHT, width, 1);
    context.globalAlpha = 1;

    const top = RULER_HEIGHT + 4;
    const bottom = surface.height - 4;
    const playX = xAt(view, time, width);
    this.#drawWave(surface, palette, view, top, bottom, playX);
    this.#drawAttacks(surface, view, bottom);
    this.#drawHover(surface, palette, view);
    this.#drawPlayhead(surface, palette, playX, true);
  }

  #begin(surface: Surface): CanvasRenderingContext2D {
    const { context, ratio } = surface;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, surface.width, surface.height);
    return context;
  }

  /** The waveform: dimmed ahead of the playhead, bright where the song has been played. */
  #drawWave(
    surface: Surface,
    palette: Palette,
    view: View,
    top: number,
    bottom: number,
    playX: number,
  ): void {
    const { context } = surface;
    const middle = (top + bottom) / 2;
    context.fillStyle = palette.wave;
    context.globalAlpha = 0.35;
    context.fillRect(0, Math.round(middle), surface.width, 1);
    const path = this.#wavePath(surface, view, top, bottom);
    if (path) {
      context.globalAlpha = 0.45;
      context.fill(path);
      if (playX > 0) {
        context.save();
        context.beginPath();
        context.rect(0, 0, playX, surface.height);
        context.clip();
        context.fillStyle = palette.played;
        context.globalAlpha = 0.85;
        context.fill(path);
        context.restore();
      }
    }
    context.globalAlpha = 1;
  }

  /** The envelope's outline, rebuilt only when the view, the size or the data change. */
  #wavePath(surface: Surface, view: View, top: number, bottom: number): Path2D | null {
    const waveform = this.#waveform;
    if (!waveform) return null;
    const key = `${view.start}|${view.span}|${surface.width}|${surface.ratio}|${top}|${bottom}`;
    const cached = this.#paths.get(surface.name);
    if (cached?.key === key) return cached.path;

    const columns = Math.max(1, Math.round(surface.width * surface.ratio));
    if (this.#columns.length < 2 * columns) this.#columns = new Float32Array(2 * columns);
    const values = this.#columns;
    envelope(waveform, view.start, view.start + view.span, columns, values);
    const middle = (top + bottom) / 2;
    const half = (bottom - top) / 2;
    const step = surface.width / columns;
    // y of the column's minimum (0) or maximum (1).
    const y = (column: number, extreme: 0 | 1) =>
      middle - (values[2 * column + extreme] ?? 0) * half;
    const path = new Path2D();
    path.moveTo(0, y(0, 1));
    for (let column = 0; column < columns; column++)
      path.lineTo((column + 0.5) * step, y(column, 1));
    path.lineTo(surface.width, y(columns - 1, 1));
    path.lineTo(surface.width, y(columns - 1, 0));
    for (let column = columns - 1; column >= 0; column--) {
      path.lineTo((column + 0.5) * step, y(column, 0));
    }
    path.lineTo(0, y(0, 0));
    path.closePath();
    this.#paths.set(surface.name, { key, path });
    return path;
  }

  /** Attacks as ticks rising from the bottom, taller and brighter the stronger they are. */
  #drawAttacks(surface: Surface, view: View, bottom: number): void {
    const attacks = this.#attacks;
    if (!attacks) return;
    const { times, strengths } = attacks;
    const { context, width } = surface;
    const first = firstAtOrAfter(times, view.start);
    const end = firstAtOrAfter(times, view.start + view.span + 1e-9);
    context.fillStyle = ATTACK_COLOR;
    for (let index = first; index < end; index++) {
      const strength = strengths[index] ?? 1;
      const height = 5 + 10 * strength;
      context.globalAlpha = 0.45 + 0.55 * strength;
      context.fillRect(xAt(view, times[index] ?? 0, width) - 0.75, bottom - height, 1.5, height);
    }
    context.globalAlpha = 1;
  }

  #drawHover(surface: Surface, palette: Palette, view: View): void {
    const hover = this.#hover;
    if (!hover) return;
    const x = xAt(view, hover.time, surface.width);
    if (x < 0 || x > surface.width) return;
    surface.context.fillStyle = palette.text;
    surface.context.globalAlpha = 0.8;
    surface.context.fillRect(Math.round(x), 0, 1, surface.height);
    surface.context.globalAlpha = 1;
  }

  #drawPlayhead(surface: Surface, palette: Palette, x: number, handle: boolean): void {
    if (x < -2 || x > surface.width + 2) return;
    const { context } = surface;
    context.fillStyle = palette.playhead;
    context.fillRect(Math.round(x) - 1, 0, 2, surface.height);
    if (handle) {
      context.beginPath();
      context.moveTo(x - 6, 0);
      context.lineTo(x + 6, 0);
      context.lineTo(x, 8);
      context.closePath();
      context.fill();
    }
  }
}

function loadPreferences(): Preferences {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(PREFERENCES_KEY) ?? "null");
    if (
      typeof stored === "object" &&
      stored !== null &&
      "follow" in stored &&
      "span" in stored &&
      FOLLOW_MODES.includes(stored.follow as FollowMode) &&
      typeof stored.span === "number" &&
      stored.span > 0
    ) {
      return { follow: stored.follow as FollowMode, span: stored.span };
    }
  } catch {
    // Unreadable or unavailable storage: start from the defaults.
  }
  return { follow: "page", span: DEFAULT_SPAN };
}

/** Theme colours, read from the CSS variables so the timeline follows light and dark modes. */
function readPalette(element: Element): Palette {
  const style = getComputedStyle(element);
  const color = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    wave: color("--muted-foreground", "#999"),
    played: color("--foreground", "#eee"),
    playhead: color("--primary", "#fff"),
    text: color("--muted-foreground", "#999"),
    font: `10px ${style.fontFamily || "sans-serif"}`,
  };
}

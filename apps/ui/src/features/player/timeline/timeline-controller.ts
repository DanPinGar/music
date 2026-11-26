import type { Attacks } from "@segno/api-client";
import { firstAtOrAfter } from "../attacks";
import { type BeatGrid, gridLines } from "../beat-grid";
import { type Loop, MIN_LOOP_SECONDS, snapTo } from "../loop";
import { DEFAULT_MARKER_COLOR, type MarkerItem, markerTimes } from "../markers";
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

/** Something on the detail view that can be dragged. */
type Handle =
  | { readonly kind: "loop"; readonly edge: "start" | "end" }
  | { readonly kind: "marker"; readonly id: string; readonly edge: "start" | "end" };

type Drag =
  /** A press that jumps, and scrubs once it moves. */
  | {
      readonly mode: "seek";
      readonly surface: SurfaceName;
      readonly pointer: number;
      readonly startX: number;
      readonly wasPlaying: boolean;
      scrubbing: boolean;
    }
  /** Shift + drag: a new loop between the press and the pointer. */
  | {
      readonly mode: "range";
      readonly surface: SurfaceName;
      readonly pointer: number;
      readonly anchor: number;
      readonly candidates: readonly number[];
    }
  /** Moving a loop edge or a marker, snapping to `candidates`. */
  | {
      readonly mode: "handle";
      readonly surface: SurfaceName;
      readonly pointer: number;
      readonly handle: Handle;
      readonly candidates: readonly number[];
    };

/** What the timeline asks the player to do with gestures that change its data. */
export interface TimelineHandlers {
  onLoopChange(loop: Loop): void;
  onMarkerMove(id: string, start: number, end: number | null): void;
  onAddMarker(time: number): void;
}

/** A change shown while it is being dragged, before it is committed. */
interface Preview {
  readonly loop?: Loop;
  readonly marker?: { readonly id: string; readonly start: number; readonly end: number | null };
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
const LOOP_COLOR = "oklch(0.72 0.14 235)";
const PREFERENCES_KEY = "segno.timeline";
/** Handles and snapping reach this far, in pixels. */
const GRAB_PIXELS = 6;
const SNAP_PIXELS = 8;
/** Height of the band where sections are named, under the ruler. */
const SECTION_BAND = 13;
/** Beat lines are drawn once beats are this far apart, in pixels. */
const BEAT_LINE_SPACING = 5;

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
  #grid: BeatGrid | null = null;
  #markers: readonly MarkerItem[] = [];
  #loop: Loop | null = null;
  #handlers: TimelineHandlers | null = null;
  #preview: Preview | null = null;
  /** Bumped whenever drawn data changes, so the next frame redraws. */
  #version = 0;
  readonly #viewListeners = new Set<() => void>();
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

  setGrid(grid: BeatGrid | null): void {
    this.#grid = grid;
    this.#version++;
    this.#surfaces.detail?.canvas.setAttribute(
      "data-grid",
      grid ? grid.beats.map((time) => time.toFixed(3)).join(",") : "",
    );
  }

  setMarkers(markers: readonly MarkerItem[]): void {
    this.#markers = markers;
    this.#version++;
  }

  setLoop(loop: Loop | null): void {
    this.#loop = loop;
    this.#version++;
    const canvas = this.#surfaces.detail?.canvas;
    if (!canvas) return;
    canvas.setAttribute("data-loop", loop ? `${loop.start.toFixed(3)}-${loop.end.toFixed(3)}` : "");
  }

  setHandlers(handlers: TimelineHandlers | null): void {
    this.#handlers = handlers;
  }

  /** The stretch the detail view shows. */
  get view(): View {
    return this.#view;
  }

  /** Shows `view` in the detail view (a restored zoom, say); following resumes at the next jump. */
  setView(view: View): void {
    this.#setView(clampView(view, this.#duration));
    this.#suspended = true;
  }

  /** Calls `listener` whenever the detail view moves or zooms. */
  subscribeView(listener: () => void): () => void {
    this.#viewListeners.add(listener);
    return () => this.#viewListeners.delete(listener);
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
      if (name === "detail") {
        listen("dblclick", (event) => {
          const x = this.#xOf(surface, event);
          if (!this.#handleAt(surface, x)) this.#handlers?.onAddMarker(this.#timeAt(surface, x));
        });
      }
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
    this.setGrid(this.#grid);
    this.setLoop(this.#loop);
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
    const key = `${time}|${view.start}|${view.span}|${hover?.surface}|${hover?.time}|${this.#version}`;
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
    const x = this.#xOf(surface, event);
    const common = { surface: surface.name, pointer: event.pointerId };
    if (event.shiftKey) {
      const candidates = surface.name === "detail" ? this.#snapCandidates(null) : [];
      const anchor = this.#snap(surface, this.#timeAt(surface, x), candidates, event.altKey);
      this.#drag = { ...common, mode: "range", anchor, candidates };
      return;
    }
    const handle = this.#handleAt(surface, x);
    if (handle) {
      this.#drag = { ...common, mode: "handle", handle, candidates: this.#snapCandidates(handle) };
      return;
    }
    this.#drag = {
      ...common,
      mode: "seek",
      startX: event.clientX,
      wasPlaying: !video.paused,
      scrubbing: false,
    };
    this.#seek(this.#timeAt(surface, x));
  }

  #pointerMove(surface: Surface, event: PointerEvent): void {
    const x = this.#xOf(surface, event);
    const time = this.#timeAt(surface, x);
    const drag = this.#drag;
    if (drag && drag.pointer === event.pointerId) {
      if (drag.mode === "seek") {
        if (!drag.scrubbing && Math.abs(event.clientX - drag.startX) >= DRAG_THRESHOLD) {
          drag.scrubbing = true;
          // Scrubbing pauses; the video plays on from the new place on release.
          if (drag.wasPlaying) this.#video()?.pause();
        }
        if (drag.scrubbing) this.#seek(time);
      } else if (drag.mode === "range") {
        const other = this.#snap(surface, time, drag.candidates, event.altKey);
        const start = Math.min(drag.anchor, other);
        const end = Math.max(drag.anchor, other);
        this.#setPreview({ loop: { start, end, enabled: true } });
      } else {
        this.#setPreview(
          this.#moved(drag.handle, this.#snap(surface, time, drag.candidates, event.altKey)),
        );
      }
    } else if (surface.name === "detail") {
      surface.canvas.style.cursor = this.#handleAt(surface, x) ? "ew-resize" : "";
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
    const preview = this.#preview;
    this.#setPreview(null);
    if (drag.mode === "seek") {
      if (drag.scrubbing && drag.wasPlaying)
        this.#video()
          ?.play()
          .catch(() => {});
      return;
    }
    if (preview?.loop && preview.loop.end - preview.loop.start >= MIN_LOOP_SECONDS) {
      this.#handlers?.onLoopChange(preview.loop);
    }
    if (preview?.marker) {
      const { id, start, end } = preview.marker;
      this.#handlers?.onMarkerMove(id, start, end);
    }
  }

  /** The loop edge or marker under `x` on the detail view, if any. */
  #handleAt(surface: Surface, x: number): Handle | null {
    if (surface.name !== "detail") return null;
    const near = (time: number) =>
      Math.abs(xAt(this.#view, time, surface.width) - x) <= GRAB_PIXELS;
    const loop = this.#loop;
    if (loop && near(loop.start)) return { kind: "loop", edge: "start" };
    if (loop && near(loop.end)) return { kind: "loop", edge: "end" };
    for (const marker of this.#markers) {
      if (marker.kind === "loop") continue;
      if (near(marker.start)) return { kind: "marker", id: marker.id, edge: "start" };
      if (marker.end !== null && near(marker.end))
        return { kind: "marker", id: marker.id, edge: "end" };
    }
    return null;
  }

  /** What dragging `handle` to `time` changes. */
  #moved(handle: Handle, time: number): Preview | null {
    if (handle.kind === "loop") {
      const loop = this.#loop;
      if (!loop) return null;
      return handle.edge === "start"
        ? { loop: { ...loop, start: Math.min(time, loop.end - MIN_LOOP_SECONDS), enabled: true } }
        : { loop: { ...loop, end: Math.max(time, loop.start + MIN_LOOP_SECONDS), enabled: true } };
    }
    const marker = this.#markers.find((item) => item.id === handle.id);
    if (!marker) return null;
    if (marker.end === null) return { marker: { id: marker.id, start: time, end: null } };
    return handle.edge === "start"
      ? {
          marker: {
            id: marker.id,
            start: Math.min(time, marker.end - MIN_LOOP_SECONDS),
            end: marker.end,
          },
        }
      : {
          marker: {
            id: marker.id,
            start: marker.start,
            end: Math.max(time, marker.start + MIN_LOOP_SECONDS),
          },
        };
  }

  /** Beats, attacks, markers and loop edges in view (except what is being dragged), in order. */
  #snapCandidates(dragged: Handle | null): number[] {
    const view = this.#view;
    const end = view.start + view.span;
    const times: number[] = [];
    if (this.#grid) {
      for (const line of gridLines(this.#grid, view.start, end)) times.push(line.time);
    }
    const attacks = this.#attacks?.times ?? [];
    for (let index = firstAtOrAfter(attacks, view.start); index < attacks.length; index++) {
      const time = attacks[index] ?? 0;
      if (time > end) break;
      times.push(time);
    }
    const others = this.#markers.filter(
      (marker) => dragged?.kind !== "marker" || marker.id !== dragged.id,
    );
    times.push(...markerTimes(others));
    if (this.#loop && dragged?.kind !== "loop") times.push(this.#loop.start, this.#loop.end);
    return times.sort((a, b) => a - b);
  }

  /** `time` snapped to the nearest candidate within reach, unless Alt is held. */
  #snap(surface: Surface, time: number, candidates: readonly number[], free: boolean): number {
    if (free || surface.name !== "detail") return time;
    const radius = (SNAP_PIXELS / (surface.width || 1)) * this.#view.span;
    return snapTo(time, candidates, radius);
  }

  #setPreview(preview: Preview | null): void {
    this.#preview = preview;
    this.#version++;
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
    for (const listener of this.#viewListeners) listener();
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
    this.#drawSections(surface, whole, 0, surface.height, false);
    this.#drawWave(surface, palette, whole, 3, surface.height - 3, playX);
    this.#drawLoop(surface, whole, false);
    this.#drawPoints(surface, whole, false);

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
    this.#drawSections(surface, view, RULER_HEIGHT + 1, surface.height, true);
    this.#drawGrid(surface, palette, view, false);
    this.#drawWave(surface, palette, view, top, bottom, playX);
    this.#drawGrid(surface, palette, view, true);
    this.#drawAttacks(surface, view, bottom);
    this.#drawLoop(surface, view, true);
    this.#drawPoints(surface, view, true);
    this.#drawHover(surface, palette, view);
    this.#drawPlayhead(surface, palette, playX, true);
  }

  #currentLoop(): Loop | null {
    return this.#preview?.loop ?? this.#loop;
  }

  /** Markers with the one being dragged at its new place. */
  #currentMarkers(): readonly MarkerItem[] {
    const moved = this.#preview?.marker;
    if (!moved) return this.#markers;
    return this.#markers.map((marker) =>
      marker.id === moved.id ? { ...marker, start: moved.start, end: moved.end } : marker,
    );
  }

  /**
   * Beat lines and brighter bar lines under the waveform, or (`labels`) the bar numbers over
   * it, so the waveform never hides them.
   */
  #drawGrid(surface: Surface, palette: Palette, view: View, labels: boolean): void {
    const grid = this.#grid;
    if (!grid) return;
    const { context, width } = surface;
    const lines = gridLines(grid, view.start, view.start + view.span);
    const spacing = lines.length > 1 ? width / (lines.length - 1) : width;
    context.fillStyle = palette.played;
    context.font = palette.font;
    context.textBaseline = "top";
    let lastLabel = Number.NEGATIVE_INFINITY;
    for (const line of lines) {
      const x = Math.round(xAt(view, line.time, width));
      if (labels) {
        if (line.bar !== null && x - lastLabel >= 24) {
          context.globalAlpha = 0.85;
          context.fillText(String(line.bar), x + 3, surface.height - 13);
          lastLabel = x;
        }
        continue;
      }
      if (line.bar === null && spacing < BEAT_LINE_SPACING) continue;
      context.globalAlpha = line.bar === null ? 0.1 : 0.28;
      context.fillRect(x, RULER_HEIGHT + 1, 1, surface.height - RULER_HEIGHT - 1);
    }
    context.globalAlpha = 1;
  }

  /** Sections as coloured stretches, named in a band under the ruler in the detail view. */
  #drawSections(surface: Surface, view: View, top: number, bottom: number, named: boolean): void {
    const { context, width } = surface;
    for (const marker of this.#currentMarkers()) {
      if (marker.kind !== "section" || marker.end === null) continue;
      const left = xAt(view, marker.start, width);
      const right = xAt(view, marker.end, width);
      if (right < 0 || left > width) continue;
      const color = marker.color ?? DEFAULT_MARKER_COLOR;
      context.fillStyle = color;
      context.globalAlpha = named ? 0.08 : 0.2;
      context.fillRect(left, top, right - left, bottom - top);
      if (!named) continue;
      context.globalAlpha = 0.55;
      context.fillRect(left, top, right - left, SECTION_BAND);
      context.globalAlpha = 1;
      context.fillStyle = "#fff";
      context.font = this.#palette?.font ?? "10px sans-serif";
      context.textBaseline = "top";
      const label = marker.label || `§${marker.number ?? ""}`;
      context.save();
      context.beginPath();
      context.rect(left, top, right - left, SECTION_BAND);
      context.clip();
      context.fillText(label, Math.max(left, 0) + 4, top + 1.5);
      context.restore();
    }
    context.globalAlpha = 1;
  }

  /** The loop: shaded when on, outlined when off; in the detail view with grabbable edges. */
  #drawLoop(surface: Surface, view: View, handles: boolean): void {
    const loop = this.#currentLoop();
    if (!loop) return;
    const { context, width, height } = surface;
    const left = xAt(view, loop.start, width);
    const right = xAt(view, loop.end, width);
    if (right < 0 || left > width) return;
    context.fillStyle = LOOP_COLOR;
    context.strokeStyle = LOOP_COLOR;
    context.globalAlpha = loop.enabled ? 0.16 : 0.06;
    context.fillRect(left, 0, right - left, height);
    context.globalAlpha = loop.enabled ? 0.9 : 0.45;
    context.fillRect(Math.round(left) - 1, 0, 2, height);
    context.fillRect(Math.round(right) - 1, 0, 2, height);
    if (handles) {
      // Brackets in the ruler, to grab the edges.
      context.fillRect(left - 1, 0, 8, 3);
      context.fillRect(right - 7, 0, 8, 3);
    }
    context.globalAlpha = 1;
  }

  /** Point markers: a line and, in the detail view, a numbered flag in the ruler. */
  #drawPoints(surface: Surface, view: View, flags: boolean): void {
    const { context, width, height } = surface;
    for (const marker of this.#currentMarkers()) {
      if (marker.kind !== "point") continue;
      const x = Math.round(xAt(view, marker.start, width));
      if (x < -60 || x > width) continue;
      const color = marker.color ?? DEFAULT_MARKER_COLOR;
      context.fillStyle = color;
      context.globalAlpha = 0.9;
      context.fillRect(x, flags ? RULER_HEIGHT : 0, 1.5, height);
      if (!flags) continue;
      const label = marker.label ? `${marker.number} ${marker.label}` : String(marker.number ?? "");
      context.font = this.#palette?.font ?? "10px sans-serif";
      const textWidth = context.measureText(label).width;
      context.globalAlpha = 1;
      context.fillRect(x, 1, textWidth + 8, RULER_HEIGHT - 3);
      context.fillStyle = "#111";
      context.textBaseline = "top";
      context.fillText(label, x + 4, 3);
    }
    context.globalAlpha = 1;
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

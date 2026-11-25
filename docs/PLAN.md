# Project plan: piano video study app

> **Status:** Phase 0 built, Phase 1A next · **Updated:** 24 November 2026 · **App name:** Segno (`io.github.danpingar.segno`)

## 0. Summary

A desktop app for studying Synthesia-style piano videos (falling notes over a keyboard): download or import them, play them with controls designed for practice, move around the song quickly and precisely, mark sections, take notes, transcribe them into notes and sheet music, and analyse their harmony at an academic level.

Key decisions:

- **Navigation comes first.** Moving around the song must be instant and comfortable: it is the core of the experience ([§4.2](#42-song-navigation-top-priority)).
- **Local-first, cloud-ready.** Everything runs on the user's computer, with no servers. The architecture lets the API and the processing move to the cloud without a rewrite.
- **Tauri 2 + React/TypeScript** for the app, a **Rust core** (API, database, downloads, job queue, media preparation, MIDI) and a stateless **Python engine** for heavy processing.
- **Hybrid transcription:** computer vision on the video (lit keys and falling bars) plus audio AI to refine rhythm, dynamics and pedalling.
- **Academic-level harmonic analysis** as the target for the first version.
- **Designed in from the start:** note editor, MIDI playback and practice with a MIDI keyboard.
- **Work order:** the base app (features 1–4) and, in parallel, a vision transcription prototype.
- **Language:** code, comments, commits and documentation in English; the user interface in Spanish and English.

## 1. Scope

| #   | Feature                     | Includes                                                                                                                                                                   | Phase             |
| --- | --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| 1   | Library and downloads       | Download from YouTube, import local files, queue with progress, search, tags                                                                                               | 1A                |
| 2   | Study player and navigation | Instant jumps, musical stepping (frame, attack, beat, bar, section), zoomable timeline, go-to, navigation history, hands-free control, speed 0.25×–2× with pitch preserved | 1A, extended in 2 |
| 3   | Markers and loops           | Points, sections and A-B loops on the timeline, speed trainer, pre-roll and count-in, YouTube chapters as markers                                                          | 1A                |
| 4   | Comments                    | Anchored to a moment or a range (later also to bars or notes), in Markdown                                                                                                 | 1A                |
| 5   | Transcription               | Video → notes (piano roll) → sheet music (MusicXML, PDF) and MIDI                                                                                                          | 1B, 2, 3          |
| 6   | Harmonic analysis           | Keys and modulations, chords, Roman numerals, functions, cadences, non-chord tones, voice leading                                                                          | 4                 |
| E1  | Note editor                 | Correct the transcription; the score and the analysis are regenerated                                                                                                      | 2                 |
| E2  | MIDI playback               | Synthesised piano in sync with the video, mute hands, metronome                                                                                                            | 2                 |
| E3  | MIDI keyboard practice      | Wait mode, scoring, statistics                                                                                                                                             | 5                 |

**Out of scope for now** (the architecture allows it, but it is not built): user accounts, multi-device sync, sharing, web and mobile versions.

## 2. Architecture principles

1. **Navigation first.** Every musical view (timeline, piano roll, score, harmony lane, markers, comments) is also a way to move through the song, and every move responds instantly. Navigation performance budgets are requirements, not wishes.
2. **Local-first.** The app is complete without internet access, except for downloading videos and models.
3. **The cloud is another deployment, not a rewrite.** The UI always talks HTTP + WebSocket to an API. On the desktop that API runs inside the app; in the cloud, on servers.
4. **Separate control plane and compute plane.** Rust owns state and orchestration. Python only turns input files into output artifacts, with no state and no database, so its workers can be scaled, replaced or moved to GPUs.
5. **Notes are the musical source of truth.** The score, the analysis and MIDI playback are all derived from the note set. A correction creates a new revision, and only what depends on it is recomputed.
6. **Immutable, versioned artifacts.** Each pipeline stage stores its output with the algorithm version and a hash of its parameters, so results are reproducible and cacheable.
7. **Typed contracts across languages.** OpenAPI for the API (the TypeScript client is generated from Rust) and JSON Schema for jobs and artifacts (the source for Rust, Python and TypeScript types).
8. **Permissive licences only.** MIT, BSD, Apache or similar, and LGPL only as a separate module. Nothing GPL or "non-commercial" inside the app, to keep the commercial route open. Checked in CI.
9. **Quality is measured, not assumed.** Transcription and analysis are evaluated against reference data on every change, and navigation latency is measured against its budget.

## 3. Architecture

### 3.1 Overview

```
Desktop app (Tauri 2)
│
├── UI · React + TypeScript (WebView2)
│     Library · Player · Markers · Comments
│     Piano roll · Score · Harmony · Mixer · Practice
│        │
│        │  HTTP + WebSocket (127.0.0.1, random port + token)
│        ▼
└── CORE · Rust
      Axum API · SQLite · On-disk library · Job queue
      Media preparation · MIDI input/output
         │                                 │
         │ helper processes                │ JSON Lines over stdio
         ▼                                 ▼
      yt-dlp · ffmpeg                   ENGINE · Python (stateless worker)
                                        Vision · Audio AI · Fusion
                                        Rhythm · Score · Harmony
```

### 3.2 User interface (`apps/ui`)

- **React + TypeScript + Vite.** Routing with TanStack Router, server data with TanStack Query and local player state with Zustand.
- **Design:** Tailwind + shadcn/ui (accessible components built on Base UI), dark theme by default, i18n with i18next (Spanish and English).
- **Video:** native `<video>` element with custom controls. `requestVideoFrameCallback` keeps the timeline, piano roll, score and synthesiser in sync with frame accuracy.
- **Timeline:** custom WebGL component with an overview and a zoomable detail view ([§4.2](#timeline)).
- **Piano roll:** WebGL with PixiJS (thousands of notes at 60 fps, with editing).
- **Score:** Verovio (WebAssembly) in a Web Worker renders MusicXML to SVG; its _timemap_ highlights the sounding notes and lets a click on a note jump the video there.
- **Audio:** Web Audio, with a sampled piano (Tone.js or smplr) for MIDI playback.
- **Commands:** a single command registry drives keyboard shortcuts, the command palette, menus, page-turner pedals and MIDI-learn bindings. Every action is reachable in every way, and user keymaps are just data.
- **Platform-independent:** everything native (file dialogs, window, updates) goes through a `PlatformAdapter`, so the same UI runs on the web unchanged.

### 3.3 Core (Rust: `crates/` and `apps/desktop`)

A Cargo workspace split by responsibility, with a hexagonal architecture: the domain knows nothing about SQLite, Axum or Tauri.

| Component                | Responsibility                                                                                                                                        |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `domain`                 | Entities (Video, Marker, Comment, Job, Artifact, NoteSet…), rules and ports (traits)                                                                  |
| `storage`                | Repositories on SQLite (sqlx) and files (`object_store`: local disk today, S3 tomorrow). PostgreSQL will be another implementation of the same traits |
| `jobs`                   | Persistent queue in SQLite with retries, cancellation and progress; manages the Python processes                                                      |
| `media`                  | yt-dlp and ffmpeg: downloads, seek proxy, thumbnails, audio extraction, waveform peaks and attack detection                                           |
| `midi`                   | Real-time MIDI input/output (midir) and .mid files (midly)                                                                                            |
| `api`                    | Axum router: REST + WebSocket, OpenAPI (utoipa), RFC 9457 errors (problem+json)                                                                       |
| `apps/desktop`           | Tauri 2: window, startup of the embedded API, dialogs, updates                                                                                        |
| `apps/server` _(future)_ | The same `api` with PostgreSQL, S3 and an external queue                                                                                              |

Foundations: Tokio (async), `tracing` (logs and traces) and `thiserror` (errors).

### 3.4 Engine (Python: `engine/`)

- **Python 3.13 in a project-local environment** (`engine/.venv`) managed by uv; package `segno_engine`.
- Two entry points over the same code:
  - `segno-engine worker --stdio`: used by the core; receives jobs and reports progress as JSON Lines.
  - `segno-engine transcribe video.mp4 --out dir/` (Phase 1B): CLI for development, tests and notebooks. The Phase 1B prototype is born inside this package, so it is not thrown away.
- Each stage is a pure function `inputs + parameters → artifact` with its own version.
- **AI models run on ONNX Runtime**: CPU first, optional GPU acceleration. Models are downloaded on demand and verified by SHA-256.
- **Installed on demand** ("Enable AI features"): the app installer stays small, and features 1–4 never depend on Python.

### 3.5 Contracts

**API (UI ↔ core).** REST + WebSocket, described by an OpenAPI document generated from Rust; the TypeScript client is generated with openapi-typescript + openapi-fetch.

```
GET  POST        /videos                 library (POST: import a file or a URL)
GET  PATCH  DEL  /videos/{id}
GET              /videos/{id}/media      seek-optimised proxy (the original until it is made), with Range support
GET              /videos/{id}/thumbnails thumbnail sprite, one tile a second
GET              /videos/{id}/peaks      waveform peaks
GET              /videos/{id}/nav        attacks, beat grid
GET  PUT         /videos/{id}/state      remembered position, speed, loop, zoom
CRUD             /videos/{id}/markers
CRUD             /videos/{id}/comments
POST             /videos/{id}/pipelines  start transcription, score or analysis
GET  POST        /videos/{id}/notes      note-set revisions (editor)
GET              /videos/{id}/score      MusicXML for a revision
GET              /videos/{id}/harmony    harmonic analysis
GET  DEL         /jobs/{id}              status / cancel
WS               /events                 progress, library changes, MIDI input
```

**Jobs (core ↔ engine).** JSON Lines messages defined in JSON Schema (`schemas/`). In the cloud, the same messages travel over a queue.

```jsonc
// core → engine
{"type":"job.start","job_id":"018f…","kind":"transcribe.vision",
 "inputs":{"video":"…/original.mp4"},"params":{"fps":"native"},"out_dir":"…/artifacts/018f…"}
// engine → core
{"type":"job.progress","job_id":"018f…","stage":"vision","progress":0.42}
{"type":"job.result","job_id":"018f…","artifacts":[{"kind":"notes","schema":"notes/v1","path":"notes.json"}]}
```

**Artifacts.** Also JSON Schema, versioned: `keyboard/v1`, `notes/v1`, `tempo/v1`, `harmony/v1`. One note in `notes/v1`:

```jsonc
{
  "id": "n_0412",
  "pitch": 63,
  "onset": 12.345,
  "offset": 12.89,
  "velocity": 72,
  "hand": "R",
  "track": 1,
  "confidence": 0.98,
  "sources": ["vision", "audio"]
}
```

### 3.6 Data

**On-disk library**, in a folder chosen by the user:

```
Library/
├─ library.db                         SQLite (WAL mode)
├─ media/<video_id>/                  original.mp4 · proxy.mp4 · thumbnails.jpg · peaks.json · onsets.json
└─ artifacts/<video_id>/<artifact>/   notes.json · score.musicxml · harmony.json · …
```

**Main tables:**

| Table                               | Key fields                                                                                     |
| ----------------------------------- | ---------------------------------------------------------------------------------------------- |
| `videos`                            | id, source (youtube/file), source_url, title, channel, duration_ms, fps, width, height, status |
| `video_state`                       | video_id, position_ms, speed, loop, step_unit, zoom, layout (to resume where you left off)     |
| `tags`, `video_tags`, `collections` | library organisation                                                                           |
| `markers`                           | id, video_id, kind (point/section/loop), start_ms, end_ms, label, color                        |
| `comments`                          | id, video_id, start_ms, end_ms, anchor (moment/bar/note), body_md                              |
| `jobs`                              | id, video_id, kind, status, progress, params, error, attempts, timestamps                      |
| `artifacts`                         | id, video_id, kind, schema, producer@version, params_hash, parents, path                       |
| `note_revisions`                    | id, video_id, number, parent, author (auto/user), artifact_id                                  |
| `practice_sessions`                 | id, video_id, range, speed, hits, timing deviation                                             |
| `settings`                          | preferences and keymaps (keys, pedals and MIDI messages → commands)                            |

Conventions ready for future sync: **UUIDv7** IDs, `created_at` and `updated_at` on every row, soft deletes (`deleted_at`) and immutable artifacts.

### 3.7 Local security

- The API listens only on 127.0.0.1, on a random port, with a per-session token known only to the UI. CORS is limited to the Tauri origin and the Host header is checked (against DNS rebinding).
- Minimal Tauri capabilities and a strict CSP.
- The Python engine needs no network: the core downloads the models.
- yt-dlp and ffmpeg are kept up to date, since they process external content.

### 3.8 Path to the cloud (when needed)

```
Browser (same React UI)
   │  HTTPS + WebSocket
   ▼
Axum API (same api crate) ──► PostgreSQL
   │                      ──► S3 / R2 (object_store)
   ▼
Queue (Redis Streams, NATS or SQS)
   │
   ▼
Python workers with GPU × N (same engine) ──► S3 / R2
```

Configuration and adapters change (PostgreSQL repositories, S3 storage, external queue, OIDC authentication); the domain and the engine do not. In the cloud, **the server will not download from YouTube**: users would upload their own videos, or the desktop app would upload only results (notes and analysis).

## 4. Design of the key features

### 4.1 Library and downloads

- **yt-dlp** + **ffmpeg** + the JavaScript runtime that yt-dlp now needs for YouTube (Deno or another supported one). The app downloads and updates them itself, because YouTube changes often.
- Preferred format: H.264/AAC in MP4 up to 1080p, **favouring 60 fps** when available: it doubles the time resolution of vision transcription.
- Metadata and thumbnail are stored, and **YouTube chapters become initial markers**.
- Drag-and-drop import of local files (MP4, MKV, WebM, MOV) and also **MIDI or MusicXML**, to attach exact notes to a video without transcribing.
- After a download or import, the video is **prepared for navigation** in the background ([§4.2](#media-prepared-for-instant-seeking)); it can be played straight away while that runs.
- Background queue with progress, cancellation and retries. The app reminds users that they are responsible for how they use downloads.

### 4.2 Song navigation (top priority)

Getting to any point of the song, and back, must take one key or one gesture, respond instantly, and land exactly where the musician means: on the beat, on the attack, at the start of the phrase, not "somewhere around there".

Main screen sketch:

```
+--------------------------------------------------+------------------+
|                                                  | Markers          |
|                      VIDEO                       |  * Intro   0:00  |
|                                                  |  * Bridge  1:05  |
|                                                  |------------------|
+--------------------------------------------------+ Comments         |
| [Piano roll]  Score                              |  1:12 Pedal here |
|   ==   ===    =   ====    ==   ===   =    ==     |  2:40 Fingering  |
+--------------------------------------------------+------------------+
| Chords:   Ebmaj7    Cm7      Ab       Bb7                           |
| Numerals: Imaj7     vi7      IV       V7                            |
| Overview: [Intro][  Verse  ][ Chorus ][  Verse  ][ Chorus ][Outro]  |
| Detail:   | . . . | . ^ . [ o . . ] . . . | . . . | . . . |         |
| |>  ||   Loop x3   Speed 75%   Step: 1 bar   Bar 23.2   1:12 / 4:30 |
+---------------------------------------------------------------------+
```

`o` playhead · `[ ]` loop · `^` marker · `|` bar line · `.` beat

#### Performance budget

Measured with a built-in diagnostics overlay and treated as release criteria:

| Interaction                                        | Target (p95)            |
| -------------------------------------------------- | ----------------------- |
| Jump to any point until the new frame is on screen | < 100 ms                |
| Step one frame forward or back                     | < 50 ms                 |
| Keyboard shortcut until visible response           | < 16 ms (next frame)    |
| Timeline scrolling, zooming and scrubbing          | 60 fps                  |
| Audible gap when a loop wraps around               | < 30 ms (aim: seamless) |

Measured on 25 November 2026 in the app on the development laptop, with [the Monk video](https://www.youtube.com/watch?v=7A12z9S4F8U) (1080p60, 2:21): the original YouTube download, with a keyframe every 5.07 s, against the seek proxy the app makes, with a keyframe every 15 frames (0.25 s at 60 fps). The `bench` command of the run-segno skill takes the p95 of 10 jumps, 30 frame steps each way and 3 play starts. Each cell gives the range over three runs:

| Interaction             | Original   | Seek proxy | Budget   |
| ----------------------- | ---------- | ---------- | -------- |
| Jump to a random point  | 253–260 ms | 29–35 ms   | < 100 ms |
| One frame back          | 261–262 ms | 15–27 ms   | < 50 ms  |
| One frame forward       | 262–263 ms | 16–26 ms   | < 50 ms  |
| Play after a jump (max) | 279–344 ms | 64–74 ms   | —        |

Every seek decodes again from the keyframe before its target, even one frame forward. The cost therefore grows with the distance to that keyframe, at about 1.6 ms a frame. A proxy with a keyframe every 0.5 s (30 frames at 60 fps) measured 25–52 ms per frame step, too close to the budget. Hence the proxy below allows at most 15 frames between keyframes.

#### Navigation units

The same gestures work at every scale:

| Unit                  | From     | Source                                                                    |
| --------------------- | -------- | ------------------------------------------------------------------------- |
| Frame                 | Phase 1A | Video                                                                     |
| Attack (note onset)   | Phase 1A | Onset detection on the audio, in the core; transcribed notes from Phase 2 |
| Beat and bar          | Phase 1A | Tap-tempo grid; automatic beat and bar tracking from Phase 2              |
| Step of N seconds     | Phase 1A | Configurable (2 s by default)                                             |
| Marker, section, loop | Phase 1A | The user and YouTube chapters; suggested sections from Phase 2            |
| Chord change          | Phase 4  | Harmonic analysis                                                         |

#### Timeline

- **Two levels:** an _overview_ of the whole song (sections as coloured bands, markers, loop, comments) and a zoomable _detail_ view (waveform, beat and bar grid, attacks and, later, notes and chords). The detail view follows the playhead by page, continuously, or not at all.
- **Hover** shows a frame preview, the time, the bar, the section and any comment at that point.
- **Click** jumps. **Drag** scrubs with sound (short audio grains, as in video editors). **Shift + drag** creates a loop. Dragging a loop edge adjusts it, **snapping** to beats, attacks and markers (hold `Alt` to disable snapping). **Double-click** adds a marker. **Right-click** opens a context menu.
- **Ctrl + wheel** (or pinch) zooms; the **wheel** (or a two-finger swipe) pans.

#### Keyboard

All shortcuts are configurable and none needs AltGr on a Spanish keyboard. The step grows with the modifier:

| Keys           | Action                                                                                           |
| -------------- | ------------------------------------------------------------------------------------------------ |
| `Space` / `K`  | Play / pause (optionally returning to where playback started, as in a DAW)                       |
| `Backspace`    | Replay the last few seconds (3 s by default)                                                     |
| `,` `.`        | Previous / next frame                                                                            |
| `Shift + ←/→`  | Previous / next attack                                                                           |
| `←` `→`        | Back / forward one step: 2 s, 1 beat or 1 bar (`S` cycles the step unit, always shown on screen) |
| `Ctrl + ←/→`   | Previous / next marker or section                                                                |
| `J` `L`        | −/+ 10 s                                                                                         |
| `Alt + ←/→`    | Navigation history back / forward, as in a browser                                               |
| `Home` / `End` | Start / end of the song, or of the loop while looping                                            |
| `1` … `9`      | Jump to marker 1–9                                                                               |
| `G`            | Go to a time (`1:23.5`), a bar (`b32`), a marker, a section or a comment                         |
| `Ctrl + K`     | Command palette                                                                                  |
| `A` / `B`      | Set loop start / end at the playhead (also while playing)                                        |
| `R`            | Loop on / off (loops the current section if no loop is set)                                      |
| `-` `+`        | Speed −/+ 5 %                                                                                    |
| `M` / `C`      | Add a marker / a comment here                                                                    |
| `T`            | Tap tempo (builds the beat grid)                                                                 |
| `?`            | Show all shortcuts                                                                               |

`Esc` always returns focus to the player, for example after typing a comment.

#### Every view is a navigation surface

Clicking a note in the piano roll, a note or bar in the score, a chord in the harmony lane, a comment or a marker jumps there; selecting a range in any of them creates a loop.

#### Practice-oriented navigation

- **Pre-roll and count-in:** when jumping to a marker or a loop start, playback can start a few beats earlier, optionally with a metronome count-in, to enter the passage in tempo.
- **Loops** with a repetition counter, a **speed trainer** (for example, start at 60 % and add 5 % per repetition up to 100 %) and edges that can be nudged by frame, attack or beat.
- **Return to start on pause** and **replay the last seconds** turn "listen → try → listen again" into a single key.

#### Hands-free control

A pianist's hands are on the keys:

- **Bluetooth page-turner pedals** work out of the box (they send keystrokes). By default, left pedal = replay or back to loop start, right pedal = play/pause.
- **MIDI learn** (Phase 2): any key, pedal or controller of a MIDI keyboard (for example the lowest key, or the soft pedal) can be bound to any navigation command.

#### Media prepared for instant seeking

Done in the background after a download or import:

- YouTube files have a keyframe only every few seconds, so jumping backwards or stepping frames means decoding seconds of video. The core creates a **seek-optimised proxy**: H.264 with no B-frames and a keyframe at most every 15 frames and every 0.5 s, at most 1080p.
  - It is encoded on the GPU: Quick Sync, or Media Foundation with any vendor's GPU, on Windows; VideoToolbox on macOS. Without a GPU encoder, OpenH264 encodes in software. All of these keep ffmpeg within the LGPL.
  - For 1080p60 it takes about 0.32 MB per second of video, twice the YouTube download, with SSIM 0.997 against it. Quick Sync makes it at about 4× real time.
  - The player switches to it as soon as it is ready, keeping the position, speed and play state. Playback uses the proxy; analysis uses the original.
- **Thumbnail sprite** for hover previews: one JPEG with a 160-pixel-wide tile per second, taken from the proxy's keyframes in about half a second. When zoomed in, a hidden second video element shows the exact frame under the cursor.
- **Waveform peaks**: the minimum and maximum of every 10 ms of mono audio, with coarser levels derived in the UI.
- Planned: **decoded audio** for audible scrubbing, and **attack detection** in Rust (spectral flux with rustfft) for stepping and snapping from day one.
- Preparation runs one video at a time, reports its progress in the library and the player, and resumes after a restart. A failure is shown on the video, which still plays from the original.

#### Playback engine

- **v1:** the native `<video>` element is the clock, with its built-in pitch-preserving speed change. Simple and robust.
- **v2**, if the measurements call for it (loop gaps or audio quality at slow speeds): audio plays through Web Audio with a high-quality time-stretcher (Signalsmith Stretch) as the clock, and the video follows it. This gives seamless loops and much better sound at 50 % speed.

#### Orientation and memory

- Always visible: time (with milliseconds and frame number when paused), bar and beat when the grid exists, current section, loop state and speed.
- Each video reopens where you left it: position, speed, loop, step unit, zoom and panel layout. The library shows how far you got in each video.
- **Suggested sections** (Phase 2): repeated passages detected in the audio (and later in the notes) are proposed as sections ("A", "B", "A′") to accept or rename.

### 4.3 Markers, loops and comments

- Three kinds on the timeline: **point** (start here), **section** (named, coloured range: "Intro", "Chorus") and **loop**.
- Markers can be added while playing (tap `M` along with the music) and renamed or moved later, with snapping.
- Markdown comments anchored to a moment or a range and, once there is a score, to a bar or a note. Filterable side panel; clicking a comment jumps there.

### 4.4 Hybrid transcription

```
video ──► keyboard ──► notes from vision ──┐
  │                                        ├──► fusion ──► notes (editable)
  └─────► audio ─────► notes from audio ───┘                │
                                                            ▼
                     harmonic analysis ◄── sheet music ◄── rhythm
```

1. **Keyboard detection.** Locates the keyboard strip (periodic gaps between white keys, groups of 2 and 3 black keys), its geometry and its range. The absolute octave is fixed by comparing with the notes detected in the audio; if that fails, the user calibrates it with two clicks.
2. **Notes from vision.** Each key's colour in every frame is compared with its colour at rest (Lab colour space, with hysteresis). The **falling bars** just above the keyboard are also tracked: since they move at constant speed, the moment they reach the keyboard is computed **with better-than-frame precision**, and their length gives the duration. Colours are clustered (k-means) to separate the **left and right hands**, which the user can reassign.
3. **Notes from audio.** A piano transcription model on ONNX provides velocity (dynamics), pedalling and notes that are not visible on screen.
4. **Fusion.** The offset between picture and sound is estimated, notes are paired, and the best of each source is combined: pitch and hand from vision; precise onset, velocity and pedal from audio. Each note carries a **confidence**, and the editor flags doubtful ones.
5. **Fallback.** If the video shows no notes, audio alone is used (without reliable hand separation).

Performance on a laptop without a dedicated GPU: vision is cheap, because only the keyboard strip is processed (cropped by ffmpeg), and the audio AI runs on the CPU (or on the integrated GPU through ONNX Runtime) in the background, with progress and cancellation.

### 4.5 From notes to sheet music

1. **Rhythm:** beats and bars (Beat This! on the audio, cross-checked with note onsets). Some videos also draw bar lines, which can be detected too.
2. **Quantisation:** onsets and durations snapped to a metrical grid that allows triplets, taking pedal and legato into account.
3. **Staves and voices:** right hand in treble clef, left hand in bass clef, with voice separation within each hand.
4. **Spelling and key signature:** F♯ or G♭ depending on the key (partitura and music21), key signature, clef changes and 8va.
5. **Outputs:** MusicXML (for MuseScore, Sibelius or Dorico), quantised MIDI and performance MIDI, and PDF.
6. **Display:** Verovio, in sync with the video; click a note to jump there.

### 4.6 Harmonic analysis (academic level)

Computed on the quantised score (symbolic data), which is far more reliable than analysing the audio directly.

| Layer                  | Content                                                                                                                                                                                       |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Key                    | Global key, **modulations** and tonicisations (windowed key profiles with HMM smoothing)                                                                                                      |
| Chords                 | Harmonic segmentation and full chord symbols: 7ths, 9ths, 11ths, 13ths, sus, add, alterations, inversions and slash chords                                                                    |
| Numerals and functions | Roman numerals, T/S/D function, **secondary dominants** (V/V, vii°/ii), **modal mixture** (iv, ♭VI, ♭VII), Neapolitan, augmented sixths, cadential 6/4                                        |
| Non-chord tones        | Passing, neighbour, suspension (with preparation and resolution), appoggiatura, escape tone, anticipation, pedal                                                                              |
| Cadences               | Perfect and imperfect authentic, half, plagal, deceptive and Phrygian                                                                                                                         |
| Voice leading          | Parallel fifths and octaves, resolution of the leading tone and of sevenths (indicative in pianistic textures)                                                                                |
| Improvisation          | Suggested scales for each chord (chord-scale theory)                                                                                                                                          |
| Jazz harmony           | ii–V–I chains and related ii–Vs, tritone substitutions, backdoor progressions, extended and altered dominants, diminished passing chords, chromatic approach chords, with chord-scale choices |

- **Jazz is first-class:** the first test video is solo jazz piano (Thelonious Monk), so jazz vocabulary is a layer of its own, not an afterthought; classical and pop pieces use the same layers.
- **Honest about ambiguity:** a passage may have several readings, so the main one is shown with its confidence plus the alternatives (for example "V7/IV or blues I7").
- Each label comes with a short explanation in Spanish or English.
- Tools: music21 and partitura as the base, a rule engine of our own and, as candidates to evaluate, Roman numeral analysis models (for example AugmentedNet).
- Shown in a harmony lane under the video and as annotations on the score. Chord changes become a navigation unit.

### 4.7 Note editor

- On the piano roll: select, move, lengthen, split, merge, delete and add notes, and change their hand or velocity. With undo and redo.
- Saving creates a **new revision** of the note set (history is kept) and re-runs the score and the analysis, which are fast because they work on symbolic data.
- Low-confidence notes are highlighted so they can be reviewed first.

### 4.8 MIDI playback

- Sampled piano in the UI (Web Audio), synchronised with the playback clock: notes are scheduled about 100 ms ahead and re-synchronised on jumps and speed changes.
- Mixer: original audio and synthesised piano, mute or solo each hand, metronome with count-in from the tempo map.
- At slow speeds the synthesised piano sounds perfect, without the artefacts of stretched audio.

### 4.9 MIDI keyboard practice

- MIDI input is handled by the Rust core (midir), not Web MIDI, which WebKit on macOS does not support. Events reach the UI over WebSocket.
- Modes: **wait** (the video stops until you play the right notes), **play along** with tolerance windows, hands-separate practice, and loops that speed up when you get them right.
- Per-section statistics: accuracy, timing deviation and progress over time. Latency calibration.

## 5. Tech stack

| Layer                      | Technology                                                                                                          | Licence                                                               |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| App                        | Tauri 2                                                                                                             | MIT / Apache-2.0                                                      |
| UI                         | React, TypeScript, Vite                                                                                             | MIT                                                                   |
|                            | TanStack Router and Query, Zustand, i18next                                                                         | MIT                                                                   |
|                            | Tailwind CSS, shadcn/ui (Base UI)                                                                                   | MIT                                                                   |
|                            | openapi-typescript + openapi-fetch (generated API client; TypeScript 5.9 until the generator supports TypeScript 7) | MIT                                                                   |
|                            | PixiJS (timeline and piano roll)                                                                                    | MIT                                                                   |
|                            | Verovio (sheet music)                                                                                               | LGPL-3.0, as a separate WASM module                                   |
|                            | Tone.js or smplr, with piano samples (e.g. Salamander Grand Piano, CC BY 3.0)                                       | MIT                                                                   |
|                            | Signalsmith Stretch (time-stretching, playback engine v2)                                                           | MIT, to be confirmed                                                  |
| Core                       | Rust, Tokio, Axum, sqlx (SQLite), utoipa, tracing                                                                   | MIT / Apache-2.0                                                      |
|                            | object_store, midir, midly, rustfft                                                                                 | Permissive (MIT / Apache-2.0 / Unlicense)                             |
| External tools             | yt-dlp                                                                                                              | Unlicense                                                             |
|                            | ffmpeg 9 (LGPL shared build from FFmpeg-Builds, OS hardware encoders)                                               | LGPL-2.1                                                              |
|                            | Deno (JavaScript runtime for yt-dlp)                                                                                | MIT                                                                   |
| Engine                     | Python 3.13, uv, Pydantic                                                                                           | PSF / MIT / Apache-2.0                                                |
|                            | OpenCV (headless), NumPy, SciPy                                                                                     | Apache-2.0 / BSD                                                      |
|                            | ONNX Runtime                                                                                                        | MIT                                                                   |
|                            | partitura                                                                                                           | Apache-2.0                                                            |
|                            | music21                                                                                                             | BSD-3                                                                 |
|                            | mido, pretty_midi, mir_eval                                                                                         | MIT                                                                   |
|                            | Beat This! (beats and bars)                                                                                         | MIT, to be confirmed                                                  |
| Audio models (to evaluate) | ByteDance Piano Transcription, Transkun, hFT-Transformer; Basic Pitch as a lightweight option                       | **Code and weight licences to be verified** (Basic Pitch: Apache-2.0) |

**Ruled out for licensing reasons**, unless the business model changes: madmom's models (CC BY-NC-SA), Rubber Band (GPL) and any GPL build of ffmpeg (which also rules out x264, hence the OS encoders for the seek proxy).

## 6. Repository layout

```
music/
├─ apps/
│  ├─ desktop/          Tauri 2 (src-tauri): window, startup, packaging
│  ├─ ui/               React UI (reusable on the web)
│  └─ server/           (future) cloud API
├─ crates/              Rust core: domain · storage · jobs · media · midi · api
├─ engine/              Python engine (uv)
│  ├─ src/segno_engine/ protocol · vision · audio · fusion · rhythm · score · harmony
│  ├─ notebooks/        prototypes and exploration
│  └─ tests/
├─ packages/
│  ├─ api-client/       TypeScript client generated from OpenAPI
│  └─ schemas-ts/       TypeScript types generated from schemas/
├─ schemas/             JSON Schema for jobs and artifacts (source of truth)
├─ eval/                evaluation harness (test videos are not committed)
├─ scripts/             developer scripts (e.g. fetch_tools.py → .tools/)
├─ docs/                PLAN.md and adr/ (decision records)
└─ .github/workflows/   CI
```

Phase 0 created `apps/ui`, `apps/desktop`, `crates/{domain,storage,api}`, `packages/api-client`, `engine` and `scripts`; the other folders arrive with their phase.

Monorepo tooling: pnpm workspaces (plus Turborepo once there are enough packages to benefit), Cargo workspace (Rust), uv (Python), Biome (TypeScript lint and format) and lefthook for git hooks. Developer tools stay inside the repository: `node_modules`, `engine/.venv` and `.tools/`.

## 7. Quality: tests, evaluation and CI

- **Tests:** Rust with cargo-nextest (unit tests, and integration tests on a temporary SQLite); Python with pytest; UI with Vitest and Testing Library; Playwright for the UI in a browser and WebDriver (tauri-driver) for the packaged app.
- **Navigation performance:** the diagnostics overlay records jump, frame-step and shortcut latencies; a scripted benchmark on reference videos checks the budget before each release.
- **Transcription evaluation:** videos with reference MIDI and standard metrics computed with mir_eval: note F1 by onset, note F1 by onset and offset, and hand accuracy. Part of the set is **generated synthetically** by rendering known MIDI files as Synthesia-style videos in varied styles (colours, speeds, effects, partial keyboards), which gives perfect reference data in unlimited quantity. Metrics are compared on every change to catch regressions.
- **Harmony evaluation:** corpora annotated with Roman numerals (e.g. When in Rome) and pop arrangements with annotated chords (e.g. POP909), checking their licences first and using them for internal evaluation only.
- **CI (GitHub Actions):** formatting and linting (rustfmt, clippy, ruff, Biome), type checks (tsc, pyright), tests, a check that code generated from the contracts is up to date, a **licence audit** (cargo-deny, pip-licenses, pnpm licenses) and a Windows installer for each release.
- **Conventions:** English everywhere in the repository, Conventional Commits, SemVer, short-lived branches with PRs, and important decisions recorded as ADRs in `docs/adr/`.

## 8. Roadmap

Each phase ends with something usable and a verifiable exit criterion.

**Phase 0: Foundations** ✔ built on 24 November 2026

- Monorepo, toolchains, CI, linting, conventions and the first ADRs.
- End-to-end skeleton: Tauri starts the embedded Axum API, SQLite with migrations, OpenAPI → TypeScript client, and one screen showing real data.
- UI shell: layout, theme, i18n and the command registry (shortcuts and command palette).
- _Exit criterion:_ one command starts the whole app in development, and CI produces a Windows installer. (`pnpm dev` works locally; the CI workflow runs on the first push.)

**Phase 1A: Base app (features 1–4), navigation first**

1. Local file import and media preparation: seek proxy, thumbnails, waveform peaks and attacks. _(Done except attacks, which come with the timeline. Import: ffprobe, hard link or copy into the library, media served with Range requests. Preparation runs in the background with progress and makes the seek proxy, the thumbnail sprite and the waveform peaks. Jumps and frame steps are within the budget; see 4.2.)_
2. The player with the full navigation layer: two-level timeline, shortcuts, frame and attack stepping, tap-tempo grid, go-to, navigation history, return to start, replay and speed. _(First version done: play/pause, ±2 s, ±10 s, frame steps, start/end, speed, replay, and the switch to the proxy once it is ready. Next: the two-level timeline with the waveform and hover thumbnails.)_
3. Markers, sections, A-B loops, pre-roll, count-in and speed trainer.
4. Comments anchored to time.
5. YouTube downloads, chapters as markers, and the library (search, tags, remembered state).

- Navigation is tried with real videos from the first build, then adjusted and measured.
- _Exit criterion:_ the navigation budget is met on your laptop, and you use the app daily to study.

**Phase 1B (in parallel): Vision transcription prototype**

- Python CLI and notebooks: keyboard detection, key activation, falling bars, hand separation and MIDI output.
- Tested on 3–5 real videos in different styles (starting with [this one](https://www.youtube.com/watch?v=7A12z9S4F8U)) and on synthetic videos with reference MIDI.
- _Exit criterion:_ onset F1 ≥ 95 % on clean Synthesia-style videos, and the `keyboard/v1` and `notes/v1` contracts frozen.

**Phase 2: Transcription inside the app**

- On-demand engine install, job protocol and worker pool.
- Integrated vision and a piano roll in sync with the video.
- Audio model (chosen after evaluating accuracy, CPU speed and licence), fusion and confidence.
- Note editor with revisions and undo.
- MIDI playback, per-hand mixer and MIDI export.
- Navigation upgrades: automatic beats and bars, note-by-note stepping, suggested sections, MIDI input for MIDI learn, and playback engine v2 if the measurements call for it.
- _Exit criterion:_ from a downloaded video to a corrected MIDI file without leaving the app.

**Phase 3: Sheet music**

- Rhythm, quantisation, one staff per hand, voices, spelling and key signature.
- Verovio in sync with the video; MusicXML and PDF export.
- _Exit criterion:_ the score of a simple piece reads correctly without corrections and opens cleanly in MuseScore.

**Phase 4: Advanced harmonic analysis**

- Layer by layer: key and modulations → chords → numerals and functions → cadences → secondary dominants, mixture and chromaticism → non-chord tones → voice leading → scales.
- Harmony lane, score annotations, explanations and alternatives; chord changes as a navigation unit.
- _Exit criterion:_ accuracy measured on the evaluation corpora and validated by you on pieces you know well.

**Phase 5: MIDI keyboard practice**

- Wait mode, play-along, scoring, statistics and latency calibration.

**Phase 6: Professional distribution**

- Code signing, automatic updates, opt-in crash reports, a user guide, and macOS and Linux builds.

**Phase 7: Cloud** (when needed)

- `apps/server`, PostgreSQL, S3, queue, GPU workers, accounts, sync and the web version.

## 9. Risks and mitigations

| Risk                                                                                                    | Mitigation                                                                                                                        |
| ------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| YouTube changes and breaks downloads                                                                    | yt-dlp with automatic updates; importing local files always works                                                                 |
| Jumps and frame stepping feel slow on YouTube files (sparse keyframes)                                  | Seek-optimised proxy: within the navigation budget on the development laptop (4.2); the `bench` command checks it                 |
| Loop gaps or poor sound at slow speeds                                                                  | Playback engine v2 (Web Audio + time-stretching)                                                                                  |
| Shortcuts clash with the webview or with typing                                                         | Central command registry, WebView2 default shortcuts disabled, clear focus rules (`Esc` returns to the player)                    |
| Wide visual variety (Synthesia, renders with effects, filmed pianos, partial keyboards, overlaid logos) | Manual calibration as a fallback, style profiles, a varied synthetic set and audio as a fallback                                  |
| Turning MIDI into readable sheet music is hard (quantisation, voices)                                   | Build on partitura and music21, the note editor and metrics; the piano roll is useful from day one even if the score takes longer |
| Harmonic analysis is ambiguous                                                                          | Alternatives with confidence, explanations and evaluation corpora; the finest features are marked as beta                         |
| Packaging Python on Windows (size, antivirus false positives)                                           | Engine installed on demand with uv and a standalone Python, plus code signing; validated at the start of Phase 2                  |
| AI model licences (many are "non-commercial")                                                           | Audit before adopting any model, and licence checks in CI                                                                         |
| Performance without a dedicated GPU                                                                     | Crop to the keyboard strip, vectorised NumPy, light ONNX models and background jobs                                               |
| Audio/picture offset in some videos                                                                     | Automatic offset estimation and manual adjustment in the UI                                                                       |
| Very large scope                                                                                        | Short phases with exit criteria; 1A and 1B are the only parallel tracks                                                           |

## 10. Development environment

Checked on your laptop (Windows 11, Intel i7-1255U, 16 GB RAM, Intel Iris Xe integrated GPU):

| Tool                                                          | Where                                                               |
| ------------------------------------------------------------- | ------------------------------------------------------------------- |
| Git 2.42, Node 22.17, pnpm 11.10, Python 3.13.5, WebView2     | System (already present)                                            |
| Visual Studio Build Tools 2022 (C++) + Windows SDK 10.0.26100 | System: the only system-wide install, required by Rust and Tauri    |
| Rust 1.100.0 (rustup)                                         | User profile; the version is pinned in `rust-toolchain.toml`        |
| uv 0.13.9                                                     | User profile (winget)                                               |
| Python packages, yt-dlp and Deno                              | `engine/.venv` (yt-dlp and Deno in the `tools` group)               |
| ffmpeg 9.0 (LGPL)                                             | `.tools/ffmpeg`, fetched by `pnpm tools` with checksum verification |

Without an NVIDIA GPU, everything is designed to run well on the CPU, with optional acceleration; the Intel GPU's Quick Sync encoder speeds up proxy creation.

## 11. Decision log

| Decision                       | Chosen                                                                         | Alternatives                           | Reason                                                                                             |
| ------------------------------ | ------------------------------------------------------------------------------ | -------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Goal                           | Local, cloud-ready                                                             | Personal use only; SaaS from the start | No server costs or YouTube blocking, without closing the door to scaling                           |
| Top priority                   | Instant, comfortable navigation                                                | —                                      | Chosen by you: moving around the song is the core of studying it                                   |
| App                            | Tauri 2 + React/TypeScript                                                     | Electron; web SaaS; PySide6            | Light, secure, cross-platform, mobile possible, UI reusable on the web                             |
| Local backend                  | Rust core + Python engine                                                      | Everything in Python (FastAPI)         | Instant start-up, features 1–4 without Python, engine on demand, scalable workers                  |
| UI ↔ core                      | HTTP + WebSocket on localhost with a token                                     | Tauri IPC (commands)                   | The same API will serve the web version; the local port is protected with a token and a Host check |
| Playback media                 | Seek-optimised proxy (keyframe ≤ 15 frames apart, GPU encoder or OpenH264)     | Playing the downloaded file directly   | Jumps and frame steps about 10× faster (measured, 4.2); keeps ffmpeg within the LGPL               |
| Playback engine                | Native `<video>` first; Web Audio + time-stretching if measurements require it | Web Audio from day one                 | Start simple, upgrade on evidence                                                                  |
| Beat grid before transcription | Tap tempo                                                                      | Wait until Phase 2                     | Musical navigation from the first version                                                          |
| Transcription                  | Hybrid vision + audio                                                          | Vision only; audio only                | Precision and hands from vision, plus dynamics, pedal and fallback from audio                      |
| Sheet music                    | Verovio                                                                        | OpenSheetMusicDisplay; VexFlow         | Better engraving and a timemap to sync with the video                                              |
| AI inference                   | ONNX Runtime                                                                   | PyTorch in production                  | Small package, good CPU performance, GPUs from any vendor                                          |
| MIDI input                     | Rust core (midir)                                                              | Web MIDI                               | Works the same on Windows and macOS (WebKit has no Web MIDI)                                       |
| Harmony in v1                  | Academic level                                                                 | Functional; chords and key             | Chosen by you                                                                                      |
| Priority                       | Base app + vision prototype                                                    | Base first; transcription first        | A useful app early, and the most uncertain part validated early                                    |
| Licences                       | Permissive only                                                                | Allowing GPL or "non-commercial"       | Keeps the commercial option open                                                                   |
| Language                       | English for code, comments, commits and docs; UI in Spanish and English        | Spanish documentation                  | Chosen by you                                                                                      |
| Name                           | Segno (`io.github.danpingar.segno`)                                            | Rubato; Leggio                         | Chosen by you: the score's "jump back here" sign suits a navigation-first app                      |
| UI components                  | shadcn/ui on Base UI                                                           | shadcn/ui on Radix; React Aria         | shadcn's current default, maintained by the original Radix authors                                 |
| JSON naming                    | `snake_case` ([ADR 0002](adr/0002-http-api-conventions.md))                    | camelCase                              | One naming scheme across Rust, Python and TypeScript                                               |
| Developer tools                | Project-local (`node_modules`, `engine/.venv`, `.tools/`)                      | Global installs                        | Chosen by you: reproducible, nothing leaks into the system except the C++ build tools              |

## 12. Open questions

1. **Prototype videos.** The Monk video in Phase 1B plus 2–4 more in different styles (other renderers, a filmed real piano, a partial keyboard).
2. **Practice hardware.** Do you have a MIDI keyboard (MIDI learn in Phase 2, practice in Phase 5) or a Bluetooth page-turner pedal? Nothing is blocked until then.
3. **Repository licence.** Open source (which licence?) or proprietary? It matters before the repository is made public.

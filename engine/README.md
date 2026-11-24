# segno-engine

The analysis engine of Segno: a stateless Python worker that turns input files into
artifacts (notes, scores, harmonic analysis). The Rust core starts it and talks to it with
JSON Lines over stdin/stdout (see `src/segno_engine/protocol.py`).

```sh
uv sync                                   # create .venv with the engine and dev tools
uv run segno-engine worker --stdio        # serve the core (try typing {"type": "ping"})
uv run pytest                             # tests
uv run ruff check . && uv run pyright     # lint and types
uv sync --group tools                     # yt-dlp + Deno, for downloading test videos
```

See [docs/PLAN.md](../docs/PLAN.md) for the transcription and analysis pipeline.

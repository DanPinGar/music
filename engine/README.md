# segno-engine

The analysis engine of Segno: a stateless Python worker that turns input files into
artifacts (notes, scores, harmonic analysis). The Rust core starts it and talks to it with
JSON Lines over stdin/stdout (see `src/segno_engine/protocol.py`).

```sh
uv sync --all-groups                      # .venv: engine, dev tools, yt-dlp and Deno
uv run segno-engine worker --stdio        # serve the core (try typing {"type": "ping"})
uv run pytest                             # tests
uv run ruff check . && uv run pyright     # lint and types
```

`uv sync` makes `.venv` match the selected groups exactly; without `--all-groups` it removes
the `tools` group (yt-dlp and Deno, used to download test videos). `uv run` never removes
packages.

See [docs/PLAN.md](../docs/PLAN.md) for the transcription and analysis pipeline.

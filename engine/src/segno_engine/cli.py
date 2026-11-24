"""Command line: `segno-engine`."""

import argparse
import io
import sys
from collections.abc import Sequence

from segno_engine import __version__
from segno_engine.worker import run_stdio_worker


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="segno-engine", description="Segno analysis engine.")
    parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    commands = parser.add_subparsers(dest="command", required=True)

    worker = commands.add_parser("worker", help="Serve requests from the Segno core.")
    worker.add_argument(
        "--stdio", action="store_true", help="Exchange JSON Lines over stdin and stdout."
    )

    args = parser.parse_args(argv)
    if args.command == "worker":
        if not args.stdio:
            parser.error("worker needs a transport: --stdio")
        _use_utf8_stdio()
        return run_stdio_worker(sys.stdin, sys.stdout)
    parser.error(f"unknown command: {args.command}")


def _use_utf8_stdio() -> None:
    """The protocol is UTF-8 with LF line endings, whatever the console code page."""
    for stream in (sys.stdin, sys.stdout):
        if isinstance(stream, io.TextIOWrapper):
            stream.reconfigure(encoding="utf-8", newline="\n")

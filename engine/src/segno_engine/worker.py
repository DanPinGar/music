"""Stdio worker: serves requests from the Segno core until told to stop."""

from typing import TextIO

from pydantic import BaseModel, ValidationError

from segno_engine import __version__
from segno_engine.protocol import REQUESTS, Hello, Ping, Pong, ProtocolError, Shutdown


def run_stdio_worker(stdin: TextIO, stdout: TextIO) -> int:
    """Answer requests read from `stdin` until `shutdown` or end of input.

    Returns the process exit code.
    """
    _send(stdout, Hello(engine_version=__version__))
    for line in stdin:
        if not line.strip():
            continue
        try:
            request = REQUESTS.validate_json(line)
        except ValidationError as error:
            first = error.errors(include_url=False)[0]
            _send(stdout, ProtocolError(message=f"invalid request: {first['msg']}"))
            continue
        match request:
            case Ping(id=request_id):
                _send(stdout, Pong(id=request_id))
            case Shutdown():
                return 0
    return 0


def _send(stdout: TextIO, message: BaseModel) -> None:
    stdout.write(message.model_dump_json() + "\n")
    stdout.flush()

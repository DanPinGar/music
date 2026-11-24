import io
import json
from typing import Any

from segno_engine import __version__
from segno_engine.worker import run_stdio_worker


def run(*lines: str) -> tuple[int, list[dict[str, Any]]]:
    stdin = io.StringIO("".join(f"{line}\n" for line in lines))
    stdout = io.StringIO()
    code = run_stdio_worker(stdin, stdout)
    return code, [json.loads(line) for line in stdout.getvalue().splitlines()]


def test_says_hello_first() -> None:
    _, messages = run()
    assert messages == [{"type": "hello", "engine_version": __version__, "protocol_version": 1}]


def test_answers_a_ping_with_the_same_id() -> None:
    _, messages = run('{"type": "ping", "id": "42"}')
    assert messages[1] == {"type": "pong", "id": "42"}


def test_reports_invalid_requests_and_keeps_serving() -> None:
    _, messages = run("not json", '{"type": "dance"}', '{"type": "ping"}')
    assert [m["type"] for m in messages] == ["hello", "error", "error", "pong"]


def test_ignores_blank_lines() -> None:
    _, messages = run("", "   ", '{"type": "ping"}')
    assert [m["type"] for m in messages] == ["hello", "pong"]


def test_stops_on_shutdown() -> None:
    code, messages = run('{"type": "shutdown"}', '{"type": "ping"}')
    assert code == 0
    assert [m["type"] for m in messages] == ["hello"]

import pytest

from segno_engine import __version__
from segno_engine.cli import main


def test_prints_its_version(capsys: pytest.CaptureFixture[str]) -> None:
    with pytest.raises(SystemExit) as exit_info:
        main(["--version"])
    assert exit_info.value.code == 0
    assert capsys.readouterr().out.strip() == f"segno-engine {__version__}"


def test_the_worker_needs_a_transport(capsys: pytest.CaptureFixture[str]) -> None:
    with pytest.raises(SystemExit) as exit_info:
        main(["worker"])
    assert exit_info.value.code == 2
    assert "--stdio" in capsys.readouterr().err

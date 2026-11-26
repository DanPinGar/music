"""Scores the attacks found in a Synthesia-style video's audio against its keyboard.

The keyboard drawn at the bottom of these videos lights a key, in colour, while its note
sounds, so the moments keys light up are the note starts. This reads two rows of pixels per
frame with ffmpeg (one through the white keys below the black ones, one through the black
keys), turns them into key presses, groups the presses that start together (a chord is one
attack) and matches them with the attacks of an `attacks.json` written by the app (or by
`cargo run --release -p segno-media --example attacks -- video.mp4 attacks.json`).

    python eval/attacks/evaluate.py video.mp4 attacks.json [--keyboard-top 840]

The layout constants fit an 88-key keyboard spanning the whole frame width, as in the
itsRemco videos. Standard library only.
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FFMPEG = ROOT / ".tools" / "ffmpeg" / "bin" / "ffmpeg.exe"
FFPROBE = ROOT / ".tools" / "ffmpeg" / "bin" / "ffprobe.exe"

WHITE_KEYS = 52  # A0 to C8
# White keys (from A) that have a black key on their right: A, C, D, F, G.
HAS_BLACK_RIGHT = {"A", "C", "D", "F", "G"}
WHITE_NAMES = ["A", "B", "C", "D", "E", "F", "G"]
# A key counts as lit when its colour is this saturated (max - min of R, G, B).
LIT_SATURATION = 60
# Presses starting this close together are one attack (a chord, or a slightly rolled one).
CHORD_SECONDS = 0.035
# An attack matches a key press this close in time (after removing the global offset).
MATCH_SECONDS = 0.05


@dataclass(frozen=True)
class Key:
    name: str
    x: int
    black: bool


def keyboard(width: int) -> list[Key]:
    white = width / WHITE_KEYS
    keys: list[Key] = []
    for index in range(WHITE_KEYS):
        name = WHITE_NAMES[index % 7]
        octave = (index + 5) // 7  # A0, B0, C1…
        keys.append(Key(f"{name}{octave}", int((index + 0.5) * white), black=False))
        if name in HAS_BLACK_RIGHT and index < WHITE_KEYS - 1:
            keys.append(Key(f"{name}#{octave}", int((index + 1) * white), black=True))
    return keys


def video_facts(video: Path) -> tuple[int, int, float]:
    output = subprocess.run(
        [
            str(FFPROBE),
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate",
            "-of",
            "json",
            str(video),
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    stream = json.loads(output)["streams"][0]
    numerator, denominator = stream["avg_frame_rate"].split("/")
    return int(stream["width"]), int(stream["height"]), int(numerator) / int(denominator)


def key_presses(video: Path, keyboard_top: int) -> list[tuple[float, str]]:
    """(time, key) for every key that lights up."""
    width, height, fps = video_facts(video)
    keys = keyboard(width)
    # Black keys end about two thirds down the keyboard; white keys alone below that.
    black_row = keyboard_top + (height - keyboard_top) * 30 // 100
    white_row = keyboard_top + (height - keyboard_top) * 80 // 100
    # Two rows per strip: 4:2:0 video cannot be cropped to an odd height. Rows 0 and 2 are read.
    graph = (
        f"[0:v]split[a][b];[a]crop={width}:2:0:{black_row // 2 * 2}[top];"
        f"[b]crop={width}:2:0:{white_row // 2 * 2}[bottom];[top][bottom]vstack"
    )
    process = subprocess.Popen(
        [
            str(FFMPEG),
            "-v",
            "error",
            "-i",
            str(video),
            "-filter_complex",
            graph,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ],
        stdout=subprocess.PIPE,
    )
    assert process.stdout is not None
    frame_bytes = width * 4 * 3
    lit_before: dict[str, bool] = {key.name: False for key in keys}
    presses: list[tuple[float, str]] = []
    frame = 0
    while chunk := process.stdout.read(frame_bytes):
        if len(chunk) < frame_bytes:
            break
        for key in keys:
            row = 0 if key.black else 2
            lit = False
            # A few pixels around the key's centre, so anti-aliasing does not matter.
            for x in range(key.x - 2, key.x + 3):
                offset = (row * width + x) * 3
                red, green, blue = chunk[offset], chunk[offset + 1], chunk[offset + 2]
                if max(red, green, blue) - min(red, green, blue) >= LIT_SATURATION:
                    lit = True
                    break
            if lit and not lit_before[key.name]:
                presses.append((frame / fps, key.name))
            lit_before[key.name] = lit
        frame += 1
    process.wait()
    return presses


def attacks_from_presses(presses: list[tuple[float, str]]) -> list[float]:
    """One attack per group of presses that start together."""
    attacks: list[float] = []
    for time, _ in sorted(presses):
        if not attacks or time - attacks[-1] > CHORD_SECONDS:
            attacks.append(time)
    return attacks


def match(truth: list[float], found: list[float], offset: float) -> list[tuple[float, float]]:
    """Pairs (truth, found) matched greedily in time order, each used once."""
    pairs: list[tuple[float, float]] = []
    j = 0
    for wanted in truth:
        while j < len(found) and found[j] - offset < wanted - MATCH_SECONDS:
            j += 1
        if j < len(found) and abs(found[j] - offset - wanted) <= MATCH_SECONDS:
            pairs.append((wanted, found[j]))
            j += 1
    return pairs


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("video", type=Path)
    parser.add_argument("attacks", type=Path)
    parser.add_argument(
        "--keyboard-top",
        type=int,
        default=840,
        help="first pixel row of the keyboard (840 in 1080p itsRemco videos)",
    )
    args = parser.parse_args()

    presses = key_presses(args.video, args.keyboard_top)
    truth = attacks_from_presses(presses)
    found = json.loads(args.attacks.read_text())["times"]

    # Audio and picture may be offset by a constant: estimate it from the nearest attacks.
    nearest = [min(found, key=lambda f, t=t: abs(f - t)) - t for t in truth] if found else [0.0]
    offset = statistics.median(d for d in nearest if abs(d) <= 0.2) if nearest else 0.0
    pairs = match(truth, found, offset)
    precision = len(pairs) / len(found) if found else 0.0
    recall = len(pairs) / len(truth) if truth else 0.0
    f_measure = 2 * precision * recall / (precision + recall) if pairs else 0.0
    errors = sorted(abs(f - offset - t) * 1000 for t, f in pairs)

    print(
        f"key presses: {len(presses)}, attacks on the keyboard: {len(truth)}, "
        f"attacks found in the audio: {len(found)}"
    )
    print(f"audio offset: {offset * 1000:+.0f} ms")
    print(
        f"precision {precision:.3f}  recall {recall:.3f}  F {f_measure:.3f} "
        f"(within {MATCH_SECONDS * 1000:.0f} ms)"
    )
    if errors:
        print(
            f"timing error: median {statistics.median(errors):.0f} ms, "
            f"p90 {errors[int(len(errors) * 0.9)]:.0f} ms"
        )
    matched = {f for _, f in pairs}
    extra = [round(f, 2) for f in found if f not in matched]
    missed = [round(t, 2) for t in truth if t not in {t for t, _ in pairs}]
    print(f"first extra attacks: {extra[:15]}")
    print(f"first missed attacks: {missed[:15]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

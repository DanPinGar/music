# /// script
# requires-python = ">=3.12"
# ///
"""Download project-local developer tools into .tools/ (ignored by git).

Today: ffmpeg and ffprobe, an LGPL "shared" build from FFmpeg-Builds (the licence family the
app will ship with), verified against the SHA-256 checksums published with the release.

    uv run --no-project scripts/fetch_tools.py [--force]
    pnpm tools
"""

import argparse
import hashlib
import http.client
import json
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.error
import urllib.request
import zipfile
from collections.abc import Callable
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
TOOLS = ROOT / ".tools"
FFMPEG_DIR = TOOLS / "ffmpeg"
RELEASE_API = "https://api.github.com/repos/FFmpeg-Builds/FFmpeg-Builds/releases/latest"
USER_AGENT = "segno-fetch-tools"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--force", action="store_true", help="download again even if present")
    args = parser.parse_args()

    exe = FFMPEG_DIR / "bin" / ("ffmpeg.exe" if sys.platform == "win32" else "ffmpeg")
    if exe.exists() and not args.force:
        print(f"ffmpeg already in {FFMPEG_DIR.relative_to(ROOT)} (use --force to update)")
        print(version_line(exe))
        return 0

    platform_tag, extension = build_target()
    release = json.loads(fetch(RELEASE_API))
    asset = newest_release_build(release["assets"], platform_tag, extension)
    checksums = fetch(asset_url(release["assets"], "checksums.sha256")).decode()
    expected = expected_sha256(checksums, asset["name"])

    TOOLS.mkdir(exist_ok=True)
    archive = TOOLS / asset["name"]
    print(f"Downloading {asset['name']} ({asset['size'] / 1e6:.0f} MB)...")
    download(asset["browser_download_url"], archive)
    actual = sha256(archive)
    if actual != expected:
        archive.unlink()
        sys.exit(f"Checksum mismatch for {asset['name']}: expected {expected}, got {actual}")

    install(archive, FFMPEG_DIR)
    archive.unlink()
    (FFMPEG_DIR / "SOURCE.txt").write_text(f"{asset['browser_download_url']}\nsha256 {expected}\n")
    print(f"Installed in {FFMPEG_DIR.relative_to(ROOT)}")
    print(version_line(exe))
    return 0


def build_target() -> tuple[str, str]:
    """Platform tag and archive extension of the build for this machine."""
    arch = "arm64" if platform.machine().lower() in {"arm64", "aarch64"} else "64"
    if sys.platform == "win32":
        return f"win{arch}", ".zip"
    if sys.platform.startswith("linux"):
        return f"linux{arch}", ".tar.xz"
    sys.exit("No prebuilt ffmpeg for this platform: install it with your package manager.")


def newest_release_build(
    assets: list[dict[str, Any]], platform_tag: str, extension: str
) -> dict[str, Any]:
    """The LGPL shared build of the newest FFmpeg release branch (e.g. n9.0)."""
    pattern = re.compile(
        rf"^ffmpeg-n(\d+)\.(\d+)-latest-{platform_tag}-lgpl-shared-\1\.\2{re.escape(extension)}$"
    )
    candidates: list[tuple[tuple[int, int], dict[str, Any]]] = []
    for asset in assets:
        match = pattern.match(asset["name"])
        if match:
            candidates.append(((int(match[1]), int(match[2])), asset))
    if not candidates:
        sys.exit(f"No LGPL shared build for {platform_tag} in the latest release")
    return max(candidates, key=lambda candidate: candidate[0])[1]


def asset_url(assets: list[dict[str, Any]], name: str) -> str:
    for asset in assets:
        if asset["name"] == name:
            return asset["browser_download_url"]
    sys.exit(f"{name} is missing from the release")


def expected_sha256(checksums: str, name: str) -> str:
    for line in checksums.splitlines():
        digest, _, file_name = line.partition("  ")
        if file_name.strip() == name:
            return digest.strip().lower()
    sys.exit(f"No checksum published for {name}")


def install(archive: Path, destination: Path) -> None:
    """Unpack the archive's single top-level folder as `destination`."""
    staging = destination.with_name(destination.name + ".staging")
    shutil.rmtree(staging, ignore_errors=True)
    staging.mkdir()
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as zip_file:
            zip_file.extractall(staging)
    else:
        with tarfile.open(archive) as tar_file:
            tar_file.extractall(staging, filter="data")
    (top,) = staging.iterdir()
    shutil.rmtree(destination, ignore_errors=True)
    top.rename(destination)
    staging.rmdir()


def version_line(exe: Path) -> str:
    output = subprocess.run([exe, "-version"], capture_output=True, text=True, check=True)
    return output.stdout.splitlines()[0]


def fetch(url: str) -> bytes:
    def attempt() -> bytes:
        request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        with urllib.request.urlopen(request, timeout=60) as response:
            return response.read()

    return with_retries(attempt)


def download(url: str, destination: Path) -> None:
    def attempt() -> None:
        request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        with (
            urllib.request.urlopen(request, timeout=60) as response,
            destination.open("wb") as file,
        ):
            shutil.copyfileobj(response, file, length=1 << 20)

    with_retries(attempt)


def with_retries[T](action: Callable[[], T], attempts: int = 3) -> T:
    """Runs a network action, retrying transient failures (GitHub drops connections)."""
    for number in range(1, attempts + 1):
        try:
            return action()
        except (urllib.error.URLError, http.client.HTTPException, ConnectionError, TimeoutError):
            if number == attempts:
                raise
            print(f"Network error, retrying ({number}/{attempts - 1})...")
            time.sleep(2 * number)
    raise AssertionError("unreachable")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


if __name__ == "__main__":
    raise SystemExit(main())

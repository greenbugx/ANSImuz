from __future__ import annotations

import hashlib
import os
import platform
import shutil
import sys
from pathlib import Path

BUNDLED_QJS_VERSION = "v0.18.0"
BUNDLED_QJS_SHA256: dict[str, str] = {
    "win32-x64": "ff03398d74ea6e6bd3a8e58058f7003225abcac229d7d57775b7aaae812a49c7",
    "linux-x64": "85ded23b9fabe8c1644fc40a8d2bbdd71157affcbd807359c618cc717ee24d7d",
}

_CANDIDATE_SYSTEM_RUNTIMES = (
    ("deno", ("deno",)),
    ("node", ("node",)),
    ("quickjs", ("qjs", "quickjs")),
    ("bun", ("bun",)),
)


def _default_data_dir() -> Path:
    env_dir = os.environ.get("ANSIMUZ_DATA_DIR")
    if env_dir:
        return Path(env_dir)

    cwd = Path.cwd()
    if (cwd / "backend").is_dir() and (cwd / "assets").is_dir():
        return cwd

    repo_root = Path(__file__).resolve().parents[3]
    if (repo_root / "backend").is_dir() and (repo_root / "assets").is_dir():
        return repo_root

    installed = Path("/usr/share/ansimuz")
    if (installed / "backend").is_dir() and (installed / "assets").is_dir():
        return installed

    return repo_root


def _bundled_qjs_filename() -> str:
    return "qjs.exe" if sys.platform.startswith("win") else "qjs"


def _bundled_qjs_path(data_dir: str | Path | None = None) -> Path:
    root = Path(data_dir) if data_dir else _default_data_dir()
    return root / "runtime" / _bundled_qjs_filename()


def _verify_sha256(path: Path, expected_hex: str) -> bool:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest().lower() == expected_hex.lower()


def _runtime_target_key() -> str:
    plat = "win32" if sys.platform.startswith("win") else sys.platform
    machine = platform.machine().lower()
    arch = "x64" if machine in ("amd64", "x86_64") else machine
    return f"{plat}-{arch}"


def _bundled_qjs_verified(data_dir: str | Path | None = None) -> Path | None:
    candidate = _bundled_qjs_path(data_dir)
    if not candidate.is_file():
        return None

    expected = BUNDLED_QJS_SHA256.get(_runtime_target_key())
    if not expected or not _verify_sha256(candidate, expected):
        return None

    return candidate


def detect_runtime(data_dir: str | Path | None = None) -> tuple[dict[str, dict[str, str]], str, str | None]:
    """
    Resolves available JS challenge runtimes in yt-dlp's expected dict format.
    Prefers verified fast system runtimes when installed, otherwise uses the
    bundled QuickJS binary if present, falling back to empty degraded mode.
    """
    bundled = _bundled_qjs_verified(data_dir)

    for runtime_name, binaries in _CANDIDATE_SYSTEM_RUNTIMES:
        if runtime_name in ("quickjs", "bun") and bundled is not None:
            return {"quickjs": {"path": str(bundled)}}, "bundled-quickjs", str(bundled)

        for binary in binaries:
            found = shutil.which(binary)
            if found:
                return {runtime_name: {"path": found}}, f"system-{runtime_name}", found

    if bundled is not None:
        return {"quickjs": {"path": str(bundled)}}, "bundled-quickjs", str(bundled)

    return {}, "none", None


def js_runtimes_param(data_dir: str | Path | None = None) -> dict[str, dict[str, str]]:
    runtimes, _, _ = detect_runtime(data_dir)
    return runtimes


def runtime_status(data_dir: str | Path | None = None) -> dict[str, str | None]:
    _, mode, path = detect_runtime(data_dir)
    return {"mode": mode, "path": path}


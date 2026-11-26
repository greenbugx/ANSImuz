from __future__ import annotations

import logging
import os
import tempfile
from logging.handlers import RotatingFileHandler
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from providers.ytdlp import runtime

# Tier 1 resolves audio without paying the JS challenge cost when mobile clients succeed
TIER1_PLAYER_CLIENTS = ["visionos", "android"]
FALLBACK_PLAYER_CLIENTS = [
    "ios",
    "mweb",
    "web",
    "web_music",
    "android_vr",
    "tv",
    "tv_samsung",
    "tv_simply",
    "default",
]
PLAYER_CLIENTS = [*TIER1_PLAYER_CLIENTS, *FALLBACK_PLAYER_CLIENTS]

_ALLOWED_HOSTS = {
    "youtube.com",
    "www.youtube.com",
    "music.youtube.com",
    "youtu.be",
}


def _default_cache_dir() -> str:
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA") or os.path.join(os.path.expanduser("~"), "AppData", "Local")
    else:
        base = os.environ.get("XDG_CACHE_HOME") or os.path.join(os.path.expanduser("~"), ".cache")
    path = Path(base) / "ansimuz" / "ytdlp"
    try:
        path.mkdir(parents=True, exist_ok=True)
        return str(path)
    except OSError:
        return tempfile.gettempdir()


class _FileOnlyLogger:
    """Routes yt-dlp diagnostics away from stdout/stderr so the TUI stays clean."""

    def __init__(self, log_path: Path | None = None):
        self._logger: logging.Logger | None = None
        if log_path is not None:
            try:
                log_path.parent.mkdir(parents=True, exist_ok=True)
                logger = logging.getLogger("ansimuz.ytdlp")
                logger.setLevel(logging.WARNING)
                logger.propagate = False
                if not logger.handlers:
                    handler = RotatingFileHandler(
                        log_path,
                        maxBytes=512 * 1024,
                        backupCount=2,
                        encoding="utf-8",
                    )
                    handler.setFormatter(logging.Formatter("%(asctime)s [%(levelname)s] %(message)s"))
                    logger.addHandler(handler)
                self._logger = logger
            except OSError:
                self._logger = None

    def debug(self, msg: str) -> None:
        return None

    def info(self, msg: str) -> None:
        return None

    def warning(self, msg: str) -> None:
        if self._logger is not None:
            self._logger.warning("%s", msg)

    def error(self, msg: str) -> None:
        if self._logger is not None:
            self._logger.error("%s", msg)


def assert_allowed_url(url: str) -> None:
    if not isinstance(url, str) or not url.strip():
        raise ValueError("Unsupported URL")
    parsed = urlparse(url.strip())
    host = (parsed.hostname or "").rstrip(".").lower()
    if parsed.scheme.lower() != "https" or parsed.username or parsed.password or host not in _ALLOWED_HOSTS:
        raise ValueError(f"Unsupported URL host: {host or 'invalid'}")


def apply_hermetic_globals() -> None:
    """
    Blocks user yt-dlp plugin discovery before YoutubeDL instantiation.
    """
    import yt_dlp.globals

    yt_dlp.globals.plugin_dirs.value = []
    yt_dlp.globals.all_plugins_loaded.value = True


def build_params(
    cache_dir: str | None = None,
    data_dir: str | None = None,
    player_clients: list[str] | None = None,
    log_path: Path | None = None,
) -> dict[str, Any]:
    effective_cache = cache_dir if cache_dir is not None else _default_cache_dir()
    clients = list(player_clients) if player_clients is not None else list(PLAYER_CLIENTS)

    return {
        "format": "bestaudio/best",
        "noplaylist": True,
        "skip_download": True,
        "quiet": True,
        "no_warnings": True,
        "noprogress": True,
        "logger": _FileOnlyLogger(log_path),
        "ignoreconfig": True,
        "cachedir": effective_cache,
        "socket_timeout": 15,
        "retries": 2,
        "extractor_retries": 1,
        "extractor_args": {"youtube": {"player_client": clients}},
        "js_runtimes": runtime.js_runtimes_param(data_dir),
        "remote_components": [],
        "allowed_extractors": [r"^youtube$"],
    }
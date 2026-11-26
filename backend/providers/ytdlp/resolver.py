from __future__ import annotations

import threading
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlparse

from exceptions import ResolverOutdatedError, StreamUnavailableError
from providers.ytdlp import errors, options, runtime

_locks = {
    "interactive": threading.Lock(),
    "background": threading.Lock(),
}
_instances: dict[tuple[str, str], Any] = {}
_cache_dir: str | None = None
_data_dir: str | None = None
_log_path: Path | None = None


def configure(
    cache_dir: str | None = None,
    data_dir: str | None = None,
    log_path: str | None = None,
) -> None:
    """Configures ANSImuz-owned paths and clears cached YoutubeDL instances if changed."""
    global _cache_dir, _data_dir, _log_path
    with _locks["interactive"], _locks["background"]:
        _cache_dir = cache_dir
        _data_dir = data_dir
        _log_path = Path(log_path) if log_path else None
        _instances.clear()


def _create_ydl(params: dict[str, Any]) -> Any:
    import yt_dlp

    options.apply_hermetic_globals()
    return yt_dlp.YoutubeDL(params)


def _get_ydl(kind: str, tier: str) -> Any:
    key = (kind, tier)
    if key not in _instances:
        clients = options.TIER1_PLAYER_CLIENTS if tier == "tier1" else options.FALLBACK_PLAYER_CLIENTS
        params = options.build_params(
            cache_dir=_cache_dir,
            data_dir=_data_dir,
            player_clients=clients,
            log_path=_log_path,
        )
        _instances[key] = _create_ydl(params)
    return _instances[key]


def _expires_at(url: str) -> int | None:
    try:
        return int(parse_qs(urlparse(url).query)["expire"][0])
    except (KeyError, IndexError, ValueError):
        return None


def _extract_stream_fields(info: dict[str, Any]) -> dict[str, Any]:
    if not info:
        raise StreamUnavailableError("No playable audio format is available for this track.")

    if info.get("entries"):
        entries = list(info["entries"])
        if not entries:
            raise StreamUnavailableError("No playable audio format is available for this track.")
        info = entries[0]

    stream_url = info.get("url")
    if not stream_url:
        for fmt in info.get("requested_formats") or []:
            if fmt.get("acodec") not in (None, "none") and fmt.get("url"):
                stream_url = fmt["url"]
                info = {**info, **fmt}
                break

    if not stream_url:
        raise StreamUnavailableError("No playable audio format is available for this track.")

    import yt_dlp

    rt_status = runtime.runtime_status(_data_dir)
    return {
        "url": stream_url,
        "headers": dict(info.get("http_headers") or {}),
        "expires_at": _expires_at(stream_url),
        "ext": info.get("ext"),
        "acodec": info.get("acodec"),
        "abr": info.get("abr"),
        "duration_s": info.get("duration"),
        "video_id": info.get("id"),
        "title": info.get("title"),
        "ytdlp_version": yt_dlp.version.__version__,
        "js_runtime": rt_status["mode"],
    }


def _extract_with_tier(url: str, kind: str, tier: str) -> dict[str, Any]:
    try:
        info = _get_ydl(kind, tier).extract_info(url, download=False)
    except Exception as exc:
        raise errors.classify_ytdlp_error(exc) from exc
    return _extract_stream_fields(info)


def resolve_stream(url: str, *, background: bool = False) -> dict[str, Any]:
    """
    Resolves a YouTube URL to a direct audio stream URL and HTTP headers.
    Tries fast non-JS audio clients first, escalating to the JS-backed fallback
    clients only when formats are unavailable or solver extraction is needed.
    """
    options.assert_allowed_url(url)
    kind = "background" if background else "interactive"

    with _locks[kind]:
        try:
            return _extract_with_tier(url, kind, "tier1")
        except (StreamUnavailableError, ResolverOutdatedError):
            return _extract_with_tier(url, kind, "fallback")


def ytdlp_status() -> dict[str, Any]:
    import yt_dlp

    rt = runtime.runtime_status(_data_dir)
    return {
        "version": yt_dlp.version.__version__,
        "bundle_age": None,
        "js_runtime": rt["mode"],
        "js_runtime_path": rt["path"],
    }


def ytdlp_warmup() -> dict[str, Any]:
    options.apply_hermetic_globals()
    with _locks["interactive"]:
        _get_ydl("interactive", "tier1")
    return ytdlp_status()